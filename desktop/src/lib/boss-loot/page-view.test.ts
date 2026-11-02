import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest'
import source from './__fixtures__/boss-loot.json'
import { createFixtureModuleSettings, createFixtureProfiles, createFixtureStats, createFixtureRuns } from './fixture-adapters'
import { buildResultsView, buildRunsView, buildRunEditor, buildSettingsView, keyboardShortcut, mouseShortcut, runsCsv, type ResultsInput } from './page-view'
import type { StoredRun } from './contracts'

async function input(): Promise<ResultsInput> {
	const [profile] = await createFixtureProfiles(source.profiles).load()
	return { profile, stats: await createFixtureStats(source.stats).load(profile, 'own'),
		scope: 'own', sort: 'rate', notCounted: (await createFixtureModuleSettings(source.moduleSettings).load()).notCounted }
}

describe('Results', () => {
	it('rounds the original unique pool rates', async () => {
		const view = buildResultsView(await input())
		expect(view.groups[0].items.map((item) => [item.name, item.drops, item.rate])).toEqual([
			['Cinderswallow Urn', 24, '31%'], ['Spinehail', 16, '21%'], ['Cane of Kulemak', 14, '18%'],
			['The Devouring Diadem', 11, '14%'], ["The Queen's Hunger", 7, '9%'], ['Bitterbind Point', 6, '8%']
		])
	})
	it('retains the uniform denominator header', async () => {
		expect(buildResultsView(await input()).rateHeader).toBe('= drops / 78 runs')
	})
	it('sorts names within their original groups ignoring leading The', async () => {
		const view = buildResultsView({ ...await input(), sort: 'name' })
		expect(view.groups.map((group) => group.items.map((item) => item.name))).toEqual([
			['Bitterbind Point', 'Cane of Kulemak', 'Cinderswallow Urn', 'The Devouring Diadem', "The Queen's Hunger", 'Spinehail'],
			['Allflame Ember of Kulemak', 'Communion Support', "Nook's Crown", 'Veiled Exalted Orb', "Zorath's Eye of the Endless"]
		])
	})
	it('does not partition currently not-counted rows when sorting rates', async () => {
		const state = await input()
		state.notCounted.push(state.profile.extra[0].id)
		const rows = buildResultsView(state).groups[1].items
		expect(rows[0]).toMatchObject({ name: "Zorath's Eye of the Endless", disabled: true, rate: '—', drops: null })
	})
	it('breaks tied rates by item name', async () => {
		const state = await input()
		state.stats.items['Metadata/Items/DivinationCards/DivinationCardNooksCrown'].drops = 1
		expect(buildResultsView(state).groups[1].items.slice(-2).map((item) => item.name))
			.toEqual(['Communion Support', "Nook's Crown"])
	})
	for (const scope of ['own', 'collective'] as const) {
		it(`shows mixed eligible provenance for ${scope}`, async () => {
			const state = { ...await input(), scope }
			state.stats.items['Cinderswallow Urn'] = { drops: 2, eligibleRuns: 5 }
			const view = buildResultsView(state)
			expect(view.rateHeader).toBe('= drops / eligible runs')
			expect(view.groups[0].items[0]).toMatchObject({ rate: '40%', provenance: '2/5 eligible runs' })
		})
		it(`shows unknown rate for zero eligible ${scope} runs`, async () => {
			const state = { ...await input(), scope }
			state.stats.items['Cinderswallow Urn'] = { drops: 0, eligibleRuns: 0 }
			expect(buildResultsView(state).groups[0].items.find((item) => item.id === 'Cinderswallow Urn'))
				.toMatchObject({ rate: '—', provenance: '0/0 eligible runs', bar: 0 })
		})
		it(`preserves measured zero drops for ${scope}`, async () => {
			const state = { ...await input(), scope }
			state.stats.items['Cinderswallow Urn'] = { drops: 0, eligibleRuns: 78 }
			expect(buildResultsView(state).groups[0].items.find((item) => item.id === 'Cinderswallow Urn'))
				.toMatchObject({ rate: scope === 'own' ? '0%' : '—', provenance: '0/78' })
		})
	}
	it('uses wiki estimates only when the boss has no runs', async () => {
		const state = await input()
		state.stats = { runs: 0, devices: 0, nextRun: 1, items: {} }
		const view = buildResultsView(state)
		expect(view.message).toBe('No runs logged for this boss yet: rates show the wiki estimate.')
		expect(view.groups[0].items.find((item) => item.id === 'Cinderswallow Urn')).toMatchObject({ rate: 'wiki 30%', drops: 0 })
	})
	it('retains disabled Communion despite historical observations', async () => {
		expect(buildResultsView(await input()).groups[1].items.find((item) => item.id === 'Communion Support'))
			.toMatchObject({ disabled: true, drops: null, rate: '—', provenance: '' })
	})
	it('reports collective device provenance', async () => {
		expect(buildResultsView({ ...await input(), scope: 'collective' }).summary).toBe('78 runs · 1 device')
	})
	it('restores measured rates when not-counted is cleared', async () => {
		expect(buildResultsView({ ...await input(), notCounted: [] }).groups[1].items.find((item) => item.id === 'Communion Support'))
			.toMatchObject({ disabled: false, drops: 1, rate: '1%' })
	})
	it('does not invent an exactly-one group measurement for heterogeneous eligibility', async () => {
		const state = await input()
		state.stats.items['Cinderswallow Urn'] = { drops: 2, eligibleRuns: 5 }
		expect(buildResultsView(state).groups[0].note).toBe('exactly one per kill')
	})
})

