import type { BossProfile, BossStats, LootItem, StatsScope, StoredRun } from './contracts'
import { groupNumber } from '$lib/stores/number-format.svelte'

export type ResultsSort = 'rate' | 'name'
export interface ResultsInput {
	profile: BossProfile
	stats: BossStats
	scope: StatsScope
	sort: ResultsSort
	notCounted: string[]
}

export function buildResultsView({ profile, stats, scope, sort, notCounted }: ResultsInput) {
	const runs = groupNumber(String(stats.runs))
	const deviceNote = scope === 'own' ? 'this device' : `${groupNumber(String(stats.devices))} device${stats.devices === 1 ? '' : 's'}`
	const items = [...profile.groups.flatMap((group) => group.items), ...profile.extra, ...profile.categories]
	const mixed = stats.runs > 0 && items.some((item) => !notCounted.includes(item.id) &&
		(Object.hasOwn(stats.items, item.id) ? stats.items[item.id].eligibleRuns : 0) !== stats.runs)
	const project = (item: LootItem) => {
		const measurement = Object.hasOwn(stats.items, item.id) ? stats.items[item.id] : { drops: 0, eligibleRuns: 0 }
		const disabled = notCounted.includes(item.id)
		const percent = measurement.eligibleRuns > 0 ? measurement.drops / measurement.eligibleRuns * 100 : null
		return { ...item, disabled, drops: disabled ? null : measurement.drops,
			rate: disabled ? '—' : stats.runs === 0 ? `wiki ${item.wikiRate}`
				: percent === null || (scope === 'collective' && measurement.drops === 0) ? '—' : `${Math.round(percent)}%`,
			provenance: disabled || stats.runs === 0 ? '' : `${groupNumber(String(measurement.drops))}/${groupNumber(String(measurement.eligibleRuns))}${mixed ? ' eligible runs' : ''}`,
			bar: disabled || stats.runs === 0 ? 0 : Math.min(100, percent ?? 0),
			// Flags change presentation only; sorting uses the underlying measurement (ADR-018).
			sortRate: percent
		}
	}
	const byName = (a: LootItem, b: LootItem) => a.name.replace(/^The /, '').localeCompare(b.name.replace(/^The /, ''))
	const sorted = (rows: LootItem[]) => rows.map(project).sort((a, b) => {
		if (sort === 'name') return byName(a, b)
		if (a.sortRate !== b.sortRate) {
			if (a.sortRate === null) return 1
			if (b.sortRate === null) return -1
			return a.sortRate > b.sortRate ? -1 : 1
		}
		return byName(a, b)
	})
	return {
		runs: stats.runs, deviceNote,
		mixed, rateHeader: mixed ? '= drops / eligible runs' : `= drops / ${runs} runs`,
		summary: `${runs} runs · ${deviceNote}`,
		message: stats.runs === 0 ? 'No runs logged for this boss yet: rates show the wiki estimate.' : '',
		groups: [
			...profile.groups.map((group) => {
				const uniform = group.items.every((item) => Object.hasOwn(stats.items, item.id) && stats.items[item.id].eligibleRuns === stats.runs)
				const single = group.min === 1 && group.max === 1
				const drops = group.items.reduce((sum, item) => sum + (Object.hasOwn(stats.items, item.id) ? stats.items[item.id].drops : 0), 0)
				return { id: group.id, title: `${group.label} pool · ${single ? 'one per kill' : `${group.min}–${group.max} per kill`}`,
					note: single && uniform && drops === stats.runs ? `${runs} of ${runs} runs have exactly one`
						: single ? 'exactly one per kill' : `${group.min}–${group.max} per kill`, items: sorted(group.items) }
			}),
			{ id: 'additional', title: 'Additional drops', note: 'any number per kill', items: sorted([...profile.extra, ...profile.categories]) }
		]
	}
}

/** Historical quantities are exported independently of today's exclusions. */
export function runsCsv(profile: BossProfile, runs: StoredRun[]): string {
	const items = [...profile.groups.flatMap((group) => group.items), ...profile.extra, ...profile.categories]
	const cell = (value: string | number) => /[",\r\n]/.test(String(value)) ? `"${String(value).replaceAll('"', '""')}"` : String(value)
	return [['Run', 'Date', ...items.map((item) => item.name)],
		...runs.map((run) => [run.run, run.date ?? '', ...items.map((item) =>
			Object.hasOwn(run.items.quantities, item.id) ? run.items.quantities[item.id] : 0)])]
		.map((row) => row.map(cell).join(',')).join('\r\n')
}
