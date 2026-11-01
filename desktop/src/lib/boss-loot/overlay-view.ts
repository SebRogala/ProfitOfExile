import type { BossProfile, BossStats, Capture, LootItem, LootLayout, LootSelection, StatsScope, SubmitOutcome } from './contracts'
import { groupNumber } from '$lib/stores/number-format.svelte'

export interface OverlayInput {
	profile: BossProfile
	capture: Capture
	/** Original OCR selection remains unchanged while the fixture's picks are edited. */
	readSelection?: LootSelection
	stats: BossStats | null
	scope: StatsScope
	layout: LootLayout
	notCounted: string[]
	autoSaveSeconds: number
	seconds: number
	outcome?: SubmitOutcome | { state: 'saved'; run: { run: number }; droppedPositiveKeys: string[] }
	previousRun?: number
	/** Read-side native authority; no browser timer or submit owner. */
	native?: { phase: string; error: string | null; deadlineMs: number | null; run: number; droppedPositiveKeys: string[] }
}

export interface ItemView extends Omit<LootItem, 'quantity'> {
	selected: boolean
	disabled: boolean
	ambiguous: boolean
	category: boolean
	radio: boolean
	quantity: number
	showQuantity: boolean
	rate: string
	provenance: string
	listProvenance: string
	tag: string
	subtitle: string
	title: string
}

export type SelectionAction = { type: 'toggle'; id: string } | { type: 'quantity'; id: string; delta: number }

/** Fixture interaction projection; WI-9 forwards the same user intent to Rust's owner. */
export function changeSelection(profile: BossProfile, selection: LootSelection, notCounted: string[], action: SelectionAction): LootSelection {
	const next = structuredClone(selection)
	if (notCounted.includes(action.id)) return next
	const group = profile.groups.find((group) => group.items.some((item) => item.id === action.id))
	if (group) {
		if (action.type !== 'toggle') return next
		const picked = next.picks[group.id] ?? []
		next.picks[group.id] = picked.includes(action.id) ? picked.filter((id) => id !== action.id)
			: group.max === 1 ? [action.id] : [...picked, action.id]
		if (next.ambiguous?.ids.includes(action.id)) next.ambiguous = null
		return next
	}
	const category = profile.categories.find((item) => item.id === action.id)
	const item = category ?? profile.extra.find((item) => item.id === action.id)
	if (!item || (action.type === 'quantity' && !category && !item.quantity)) return next
	const quantities = category ? next.categories : next.extra
	const current = Object.hasOwn(quantities, item.id) ? quantities[item.id] : 0
	const quantity = action.type === 'toggle' ? (current > 0 ? 0 : 1) : Math.max(0, current + action.delta)
	if (Number.isSafeInteger(quantity)) quantities[item.id] = quantity
	return next
}

/** Both the native forwarded path and DOM bubbling enter here once per click. */
export function dispatchOverlayAction(action: string, touch: () => void, perform: (action: string) => void): void {
	touch()
	if (action !== 'loot.touch') perform(action)
}

