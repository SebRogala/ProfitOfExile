/**
 * Wording for the captured mercenary's own trade search (POE-202).
 *
 * The same split as `capture-view.ts`: the page renders, this file decides what
 * the words are. It exists as its own module rather than as functions in the
 * page because the merc overlay is expected to render the same state later
 * (`docs/OVERLAY-GUIDE.md`), and two surfaces wording one status differently is
 * the failure this repo already paid for once with the verdict headline.
 *
 * Pure and total: every `MercTradeStatus` has a label, and nothing here reads
 * the clock, the store, or the DOM.
 */

import { formatListingAmount, type MercTradeResult } from '$lib/tradeApi';
import type { MercTradeState } from './capture';
import type { OutcomeTone } from './capture-view';

/**
 * The badge next to the section title — what the search is DOING.
 *
 * Distinct per status on purpose: this line is the only place a user can tell
 * "the app is not searching" (`idle`) from "the app cannot search yet"
 * (`waiting-league`) from "the app is waiting its turn" (`queued`). Collapsing
 * any two of them would make a bounded wait look like a broken feature.
 *
 * `cancelled` is not a failure — it is what retiring a capture mid-search
 * leaves behind (Rust's `CANCELLED`), so it is worded as the deliberate act it
 * is rather than as an error the user should go fix.
 */
export function tradeStatusLabel(state: MercTradeState): string {
	switch (state.status) {
		case 'off':
			return 'module off';
		case 'idle':
			return 'not searching';
		case 'waiting-league':
			return 'waiting for league';
		case 'queued':
			return 'queued';
		case 'searching':
			return 'searching';
		case 'done':
			return 'search done';
		case 'error':
			return isCancelled(state) ? 'search cancelled' : 'search failed';
	}
}

/**
 * The badge's colour bucket, on the page's own five-tone scale.
 *
 * `cancelled` is `muted`, not `fail`, for the same reason an unreachable
 * template pool is (`poolSyncView`): retiring a capture mid-search is the user
 * ending the question, and painting it red sends them hunting for a break that
 * did not happen. A real error is the only `fail` here — a missing league is
 * `unknown`, because the app is waiting on the game, not failing.
 */
export function tradeStatusTone(state: MercTradeState): OutcomeTone {
	switch (state.status) {
		case 'off':
		case 'idle':
			return 'muted';
		case 'waiting-league':
		case 'queued':
		case 'searching':
			return 'unknown';
		case 'done':
			return 'pass';
		case 'error':
			return isCancelled(state) ? 'muted' : 'fail';
	}
}

/**
 * Rust's per-capture search ceiling, mirrored for the wording that quotes it.
 *
 * The owner is `mercenary/search.rs`'s `MAX_SEARCHES` — Rust decides when the
 * budget is spent (`TriggerAction::UrlOnly`), and this constant only lets the
 * page say the same number Rust enforced instead of hard-coding a second `3`
 * next to `searchesUsed`.
 */
export const MERC_TRADE_MAX_SEARCHES = 3;

/**
 * The one line above the listings — what the search FOUND, or why it has not.
 *
 * `null` means "print nothing", and the three waiting states use it: the badge
 * already says `waiting for league` / `queued` / `searching`, and repeating the
 * badge one line lower is noise, not information.
 *
 * `off` and `idle` share an arm because they share a slice. Neither status is
 * a fresh answer, but both can be sitting on a retained one:
 * `compose_snapshot` forces only the STATUS to `off` and leaves `result` and
 * `url` alone, and Rust's `Idle` is reached three ways — the auto-search
 * toggled off (result kept), the search budget spent (`UrlOnly`: url kept,
 * result dropped), and no expressible query (everything cleared). So the arm
 * branches on what the slice actually holds rather than on the status name.
 *
 * The price is the cheapest listing's RAW seller amount and currency, taken
 * from `listings[0]` — the query sorts `price asc`, so the first row IS the
 * floor. `floorChaos` is deliberately not used: it is a chaos normalisation,
 * and this page has no divine rate to undo it with, so quoting it would print a
 * number no seller ever asked for.
 */
export function tradeHeadline(state: MercTradeState): string | null {
	switch (state.status) {
		case 'off':
		case 'idle':
			return retainedHeadline(state);
		case 'waiting-league':
		case 'queued':
		case 'searching':
			return null;
		case 'error':
			if (isCancelled(state)) return 'search cancelled';
			return state.error ?? 'search failed';
		case 'done':
			// Defensive: Rust sets `done` and the result in one write. A `done`
			// with nothing to show is a bug elsewhere, not a headline.
			return state.result === null ? null : resultHeadline(state.result);
	}
}

/** What an `off` or `idle` slice still has to say. A retained result outranks
 *  the budget note: the listings ARE the answer the budget bought. */
function retainedHeadline(state: MercTradeState): string | null {
	if (state.result !== null) return resultHeadline(state.result);
	if (state.url !== null && state.searchesUsed >= MERC_TRADE_MAX_SEARCHES) {
		return 'search budget spent — link still live';
	}
	return null;
}

/** `total` is GGG's count for the query; `listings` is the fetched page of it,
 *  so the count comes from `total` and the price clause only from a row that
 *  exists. `total > 0` with an empty page is a fetch that returned nothing to
 *  quote — still not "none found", which is a claim about the market. */
function resultHeadline(result: MercTradeResult): string {
	const found = describeFound(result);
	// `truncated` means the complexity budget cost the query support cells the
	// capture read, so these listings answer a LOOSER question than the panel
	// showed — worth saying as a caveat about the query, not as an apology for
	// the search. Dropping the app's own tier loosening does NOT set it: what
	// survives that asks for exactly what was read.
	return result.truncated ? `${found} · looser query — fewer filters than the capture` : found;
}

function describeFound(result: MercTradeResult): string {
	if (result.total === 0) return 'none found';
	const count = `${result.total} ${result.total === 1 ? 'listing' : 'listings'}`;
	const cheapest = result.listings[0];
	if (cheapest === undefined) return count;
	return `${count} · from ${formatListingAmount(cheapest.amount)} ${cheapest.currency}`;
}

/** Rust's `CANCELLED` marker (`trade/client.rs`), the one error string that is
 *  not a failure. */
function isCancelled(state: MercTradeState): boolean {
	return state.error === 'cancelled';
}
