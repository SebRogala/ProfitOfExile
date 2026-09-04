/**
 * Trade links for mercenary rulesets — the saved search and the derived one.
 *
 * Two different addressing schemes live here on purpose. A ruleset's own search
 * is a *saved* search, addressed by the hash GGG assigned it, so the URL is a
 * bare path with no query string (`savedSearchUrl`). The derived search is the
 * same ruleset rebuilt from the data model with the verdict's toggles flipped,
 * so it has to travel as an encoded query (`rulesetQuery` + `derivedSearchUrl`)
 * — GGG never assigned it a hash. The saved link stays the primary one; the
 * derived link is what you open to comp a specific mercenary.
 *
 * A ruleset transcribed from PROSE has no saved search at all (guide-c), so it
 * gets the derived link and nothing else — see `MercAuthoredQuery`.
 *
 * Distinct from `lib/trade-utils.ts`, which builds gem searches.
 */

import type { MercFilterEntry, MercFilterGroup, MercRuleset } from './rulesets';

/** The league + hash pair identifying one GGG saved search. */
export interface MercSavedSearch {
	league: string;
	hash: string;
}

/**
 * A query this app WROTE, from a guide whose author published prose instead of
 * trade links — guide-c is the case: CaptainLance lists ideal skill and support
 * combinations and saves no searches.
 *
 * It names a committed fixture file rather than a GGG hash, and that difference
 * is not cosmetic. A saved search is re-fetchable and is its own oracle; an
 * authored query's fixture is OUR transcription of the prose, so the only thing
 * it can prove is that the typed data model still says what it said when a human
 * checked it against the guide. Carrying both in one hash-shaped string would
 * let `savedSearchUrl` build a trade link to a hash GGG never issued, so the two
 * live in different fields and `MercRuleset` carries exactly one of them.
 */
export interface MercAuthoredQuery {
	/** `__fixtures__/<file>.json`, without the directory or the extension. */
	file: string;
}

/**
 * Build the trade-site URL for a saved search.
 * `league` MUST be a resolved league — there is no default-league fallback here,
 * on purpose: a saved search belongs to the league it was saved in, and guessing
 * would silently point at a different market.
 */
export function savedSearchUrl(savedSearch: MercSavedSearch): string {
	return `https://www.pathofexile.com/trade/search/${encodeURIComponent(savedSearch.league)}/${savedSearch.hash}`;
}

/** One stat filter as the trade site serialises it. `disabled` is absent when the filter is on. */
export interface TradeStatFilter {
	id: string;
	disabled?: boolean;
}

/** One stat group. `value.min` is absent when the group has no minimum. */
export interface TradeStatGroup {
	type: string;
	value?: { min: number };
	disabled?: boolean;
	filters: TradeStatFilter[];
}

/**
 * The `query` object of a trade search — the same shape the saved-search
 * fixtures carry under their own `query` key. No `sort`: none of the
 * twenty-seven saved searches has one, and adding one here would make the
 * derived search order differently from the search it was derived from.
 *
 * Both filter blocks are optional because the app builds queries from two
 * places and each names one of them: `rulesetQuery` below carries a guide's
 * `ilvl` floor, and Rust's captured-mercenary query
 * (`mercenary/search.rs::build_capture_query`) carries the `priced` sale type
 * and no ilvl floor at all. `derivedSearchUrl` links both, so the type has to
 * describe both — see the parity fixture in `__fixtures__`.
 */
export interface TradeQuery {
	stats: TradeStatGroup[];
	status: { option: string };
	filters?: {
		misc_filters?: { filters: { ilvl: { min: number } } };
		trade_filters?: { filters: { sale_type: { option: string } } };
	};
}

/**
 * Per-entry overrides for `rulesetQuery`, keyed `<groupId>/<entryId>`.
 *
 * The verdict engine enables the bonuses a mercenary actually fired and the
 * buyer-contextual entries it actually has, and disables the contextual ones it
 * lacks — so the derived search comps THIS mercenary rather than the ruleset's
 * floor case. Group switches are never flipped: a parked `not` group stays
 * parked, and a group the guide switched off stays off.
 */
export interface QueryFlips {
	enable?: ReadonlySet<string>;
	disable?: ReadonlySet<string>;
	/**
	 * Group ids to switch ON — the one case where a group switch moves. A bonus
	 * the guide parked by switching its whole group off (guide B's Mid rung does
	 * this to Haste) cannot be comped through an entry flip alone, because the
	 * trade site ignores every filter of a disabled group. `not` groups are never
	 * in here: a parked denial stays parked.
	 *
	 * A revived group's `min` is clamped by `rulesetQuery` — see the note there.
	 */
	enableGroups?: ReadonlySet<string>;
}

