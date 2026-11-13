/** Fixture-only adapters. Each factory accepts just its owner's data. No local run history. */
import type fixture from './__fixtures__/boss-loot.json'
import type {
	BossProfile, Capture, CapturePort, DevicePort, GuaranteedGroup, LootItem, LootSelection,
	ModuleSettings, ModuleSettingsPort, OverlaySettings, OverlaySettingsPort, ProfileRef,
	ProfilesPort, Quantities, RunsPort, StatsPort, StoredRun, SubmitOutcome, SubmitPort
} from './contracts'

type Fixture = typeof fixture
type FixtureProfile = {
	name: string; fullName?: string; key: string; area: string; level: number | string
	groups: GuaranteedGroup[]; extra: LootItem[]; categories: LootItem[]; capture?: LootSelection
}

/** Fixture aliases stop here. Removed profile keys remain for historical runs and stats. */
const CATARINA_KEYS: Record<string, string> = {
	urn: 'Cinderswallow Urn',
	spinehail: 'Spinehail',
	cane: 'Cane of Kulemak',
	diadem: 'The Devouring Diadem',
	bitterbind: 'Bitterbind Point',
	queens: "The Queen's Hunger",
	eye: 'Metadata/Items/MapFragments/AbyssPinnacleKeyGhastly',
	veiledex: 'Metadata/Items/Currency/CurrencyRerollRareVeiled',
	propagation: 'Metadata/Items/MapFragments/PropagationAllflamePack',
	allflame: 'Metadata/Items/MapFragments/BetrayalAllflamePack',
	ethereal: 'Metadata/Items/MapFragments/CleansedAllflamePack',
	communion: 'Communion Support',
	nooks: 'Metadata/Items/DivinationCards/DivinationCardNooksCrown',
	keepers: 'Metadata/Items/DivinationCards/DivinationCardKeepersCorruption', // Historical only; absent from the current profile.
	rareveiled: 'category:catarina:1:rare-two-veiled-modifiers'
}

function catarinaKey(alias: string): string {
	if (!Object.hasOwn(CATARINA_KEYS, alias)) throw new Error(`Unknown Catarina fixture item: ${alias}`)
	return CATARINA_KEYS[alias]
}

function mapQuantities(values: Quantities, key: (alias: string) => string): Quantities {
	return Object.fromEntries(Object.entries(values).map(([alias, quantity]) => [key(alias), quantity]))
}

function mapSelection(selection: LootSelection, key: (alias: string) => string): LootSelection {
	return {
		picks: Object.fromEntries(Object.entries(selection.picks).map(([group, ids]) => [group, ids.map(key)])),
		extra: mapQuantities(selection.extra, key),
		categories: mapQuantities(selection.categories, key),
		ambiguous: selection.ambiguous ? { ...selection.ambiguous, ids: selection.ambiguous.ids.map(key) } : null
	}
}

export function createFixtureProfiles(source: Record<string, FixtureProfile>): ProfilesPort {
	return {
		load: async () => Object.entries(source).map(([boss, raw]): BossProfile => {
			const profile = structuredClone(raw)
			const keys = new Map<string, string>()
			for (const item of [...profile.groups.flatMap((g) => g.items), ...profile.extra, ...profile.categories]) {
				keys.set(item.id, boss === 'catarina' ? catarinaKey(item.id) :
					profile.categories.includes(item) ? `category:${boss}:1:${item.name}` : item.name)
			}
			const key = (id: string) => {
				const mapped = keys.get(id)
				if (mapped === undefined) throw new Error(`Unknown ${boss} fixture item: ${id}`)
				return mapped
			}
			const item = (value: LootItem): LootItem => ({ ...value, id: key(value.id) })
			return {
				...profile, boss, profileKey: profile.key, profileVersion: 2,
				groups: profile.groups.map((group) => ({ ...group, items: group.items.map(item) })),
				extra: profile.extra.map(item), categories: profile.categories.map(item),
				...(profile.capture ? { capture: mapSelection(profile.capture, key) } : {})
			}
		})
	}
}

export function createFixtureCapture(source: Capture): CapturePort {
	return {
		load: async (): Promise<Capture> => ({ ...structuredClone(source), ...mapSelection(source, catarinaKey) })
	}
}

/** Synthetic envelopes preserve the designer's unknown historical identity and dates. */
const FIXTURE_PROFILE: ProfileRef = { boss: 'catarina', profileKey: 'Normal', profileVersion: 2 }

function requireFixtureProfile(profile: ProfileRef): void {
	if (profile.boss !== FIXTURE_PROFILE.boss || profile.profileKey !== FIXTURE_PROFILE.profileKey ||
		profile.profileVersion !== FIXTURE_PROFILE.profileVersion) throw new Error('Unknown fixture profile')
}

