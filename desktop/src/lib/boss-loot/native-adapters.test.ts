import { describe, expect, it, vi } from 'vitest'
import { createNativeProfiles, createNativeStats, createNativeRuns, createNativeModuleSettings, createNativeDevice, createNativeCapture, mapNativeOverlay, saveNativeCsv, type NativeBoundary } from './native-adapters'
import type { BossLootProfile, BossLootSlice } from '$lib/stores/ssot.svelte'

export const nativeProfile: BossLootProfile = {
	boss: 'catarina', name: 'Catarina', fullName: 'Catarina, Master of Undeath', key: 'Normal', profileVersion: 1,
	area: 'BetrayalMastermindFight', level: 83,
	groups: [{ id: 'unique', label: 'Unique pool', min: 1, max: 1, items: [
		{ id: 'Cinderswallow Urn', name: 'Cinderswallow Urn', base: 'Silver Flask', wikiRate: '30%' },
		{ id: 'Cane of Kulemak', name: 'Cane of Kulemak', base: 'Serpentine Staff', wikiRate: '18%' }
	] }],
	extra: [{ id: 'Metadata/Items/MapFragments/AbyssPinnacleKeyGhastly', name: "Zorath's Eye of the Endless", base: 'Map fragment', wikiRate: '~50%' }], categories: []
}
const ref = { boss: 'catarina', profileKey: 'Normal', profileVersion: 1 }
const row = { ...ref, id: '31', league: 'Historical', version: 7, run: 31, date: '2026-04-12T12:00:00Z', seed: '42',
	items: { quantities: { 'Cinderswallow Urn': 1, 'hidden-eye': 3 }, counted: ['Cinderswallow Urn', 'hidden-eye'] } }
