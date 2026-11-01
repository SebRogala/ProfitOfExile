import { invoke } from '@tauri-apps/api/core'
import { ssot, fetchSsot, type BossLootSlice, type BossLootSettings, type BossLootShortcut, type BossLootProfile } from '$lib/stores/ssot.svelte'
import { store } from '$lib/stores/status.svelte'
import type { BossProfile, BossStats, Capture, DevicePort, ModuleSettings, ModuleSettingsPort, ProfileRef, ProfilesPort, RunsPort, RunsPage, StoredRun, StatsPort, StatsScope, Quantities, LootSelection } from './contracts'
import type { OverlayInput } from './overlay-view'

/** Inject only native transport/read boundaries; SSOT remains the single snapshot owner. */
export interface NativeBoundary {
	call(command: string, args?: Record<string, unknown>): Promise<unknown>
	snapshot(): BossLootSlice | null
	reconcile(): Promise<void>
	deviceId(): string | undefined
}
const native: NativeBoundary = {
	call: invoke, snapshot: () => ssot.bossLoot, reconcile: () => fetchSsot('write'),
	deviceId: () => store.status?.device_id
}
function invalid(field: string): never { throw new Error(`Invalid native boss loot ${field}`) }
function object(value: unknown): Record<string, unknown> {
	if (!value || typeof value !== 'object' || Array.isArray(value)) return invalid('object')
	return value as Record<string, unknown>
}
function string(value: unknown): string { return typeof value === 'string' ? value : invalid('string') }
function boolean(value: unknown): boolean { return typeof value === 'boolean' ? value : invalid('boolean') }
function integer(value: unknown, minimum = 0): number {
	return typeof value === 'number' && Number.isSafeInteger(value) && value >= minimum ? value : invalid('integer')
}
function array<T>(value: unknown, map: (item: unknown) => T): T[] {
	return Array.isArray(value) ? value.map(map) : invalid('array')
}
function nullableString(value: unknown): string | null { return value === null ? null : string(value) }
function quantities(value: unknown): Quantities {
	return Object.fromEntries(Object.entries(object(value)).map(([key, quantity]) => [key, integer(quantity, 1)]))
}
function runItems(value: unknown) {
	const item = object(value)
	const result = { quantities: quantities(item.quantities), counted: array(item.counted, string) }
	if (new Set(result.counted).size !== result.counted.length || Object.keys(result.quantities).some((key) => !result.counted.includes(key))) invalid('counted set')
	return result
}
function profileRef(profile: ProfileRef): ProfileRef {
	return { boss: profile.boss, profileKey: profile.profileKey, profileVersion: profile.profileVersion }
}
function mapRun(value: unknown): StoredRun {
	const row = object(value)
	return { boss: string(row.boss), profileKey: string(row.profileKey), profileVersion: integer(row.profileVersion, 1),
		id: string(row.id), league: string(row.league), run: integer(row.run, 1), version: integer(row.version, 1),
		date: nullableString(row.date), seed: nullableString(row.seed), items: runItems(row.items) }
}
export function mapNativeProfile(value: unknown): BossProfile {
	const profile = object(value)
	const item = (value: unknown) => {
		const row = object(value)
		if (row.quantity !== undefined && typeof row.quantity !== 'boolean') invalid('quantity flag')
		if (row.notCountedDefault !== undefined && typeof row.notCountedDefault !== 'boolean') invalid('not-counted flag')
		return { id: string(row.id), name: string(row.name), base: string(row.base), wikiRate: string(row.wikiRate),
			quantity: row.quantity as boolean | undefined, notCountedDefault: row.notCountedDefault as boolean | undefined }
	}
	return { boss: string(profile.boss), name: string(profile.name), fullName: string(profile.fullName),
		key: string(profile.key), profileKey: string(profile.key), profileVersion: integer(profile.profileVersion, 1),
		area: string(profile.area), level: integer(profile.level, 1),
		groups: array(profile.groups, (value) => { const group = object(value); return {
			id: string(group.id), label: string(group.label), min: integer(group.min), max: integer(group.max), items: array(group.items, item)
		} }), extra: array(profile.extra, item), categories: array(profile.categories, item) }
}
export function createNativeProfiles(deps = native): ProfilesPort {
	return { load: async () => array(await deps.call('boss_loot_profiles'), mapNativeProfile) }
}
export function createNativeStats(deps = native): StatsPort {
	return { load: async (profile, scope): Promise<BossStats> => {
		const stats = object(await deps.call('boss_loot_stats', { profile: profileRef(profile), scope }))
		return { runs: integer(stats.runs), devices: integer(stats.devices), nextRun: integer(stats.nextRun, 1),
			items: Object.fromEntries(Object.entries(object(stats.items)).map(([key, value]) => {
				const item = object(value); return [key, { drops: integer(item.drops), eligibleRuns: integer(item.eligibleRuns) }]
			})) }
	} }
}
export function createNativeRuns(deps = native): RunsPort {
	return {
		list: async (profile, page, pageSize): Promise<RunsPage> => {
			const response = object(await deps.call('boss_loot_runs', { profile: profileRef(profile), page, pageSize }))
			return { total: integer(response.total), page: integer(response.page, 1), pageSize: integer(response.pageSize, 1), items: array(response.items, mapRun) }
		},
		edit: async (request) => mapRun(await deps.call('boss_loot_edit', { ...request })),
		delete: async (request) => { await deps.call('boss_loot_delete', { ...request }) }
	}
}
const mouseKeys: Record<string, number> = { 'Left click': 1, 'Right click': 2, 'Middle click': 4, 'Mouse 4': 5, 'Mouse 5': 6 }
const triggerKeys: Record<string, number> = { ...mouseKeys, Backspace: 8, Tab: 9, Enter: 13, Space: 32, PageUp: 33, PageDown: 34, End: 35, Home: 36, ArrowLeft: 37, ArrowUp: 38, ArrowRight: 39, ArrowDown: 40, Insert: 45, Delete: 46 }
export function shortcutLabel(shortcut: BossLootShortcut): string {
	const named = Object.entries(triggerKeys).find(([, key]) => key === shortcut.key)?.[0]
	const key = named ?? (shortcut.key >= 112 && shortcut.key <= 135 ? `F${shortcut.key - 111}`
		: shortcut.key >= 48 && shortcut.key <= 90 ? String.fromCharCode(shortcut.key)
		: `Key ${shortcut.key}`)
	return [...(shortcut.ctrl ? ['Ctrl'] : []), ...(shortcut.alt ? ['Alt'] : []), ...(shortcut.shift ? ['Shift'] : []), key].join(' + ')
}
function parseShortcut(label: string, existing: BossLootShortcut): BossLootShortcut {
	if (label === shortcutLabel(existing)) return existing
	const parts = label.split(' + ')
	const name = parts.pop()!
	if (parts.some((part) => !['Shift', 'Ctrl', 'Alt'].includes(part))) invalid('shortcut modifier')
	const key = Object.hasOwn(triggerKeys, name) ? triggerKeys[name] : /^F([1-9]|1\d|2[0-4])$/.test(name) ? Number(name.slice(1)) + 111
		: /^[A-Z0-9]$/.test(name) ? name.charCodeAt(0) : invalid('shortcut')
	return { key, shift: parts.includes('Shift'), ctrl: parts.includes('Ctrl'), alt: parts.includes('Alt') }
}
async function snapshot(deps: NativeBoundary): Promise<BossLootSlice> {
	if (!deps.snapshot()) await deps.reconcile()
	return deps.snapshot() ?? invalid('snapshot unavailable')
}
export function moduleSettingsFromSnapshot(slice: BossLootSlice): ModuleSettings {
	const shortcut = (value: unknown): BossLootShortcut => {
		const binding = object(value)
		return { key: integer(binding.key, 1), shift: boolean(binding.shift), ctrl: boolean(binding.ctrl), alt: boolean(binding.alt) }
	}
	const settings = object(slice.settings)
	return { enabled: boolean(slice.enabled), autoSaveSeconds: integer(settings.autoSaveSeconds), autoSaveOptions: [5, 10, 20, 30, 0],
		captureShortcut: shortcutLabel(shortcut(settings.captureShortcut)), previewShortcut: shortcutLabel(shortcut(settings.previewShortcut)), notCounted: array(settings.notCounted, string) }
}
export function createNativeModuleSettings(deps = native): ModuleSettingsPort {
	return {
		load: async () => moduleSettingsFromSnapshot(await snapshot(deps)),
		save: async (next) => {
			const current = (await snapshot(deps)).settings
			const settings: BossLootSettings = { ...current, autoSaveSeconds: next.autoSaveSeconds, notCounted: next.notCounted,
				captureShortcut: parseShortcut(next.captureShortcut, current.captureShortcut), previewShortcut: parseShortcut(next.previewShortcut, current.previewShortcut) }
			await deps.call('boss_loot_set_settings', { settings })
			await deps.reconcile()
			return moduleSettingsFromSnapshot(await snapshot(deps))
		}
	}
}
export function createNativeDevice(deps = native): DevicePort {
	return { load: async () => {
		const id = deps.deviceId() ?? string(object(await deps.call('get_status')).device_id)
		if (!id) invalid('device identity')
		return { id }
	} }
}
export async function saveNativeCsv(profile: ProfileRef, deps = native): Promise<void> {
	const response = object(await deps.call('boss_loot_export', { profile: profileRef(profile) }))
	if (response.state !== 'saved' && response.state !== 'cancelled') invalid('export result')
}
export function createNativeCapture(deps = native) {
	return {
		load: () => snapshot(deps),
		action: async (id: number, action: { type: 'touch' | 'forceNew' | 'discard' } | { type: 'select'; key: string; quantity: number }) => {
			try { await deps.call('boss_loot_action', { id, action }) }
			finally { await deps.reconcile() }
		}
	}
}
export function createNativeSubmit(deps = native) {
	return {
		submit: async (id: number) => {
			try { await deps.call('boss_loot_submit', { id }) }
			finally { await deps.reconcile() }
		}
	}
}
function selection(profile: BossProfile, quantities: Quantities, ambiguous: string[][]): LootSelection {
	const candidates = ambiguous[0]
	const group = candidates && profile.groups.find((group) => group.items.some((item) => candidates.includes(item.id)))
	return {
		picks: Object.fromEntries(profile.groups.map((group) => [group.id, group.items.filter((item) => (quantities[item.id] ?? 0) > 0).map((item) => item.id)])),
		extra: Object.fromEntries(profile.extra.map((item) => [item.id, quantities[item.id] ?? 0])),
		categories: Object.fromEntries(profile.categories.map((item) => [item.id, quantities[item.id] ?? 0])),
		ambiguous: group ? { group: group.id, base: group.items.find((item) => candidates.includes(item.id))!.base, ids: candidates } : null
	}
}
/** Display-only tags mirror LootProfile::match_line using immutable OCR labels.
 * Native incoming changes on edits; these tags never drive native selections or writes. */
