/**
 * The guide ids exist twice — once as the data model here, once as Rust's
 * validation list — and neither side can see the other.
 *
 * `merc_set_sources_off` refuses an id that is not in `sources::SOURCE_IDS`, and
 * `apply_to_state` DROPS one it does not know when loading the settings file. So
 * a guide added to `rulesets.ts` alone would have a toggle the page can flip and
 * Rust rejects; a guide removed from `rulesets.ts` alone would keep being stored
 * and silently ignored. Neither fails to compile, and neither fails any
 * behaviour test — reading the Rust source is the only place the two meet.
 *
 * Same mechanism as `overlay/manager.test.ts`: Vite's `?raw`, because this app
 * has no `@types/node`.
 */
import { describe, expect, it } from 'vitest';
import sourcesRs from '../../../src-tauri/src/mercenary/sources.rs?raw';
import { SOURCE_IDS } from './rulesets';

/**
 * The ids the `SOURCE_IDS` const declares, in declaration order.
 *
 * Scoped to that const rather than to the file: `sources.rs` also spells guide
 * ids in its own tests, and a whole-file match would let this pass on a test
 * fixture while the real list had changed. A const this regex cannot find
 * yields no ids at all, so the assertion fails rather than degrading.
 */
function rustSourceIds(): string[] {
	const literal = sourcesRs.match(/pub const SOURCE_IDS:[^=]*=\s*&\[([^\]]*)\]/)?.[1] ?? '';
	return [...literal.matchAll(/"([^"]+)"/g)].map((m) => m[1]);
}

describe('the guide ids', () => {
	it('are the same list on both sides of the IPC boundary', () => {
		// Order included: both sides normalise the stored off-list into it, so a
		// divergence would make the page and Rust write different values for the
		// same choice.
		expect(rustSourceIds()).toEqual([...SOURCE_IDS]);
	});
});