describe('CSV', () => {
	it.each(['2026-10-31T23:45:00Z', '2026-10-31', null])('exports the original date %s', async (date) => {
		const { profile, runs } = await runInput()
		runs.items[0].date = date
		expect(runsCsv(profile, [runs.items[0]]).split('\r\n')[1].split(',')[1]).toBe(date ?? '')
	})
	it('uses profile-ordered item names as CSV columns', async () => {
		const { profile } = await input()
		const run: StoredRun = { ...profile, id: 'row', league: 'test', run: 8, date: null, seed: null,
			version: 1, items: { quantities: { 'Cane of Kulemak': 1, 'Communion Support': 3 }, counted: [] } }
		const csv = runsCsv(profile, [run])
		expect(csv.split('\r\n')[0]).toBe('Run,Date,Cinderswallow Urn,Spinehail,Cane of Kulemak,The Devouring Diadem,Bitterbind Point,The Queen\'s Hunger,Zorath\'s Eye of the Endless,Veiled Exalted Orb,Allflame Ember of Kulemak,Communion Support,Nook\'s Crown')
	})
	it('exports hidden historical quantities independently of current eligibility', async () => {
		const { profile } = await input()
		const run: StoredRun = { ...profile, id: 'row', league: 'test', run: 8, date: null, seed: null,
			version: 1, items: { quantities: { 'Cane of Kulemak': 1, 'Communion Support': 3 }, counted: [] } }
		expect(runsCsv(profile, [run]).split('\r\n')[1]).toBe('8,,0,0,1,0,0,0,0,0,0,3,0')
	})
})

async function runInput(page = 1) {
	const { profile, notCounted } = await input()
	return { profile, notCounted, runs: await createFixtureRuns(source.runs).list(profile, page, 10) }
}

describe('Runs projection', () => {
	beforeAll(() => vi.stubEnv('TZ', 'Europe/Warsaw'))
	afterAll(() => vi.unstubAllEnvs())
	it('shows the newest ten fixture runs on page one', async () => {
		const { profile, runs } = await runInput()
		const view = buildRunsView(profile, runs)
		expect(view.rows.map((row) => row.run)).toEqual([78, 77, 76, 75, 74, 73, 72, 71, 70, 69])
		expect(view.range).toBe('Runs 78–69 of 78, newest first')
		expect(view.pages).toEqual([1, 2, 3, 4, 5, 6, 7, 8])
		expect(view.first).toBe(true)
		expect(view.last).toBe(false)
	})
	it('shows the final eight fixture runs on page eight', async () => {
		const { profile, runs } = await runInput(8)
		const view = buildRunsView(profile, runs)
		expect(view.rows.map((row) => row.run)).toEqual([8, 7, 6, 5, 4, 3, 2, 1])
		expect(view.range).toBe('Runs 8–1 of 78, newest first')
		expect(view.first).toBe(false)
		expect(view.last).toBe(true)
	})
	it('shows only current additional chips for fixture run six', async () => {
		const { profile, runs } = await runInput(8)
		expect(buildRunsView(profile, runs).rows.find((row) => row.run === 6)?.extra.map((item) => item.name))
			.toEqual(["Zorath's Eye of the Endless", 'Veiled Exalted Orb', 'Allflame Ember of Kulemak'])
	})
	it('renders a null historical date as no date', async () => {
		const { profile, runs } = await runInput()
		expect(buildRunsView(profile, runs).rows[0].date).toBe('no date')
	})
	it.each([
		['2026-10-31T23:45:00Z', '2026-11-01 00:45'],
		['2026-06-01T12:05:00Z', '2026-06-01 14:05'],
		['2026-10-31T23:45:00-07:00', '2026-11-01 07:45']
	])('renders timestamp %s in viewer local time', async (date, expected) => {
		const { profile, runs } = await runInput()
		runs.items[0].date = date
		expect(buildRunsView(profile, runs).rows[0].date).toBe(expected)
	})
	it('retains a date-only value without inventing a time', async () => {
		const { profile, runs } = await runInput()
		runs.items[0].date = '2026-10-31'
		expect(buildRunsView(profile, runs).rows[0].date).toBe('2026-10-31')
	})
	it('retains the server order when display run numbers have gaps', async () => {
		const { profile, runs } = await runInput()
		runs.items = [runs.items[3], runs.items[0]]
		expect(buildRunsView(profile, runs).rows.map((row) => row.run)).toEqual([75, 78])
	})
	it('does not hide historical Communion from the read-only run row', async () => {
		const { profile, runs } = await runInput(8)
		expect(buildRunsView(profile, runs).rows.at(-1)?.extra.map((item) => item.name)).toContain('Communion Support')
	})
	it('shows an empty history without invented run numbers', async () => {
		const { profile, runs } = await runInput()
		const view = buildRunsView(profile, { ...runs, total: 0, items: [] })
		expect(view.rows).toEqual([])
		expect(view.range).toBe('No runs logged for this boss yet.')
		expect(view.pages).toEqual([])
	})
})