export function buildOverlayView(input: OverlayInput) {
	const { profile, capture, stats, scope, notCounted, outcome, autoSaveSeconds } = input
	const items = [...profile.groups.flatMap((group) => group.items), ...profile.extra, ...profile.categories]
	const noRuns = stats?.runs === 0
	const mixed = !!stats && !noRuns && items.some((item) => !notCounted.includes(item.id) &&
		(Object.hasOwn(stats.items, item.id) ? stats.items[item.id].eligibleRuns : 0) !== stats.runs)
	const read = input.readSelection ?? capture
	const reads = new Set([...Object.values(read.picks).flat(),
		...Object.entries({ ...read.extra, ...read.categories }).filter(([, quantity]) => quantity > 0).map(([id]) => id)])
	const project = (item: LootItem, selected: boolean, quantity: number, category = false, radio = false): ItemView => {
		const disabled = notCounted.includes(item.id)
		const ambiguous = capture.ambiguous?.ids.includes(item.id) ?? false
		const measurement = stats && Object.hasOwn(stats.items, item.id) ? stats.items[item.id] : { drops: 0, eligibleRuns: 0 }
		const denominator = measurement.eligibleRuns
		const rate = disabled || !stats ? '—' : noRuns ? '' : denominator === 0 || (scope === 'collective' && measurement.drops === 0)
			? '—' : `${Math.round(measurement.drops / denominator * 100)}%`
		const provenance = disabled || !stats ? '' : noRuns ? category ? 'count by hand' : `wiki ${item.wikiRate}`
			: `${groupNumber(String(measurement.drops))}/${groupNumber(String(denominator))}${mixed ? ' eligible runs' : ''}`
		return {
			...item, selected: selected && !disabled && !ambiguous, disabled, ambiguous, category, radio,
			quantity, showQuantity: !disabled && (category || (!!item.quantity && selected)), rate, provenance,
			listProvenance: mixed && !disabled ? provenance : '',
			tag: ambiguous ? `${capture.ambiguous!.base}?` : category ? 'category' : reads.has(item.id) ? 'read' : '',
			subtitle: disabled ? 'your loot filter hides it' : ambiguous ? `${capture.ambiguous!.base}? pick if this one dropped`
				: `${item.base}${reads.has(item.id) ? ' · read' : ''}`,
			title: disabled ? 'Not counted: change on the Boss loot page' : `${item.base} · wiki ${item.wikiRate}`
		}
	}
	const groups = profile.groups.map((group) => {
		const picked = capture.picks[group.id] ?? []
		const ambiguity = capture.ambiguous?.group === group.id ? capture.ambiguous : null
		const count = picked.length
		const valid = !ambiguity && count >= group.min && count <= group.max
		return {
			id: group.id, label: group.label, radio: group.max === 1,
			rule: group.min === group.max ? `exactly ${group.min}` : `${group.min}–${group.max}`,
			status: ambiguity ? 'which one?' : count > group.max ? `${count} picked: max ${group.max}`
				: count < group.min ? `pick ${group.min === group.max ? group.min : `at least ${group.min}`}` : `${count} picked`,
			tone: ambiguity || count < group.min ? 'warning' : count > group.max ? 'error' : 'success',
			ambiguity, valid,
			items: group.items.map((item) => project(item, picked.includes(item.id), picked.includes(item.id) ? 1 : 0, false, group.max === 1))
		}
	})
	const extra = [
		...profile.extra.map((item) => project(item, (capture.extra[item.id] ?? 0) > 0, capture.extra[item.id] ?? 0)),
		...profile.categories.map((item) => project(item, (capture.categories[item.id] ?? 0) > 0, capture.categories[item.id] ?? 0, true))
	]
	const quantity = extra.reduce((sum, item) => sum + (item.selected ? item.quantity : 0), 0)
	const valid = input.native ? input.native.phase === 'open' && input.native.error === null : !capture.ambiguous && groups.every((group) => group.valid)
	const open = outcome === undefined
	const counting = open && valid && autoSaveSeconds > 0 && (!input.native || input.native.deadlineMs !== null)
	const pause = capture.ambiguous ? `which ${capture.ambiguous.base}?` : 'check guaranteed'
	const badge = outcome?.state === 'saving' ? 'Saving' : outcome?.state === 'saved' ? 'Saved'
		: outcome?.state === 'unreachable' ? 'Not saved' : outcome?.state === 'conflict' || (open && !valid)
			? `Paused: ${pause}` : counting ? `Saving in ${input.seconds} s` : ''
	const dropped = outcome?.state === 'saved' ? outcome.droppedPositiveKeys : []
	return {
		layout: input.layout, width: input.layout === 'grid' ? 400 : 420, groups, extra,
		rateHeader: !stats ? '' : mixed ? '= drops / eligible runs' : `= drops / ${groupNumber(String(stats.runs))} runs`, mixed,
		runsNote: !stats ? '' : noRuns ? 'No runs logged for this boss yet: rates show the wiki estimate.'
			: `Chances from ${groupNumber(String(stats.runs))} runs${scope === 'collective' ? ` · ${groupNumber(String(stats.devices))} device${stats.devices === 1 ? '' : 's'}` : ''}`,
		extraMessage: `${groupNumber(String(quantity))} item${quantity === 1 ? '' : 's'}`,
		unknownLine: capture.unknownLines.length ? `Not in table: ${capture.unknownLines.join(', ')}` : '',
		badge, barPercent: counting ? Math.min(100, Math.max(0, input.seconds / autoSaveSeconds * 100)) : 0,
		canSave: open && valid, open,
		message: outcome?.state === 'saved' ? `Saved as run ${groupNumber(String(outcome.run.run))}.`
			: outcome?.state === 'unreachable' ? 'Server unreachable. This run was not recorded.' : input.native?.error ?? '',
		excludedMessage: dropped.length ? `Not added (not counted in the original run): ${dropped.map((id) => items.find((item) => item.id === id)?.name ?? id).join(', ')}.` : '',
		mergeMessage: open && input.previousRun !== undefined ? `Same seed as run ${groupNumber(String(input.previousRun))}: added to it.` : '',
		tone: outcome?.state === 'unreachable' ? 'error' : outcome?.state === 'saved' ? 'success'
			: outcome?.state === 'conflict' || (open && !valid) ? 'warning' : 'info'
	}
}

export type OverlayView = ReturnType<typeof buildOverlayView>