function ocrReadTags(profile: BossProfile, lines: string[]): LootSelection {
	const label = (value: string) => value.replace(/['’`‘]/g, '').toUpperCase().replace(/\s+/g, ' ').trim()
		.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, '').replace(/^SUPERIOR /, '')
	const quantities: Quantities = {}
	for (const line of lines) {
		const normalized = label(line)
		const candidates = [...profile.groups.flatMap((group) => group.items).filter((item) => label(item.base) === normalized || label(item.name) === normalized),
			...profile.extra.filter((item) => label(item.name) === normalized)]
		if (candidates.length === 1) quantities[candidates[0].id] = 1
	}
	return selection(profile, quantities, [])
}
export function mapNativeOverlay(slice: BossLootSlice, stats: BossStats | null, scope: StatsScope): OverlayInput | null {
	const pending = slice.pending
	if (!pending || pending.phase === 'reading' || pending.phase === 'loading') return null
	const profile = mapNativeProfile(pending.profile)
	// PendingCapture.items retains the last valid submission map while incoming remains editable.
	const capture: Capture = { ...selection(profile, pending.error || !pending.target ? pending.incoming : pending.items.quantities, pending.ambiguous), boss: profile.boss,
		run: pending.run, seed: pending.attribution.seed, attribution: { ...pending.attribution, profileKey: profile.profileKey }, readLines: pending.readLines, unknownLines: pending.unknownLines }
	const allItems = [...profile.groups.flatMap((group) => group.items), ...profile.extra, ...profile.categories]
	return { profile, capture, readSelection: ocrReadTags(profile, pending.readLines), stats, scope, layout: pending.layout,
		notCounted: allItems.filter((item) => !pending.items.counted.includes(item.id)).map((item) => item.id),
		autoSaveSeconds: slice.settings.autoSaveSeconds,
		seconds: pending.deadlineMs === null ? 0 : Math.max(0, Math.ceil((pending.deadlineMs - slice.nowMs) / 1000)),
		previousRun: pending.target && !pending.forceNew ? pending.target.run : undefined,
		outcome: pending.phase === 'saved' ? { state: 'saved', run: { run: pending.run }, droppedPositiveKeys: pending.droppedPositiveKeys }
			: pending.phase === 'saving' ? { state: 'saving' } : pending.phase === 'unreachable' ? { state: 'unreachable' } : undefined,
		native: { phase: pending.phase, error: pending.error, deadlineMs: pending.deadlineMs, run: pending.run, droppedPositiveKeys: pending.droppedPositiveKeys }
	}
}