/** Flip key for one entry — the same key the verdict engine emits. */
export function flipKey(groupId: string, entryId: string): string {
	return `${groupId}/${entryId}`;
}

/**
 * A group stays as the guide left it unless the flips switch it on — and a
 * denial group is never switched on, whatever the flips say.
 */
function groupEnabled(group: MercFilterGroup, flips: QueryFlips | undefined): boolean {
	if (group.enabledInSearch) return true;
	return group.type !== 'not' && flips?.enableGroups?.has(group.id) === true;
}

function entryEnabled(
	group: MercFilterGroup,
	entry: MercFilterEntry,
	flips: QueryFlips | undefined
): boolean {
	// A `not` group's entries are the guide's denial list; flipping one on or
	// off would change what the search rejects, not what it comps.
	if (!flips || group.type === 'not') return entry.enabledInSearch;
	const key = flipKey(group.id, entry.id);
	if (flips.enable?.has(key)) return true;
	if (flips.disable?.has(key)) return false;
	return entry.enabledInSearch;
}

/**
 * Rebuild a ruleset's trade query from the data model.
 *
 * The output is the canonical form — a switched-on filter carries no `disabled`
 * key at all — and this builder does NOT normalise: it never emits
 * `disabled: false`, and it never reorders or drops anything the ruleset
 * declares. The saved searches themselves are inconsistent about spelling out
 * `disabled: false`, so the round-trip test owns the normaliser that makes both
 * sides comparable; putting that normalisation here would let the builder
 * launder a transcription error into a match.
 *
 * ONE number moves, on any group the flips ALTERED — revived from parked, or
 * left live with an entry toggled. Both cases drop filters the guide's `min` was
 * written for, so the group can end up asking for more filters than it still has
 * switched on: a search the trade site can never answer. Measured 2026-08-26 on
 * the Manyshot GG rung's parked `projectiles` group (`min: 4` over the three
 * filters that survived revival); a live group whose buyer-contextual entry
 * switches off reaches the same dead end by the other door. An altered group's
 * `min` is therefore clamped DOWN to its enabled-filter count.
 *
 * Down, never up. The derived link asks for "mercenaries like this one", so the
 * floor is what this one actually carries; raising a `min` would comp against a
 * mercenary nobody captured.
 *
 * A group the flips did not touch keeps the guide's own number even where that
 * number already exceeds what the search has switched on — that is the guide's
 * saved search, faithfully reproduced, and not this builder's to correct. A
 * group with no `min` stays without one. So a no-flips call, which is what the
 * fixture-fidelity tests make, is byte-for-byte the saved search.
 */
export function rulesetQuery(ruleset: MercRuleset, flips?: QueryFlips): TradeQuery {
	const stats: TradeStatGroup[] = ruleset.groups.map((group) => {
		const entryStates = group.entries.map((entry) => ({
			entry,
			on: entryEnabled(group, entry, flips)
		}));
		const filters = entryStates.map(({ entry, on }) =>
			on ? { id: entry.id } : { id: entry.id, disabled: true }
		);
		const enabled = groupEnabled(group, flips);
		const altered =
			enabled !== group.enabledInSearch ||
			entryStates.some(({ entry, on }) => on !== entry.enabledInSearch);
		const built: TradeStatGroup = { type: group.type, filters };
		if (group.min !== undefined) {
			const on = entryStates.filter((state) => state.on).length;
			built.value = { min: altered ? Math.min(group.min, on) : group.min };
		}
		if (!enabled) built.disabled = true;
		return built;
	});

	const query: TradeQuery = { stats, status: { option: ruleset.status } };
	if (ruleset.ilvlMin !== undefined) {
		query.filters = { misc_filters: { filters: { ilvl: { min: ruleset.ilvlMin } } } };
	}
	return query;
}

/**
 * Build a trade-site URL for a query the app assembled itself.
 *
 * The `q` parameter carries the request BODY, not the bare query — the trade
 * site reads `{"query": ...}` (same envelope as `lib/trade-utils.ts` sends, and
 * the same key the saved-search responses store their query under). `league`
 * MUST be resolved, for the reason `savedSearchUrl` gives.
 */
export function derivedSearchUrl(league: string, query: TradeQuery): string {
	const body = JSON.stringify({ query });
	return `https://www.pathofexile.com/trade/search/${encodeURIComponent(league)}?q=${encodeURIComponent(body)}`;
}
