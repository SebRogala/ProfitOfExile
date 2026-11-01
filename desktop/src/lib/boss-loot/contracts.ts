/** POE-317 desktop ports. Native Rust owns profiles, capture validation and writes. */
export type ItemKey = string
export type Quantities = Record<ItemKey, number>
export type StatsScope = 'own' | 'collective'
export type LootLayout = 'grid' | 'list'

export interface ProfileRef {
	boss: string
	profileKey: string
	profileVersion: number
}

export interface LootItem {
	id: ItemKey
	name: string
	base: string
	wikiRate: string
	quantity?: boolean
	notCountedDefault?: boolean
}

export interface GuaranteedGroup {
	id: string
	label: string
	min: number
	max: number
	items: LootItem[]
}

export interface BossProfile extends ProfileRef {
	name: string
	fullName?: string
	key: string
	area: string
	level: number | string
	groups: GuaranteedGroup[]
	extra: LootItem[]
	categories: LootItem[]
	/** Present only when a test-only profile supplies a capture example. */
	capture?: LootSelection
}

export interface LootSelection {
	picks: Record<string, ItemKey[]>
	extra: Quantities
	categories: Quantities
	ambiguous: { group: string; base: string; ids: ItemKey[] } | null
}

export interface Capture extends LootSelection {
	boss: string
	run: number
	seed: string | null
	attribution: { area: string; level: number; profileKey: string }
	readLines: string[]
	unknownLines: string[]
	refusedExample?: { area: string; level: number }
	mergedExample?: { previousRun: number }
}

/** Counted is historical eligibility, independent of today's not-counted settings. */
export interface RunItems {
	quantities: Quantities
	counted: ItemKey[]
}

export interface StoredRun extends ProfileRef {
	id: string
	league: string
	run: number
	date: string | null
	seed: string | null
	version: number
	items: RunItems
}

export interface BossStats {
	runs: number
	devices: number
	nextRun: number
	items: Record<ItemKey, { drops: number; eligibleRuns: number }>
}

export interface RunsPage {
	total: number
	page: number
	pageSize: number
	items: StoredRun[]
}

export interface EditRun {
	id: string
	expectedVersion: number
	items: RunItems
}

export interface DeleteRun {
	id: string
	expectedVersion: number
}

export type SubmitRun = ProfileRef & { league: string; seed: string | null; items: RunItems } & (
	| { mode: 'new' | 'forceNew' }
	| {
		mode: 'merge'
		target: { id: string; expectedVersion: number }
		/** Original capture positives, retained even when effective quantities exclude them. */
		incomingPositiveKeys: ItemKey[]
	}
)

export type SubmitOutcome =
	| { state: 'saving' }
	| { state: 'saved'; run: StoredRun; droppedPositiveKeys: ItemKey[] }
	| { state: 'unreachable' }
	/** HTTP 409; re-read the target before recomposing and validating in Rust. */
	| { state: 'conflict' }

export interface ModuleSettings {
	enabled: boolean
	autoSaveSeconds: number
	autoSaveOptions: number[]
	captureShortcut: string
	previewShortcut: string
	notCounted: ItemKey[]
}

export interface OverlaySettings {
	show: boolean
	layout: LootLayout
	layoutOptions: LootLayout[]
	defaultPlacement: { x: number; y: number; hostWidth: number; hostHeight: number; size: string }
}

export interface ProfilesPort { load(): Promise<BossProfile[]> }
export interface CapturePort { load(): Promise<Capture | null> }
export interface StatsPort { load(profile: ProfileRef, scope: StatsScope): Promise<BossStats> }
export interface RunsPort {
	list(profile: ProfileRef, page: number, pageSize: number): Promise<RunsPage>
	edit(request: EditRun): Promise<StoredRun>
	delete(request: DeleteRun): Promise<void>
}
export interface SubmitPort { submit(request: SubmitRun, onOutcome: (outcome: SubmitOutcome) => void): Promise<SubmitOutcome> }
export interface ModuleSettingsPort {
	load(): Promise<ModuleSettings>
	save(settings: ModuleSettings): Promise<ModuleSettings>
}
export interface OverlaySettingsPort {
	load(): Promise<OverlaySettings>
	save(settings: OverlaySettings): Promise<OverlaySettings>
}
export interface DevicePort { load(): Promise<{ id: string }> }

/** Explicit transport fields keep merge provenance separate from the validated full map. */
export function serializeSubmitRun(request: SubmitRun): string {
	const payload = {
		boss: request.boss, profileKey: request.profileKey, profileVersion: request.profileVersion,
		league: request.league, seed: request.seed, mode: request.mode, items: request.items
	}
	return JSON.stringify(request.mode === 'merge' ? {
		...payload, target: request.target, incomingPositiveKeys: request.incomingPositiveKeys
	} : payload)
}

/** D12: only explicit editable keys may change; the complete historical map travels. */
export function editRunItems(run: StoredRun, editableKeys: ItemKey[], changes: Quantities): EditRun {
	const quantities = { ...run.items.quantities }
	for (const [key, quantity] of Object.entries(changes)) {
		if (!editableKeys.includes(key)) throw new Error(`Item is not editable: ${key}`)
		if (!run.items.counted.includes(key)) throw new Error(`Item was not counted: ${key}`)
		if (!Number.isSafeInteger(quantity) || quantity < 0) throw new Error(`Invalid quantity: ${key}`)
		if (quantity === 0) delete quantities[key]
		else quantities[key] = quantity
	}
	return { id: run.id, expectedVersion: run.version, items: { quantities, counted: [...run.items.counted] } }
}
