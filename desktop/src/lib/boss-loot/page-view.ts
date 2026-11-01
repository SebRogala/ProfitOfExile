import { editRunItems, type BossProfile, type BossStats, type LootItem, type Quantities, type RunsPage, type StatsScope, type StoredRun } from './contracts'
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

export function buildRunsView(profile: BossProfile, runs: RunsPage) {
	const uniques = profile.groups.flatMap((group) => group.items)
	const extras = [...profile.extra, ...profile.categories]
	const rows = runs.items.map((run) => {
		const positive = (item: LootItem) => Object.hasOwn(run.items.quantities, item.id) && run.items.quantities[item.id] > 0
		const project = (item: LootItem) => ({ ...item, quantity: run.items.quantities[item.id] })
		return { id: run.id, run: run.run, date: run.date === null ? 'no date' : run.date.slice(0, 10),
			unique: uniques.filter(positive).map(project), extra: extras.filter(positive).map(project) }
	})
	const pages = Array.from({ length: Math.ceil(runs.total / runs.pageSize) }, (_, index) => index + 1)
	return { rows, page: runs.page, pages, first: runs.page <= 1, last: runs.page >= pages.length,
		range: rows.length ? `Runs ${groupNumber(String(rows[0].run))}–${groupNumber(String(rows.at(-1)!.run))} of ${groupNumber(String(runs.total))}, newest first`
			: 'No runs logged for this boss yet.' }
}

/** The request begins with the complete stored map, including keys absent from today's chips. */
export function buildRunEditor(profile: BossProfile, run: StoredRun, notCounted: string[], changes: Quantities) {
	const editable = (item: LootItem) => !notCounted.includes(item.id) && run.items.counted.includes(item.id)
	const editableKeys = [...profile.groups.flatMap((group) => group.items), ...profile.extra, ...profile.categories]
		.filter(editable).map((item) => item.id)
	const request = editRunItems(run, editableKeys, changes)
	const quantity = (id: string) => Object.hasOwn(request.items.quantities, id) ? request.items.quantities[id] : 0
	const project = (item: LootItem) => ({ ...item, selected: quantity(item.id) > 0, quantity: quantity(item.id) })
	const valid = profile.groups.every((group) => {
		const picked = group.items.filter((item) => quantity(item.id) > 0)
		return picked.length >= group.min && picked.length <= group.max && picked.every((item) => quantity(item.id) === 1)
	})
	return { id: run.id, run: run.run, request, editableKeys, valid,
		groups: profile.groups.map((group) => ({ id: group.id, label: group.label,
			rule: group.min === 1 && group.max === 1 ? 'pick one' : `pick ${group.min}–${group.max}`,
			items: group.items.filter(editable).map(project) })),
		extra: [...profile.extra, ...profile.categories].filter(editable).map(project) }
}
