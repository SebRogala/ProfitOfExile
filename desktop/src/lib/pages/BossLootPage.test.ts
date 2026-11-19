/**
 * BossLootPage server-rendered from a captured real controller, as
 * `HarvestFlippingPage.test.ts` does: the factory mock hands the page a
 * controller this test built from the fixture ports and loaded first.
 * `render()` runs no `$effect`, so the page's own loads stay off; the
 * persisted `bossLootTab` pick is pinned to `runs` because SSR never sees the
 * async prefs load land.
 */
import { describe, expect, it, vi } from 'vitest'
import { render } from 'svelte/server'
import { Window } from 'happy-dom'
import source from '$lib/boss-loot/__fixtures__/boss-loot.json'
import { createFixtureProfiles, createFixtureModuleSettings, createFixtureStats, createFixtureRuns, createFixtureDevice } from '$lib/boss-loot/fixture-adapters'
import type { PagePorts } from '$lib/boss-loot/page-controller.svelte'

const pageHarness = vi.hoisted(() => ({
	controller: null as ReturnType<typeof import('$lib/boss-loot/page-controller.svelte').createBossLootPageController> | null
}))

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => ({})) }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }))

vi.mock('$lib/prefs.svelte', async () => {
	const actual = await vi.importActual<typeof import('$lib/prefs.svelte')>('$lib/prefs.svelte')
	return {
		...actual,
		persisted: (key: string, initial: string) => ({ value: key === 'bossLootTab' ? 'runs' : initial })
	}
})

vi.mock('$lib/boss-loot/page-controller.svelte', async () => {
	const actual = await vi.importActual<typeof import('$lib/boss-loot/page-controller.svelte')>('$lib/boss-loot/page-controller.svelte')
	return {
		...actual,
		createBossLootPageController: () => {
			if (!pageHarness.controller) throw new Error('the test builds the controller before render')
			return pageHarness.controller
		}
	}
})

const { createBossLootPageController } = await vi.importActual<typeof import('$lib/boss-loot/page-controller.svelte')>('$lib/boss-loot/page-controller.svelte')

import BossLootPage from './BossLootPage.svelte'

function ports(): PagePorts {
	return { profiles: createFixtureProfiles(source.profiles), stats: createFixtureStats(source.stats),
		moduleSettings: createFixtureModuleSettings(source.moduleSettings), runs: createFixtureRuns(source.runs),
		device: createFixtureDevice(source.device), saveCsv: async () => {} }
}

function parse(html: string): Document {
	const document = new Window().document as unknown as Document
	document.body.innerHTML = html
	return document
}

function words(element: Element): string {
	return (element.textContent ?? '').replace(/\s+/g, ' ').trim()
}

describe('BossLootPage', () => {
	it('runs table per loot group', async () => {
		const page = createBossLootPageController(ports())
		await page.load()
		await page.loadRuns(8)
		const run = page.runsView!.rows.find((row) => row.run === 6)!
		page.beginEdit(run.id)
		pageHarness.controller = page

		const body = render(BossLootPage, { props: { ports: ports() } }).body

		const document = parse(body)

		const headers = [...document.querySelectorAll('[role="columnheader"]')].map(words)
		expect(headers).toEqual(['Run', 'Date', 'Unique', 'Allflame Ember', 'Additional', ''])

		const row = document.querySelector('.runrow[data-run="6"]')!
		const cells = [...row.querySelectorAll(':scope > [role="cell"]')]
			.map((cell) => [...cell.querySelectorAll('.item, .history-chip')].map(words))
		expect(cells.slice(2, 5)).toEqual([
			['Cinderswallow Urn'],
			['Allflame Ember of Kulemak'],
			["Zorath's Eye of the Endless", 'Veiled Exalted Orb']
		])

		const editor = document.querySelector('[aria-label="Edit run 6"]')!
		expect(editor.getAttribute('aria-colspan')).toBe('6')
	})
})
