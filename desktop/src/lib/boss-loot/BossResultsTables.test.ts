import { describe, expect, it } from 'vitest'
import { render } from 'svelte/server'
import BossResultsTables from './BossResultsTables.svelte'
import { buildResultsView, type ResultsInput } from './page-view'
import type { BossProfile, LootItem } from './contracts'
import source from './__fixtures__/boss-loot.json'
import { createFixtureProfiles, createFixtureStats, createFixtureModuleSettings } from './fixture-adapters'

const item = (id: string, name: string): LootItem => ({ id, name, base: 'Test base', wikiRate: '25%' })
const profile: BossProfile = {
	boss: 'synthetic', profileKey: 'synthetic', profileVersion: 1, key: 'synthetic',
	name: 'Synthetic boss', area: 'Test area', level: 80,
	groups: [
		{ id: 'first', label: 'Relics', min: 1, max: 1, items: [item('z', 'Zebra'), item('a', 'The Amber')] },
		{ id: 'second', label: 'Tokens', min: 0, max: 2, items: [item('t', 'Token')] }
	],
	extra: [item('e', 'Extra')], categories: [item('c', 'Category')]
}
const input: ResultsInput = {
	profile, scope: 'own', sort: 'rate', notCounted: ['e'],
	stats: { runs: 10, devices: 2, nextRun: 11, items: {
		z: { drops: 2, eligibleRuns: 5 }, a: { drops: 6, eligibleRuns: 10 },
		t: { drops: 0, eligibleRuns: 0 }, e: { drops: 8, eligibleRuns: 10 }, c: { drops: 0, eligibleRuns: 10 }
	} }
}

function layouts(state = input) {
	const html = render(BossResultsTables, { props: {
		profile: state.profile, view: buildResultsView(state), sort: state.sort, onSort: () => {}
	} }).body.replace(/<!--[\s\S]*?-->/g, '')
	// SSR emits both alternatives. Chromium evidence verifies which CSS exposes at 1099/1100px.
	const stacked = html.match(/<div class="results-stacked[^\"]*">([\s\S]*?)<\/div>\s*<div class="results-columns/)
	const columns = html.slice(html.indexOf('<div class="results-columns'))
	expect(stacked, 'stacked table must precede the column alternative').not.toBeNull()
	return { stacked: stacked![1], columns }
}

function tableNames(html: string) {
	return [...html.matchAll(/<table\b[^>]*>([\s\S]*?)<\/table>/g)].map((table) =>
		[...table[1].matchAll(/<span class="item[^\"]*">\s*<span\b[^>]*>([^<]+)/g)].map((match) => match[1]))
}

describe('BossResultsTables render alternatives', () => {
	it('preserves the original fixture in a single narrow table', async () => {
		const [profile] = await createFixtureProfiles(source.profiles).load()
		const html = layouts({ profile, stats: await createFixtureStats(source.stats).load(profile, 'own'),
			scope: 'own', sort: 'rate', notCounted: (await createFixtureModuleSettings(source.moduleSettings).load()).notCounted })
		expect(tableNames(html.stacked)).toEqual([[
			'Cinderswallow Urn', 'Spinehail', 'Cane of Kulemak', 'The Devouring Diadem', "The Queen's Hunger", 'Bitterbind Point',
			'Allflame Ember of Kulemak', 'Allflame Ember of Propagation', 'Allflame Ember of the Ethereal',
			"Zorath's Eye of the Endless", 'Veiled Exalted Orb', 'Communion Support', "Nook's Crown"
		]])
	})
	it('separates the original fixture pools from additional drops in the wide layout', async () => {
		const [profile] = await createFixtureProfiles(source.profiles).load()
		const html = layouts({ profile, stats: await createFixtureStats(source.stats).load(profile, 'own'),
			scope: 'own', sort: 'rate', notCounted: (await createFixtureModuleSettings(source.moduleSettings).load()).notCounted })
		expect(tableNames(html.columns)).toEqual([
			['Cinderswallow Urn', 'Spinehail', 'Cane of Kulemak', 'The Devouring Diadem', "The Queen's Hunger", 'Bitterbind Point',
				'Allflame Ember of Kulemak', 'Allflame Ember of Propagation', 'Allflame Ember of the Ethereal'],
			["Zorath's Eye of the Endless", 'Veiled Exalted Orb', 'Communion Support', "Nook's Crown"]
		])
	})
	it('provides one stacked table with every group in profile order for narrow panels', () => {
		expect(tableNames(layouts().stacked)).toEqual([['The Amber', 'Zebra', 'Token', 'Extra', 'Category']])
	})
	it('provides two wide tables separating all pools from extras and categories', () => {
		expect(tableNames(layouts().columns)).toEqual([['The Amber', 'Zebra', 'Token'], ['Extra', 'Category']])
	})
	it('renders all four headers on each wide table', () => {
		const tables = [...layouts().columns.matchAll(/<thead\b[^>]*>([\s\S]*?)<\/thead>/g)]
		expect(tables).toHaveLength(2)
		for (const [_, header] of tables) {
			expect(header.replace(/<[^>]*>/g, '')).toMatch(/Item[\s\S]*Drops[\s\S]*Rate[\s\S]*Wiki/)
		}
	})
	it('keeps name sorting within each profile group in both alternatives', () => {
		const html = layouts({ ...input, sort: 'name' })
		for (const layout of Object.values(html)) {
			expect(tableNames(layout).flat()).toEqual(['The Amber', 'Zebra', 'Token', 'Category', 'Extra'])
			expect(layout).toContain('aria-sort="ascending"')
		}
	})
	it('preserves mixed eligible provenance in both alternatives', () => {
		for (const layout of Object.values(layouts())) {
			expect(layout).toContain('= drops / eligible runs')
			expect(layout).toContain('2/5 eligible runs')
			expect(layout).toContain('0/0 eligible runs')
		}
	})
	it('preserves the not-counted treatment without rate provenance', () => {
		for (const layout of Object.values(layouts())) {
			const row = layout.match(/<tr>[^]*?Extra[^]*?<\/tr>/)?.[0].split('<tr>').at(-1)
			expect(row).toContain('not counted')
			expect(row).not.toContain('8/10')
		}
	})
	it('renders collective zero drops as a dash with positive eligibility', () => {
		for (const layout of Object.values(layouts({ ...input, scope: 'collective' }))) {
			const row = layout.match(/<tr>[^]*?Category[^]*?<\/tr>/)?.[0].split('<tr>').at(-1)
			expect(row).toMatch(/<b[^>]*>—<\/b>/)
			expect(row).toContain('0/10 eligible runs')
		}
	})
	it('renders the synthetic multi-pool rules without assuming a single unique group', () => {
		for (const layout of Object.values(layouts())) {
			expect(layout).toContain('Relics pool · one per kill')
			expect(layout).toContain('Tokens pool · 0–2 per kill')
			expect(layout).toContain('any number per kill')
		}
	})
	it('keeps extras in the second wide table for a profile with no guaranteed pools', () => {
		const state = { ...input, profile: { ...profile, groups: [] } }
		expect(tableNames(layouts(state).columns)).toEqual([[], ['Extra', 'Category']])
	})
})
