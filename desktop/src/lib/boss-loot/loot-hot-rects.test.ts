import { afterEach, describe, expect, it, vi } from 'vitest'
import { render } from 'svelte/server'
import LootConfirmation from './LootConfirmation.svelte'
import QuantityStepper from './QuantityStepper.svelte'
import source from './__fixtures__/boss-loot.json'
import { createFixtureCapture, createFixtureModuleSettings, createFixtureProfiles, createFixtureStats } from './fixture-adapters'
import { useHotRects } from '$lib/overlay/widgets/use-hot-rects'
import { invoke } from '@tauri-apps/api/core'
import { ssot } from '$lib/stores/ssot.svelte'
import type { OverlayInput } from './overlay-view'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => {}) }))
afterEach(() => { vi.unstubAllGlobals(); vi.clearAllMocks() })
async function input(): Promise<OverlayInput> {
	const [profile] = await createFixtureProfiles(source.profiles).load()
	return { profile, capture: (await createFixtureCapture(source.capture).load())!, stats: await createFixtureStats(source.stats).load(profile, 'own'),
		notCounted: (await createFixtureModuleSettings(source.moduleSettings).load()).notCounted, layout: 'grid', scope: 'own', autoSaveSeconds: 10, seconds: 7 }
}
async function panel() {
	return render(LootConfirmation, { props: { input: await input(), iconFor: () => null, ontouch: () => {}, onaction: () => {} } }).body
}
describe('loot rendered controls and shared hot-rect owner', () => {
	it('renders exactly one actual hot panel', async () => {
		expect((await panel()).match(/\bdata-hot(?:=|\s|>)/g)).toHaveLength(1)
	})
	it('puts the Save action on its actual enabled button', async () => {
		const button = (await panel()).match(/<button\b[^>]*data-action="loot\.save"[^>]*>[\s\S]*?<\/button>/)?.[0] ?? ''
		expect(button).toContain('Save now')
		expect(button).not.toMatch(/\bdisabled/)
	})
	it('keeps the Save action on its actual disabled button', async () => {
		const state = await input(); state.capture.picks.unique = []
		const html = render(LootConfirmation, { props: { input: state, iconFor: () => null, ontouch: () => {}, onaction: () => {} } }).body
		const button = html.match(/<button\b(?=[^>]*\bdisabled)(?=[^>]*data-action="loot\.save")[^>]*>[\s\S]*?<\/button>/)?.[0] ?? ''
		expect(button).toContain('Save now')
	})
	it.each([['space', '1 234'], ['comma', '1,234'], ['none', '1234']] as const)('groups the displayed stepper quantity in %s mode', (format, expected) => {
		const previous = ssot.bossLoot
		ssot.bossLoot = { ...previous!, numberFormat: format }
		try {
			const html = render(QuantityStepper, { props: { id: 'category:test', name: 'Category', quantity: 1234 } }).body
			expect(html.match(/<span[^>]*aria-live="polite"[^>]*>([^<]*)<\/span>/)?.[1]).toBe(expected)
		} finally { ssot.bossLoot = previous }
	})
	it('withdraws the shared native rect declaration when the rendered panel disappears', async () => {
		let html = await panel()
		let frame: FrameRequestCallback | undefined
		let changed: MutationCallback | undefined
		vi.stubGlobal('requestAnimationFrame', (next: FrameRequestCallback) => { frame = next; return 1 })
		vi.stubGlobal('cancelAnimationFrame', () => {})
		vi.stubGlobal('ResizeObserver', class { observe() {} disconnect() {} })
		vi.stubGlobal('MutationObserver', class { constructor(callback: MutationCallback) { changed = callback } observe() {} disconnect() {} })
		const element = { getBoundingClientRect: () => ({ left: 20, top: 30, right: 420, bottom: 630 }) }
		const host = { querySelectorAll: () => html.includes('data-hot') ? [element] : [] } as unknown as HTMLElement
		const action = useHotRects(host, { module: 'loot', scaleFactor: 1 })
		frame!(0)
		expect(invoke).toHaveBeenCalledWith('set_overlay_hot_rects', { label: 'loot', rects: [{ x: 20, y: 30, w: 400, h: 600 }] })
		expect(invoke).toHaveBeenCalledWith('set_overlay_has_content', { label: 'loot', hasContent: true })
		vi.mocked(invoke).mockClear()
		html = ''
		changed!([], {} as MutationObserver); frame!(0)
		expect(invoke).toHaveBeenCalledWith('set_overlay_hot_rects', { label: 'loot', rects: [] })
		expect(invoke).toHaveBeenCalledWith('set_overlay_has_content', { label: 'loot', hasContent: false })
		action.destroy()
	})
})
