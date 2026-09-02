/**
 * The rule that decides whether a hot-rect declaration also has to move the
 * window's `has_content` flag.
 *
 * The flag is not decoration. `overlay_hook::hit_test` returns `None` for a
 * window whose `has_content` is false without looking at one rect, and a
 * registry entry starts false — so a widget host that declared rects and never
 * touched the flag would have every button it draws swallowed by the game, with
 * nothing anywhere reporting a failure. That is a Windows-only path with no
 * harness, which is why the decision is a function rather than an `if` in the
 * action.
 */
import { describe, expect, it } from 'vitest';
import actionSource from './use-hot-rects.ts?raw';
import { hasContentTransition } from './use-hot-rects';

describe('whether a declaration moves the has_content flag', () => {
	it('arms the flag when the first rect appears', () => {
		expect(hasContentTransition(0, 1)).toBe(true);
	});

	it('clears the flag when the last rect goes away', () => {
		expect(hasContentTransition(2, 0)).toBe(false);
	});

	// The common case by a wide margin: the host re-measures on every animation
	// frame a mutation touches, and a button that moved a pixel has changed its
	// rect and not its content. Re-asserting the flag there would put an IPC
	// call behind every frame the overlay runs.
	it('says nothing when the count changed but the emptiness did not', () => {
		expect(hasContentTransition(2, 3)).toBeNull();
	});

	it('says nothing when there was nothing before and nothing now', () => {
		expect(hasContentTransition(0, 0)).toBeNull();
	});
});

describe('the action that consumes it', () => {
	// A source assertion because the action needs a DOM and this suite runs on
	// node: what it pins is the wiring the pure function above cannot reach —
	// that the flag is declared at all, and under the same label as the rects.
	// Deleting either call is what the FATAL finding was.
	it('declares the flag alongside the rects, for the same window label', () => {
		expect(actionSource).toContain("invoke('set_overlay_has_content', { label: current.module,");
		expect(actionSource).toContain("invoke('set_overlay_hot_rects', { label: current.module,");
	});

	it('clears the flag when the host is torn down', () => {
		const destroy = actionSource.slice(actionSource.indexOf('destroy()'));
		expect(destroy).toContain('setHasContent(false)');
	});
});
