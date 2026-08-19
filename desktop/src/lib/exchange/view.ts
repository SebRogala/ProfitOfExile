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
	CurrencyExchangeLeg,
	CurrencyExchangeMode,
	CurrencyExchangeResponse
} from '$lib/api';

/**
 * Re-exported from `$lib/api` so the page can import its mode type from the
 * same module as the helpers that consume it. One definition, two entry
 * points — the fetcher and the picker cannot drift.
 */
export type { CurrencyExchangeMode };

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

// ------------------------------------------------------------ the numbers --

/**
 * An edge fraction as a signed percentage: `0.1234` → `"+12.3%"`,
 * `-0.05` → `"-5.0%"`.
 *
 * The sign is always explicit — a bare "12.3%" next to a "-5.0%" reads as an
 * absolute price rather than a delta. The sign is taken from the *rounded*
 * magnitude, so an edge that rounds to nothing prints "+0.0%" instead of the
 * nonsense "-0.0%".
 */
export function formatEdge(edge: number): string {
	const percent = edge * 100;
	const magnitude = Math.abs(percent).toFixed(1);
	const sign = Number(magnitude) !== 0 && percent < 0 ? '-' : '+';
	return `${sign}${magnitude}%`;
}

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
