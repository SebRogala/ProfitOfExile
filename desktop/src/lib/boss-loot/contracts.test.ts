import { describe, expect, it } from 'vitest'
import source from './__fixtures__/boss-loot.json'
import {
	createFixtureCapture, createFixtureDevice, createFixtureModuleSettings, createFixtureOverlaySettings,
	createFixtureProfiles, createFixtureRuns, createFixtureStats, createFixtureSubmit
} from './fixture-adapters'
import { editRunItems, serializeSubmitRun } from './contracts'
import type { BossStats, StoredRun, SubmitOutcome, SubmitRun } from './contracts'

const profile = { boss: 'catarina', profileKey: 'Normal', profileVersion: 2 }
const eye = 'Metadata/Items/MapFragments/AbyssPinnacleKeyGhastly'
const urn = 'Cinderswallow Urn'
const cane = 'Cane of Kulemak'

function historicalRun(): StoredRun {
	return {
		...profile, id: 'owned-row', league: 'historical-fixture', run: 79, version: 7,
		seed: '2667568808', date: null,
		items: { quantities: { [urn]: 1, [eye]: 3 }, counted: [urn, cane, eye] }
	}
}

function mergeRequest(): SubmitRun {
	return {
		...profile, league: 'historical-fixture', seed: '2667568808', mode: 'merge',
		target: { id: 'owned-row', expectedVersion: 7 },
		items: { quantities: { [cane]: 1 }, counted: [urn, cane] },
		incomingPositiveKeys: [cane, eye]
	}
}

describe('owner fixture independence', () => {
	it('isolates a profile read from caller mutation', async () => {
		const profiles = createFixtureProfiles(source.profiles)
		const first = await profiles.load()
		first[0].extra[2].notCountedDefault = false
		const next = await profiles.load()
		expect(next[0].extra[2].notCountedDefault).toBe(true)
	})

	it('does not rewrite the capture when not-counted settings change', async () => {
		const capture = createFixtureCapture(source.capture)
		const settings = createFixtureModuleSettings(source.moduleSettings)
		await settings.save({ ...await settings.load(), notCounted: [eye] })
		expect((await capture.load())?.extra[eye]).toBe(1)
	})

	it('does not rewrite historical run quantities when not-counted settings change', async () => {
		const settings = createFixtureModuleSettings(source.moduleSettings)
		const runs = createFixtureRuns(source.runs)
		await settings.save({ ...await settings.load(), notCounted: [eye] })
		const page = await runs.list(profile, 8, 10)
		expect(page.items.find((row) => row.run === 8)?.items.quantities).toEqual({ [cane]: 1, [eye]: 1, ['Metadata/Items/MapFragments/BetrayalAllflamePack']: 1 })
	})

	it('reads Own from only the stats owner', async () => {
		const own = { ...source.stats.catarina.own, runs: 12, items: { urn: 5 } }
		const stats = createFixtureStats({ catarina: { ...source.stats.catarina, own } })
		expect((await stats.load(profile, 'own')).items[urn]).toEqual({ drops: 5, eligibleRuns: 12 })
	})

	it('keeps independent module-settings instances', async () => {
		const first = createFixtureModuleSettings(source.moduleSettings)
		await first.save({ ...await first.load(), autoSaveSeconds: 0 })
		expect((await createFixtureModuleSettings(source.moduleSettings).load()).autoSaveSeconds).toBe(10)
	})

	it('keeps typed layout writes separate from module behavior settings', async () => {
		const overlay = createFixtureOverlaySettings(source.overlaySettings)
		await overlay.save({ ...await overlay.load(), layout: 'list' })
		expect((await overlay.load()).layout).toBe('list')
		expect((await createFixtureModuleSettings(source.moduleSettings).load()).autoSaveSeconds).toBe(10)
	})

	it('returns the device owner placeholder without another owner', async () => {
		expect(await createFixtureDevice(source.device).load()).toEqual({ id: '[device id]' })
	})
})

