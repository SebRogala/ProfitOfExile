/**
 * The divine rate's pure half (POE-284): normalization, the state derivation,
 * the formatters and the chip/tooltip text. The rune store
 * (`divine-rate.svelte.ts`) holds the snapshot; this file decides what it
 * means and how it reads, because a `.svelte` file has no unit-test harness
 * here and any surface showing the rate words it through these.
 *
 * Copy and formats are the design handoff's
 * (`design/handoffs/poe-284--desktop-app--top-bar--divine-rate/`).
 */
import type { DivineRate } from '$lib/api';

export type DivineRateState = DivineRate['state'];

/** What the store holds: the port's fields, `updatedAt` null before the first read. */
export interface DivineRateSnapshot {
	divineChaosRate: number | null;
	priceHour: string | null;
	updatedAt: string | null;
	state: DivineRateState;
	unreachableSince: string | null;
}

export interface DivineRateView {
	state: DivineRateState;
	chip: string;
	tooltipHtml: string;
}

/**
 * A rate is usable only as a finite number > 0. The owner answers `0` for an
 * hour with no divine/chaos trade (`divineRateOf` in `internal/exchange/plays.go`),
 * and a consumer divides by this value, so anything else is stored as unknown.
 */
export function normalizeDivineRate(port: DivineRate): DivineRate {
	const r = port.divineChaosRate;
	return { ...port, divineChaosRate: typeof r === 'number' && Number.isFinite(r) && r > 0 ? r : null };
}

/**
 * Cold whenever there is no rate — that beats the source's state, so boot,
 * loading and "no exchange hour" all render "—". With a rate, the source
 * decides between stale and ready; the store derives no liveness of its own.
 */
export function deriveDivineRateState(snap: Pick<DivineRateSnapshot, 'divineChaosRate' | 'state'>): DivineRateState {
	if (snap.divineChaosRate === null) return 'cold';
	return snap.state === 'stale' ? 'stale' : 'ready';
}

/** Whole chaos, grouped from 1,000 (`_formats.chip`). */
export function formatChipRate(rate: number): string {
	return Math.round(rate).toLocaleString('en-US');
}

/** One decimal (`_formats.tooltip`); grouping at ≥1,000 matches the chip. */
export function formatTooltipRate(rate: number): string {
	return rate.toLocaleString('en-US', { minimumFractionDigits: 1, maximumFractionDigits: 1 });
}

/**
 * `en-US` with explicit parts rather than `toLocaleString()`: an unpinned
 * locale changes month names and separators per machine, and `en-GB` prints
 * "Sept" on newer ICU. `timeZone` undefined is the machine's own zone.
 */
function timeParts(iso: string, timeZone: string | undefined, withDate: boolean): Record<string, string> {
	const fmt = new Intl.DateTimeFormat('en-US', {
		timeZone,
		...(withDate ? { day: '2-digit', month: 'short' } : {}),
		hour: '2-digit',
		minute: '2-digit',
		hourCycle: 'h23'
	} as Intl.DateTimeFormatOptions);
	const parts: Record<string, string> = {};
	for (const p of fmt.formatToParts(new Date(iso))) parts[p.type] = p.value;
	return parts;
}

/** `09 Sep 23:00` in local time (`_formats.hour`). */
export function formatPriceHour(iso: string, timeZone?: string): string {
	const p = timeParts(iso, timeZone, true);
	return `${p.day} ${p.month} ${p.hour}:${p.minute}`;
}

/** `21:04` in local time — the stale line's clock. */
export function formatClock(iso: string, timeZone?: string): string {
	const p = timeParts(iso, timeZone, false);
	return `${p.hour}:${p.minute}`;
}

/** `updated N min ago` in whole minutes, never negative (`_formats.age`). */
export function formatAge(updatedAt: string, now: number): string {
	return `updated ${Math.max(0, Math.floor((now - Date.parse(updatedAt)) / 60_000))} min ago`;
}

const COLD_CHIP = '—';
const LINE_3 = 'Every chaos ↔ divine figure in the app uses this rate.';

function lines(head: string, detail: string): string {
	const secondary = (text: string) => `<div style="color:var(--color-lab-text-secondary)">${text}</div>`;
	return `<div style="font-weight:700">${head}</div>${secondary(detail)}${secondary(LINE_3)}`;
}

/**
 * The chip text and its three-line tooltip for one snapshot. The HTML is built
 * from fixed copy plus formatter output only (digits, separators, month
 * names), so nothing a server or user wrote reaches `Tooltip`'s `innerHTML`.
 */
export function divineRateView(snap: DivineRateSnapshot, now: number, timeZone?: string): DivineRateView {
	const state = deriveDivineRateState(snap);
	const rate = snap.divineChaosRate;
	if (state === 'cold' || rate === null) {
		return {
			state: 'cold',
			chip: COLD_CHIP,
			tooltipHtml: lines('No divine rate yet', 'Waiting for the first Currency Exchange hour.')
		};
	}
	const head = `1 divine = ${formatTooltipRate(rate)} chaos`;
	if (state === 'stale') {
		const from = snap.priceHour ? formatClock(snap.priceHour, timeZone) : COLD_CHIP;
		const since = snap.unreachableSince ? formatClock(snap.unreachableSince, timeZone) : COLD_CHIP;
		return {
			state,
			chip: formatChipRate(rate),
			tooltipHtml: lines(`${head} (stale)`, `Last known rate from ${from}; server unreachable since ${since}.`)
		};
	}
	const hour = snap.priceHour ? formatPriceHour(snap.priceHour, timeZone) : COLD_CHIP;
	const age = snap.updatedAt ? ` · ${formatAge(snap.updatedAt, now)}` : '';
	return {
		state,
		chip: formatChipRate(rate),
		tooltipHtml: lines(head, `Currency Exchange, hour of ${hour}${age}`)
	};
}
