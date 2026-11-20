import type { CurrencyExchangeResponse } from '$lib/api';
import { MERCURE_REFETCH_DEBOUNCE_MS, MERCURE_REFETCH_JITTER_MS } from '$lib/stores/mercure.svelte';

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
	/** Structural: CX passes its response; Harvest passes its two owners combined (POE-283). */
	result: { warm: boolean; lastUpdated: string | null } | null;
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

// ------------------------------------------------------------ the refetch --

/*
 * Mercure refetch timing lives in `$lib/stores/mercure.svelte`
 * (`MERCURE_REFETCH_DEBOUNCE_MS`/`MERCURE_REFETCH_JITTER_MS`), shared with the
 * lab controller.
 */

/**
 * How long to wait before a Mercure-triggered refetch.
 *
 * Re-roll it on every fire rather than once per session: a fixed per-client
 * offset would land the same clients in the same slot on every publish, which
 * spreads the herd once instead of on each tick. `random` is injectable so the
 * spread is testable.
 */
export function refetchDelay(random: () => number = Math.random): number {
	return MERCURE_REFETCH_DEBOUNCE_MS + random() * MERCURE_REFETCH_JITTER_MS;
}