describe('original fixture meanings', () => {
	it('ships only Catarina when given the production profile slice', async () => {
		expect((await createFixtureProfiles(source.profiles).load()).map((p) => p.boss)).toEqual(['catarina'])
	})

	it('retains all three test-only profile shapes', async () => {
		const profiles = await createFixtureProfiles(source.testProfiles).load()
		expect(profiles.map((p) => [p.boss, p.groups.length])).toEqual([['maven', 1], ['fear', 2], ['blackstar', 0]])
	})

	it('retains Maven ambiguity without prefilling the shared base', async () => {
		const [maven] = await createFixtureProfiles(source.testProfiles).load()
		expect(maven.capture?.picks).toEqual({ unique: [] })
		expect(maven.capture?.ambiguous).toEqual({
			group: 'unique', base: 'Cloth Belt', ids: ["Graven's Secret", "Arn's Anguish", "Olesya's Delight"]
		})
	})

	it('retains Black Star quantities', async () => {
		const profiles = await createFixtureProfiles(source.testProfiles).load()
		expect(profiles[2].capture?.extra).toEqual({ 'The Eternal Struggle': 1, 'Grand Eldritch Ember': 3, 'Eldritch Chaos Orb': 1 })
	})

	it('maps the current Catarina additional item ids', async () => {
		const [catarina] = await createFixtureProfiles(source.profiles).load()
		expect(catarina.extra.map((item) => item.id)).toEqual([
			eye, 'Metadata/Items/Currency/CurrencyRerollRareVeiled',
			'Communion Support', 'Metadata/Items/DivinationCards/DivinationCardNooksCrown'
		])
	})

	it('ships Catarina without manual categories', async () => {
		const [catarina] = await createFixtureProfiles(source.profiles).load()
		expect(catarina.categories).toEqual([])
	})

	it('preserves wiki text without parsing it', async () => {
		const [catarina] = await createFixtureProfiles(source.profiles).load()
		expect(catarina.extra.map((item) => item.wikiRate)).toEqual(['~50%', '25%', '10%', '<0.5%'])
	})

	it('retains the seed as the original string', async () => {
		expect((await createFixtureCapture(source.capture).load())?.seed).toBe('2667568808')
	})

	it('maps capture picks onto canonical item keys', async () => {
		expect((await createFixtureCapture(source.capture).load())?.picks).toEqual({ unique: [urn], allflame: ['Metadata/Items/MapFragments/BetrayalAllflamePack'] })
	})

	it('retains unknown OCR lines', async () => {
		expect((await createFixtureCapture(source.capture).load())?.unknownLines).toEqual(['Spidersilk Robe'])
	})

	it.each(['own', 'collective'] as const)('retains 24 Urn drops in 78 %s runs', async (scope) => {
		const stats = await createFixtureStats(source.stats).load(profile, scope)
		expect({ runs: stats.runs, devices: stats.devices, urn: stats.items[urn] }).toEqual({
			runs: 78, devices: 1, urn: { drops: 24, eligibleRuns: 78 }
		})
	})

	it('retains all 78 original run rows', async () => {
		const page = await createFixtureRuns(source.runs).list(profile, 1, 78)
		expect(page.total).toBe(78)
		expect(page.items.map((row) => row.run)).toEqual(Array.from({ length: 78 }, (_, i) => 78 - i))
	})

	it('retains null dates', async () => {
		const page = await createFixtureRuns(source.runs).list(profile, 1, 78)
		expect(page.items.map((row) => row.date)).toEqual(Array(78).fill(null))
	})

	it('returns the last page newest first', async () => {
		const page = await createFixtureRuns(source.runs).list(profile, 8, 10)
		expect(page.items.map((row) => row.run)).toEqual([8, 7, 6, 5, 4, 3, 2, 1])
	})

	it('retains the historical Communion quantity', async () => {
		const page = await createFixtureRuns(source.runs).list(profile, 8, 10)
		expect(page.items[7].items.quantities).toEqual({ [cane]: 1, 'Communion Support': 1 })
	})

	it('defaults Communion to currently not counted', async () => {
		expect((await createFixtureModuleSettings(source.moduleSettings).load()).notCounted).toEqual(['Communion Support'])
	})

	it('defaults the timeout to ten seconds', async () => {
		expect((await createFixtureModuleSettings(source.moduleSettings).load()).autoSaveSeconds).toBe(10)
	})

	it.each(['constructor', '__proto__', 'unknown'])('rejects an unmapped fixture item %s', async (alias) => {
		const capture = { ...source.capture, extra: { [alias]: 1 } }
		await expect(createFixtureCapture(capture).load()).rejects.toThrow('Unknown Catarina fixture item')
	})

	it('rejects an unknown profile version', async () => {
		await expect(createFixtureStats(source.stats).load({ ...profile, profileVersion: 1 }, 'own')).rejects.toThrow('Unknown fixture profile')
	})

	it('rejects page zero', async () => {
		await expect(createFixtureRuns(source.runs).list(profile, 0, 10)).rejects.toThrow('Invalid pagination')
	})
})

describe('lossless historical edits', () => {
	it('preserves hidden quantities in the serialized full map', () => {
		const payload = editRunItems(historicalRun(), [urn, cane], { [urn]: 0, [cane]: 1 })
		expect(JSON.parse(JSON.stringify(payload))).toEqual({
			id: 'owned-row', expectedVersion: 7,
			items: { quantities: { [cane]: 1, [eye]: 3 }, counted: [urn, cane, eye] }
		})
	})

	it('does not mutate the historical row when building an edit', () => {
		const row = historicalRun()
		editRunItems(row, [urn, cane], { [urn]: 0, [cane]: 1 })
		expect(row.items.quantities).toEqual({ [urn]: 1, [eye]: 3 })
	})

	it('rejects a change to a hidden key', () => {
		expect(() => editRunItems(historicalRun(), [urn], { [eye]: 0 })).toThrow('Item is not editable')
	})

	it('rejects a historical eligibility change', () => {
		expect(() => editRunItems(historicalRun(), ['Communion Support'], { 'Communion Support': 1 })).toThrow('Item was not counted')
	})

	it.each([-1, 1.5, Infinity, NaN])('rejects invalid quantity %s', (quantity) => {
		expect(() => editRunItems(historicalRun(), [urn], { [urn]: quantity })).toThrow('Invalid quantity')
	})

	it('retains the stored counted set in the fixture edit response', async () => {
		const runs = createFixtureRuns(source.runs)
		const row = (await runs.list(profile, 8, 10)).items[0]
		const result = await runs.edit({ id: row.id, expectedVersion: 1, items: { quantities: { [cane]: 1, [eye]: 1 }, counted: [] } })
		expect(result.items.counted).toEqual(row.items.counted)
		expect(result.version).toBe(2)
	})

	it('rejects a stale row version', async () => {
		await expect(createFixtureRuns(source.runs).edit({
			id: 'fixture-catarina-8', expectedVersion: 0, items: { quantities: {}, counted: [] }
		})).rejects.toThrow('Version conflict')
	})

	it('rejects deleting an absent row', async () => {
		await expect(createFixtureRuns(source.runs).delete({ id: 'missing', expectedVersion: 1 })).rejects.toThrow('Run not found')
	})
})

