/**
 * Which guides take part in the verdict.
 *
 * Until POE-199 this was an ADR-013 `persisted()` string owned by the
 * Mercenaries page. It is now Rust's: the value reaches every window as
 * `ssot.mercenary.sourcesOff` and is written through `setMercSourcesOff`.
 * The reason is the verdict overlay — the page and the overlay evaluate the
 * SAME capture, and a prefs map that is fetched once per webview and written
 * back with no notification let the two print different headlines for one
 * mercenary. ADR-013's boundary rule was already the answer: a value two
 * surfaces read is not a view preference.
 *
 * What is left here is the reading of that list, in one place so no consumer
 * re-derives the inversion or the unknown-id rule.
 */

import { SOURCE_IDS, type MercSourceId } from './rulesets';

/**
 * Read the off-list as the SSOT delivers it.
 *
 * Unknown ids are dropped rather than kept: Rust validates what it STORES, but
 * the snapshot still crosses a version boundary — an older app polling a newer
 * Rust (or a settings file hand-edited between builds) can carry an id this
 * webview has no rules for, and such an id must not be able to switch off a
 * guide that now spells its name differently.
 *
 * The list is stored as the OFF set so a guide added to `SOURCE_IDS` later
 * starts enabled for everyone instead of silently off for every install.
 */
export function parseSourcesOff(raw: readonly string[] | undefined): Set<MercSourceId> {
	const known = new Set<string>(SOURCE_IDS);
	return new Set((raw ?? []).filter((id): id is MercSourceId => known.has(id)));
}

/** The guides still switched on — what the verdict engine wants. */
export function enabledSources(raw: readonly string[] | undefined): Set<MercSourceId> {
	const off = parseSourcesOff(raw);
	return new Set(SOURCE_IDS.filter((id) => !off.has(id)));
}

/**
 * The off-list a toggle produces, in `SOURCE_IDS` order.
 *
 * Ordering here as well as in Rust is not duplication for its own sake: this is
 * what the page SENDS, and sending a stable value keeps a no-op toggle from
 * looking like a change to anyone reading the settings file. Rust normalises
 * again because it must not trust a caller.
 */
export function withSourceEnabled(
	raw: readonly string[] | undefined,
	id: MercSourceId,
	on: boolean
): string[] {
	const off = parseSourcesOff(raw);
	if (on) off.delete(id);
	else off.add(id);
	return SOURCE_IDS.filter((candidate) => off.has(candidate));
}
