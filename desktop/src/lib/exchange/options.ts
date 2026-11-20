import type { CurrencyExchangeHorizon, CurrencyExchangeMode } from '$lib/api';

// ------------------------------------------------------------ the filter --

/** The mode picker's entries, in display order, for `SegmentedButtons`. */
export const MODE_OPTIONS: { value: CurrencyExchangeMode; label: string }[] = [
	{ value: 'all', label: 'All' },
	{ value: 'direct', label: 'Direct' },
	{ value: '1-hop', label: '1-hop' }
];

/**
 * Narrow a persisted or user-supplied string to a mode.
 *
 * Anything unrecognised — an empty preference, a value written by an older
 * build, a mode the server has since dropped — becomes `'all'` rather than
 * reaching the API and coming back as a 400.
 */
export function parseMode(raw: string): CurrencyExchangeMode {
	return MODE_OPTIONS.some((option) => option.value === raw)
		? (raw as CurrencyExchangeMode)
		: 'all';
}

/**
 * The horizon picker's entries. The labels carry the window length because
 * "Recent" and "Day" alone do not say how much history ranked the list, and
 * that length is what the Hours column counts against.
 */
export const HORIZON_OPTIONS: { value: CurrencyExchangeHorizon; label: string }[] = [
	{ value: 'recent', label: 'Recent 6h' },
	{ value: 'day', label: 'Day 24h' }
];

/**
 * Narrow a persisted or user-supplied string to a horizon.
 *
 * `'recent'` is both the API default and the fallback, so a preference written
 * by a build that predates the horizon toggle resolves to the window the page
 * has always fetched rather than silently widening it.
 */
export function parseHorizon(raw: string): CurrencyExchangeHorizon {
	return HORIZON_OPTIONS.some((option) => option.value === raw)
		? (raw as CurrencyExchangeHorizon)
		: 'recent';
}

/**
 * Which number the table is ordered by — and, since POE-220, ONLY that number:
 * each option names a column and orders by the figure that column prints, with
 * no flag partitioning the list underneath it. The server sorts (ADR-030), on
 * the Go-computed `display.sortKeys`.
 *
 * `'expected'` reads the fill-simulated Exp. ROI column (ADR-016), which is the
 * measured question; `'roi'` the BEST-CASE chaos the ROI column prints, which is
 * a different one — a 40% best case on 2c is not the play a stocked account
 * wants; and `'fastest'` how long the market needs to absorb the play's
 * worthwhile scale (`worthwhileScale().hours`), which is the question a big
 * return on a thin market answers badly.
 */
export type ExchangeSort = 'expected' | 'roi' | 'fastest';

/** The sort picker's entries, in display order. */
export const SORT_OPTIONS: { value: ExchangeSort; label: string }[] = [
	{ value: 'expected', label: 'Exp. ROI' },
	{ value: 'roi', label: 'ROI' },
	{ value: 'fastest', label: 'Fastest' }
];

/**
 * Narrow a persisted or user-supplied string to a sort; default `'expected'`.
 *
 * Two older spellings map FORWARD rather than falling back, because in both
 * cases the order the reader picked still exists under a new name. `'fill'` is
 * what `'fastest'` was called while the table scaled by a typed Quantity
 * (POE-192 replaced that with the derived scale). `'roiPct'` is what the
 * default pick was called while the server ranked on the best-case percentage;
 * POE-193 re-based that ranking on `expectedRoi`, and the pick has always meant
 * "the table's headline order", so it resolves to `'expected'` rather than to a
 * percentage sort that no longer exists.
 *
 * Everything else — a mode from a build this one has never seen, an empty
 * preference — also answers `'expected'`, the measured number, which is the
 * order a reader who has picked nothing is best served by.
 */
export function parseSort(raw: string): ExchangeSort {
	if (raw === 'fill') return 'fastest';
	if (raw === 'roiPct') return 'expected';
	return SORT_OPTIONS.some((option) => option.value === raw) ? (raw as ExchangeSort) : 'expected';
}

/**
 * How much of each row is drawn. `'dense'` drops every sub-line and shrinks the
 * icons; the content the sub-lines carried moves into the cell tooltips rather
 * than disappearing.
 */
export type ExchangeDensity = 'comfortable' | 'dense';

/** The density picker's entries, in display order. */
export const DENSITY_OPTIONS: { value: ExchangeDensity; label: string }[] = [
	{ value: 'comfortable', label: 'Comfortable' },
	{ value: 'dense', label: 'Dense' }
];

/**
 * Narrow a persisted or user-supplied string to a density; default
 * `'comfortable'`, which is the layout every sub-line's copy was written for.
 */
export function parseDensity(raw: string): ExchangeDensity {
	return DENSITY_OPTIONS.some((option) => option.value === raw)
		? (raw as ExchangeDensity)
		: 'comfortable';
}

/** Which currency the investment bounds are typed in. */
export type ExchangeUnit = 'chaos' | 'divine';

/** The unit picker's entries, in display order. */
export const UNIT_OPTIONS: { value: ExchangeUnit; label: string }[] = [
	{ value: 'chaos', label: 'Chaos' },
	{ value: 'divine', label: 'Divine' }
];

/**
 * Narrow a persisted or user-supplied string to a unit; default `'chaos'`,
 * which is the currency every wire number is already denominated in and the
 * only one that stays meaningful when `divineChaosRate` is 0.
 */
export function parseUnit(raw: string): ExchangeUnit {
	return UNIT_OPTIONS.some((option) => option.value === raw) ? (raw as ExchangeUnit) : 'chaos';
}

/**
 * How much of the ranked list the table draws at once. `'all'` is the cap off.
 *
 * A cap and not page numbers, by owner ruling (POE-222): the table is one
 * ranked list read from the top, and 954 rows under the shipped filters is a
 * scroll that has no end a reader ever reaches. Page numbers would ask them to
 * hold a position in a list whose order changes every hour; a cap only says how
 * much of the head is on screen, and says it out loud (`rowCounts`).
 */
export type ExchangeRowCap = '25' | '50' | '100' | '200' | 'all';

/** The row-cap picker's entries, in display order. */
export const ROW_CAP_OPTIONS: { value: ExchangeRowCap; label: string }[] = [
	{ value: '25', label: '25' },
	{ value: '50', label: '50' },
	{ value: '100', label: '100' },
	{ value: '200', label: '200' },
	{ value: 'all', label: 'All' }
];

/**
 * Narrow a persisted or user-supplied string to a row cap; default `'50'`.
 *
 * A number rather than `'all'`, because the first screen is what an unset
 * preference is asking for — and unlike a filter default, this one is allowed
 * to be a number precisely because it hides nothing the counter does not name
 * and the "show all" beside that counter does not undo in one click.
 */
export function parseRowCap(raw: string): ExchangeRowCap {
	return ROW_CAP_OPTIONS.some((option) => option.value === raw) ? (raw as ExchangeRowCap) : '50';
}

/**
 * The pick as a row count; `'all'` is `Infinity`. The request sends the pick
 * itself as `pageSize` (`exchangePageParams`).
 *
 * Kept apart from `parseRowCap` because the two answers have different jobs:
 * the picker is SET to a string it has an entry for, and the slice takes a
 * number. Folding them would leave the control reading `Infinity`, which is on
 * no option.
 */
export function rowCapLimit(cap: ExchangeRowCap): number {
	return cap === 'all' ? Infinity : Number(cap);
}