function boundary(response: unknown = null) {
	const call = vi.fn(async (_command: string, _args?: Record<string, unknown>): Promise<unknown> => response)
	const slice: BossLootSlice = { enabled: true, numberFormat: 'comma', nowMs: 2000, pending: null, frame: null, rebind: null,
		settings: { layout: 'grid', autoSaveSeconds: 10, notCounted: [], captureShortcut: { key: 2, shift: true, ctrl: false, alt: false }, previewShortcut: { key: 119, shift: true, ctrl: false, alt: false } } }
	const deps: NativeBoundary = { call, snapshot: () => slice, reconcile: async () => {}, deviceId: () => 'test-device' }
	return { deps, slice, call }
}
describe('native boss loot owners', () => {
	it('maps native profile key without fixture aliases', async () => {
		const { deps } = boundary([nativeProfile])
		const [profile] = await createNativeProfiles(deps).load()
		expect(profile.profileKey).toBe('Normal')
		expect(profile.groups[0].items[0].id).toBe('Cinderswallow Urn')
		expect(profile.capture).toBeUndefined()
	})
	it.each([null, {}, [{ ...nativeProfile, profileVersion: '1' }], [{ ...nativeProfile, groups: null }]])('rejects malformed profiles without fixture fallback (%j)', async (bad) => {
		await expect(createNativeProfiles(boundary(bad).deps).load()).rejects.toThrow('Invalid')
	})
	it.each(['own', 'collective'] as const)('preserves eligible denominators independently in %s', async (scope) => {
		const stats = { runs: 100, devices: 2, nextRun: 32, items: { eye: { drops: 3, eligibleRuns: 7 }, never: { drops: 0, eligibleRuns: 0 } } }
		const { deps, call } = boundary(stats)
		expect(await createNativeStats(deps).load(ref, scope)).toEqual(stats)
		expect(call).toHaveBeenCalledWith('boss_loot_stats', { profile: ref, scope })
	})
	it('rejects missing eligible denominators', async () => {
		await expect(createNativeStats(boundary({ runs: 5, devices: 1, nextRun: 6, items: { eye: { drops: 2 } } }).deps).load(ref, 'own')).rejects.toThrow('Invalid')
	})
	it('surfaces native failure without fabricated rows', async () => {
		const { deps } = boundary(); deps.call = async () => { throw new Error('unreachable') }
		await expect(createNativeRuns(deps).list(ref, 1, 10)).rejects.toThrow('unreachable')
	})
	it('preserves historical identity and complete maps through list mapping', async () => {
		const response = { total: 1, page: 1, pageSize: 10, items: [row] }
		expect(await createNativeRuns(boundary(response).deps).list(ref, 1, 10)).toEqual(response)
	})
	it('sends full edit map and expected row version', async () => {
		const { deps, call } = boundary({ ...row, version: 8 })
		const request = { id: row.id, expectedVersion: 7, items: row.items }
		expect((await createNativeRuns(deps).edit(request)).version).toBe(8)
		expect(call).toHaveBeenCalledWith('boss_loot_edit', request)
	})
	it('maps settings from the native snapshot', async () => {
		const { deps } = boundary()
		expect(await createNativeModuleSettings(deps).load()).toMatchObject({ autoSaveSeconds: 10, captureShortcut: 'Shift + Right click', previewShortcut: 'Shift + F8' })
	})
	it('preserves appearance when saving behavior through its owner', async () => {
		const { deps, slice, call } = boundary(); slice.settings.layout = 'list'
		const owner = createNativeModuleSettings(deps)
		await owner.save({ ...await owner.load(), autoSaveSeconds: 20 })
		expect(call).toHaveBeenCalledWith('boss_loot_set_settings', { settings: { ...slice.settings, autoSaveSeconds: 20 } })
	})
	it('loads device identity from the existing status owner', async () => {
		expect(await createNativeDevice(boundary().deps).load()).toEqual({ id: 'test-device' })
	})
	it('cancels export without errors or a destination argument', async () => {
		const { deps, call } = boundary({ state: 'cancelled' })
		await expect(saveNativeCsv(ref, deps)).resolves.toBeUndefined()
		expect(call).toHaveBeenCalledWith('boss_loot_export', { profile: ref })
	})
	it('rejects malformed export responses', async () => {
		await expect(saveNativeCsv(ref, boundary({}).deps)).rejects.toThrow('Invalid')
	})
	it('forwards actions using the native capture identity', async () => {
		const { deps, call } = boundary(); const owner = createNativeCapture(deps)
		await owner.action(81, { type: 'select', key: 'Cane of Kulemak', quantity: 1 })
		expect(call).toHaveBeenCalledWith('boss_loot_action', { id: 81, action: { type: 'select', key: 'Cane of Kulemak', quantity: 1 } })
	})
	it('projects native phase and deadline without creating a second countdown', () => {
		const { slice } = boundary()
		slice.pending = { id: 81, attribution: { area: nativeProfile.area, level: 83, seed: '42', profile: ref }, league: 'Historical', profile: nativeProfile,
			layout: 'list', rect: [10, 20, 900, 750], incoming: { 'Cinderswallow Urn': 1 }, items: { quantities: { 'Cinderswallow Urn': 1 }, counted: ['Cinderswallow Urn'] },
			readLines: ['SILVER FLASK'], unknownLines: ['Spidersilk Robe'], ambiguous: [], phase: 'open', deadlineMs: 4500, closeAtMs: null,
			target: row, forceNew: false, run: 31, nextRun: 32, mergeBlocked: false, droppedPositiveKeys: [], error: null }
		const stats = { runs: 1, devices: 1, nextRun: 32, items: {} }
		const projected = mapNativeOverlay(slice, stats, 'own')!
		expect(projected.seconds).toBe(3)
		expect(projected.layout).toBe('list')
		expect(projected.previousRun).toBe(31)
		expect(projected.capture.unknownLines).toEqual(['Spidersilk Robe'])
		slice.pending.phase = 'saved'; slice.pending.droppedPositiveKeys = ['hidden-eye']
		expect(mapNativeOverlay(slice, stats, 'own')?.native).toMatchObject({ phase: 'saved', run: 31, droppedPositiveKeys: ['hidden-eye'] })
	})
})
