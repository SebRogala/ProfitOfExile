/**
 * Presentation derivations for the Currency Exchange page (POE-176).
 *
 * The sibling of `temple/view.ts` and `mercenaries/ladder-view.ts`, and for
 * the same reason: a `.svelte` file has no unit-test harness in this app, so
 * everything the page would otherwise compute inline lives here as pure
 * functions over the wire types in `$lib/api`.
 *
 * It decides nothing about the market. The ranking, the edge and the depth are
 * the server's (POE-175); this file only words them, and turns the page's four
 * loose variables (result / lastFetchedAt / lastError / now) into the single
 * state the header renders.
 *
 * Pure TypeScript on purpose — no Svelte runes, no Tauri imports — so the
 * whole file is reachable from vitest without a component harness.
 */
import type {
	CurrencyExchangeHorizon,
	CurrencyExchangeLeg,
	CurrencyExchangeMode,
	CurrencyExchangePlay,
	CurrencyExchangeResponse
} from '$lib/api';

/**
 * Re-exported from `$lib/api` so the page can import its wire enums from the
 * same module as the helpers that consume them. One definition, two entry
 * points — the fetcher and the pickers cannot drift.
 */
export type { CurrencyExchangeMode, CurrencyExchangeHorizon };

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
 * Which number the table is ordered by. `'roiPct'` is the server's own
 * ranking; `'roi'` re-orders by chaos gained per exchange, which is a different
 * question — a 40% return on 2c is not the play a stocked account wants.
 */
export type ExchangeSort = 'roiPct' | 'roi';

/** The sort picker's entries, in display order. */
export const SORT_OPTIONS: { value: ExchangeSort; label: string }[] = [
	{ value: 'roiPct', label: 'ROI%' },
	{ value: 'roi', label: 'ROI' }
];

