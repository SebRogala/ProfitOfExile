/**
 * What a widget host's fresh measurement actually sends to Rust.
 *
 * Both calls fail invisibly, which is why the decision is a function rather
 * than an `if` inside a Svelte action with no harness. `set_overlay_hot_rects`
 * re-sent on every animation frame is an IPC call per frame for a window whose
 * buttons did not move; `set_overlay_has_content` left false has
 * `overlay_hook::hit_test` return `None` before it looks at one rect — a
 * registry entry starts false — so every button the host draws is swallowed by
 * the game with nothing anywhere reporting a failure. That is a Windows-only
 * path this suite cannot run.
 */
import { describe, expect, it } from 'vitest';
import { nextHotRectCalls } from './use-hot-rects';
import type { HotRect } from '../hot-rects';

const A: HotRect = { x: 10, y: 20, w: 100, h: 30 };
const MOVED: HotRect = { x: 11, y: 20, w: 100, h: 30 };
const B: HotRect = { x: 400, y: 20, w: 60, h: 30 };

describe('what a measurement has to invoke', () => {
	it('declares the rects and arms the flag when the first button appears', () => {
		expect(nextHotRectCalls(null, [A])).toEqual({ rects: [A], hasContent: true });
	});

	// The common case by a wide margin: the host re-measures on every frame a
	// mutation touches, and it re-renders on every SSOT poll.
	it('invokes nothing at all when the rects are unchanged', () => {
		expect(nextHotRectCalls([A, B], [A, B])).toEqual({});
	});

	it('declares the rects and leaves the flag alone when a button only moved', () => {
		// Changed rects, unchanged emptiness. Re-asserting the flag here is the
		// IPC-per-frame the emptiness test exists to prevent.
		expect(nextHotRectCalls([A], [MOVED])).toEqual({ rects: [MOVED] });
	});

	it('declares the rects and leaves the flag alone when a second button appears', () => {
		expect(nextHotRectCalls([A], [A, B])).toEqual({ rects: [A, B] });
	});

	it('withdraws the rects and clears the flag when the last button goes away', () => {
		// A rect left behind for a button that unmounted is a hole in the game
		// the player cannot see, and a flag left armed keeps the hook consuming
		// clicks against whatever it last heard about.
		expect(nextHotRectCalls([A], [])).toEqual({ rects: [], hasContent: false });
	});

	// Teardown resets `sent` to null precisely so this is not suppressed as a
	// no-change: a window that never claimed anything must still leave nothing
	// behind.
	it('still withdraws when nothing had been sent yet', () => {
		expect(nextHotRectCalls(null, [])).toEqual({ rects: [] });
	});
});