describe('Runs editor projection', () => {
	it.each(['Metadata/Items/Currency/AstrolabeGeneric', 'category:catarina:1:rare-two-veiled-modifiers',
		'Metadata/Items/DivinationCards/DivinationCardKeepersCorruption'])(
		'preserves removed key %s outside the editable rows', async (key) => {
			const { profile, runs } = await runInput(8)
			const run = runs.items[0]
			run.items.quantities[key] = 2
			run.items.counted.push(key)
			const view = buildRunEditor(profile, run, [], { 'Cane of Kulemak': 0, 'Cinderswallow Urn': 1 })
			expect(view.extra.map((item) => item.id)).not.toContain(key)
			expect(view.request.items.quantities[key]).toBe(2)
			expect(view.request.items.counted).toContain(key)
		})
	it('hides a currently excluded Eye without changing its historical quantity', async () => {
		const { profile, runs } = await runInput(8)
		const view = buildRunEditor(profile, runs.items[0], [profile.extra[0].id], { 'Cane of Kulemak': 0, 'Cinderswallow Urn': 1 })
		expect(view.extra.map((item) => item.id)).not.toContain(profile.extra[0].id)
		expect(view.request.items.quantities).toEqual({ 'Cinderswallow Urn': 1, [profile.extra[0].id]: 1 })
		expect(view.request.items.counted).toEqual(runs.items[0].items.counted)
	})
	it('omits items outside the original counted set from editable chips', async () => {
		const { profile, runs } = await runInput(8)
		runs.items[0].items.counted = ['Cane of Kulemak']
		const view = buildRunEditor(profile, runs.items[0], [], {})
		expect(view.groups[0].items.map((item) => item.id)).toEqual(['Cane of Kulemak'])
		expect(view.extra).toEqual([])
	})
	it('rejects an edit to a currently hidden key', async () => {
		const { profile, runs } = await runInput(8)
		expect(() => buildRunEditor(profile, runs.items[0], [profile.extra[0].id], { [profile.extra[0].id]: 0 }))
			.toThrow('Item is not editable')
	})
	it('withholds saving when a full guaranteed group would be empty', async () => {
		const { profile, runs } = await runInput(8)
		expect(buildRunEditor(profile, runs.items[0], [], { 'Cane of Kulemak': 0 }).valid).toBe(false)
	})
})

describe('Settings projection', () => {
	it('uses the settings owners timeout options including Off', async () => {
		const { profile } = await input()
		const settings = await createFixtureModuleSettings(source.moduleSettings).load()
		expect(buildSettingsView(profile, settings).timeouts).toEqual([
			{ value: '5', label: '5 s' }, { value: '10', label: '10 s' }, { value: '20', label: '20 s' },
			{ value: '30', label: '30 s' }, { value: '0', label: 'Off' }
		])
	})
	it('marks Communion as excluded in the named additional checklist', async () => {
		const { profile } = await input()
		const settings = await createFixtureModuleSettings(source.moduleSettings).load()
		expect(buildSettingsView(profile, settings).notCounted.map((item) => [item.name, item.checked])).toEqual([
			["Zorath's Eye of the Endless", false], ['Veiled Exalted Orb', false], ['Allflame Ember of Kulemak', false],
			['Communion Support', true], ["Nook's Crown", false]
		])
	})
})

const modifiers = { ctrlKey: false, altKey: false, shiftKey: false, metaKey: false }
describe('Shortcut capture projection', () => {
	it('formats the default preview keyboard shortcut', () => {
		expect(keyboardShortcut({ ...modifiers, key: 'F8', shiftKey: true })).toBe('Shift + F8')
	})
	it('formats a keyboard binding with multiple modifiers', () => {
		expect(keyboardShortcut({ ...modifiers, key: 'a', ctrlKey: true, altKey: true })).toBe('Ctrl + Alt + A')
	})
	it('does not bind a modifier pressed on its own', () => {
		expect(keyboardShortcut({ ...modifiers, key: 'Shift', shiftKey: true })).toBeNull()
	})
	it('does not bind Escape', () => {
		expect(keyboardShortcut({ ...modifiers, key: 'Escape' })).toBeNull()
	})
	it('formats the default right mouse shortcut', () => {
		expect(mouseShortcut({ ...modifiers, button: 2, shiftKey: true })).toBe('Shift + Right click')
	})
	it('formats an auxiliary mouse button shortcut', () => {
		expect(mouseShortcut({ ...modifiers, button: 3, ctrlKey: true })).toBe('Ctrl + Mouse 4')
	})
	it('does not invent an unsupported mouse button name', () => {
		expect(mouseShortcut({ ...modifiers, button: -1 })).toBeNull()
	})
})
