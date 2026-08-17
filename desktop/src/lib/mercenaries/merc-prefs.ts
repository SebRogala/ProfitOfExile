/**
 * Mercenaries page preferences.
 *
 * Separate from `capture.ts` (which mirrors Rust-owned state) because this is
 * webview-owned: an ADR-013 `persisted()` string the page reads and writes. The
 * key, the parse and the inversion live together so no consumer re-derives any
 * of the three.
 */

import { SOURCE_IDS, type MercSourceId } from './rulesets';

/**
 * Preference key holding the sources the user switched OFF, comma-separated.
 *
 * Stored as the off-list rather than the on-list so a source added to
 * `SOURCE_IDS` later starts enabled for everyone. ADR-013 keys are camelCase
 * and surface-prefixed, which is why the source ids (`guide-a`) live in the
 * VALUE and never in the key.
 */
export const MERC_SOURCES_OFF_PREF_KEY = 'mercSourcesOff';

/**
 * Read the persisted off-list. Unknown ids are dropped rather than kept:
 * the value outlives the code that wrote it, and a stale id must not be able
 * to switch off a source that now has a different name.
 */
export function parseSourcesOff(raw: string): Set<MercSourceId> {
	const known = new Set<string>(SOURCE_IDS);
	const off = raw
		.split(',')
		.map((part) => part.trim())
		.filter((part) => known.has(part)) as MercSourceId[];
	return new Set(off);
}

/** The sources still switched on — what the verdict engine wants. */
export function enabledSources(raw: string): Set<MercSourceId> {
	const off = parseSourcesOff(raw);
	return new Set(SOURCE_IDS.filter((id) => !off.has(id)));
}
