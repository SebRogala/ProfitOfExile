import { describe, expect, it } from 'vitest'
import source from './__fixtures__/boss-loot.json'
import { createFixtureModuleSettings, createFixtureProfiles, createFixtureStats } from './fixture-adapters'
import { buildResultsView, runsCsv, type ResultsInput } from './page-view'
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
			['Allflame Ember of Kulemak', 'Communion Support', "Keeper's Corruption", "Nook's Crown", 'Rare, two veiled modifiers', 'Templar Astrolabe', 'Veiled Exalted Orb', "Zorath's Eye of the Endless"]
		])
	})
	it('does not partition currently not-counted rows when sorting rates', async () => {
		const state = await input()
		state.notCounted.push(state.profile.extra[0].id)
		const rows = buildResultsView(state).groups[1].items
		expect(rows[0]).toMatchObject({ name: "Zorath's Eye of the Endless", disabled: true, rate: '—', drops: null })
	})
	it('breaks tied rates by item name', async () => {
		expect(buildResultsView(await input()).groups[1].items.slice(-3).map((item) => item.name))
			.toEqual(["Keeper's Corruption", "Nook's Crown", 'Rare, two veiled modifiers'])
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
	it('uses profile-ordered item names and historical quantities', async () => {
		const { profile } = await input()
		const run: StoredRun = { ...profile, id: 'row', league: 'test', run: 8, date: null, seed: null,
			version: 1, items: { quantities: { 'Cane of Kulemak': 1, 'Communion Support': 3 }, counted: [] } }
		const csv = runsCsv(profile, [run])
		expect(csv.split('\r\n')[0]).toBe('Run,Date,Cinderswallow Urn,Spinehail,Cane of Kulemak,The Devouring Diadem,Bitterbind Point,The Queen\'s Hunger,Zorath\'s Eye of the Endless,Veiled Exalted Orb,Allflame Ember of Kulemak,Communion Support,Templar Astrolabe,Nook\'s Crown,Keeper\'s Corruption,"Rare, two veiled modifiers"')
		expect(csv.split('\r\n')[1]).toBe('8,,0,0,1,0,0,0,0,0,0,3,0,0,0,0')
	})
})