function fixtureRun(run: number, date: string | null, quantities: Quantities): StoredRun {
	return {
		...FIXTURE_PROFILE, id: `fixture-catarina-${run}`, league: 'fixture-only', run, date,
		seed: null, version: 1, items: { quantities, counted: Object.values(CATARINA_KEYS) }
	}
}

type FixtureStats = Record<string, Record<'own' | 'collective', {
	runs: number; devices: number; items: Quantities; eligibleRuns?: Quantities; nextRun?: number
}>>

export function createFixtureStats(source: FixtureStats): StatsPort {
	return {
		load: async (profile, scope) => {
			requireFixtureProfile(profile)
			const stats = source.catarina[scope]
			return {
				runs: stats.runs, devices: stats.devices, nextRun: stats.nextRun ?? stats.runs + 1,
				items: Object.fromEntries(Object.entries(CATARINA_KEYS).map(([alias, key]) => [key, {
					drops: Object.hasOwn(stats.items, alias) ? (stats.items as Quantities)[alias] : 0,
					eligibleRuns: stats.eligibleRuns && Object.hasOwn(stats.eligibleRuns, alias)
						? stats.eligibleRuns[alias] : stats.runs
				}]))
			}
		}
	}
}

export function createFixtureRuns(source: Fixture['runs']): RunsPort {
	const rows = () => source.catarina.items.map((row) =>
		fixtureRun(row.run, row.date, Object.fromEntries(row.items.map((alias) => [catarinaKey(alias), 1]))))
	const target = (id: string, version: number) => {
		const row = rows().find((row) => row.id === id)
		if (!row) throw new Error('Run not found')
		if (row.version !== version) throw new Error('Version conflict')
		return row
	}
	return {
		list: async (profile, page, pageSize) => {
			requireFixtureProfile(profile)
			if (!Number.isSafeInteger(page) || page < 1 || !Number.isSafeInteger(pageSize) || pageSize < 1) {
				throw new Error('Invalid pagination')
			}
			return {
				total: source.catarina.total, page, pageSize,
				items: rows().sort((a, b) => b.run - a.run).slice((page - 1) * pageSize, page * pageSize)
			}
		},
		edit: async (request) => {
			const row = target(request.id, request.expectedVersion)
			return { ...row, version: row.version + 1, items: {
				quantities: structuredClone(request.items.quantities), counted: row.items.counted
			} }
		},
		delete: async (request) => { target(request.id, request.expectedVersion) }
	}
}

/** The terminal outcome is selected by the harness, never by another owner's fixture. */
export function createFixtureSubmit(
	source: Fixture['submit'], terminal: 'saved' | 'unreachable' | 'conflict',
	savedRun?: StoredRun, droppedPositiveKeys: string[] = []
): SubmitPort {
	return {
		submit: async (_request, onOutcome) => {
			if (!source.outcomes.includes('saving')) throw new Error('Missing saving fixture outcome')
			if (terminal !== 'conflict' && !source.outcomes.includes(terminal)) throw new Error('Missing terminal fixture outcome')
			if (terminal === 'saved' && !savedRun) throw new Error('Saved fixture outcome requires a server row')
			onOutcome({ state: 'saving' })
			const result: SubmitOutcome = terminal === 'saved'
				? { state: 'saved', run: structuredClone(savedRun!), droppedPositiveKeys: [...droppedPositiveKeys] }
				: { state: terminal }
			onOutcome(structuredClone(result))
			return result
		}
	}
}

export function createFixtureModuleSettings(source: Fixture['moduleSettings']): ModuleSettingsPort {
	let settings: ModuleSettings = { ...structuredClone(source), notCounted: source.notCounted.map(catarinaKey) }
	return {
		load: async () => structuredClone(settings),
		save: async (next) => { settings = structuredClone(next); return structuredClone(settings) }
	}
}

/** Layout belongs to typed Settings/SSOT under D9, despite the original fixture's ui_prefs note. */
export function createFixtureOverlaySettings(source: Fixture['overlaySettings']): OverlaySettingsPort {
	if (source.layout !== 'grid' && source.layout !== 'list') throw new Error('Invalid loot layout')
	const layoutOptions: OverlaySettings['layoutOptions'] = source.layoutOptions.map((layout) => {
		if (layout !== 'grid' && layout !== 'list') throw new Error('Invalid loot layout option')
		return layout
	})
	let settings: OverlaySettings = { ...structuredClone(source), layout: source.layout, layoutOptions }
	return {
		load: async () => structuredClone(settings),
		save: async (next) => { settings = structuredClone(next); return structuredClone(settings) }
	}
}

export function createFixtureDevice(source: Fixture['device']): DevicePort {
	return { load: async () => ({ id: source.id }) }
}