describe('submit boundary', () => {
	it('preserves original incoming positives through filtered-map serialization', async () => {
		const request = mergeRequest()
		expect(JSON.parse(serializeSubmitRun(request))).toEqual({
			...profile, league: 'historical-fixture', seed: '2667568808', mode: 'merge',
			target: { id: 'owned-row', expectedVersion: 7 },
			items: { quantities: { [cane]: 1 }, counted: [urn, cane] }, incomingPositiveKeys: [cane, eye]
		})
	})

	it('exposes saving followed by the server-confirmed row with dropped-positive provenance', async () => {
		const saved = historicalRun()
		saved.version = 8
		const outcomes: SubmitOutcome[] = []
		await createFixtureSubmit(source.submit, 'saved', saved, [eye]).submit(mergeRequest(), (value) => outcomes.push(value))
		expect(outcomes.map((value) => value.state)).toEqual(['saving', 'saved'])
		expect(JSON.parse(JSON.stringify(outcomes[1]))).toMatchObject({
			state: 'saved', run: { id: 'owned-row', version: 8, seed: '2667568808', date: null }, droppedPositiveKeys: [eye]
		})
	})

	it('exposes unreachable without adding a local run', async () => {
		const runs = createFixtureRuns(source.runs)
		const outcomes: SubmitOutcome[] = []
		await createFixtureSubmit(source.submit, 'unreachable').submit(mergeRequest(), (value) => outcomes.push(value))
		expect(outcomes).toEqual([{ state: 'saving' }, { state: 'unreachable' }])
		expect((await runs.list(profile, 1, 100)).items.map((row) => row.run)).toEqual(
			Array.from({ length: 78 }, (_, i) => 78 - i)
		)
	})

	it('exposes conflict separately from unreachable', async () => {
		const outcomes: SubmitOutcome[] = []
		await createFixtureSubmit(source.submit, 'conflict').submit(mergeRequest(), (value) => outcomes.push(value))
		expect(outcomes).toEqual([{ state: 'saving' }, { state: 'conflict' }])
	})

	it('requires a server row for a saved fixture outcome', async () => {
		await expect(createFixtureSubmit(source.submit, 'saved').submit(mergeRequest(), () => {})).rejects.toThrow('requires a server row')
	})
})

describe('per-item eligible denominators', () => {
	it('preserves mixed denominators through the stats boundary roundtrip', async () => {
		const stats = createFixtureStats({ catarina: {
			own: { runs: 8, devices: 1, nextRun: 12, items: { urn: 4, eye: 1 }, eligibleRuns: { urn: 8, eye: 2 } },
			collective: source.stats.catarina.collective
		} })
		const result: BossStats = JSON.parse(JSON.stringify(await stats.load(profile, 'own')))
		expect({ runs: result.runs, nextRun: result.nextRun, urn: result.items[urn], eye: result.items[eye] }).toEqual({
			runs: 8, nextRun: 12, urn: { drops: 4, eligibleRuns: 8 }, eye: { drops: 1, eligibleRuns: 2 }
		})
	})

	it('retains a zero eligible denominator despite positive boss runs', async () => {
		const stats = createFixtureStats({ catarina: {
			own: { runs: 8, devices: 1, items: {}, eligibleRuns: { eye: 0 } }, collective: source.stats.catarina.collective
		} })
		expect((await stats.load(profile, 'own')).items[eye]).toEqual({ drops: 0, eligibleRuns: 0 })
	})

	it('retains eligible zero drops', async () => {
		expect((await createFixtureStats(source.stats).load(profile, 'collective')).items['Metadata/Items/DivinationCards/DivinationCardNooksCrown']).toEqual({ drops: 0, eligibleRuns: 78 })
	})

	it('keeps zero boss runs distinct from a zero eligible item', async () => {
		const stats = createFixtureStats({ catarina: {
			own: { runs: 0, devices: 0, items: {} }, collective: source.stats.catarina.collective
		} })
		expect((await stats.load(profile, 'own')).runs).toBe(0)
	})
})
