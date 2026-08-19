import { describe, it, expect } from 'vitest';
import { nav, viewToPath, VIEW_PATHS, type View } from './navigation.svelte';

/**
 * `viewToPath` and `go` are two halves of one mapping, and the round-trip alone
 * cannot catch a wrong-but-absorbed path: if `viewToPath('mercenaries')` returned
 * '/mercs', `go` would fall through to 'lab' — a round-trip test would fail, but
 * one that only pinned `go('/mercenaries')` would not. So the exact string is
 * pinned per view as well.
 *
 * `nav` is module-level mutable state. Every test below seeds `nav.view`
 * directly rather than through `go`, so the seed cannot be broken by the same
 * production change the assertion is meant to catch.
 */

/** Put the store on a view other than `view`, so a pass proves the write happened. */
function seedAwayFrom(view: View): void {
	nav.view = view === 'lab' ? 'settings' : 'lab';
}

describe('viewToPath', () => {
	it("maps 'lab' to '/' — the root path, not '/lab'", () => {
		expect(viewToPath('lab')).toBe('/');
	});

	it("maps 'settings' to '/settings'", () => {
		expect(viewToPath('settings')).toBe('/settings');
	});

	it("maps 'dev' to '/dev'", () => {
		expect(viewToPath('dev')).toBe('/dev');
	});

	it("maps 'mercenaries' to '/mercenaries'", () => {
		expect(viewToPath('mercenaries')).toBe('/mercenaries');
	});

	it("maps 'temple' to '/temple'", () => {
		expect(viewToPath('temple')).toBe('/temple');
	});
});

describe('go', () => {
	// Derived from the source table, not hand-listed: a View added with a
	// VIEW_PATHS entry but no go() branch fails here without anyone having to
	// remember to extend this file.
	const views = Object.keys(VIEW_PATHS) as View[];

	it.each(views)("selects '%s' from the path viewToPath gives for it", (view) => {
		seedAwayFrom(view);
		nav.go(viewToPath(view));
		expect(nav.view).toBe(view);
	});

	it("falls back to 'lab' for a path no view claims", () => {
		seedAwayFrom('lab');
		nav.go('/mercs');
		expect(nav.view).toBe('lab');
	});

	it("falls back to 'lab' for the empty path", () => {
		seedAwayFrom('lab');
		nav.go('');
		expect(nav.view).toBe('lab');
	});
});