/** Narrow a persisted or user-supplied string to a sort; default `'roiPct'`. */
export function parseSort(raw: string): ExchangeSort {
	return SORT_OPTIONS.some((option) => option.value === raw) ? (raw as ExchangeSort) : 'roiPct';
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

/** How many exchanges the row's figures are multiplied by when nothing is set. */
export const DEFAULT_QUANTITY = 1;

/**
 * Narrow a persisted or user-supplied string to a repeat count.
 *
 * Whole exchanges only, and never below one: the quantity multiplies
 * investment, ROI and the depth comparison, so a `0` would flatten the whole
 * table to zero and a fraction would claim a partial exchange the book cannot
 * fill. A typed-in decimal truncates rather than rejecting, because the stepper
 * writes while the user is still typing.
 */
export function parseQuantity(raw: string): number {
	const value = Number(raw);
	if (!Number.isFinite(value) || value < DEFAULT_QUANTITY) return DEFAULT_QUANTITY;
	return Math.floor(value);
}

// ------------------------------------------------------------- the state --

/**
 * What the header says about the data below it.
 *
 * - `loading` — nothing fetched yet and nothing has failed.
 * - `warming` — the server answered, but no Currency Exchange hour has closed.
 * - `ready` — a warm result is on screen.
 * - `stale` — a result is on screen but the latest fetch failed; the table stays.
 * - `unreachable` — the fetch failed and there is nothing to fall back on.
 */
export type ViewStateKind = 'loading' | 'warming' | 'ready' | 'stale' | 'unreachable';

/** The four page variables `deriveState` reads. */
export interface ViewStateInput {
	result: CurrencyExchangeResponse | null;
	lastFetchedAt: Date | null;
	lastError: string | null;
	now: Date;
}

/** The derived state, plus whichever relative string its kind displays. */
export interface ViewState {
	kind: ViewStateKind;
	/** `stale` only: how long ago the last successful fetch landed. */
	staleSince?: string;
	/** `ready` only: how long ago the server's data was computed. */
	updatedAgo?: string;
}

/**
 * Collapse result / error / timestamps into the one state the header renders.
 *
 * An error outranks `warming`: a stale warm-up is still a connectivity
 * problem, and "waiting for the first hour" would hide the fact that the last
 * request failed. `stale` falls back to `now` when nothing recorded a fetch
 * time, which yields "just now" instead of an empty gap in the sentence.
 */
export function deriveState({ result, lastFetchedAt, lastError, now }: ViewStateInput): ViewState {
	if (!result) {
		return { kind: lastError ? 'unreachable' : 'loading' };
	}
	if (lastError) {
		return { kind: 'stale', staleSince: formatTimeAgo(lastFetchedAt ?? now, now) };
	}
	if (result.warm === false) {
		return { kind: 'warming' };
	}
	return {
		kind: 'ready',
		updatedAgo: result.lastUpdated === null ? undefined : formatTimeAgo(result.lastUpdated, now)
	};
}

// -------------------------------------------------------------- the time --

const MINUTE_MS = 60_000;
const HOUR_MS = 60 * MINUTE_MS;
const DAY_MS = 24 * HOUR_MS;

/**
 * A coarse "how long ago" for the header.
 *
 * Deliberately single-unit ("2 h ago", not "2 h 14 m ago"): the page re-derives
 * it from a ticking `now` every 30s, and the extra unit would only make that
 * flicker more visible. A `null` value, an unparseable date, and a timestamp in
 * the future are all rendered as their least alarming reading — `""` for the
 * first two, "just now" for a clock that runs ahead of the server's.
 */
export function formatTimeAgo(value: string | Date | null, now: Date): string {
	if (value === null) return '';
	const then = value instanceof Date ? value : new Date(value);
	const elapsed = now.getTime() - then.getTime();
	if (Number.isNaN(elapsed)) return '';
	if (elapsed < MINUTE_MS) return 'just now';
	if (elapsed < HOUR_MS) return `${Math.floor(elapsed / MINUTE_MS)} min ago`;
	if (elapsed < DAY_MS) return `${Math.floor(elapsed / HOUR_MS)} h ago`;
	return `${Math.floor(elapsed / DAY_MS)} d ago`;
}

/**
 * The absolute clock time behind a relative string, for the tooltip.
 *
 * Built from `getHours`/`getMinutes` rather than `toLocaleTimeString` so it is
 * 24-hour "HH:MM" in every locale — the header pairs it with a relative string
 * that is already English-only, and a stray "01:30 PM" would read as a
 * different quantity. Local time, since that is the clock the user farms by.
 */
export function formatTime(iso: string | null): string {
	if (iso === null) return '';
	const date = new Date(iso);
	if (Number.isNaN(date.getTime())) return '';
	return `${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`;
}

/** The clock reading and the relative age the status line leads with. */
export interface DataAge {
	/** `"as of 14:35"` — the local clock time the prices below were read at. */
	label: string;
	/** How long ago that was, in `formatTimeAgo`'s wording. */
	ago: string;
}

/**
 * How old the table is, for the status line.
 *
 * Dated from `to` — the end of the settled hour the prices come from — and not
 * from `lastUpdated`, which is when the server computed the ranking: the feed
 * publishes 40-60 minutes after an hour closes, so the two differ by most of an
 * hour and only the first answers "how stale are these prices". `lastUpdated`
 * is the fallback for a body served without a window, and `null` (no timestamp
 * at all, or one that will not parse) means the page shows no badge rather than
 * an "as of :" with a hole in it.
 */
export function dataAgeParts(
	response: CurrencyExchangeResponse | null,
	now: Date
): DataAge | null {
	const stamp = response?.to ?? response?.lastUpdated ?? null;
	if (stamp === null) return null;
	const clock = formatTime(stamp);
	if (clock === '') return null;
	return { label: `as of ${clock}`, ago: formatTimeAgo(stamp, now) };
}

// ------------------------------------------------------------ the numbers --

/**
 * An ROI fraction as a signed percentage: `0.1234` → `"+12.3%"`,
 * `-0.05` → `"-5.0%"`.
 *
 * Exchange ROI arrives as a FRACTION (0.05 = +5%), unlike the gem side's
 * percentage points — this formatter multiplies, so pointing it at a gem number
 * inflates it a hundredfold.
 *
 * The sign is always explicit — a bare "12.3%" next to a "-5.0%" reads as an
 * absolute price rather than a delta. The sign is taken from the *rounded*
 * magnitude, so a return that rounds to nothing prints "+0.0%" instead of the
 * nonsense "-0.0%".
 */
export function formatRoiPct(roiPct: number): string {
	const percent = roiPct * 100;
	const magnitude = Math.abs(percent).toFixed(1);
	const sign = Number(magnitude) !== 0 && percent < 0 ? '-' : '+';
	return `${sign}${magnitude}%`;
}

/**
 * @deprecated Use `formatRoiPct`. Kept only so `CurrencyExchangePage.svelte`
 * keeps compiling until chunk 5 rewrites its cells; that chunk deletes this
 * alias together with the page's last `play.edge` read.
 */
export const formatEdge = formatRoiPct;

/**
 * A per-hour volume, abbreviated: `0`, `42`, `1.2k`, `13.0M`.
 *
 * Depth spans several orders of magnitude across currencies, and the column is
 * scanned for rank rather than read for an exact count — one decimal keeps the
 * cells the same width without collapsing 1.2k and 9.9k onto the same string.
 */
export function formatVolume(v: number): string {
	if (v >= 1_000_000) return `${(v / 1_000_000).toFixed(1)}M`;
	if (v >= 1000) return `${(v / 1000).toFixed(1)}k`;
	return String(Math.round(v));
}

/** Precision floor for sub-1 leg prices, in decimal places. */
const MIN_PRICE_DECIMALS = 2;
/** Significant digits kept for sub-1 leg prices. */
const PRICE_SIGNIFICANT_DIGITS = 4;

/**
 * A leg price at a precision that stays readable across the whole currency
 * range: `196`, `0.50`, `0.1429`, `0.004975`.
 *
 * Prices here are quote-per-item in arbitrary currencies, so there is no single
 * decimal count that works — a mirror-priced item needs none, and a fragment
 * priced in divine needs four leading zeros before the first real digit. Above
 * 1 the scale carries the meaning, so fixed decimals are enough; below 1 the
 * significant digits do, so the count floats with the exponent and trailing
 * zeros are trimmed back to a two-decimal floor ("0.5" would read as less
 * precise than it is).
 */
export function formatLegPrice(price: number): string {
	if (!Number.isFinite(price)) return '0';
	const magnitude = Math.abs(price);
	if (magnitude >= 100) return price.toFixed(0);
	if (magnitude >= 1) return price.toFixed(MIN_PRICE_DECIMALS);
	if (magnitude === 0) return price.toFixed(MIN_PRICE_DECIMALS);

	// One decimal place past the first significant digit, e.g. 0.004975 (first
	// digit at 1e-3) keeps six places, 0.1429 (first digit at 1e-1) keeps four.
	const firstDigitExponent = Math.floor(Math.log10(magnitude));
	const decimals = Math.min(PRICE_SIGNIFICANT_DIGITS - 1 - firstDigitExponent, 20);
	const trimmed = price.toFixed(decimals).replace(/0+$/, '');
	const fractionDigits = trimmed.length - trimmed.indexOf('.') - 1;
	return fractionDigits < MIN_PRICE_DECIMALS ? price.toFixed(MIN_PRICE_DECIMALS) : trimmed;
}

// -------------------------------------------------------------- the order --

/**
 * The table's rows in the order the sort picker asks for.
 *
 * `'roiPct'` is the list exactly as served: the server already ranks clean
 * before suspect, then `roiPct` desc, then turnover, then direct-first, then
 * key (POE-188). Re-sorting it here on `roiPct` alone would throw away those
 * tie-breaks, so the ROI% sort keeps the served order — copied, so a caller
 * holding the result can never mutate the response's own array through it.
 *
 * `'roi'` re-sorts by chaos per exchange, and keeps the one property the server
 * ordering exists to carry: every suspect play stays after every clean one,
 * however large its ROI. A suspect number is the reason it ranks last, so
 * letting it out-sort a clean play would hand the reader the very row the flag
 * warns about. Within a partition the sort is stable, so plays tied on `roi`
 * keep the server's remaining tie-breaks.
 */
export function sortPlays(
	plays: CurrencyExchangePlay[],
	sort: ExchangeSort
): CurrencyExchangePlay[] {
	if (sort === 'roiPct') return [...plays];
	return [...plays].sort((a, b) => {
		if (a.suspect !== b.suspect) return a.suspect ? 1 : -1;
		return b.roi - a.roi;
	});
}

// --------------------------------------------------------------- the legs --

/**
 * One leg as a sentence: `buy Mod Values with Reroll Rare @ 0.004975`.
 *
 * Wording follows the direction of the swap — you buy an item *with* a quote
 * currency and sell it *for* one — so a two-leg play reads as the sequence the
 * player performs. Display names only; the raw ids stay in the row's `title`
 * attribute, where they are available without cluttering the chip.
 */
export function legLabel(leg: CurrencyExchangeLeg): string {
	const preposition = leg.action === 'buy' ? 'with' : 'for';
	return `${leg.action} ${leg.itemName} ${preposition} ${leg.quoteName} @ ${formatLegPrice(leg.price)}`;
}

/**
 * A leg's `itemIcon`/`quoteIcon` as a URL the browser can fetch.
 *
 * The server sends API-relative paths (`/currency-exchange/icon/<escaped id>`)
 * rather than poewiki URLs, because production cannot reach poewiki (ADR-012)
 * and serves the artwork from its own cache instead. The join is here, not in
 * `ItemIcon.svelte`, so it is unit-testable and so the page reads the API base
 * once for every chip on screen.
 *
 * `null` and `""` both mean "no artwork" and both answer `null` — an empty path
 * joined onto the base would request the base itself, which answers with
 * something that is not an image. The trailing slash is trimmed off the base
 * because a `server_url` typed with one would otherwise produce `//` in the
 * path, which some proxies normalise and others 404.
 */
export function iconSrc(apiBase: string, path: string | null): string | null {
	if (!path) return null;
	const base = apiBase.replace(/\/+$/, '');
	return path.startsWith('/') ? `${base}${path}` : `${base}/${path}`;
}

// ------------------------------------------------------------ the refetch --

/**
 * Debounce window for a Mercure-triggered refetch. Collapses a burst of
 * publishes into a single request.
 */
export const REFETCH_DEBOUNCE_MS = 2000;

/**
 * Random spread added on top of the debounce. The debounce alone does not
 * disperse a herd, it aligns one: every client receives the same publish within
 * milliseconds, so a fixed delay puts all of them on `publish + 2000ms`
 * exactly. The offset scatters arrivals across 2–6s instead. Same two values as
 * `LabPage.svelte`'s `MERCURE_DEBOUNCE_MS`/`MERCURE_JITTER_MS`, against the
 * same server, and for the same reason.
 */
export const REFETCH_JITTER_MS = 4000;

/**
 * How long to wait before a Mercure-triggered refetch.
 *
 * Re-roll it on every fire rather than once per session: a fixed per-client
 * offset would land the same clients in the same slot on every publish, which
 * spreads the herd once instead of on each tick. `random` is injectable so the
 * spread is testable.
 */
export function refetchDelay(random: () => number = Math.random): number {
	return REFETCH_DEBOUNCE_MS + random() * REFETCH_JITTER_MS;
}
