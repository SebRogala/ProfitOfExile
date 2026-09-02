/**
 * A Svelte action that keeps a widget host's declared HOT RECTS in step with
 * whatever buttons it is currently drawing (POE-225).
 *
 * A widget overlay is click-through at the OS level, so a button inside it
 * receives no clicks at all unless the window has told the Windows mouse hook
 * which rectangles to consume — `set_overlay_hot_rects`, documented in
 * `docs/OVERLAY-GUIDE.md` and converted by `../hot-rects.ts`. The comparator
 * does this by hand for its two fixed elements; a widget host cannot, because
 * it does not know what its widgets draw. So the contract is a DOM one: any
 * element inside the host that carries `data-hot` is claimed, and any element
 * that also carries `data-action` is routed by the host's `overlay-click`
 * handler through `elementFromPoint`.
 *
 * Withdrawal matters as much as declaration. A rect left behind for a button
 * that has unmounted is a hole in the game the player cannot see, so the action
 * clears the whole declaration on destroy and re-measures whenever the subtree
 * changes.
 *
 * # What triggers a re-measure
 *
 * - `update` — the host re-ran with different parameters (the scale factor
 *   resolving, config mode ending).
 * - a `ResizeObserver` on the host — the window changed size or scale.
 * - a `MutationObserver` on the subtree. This is the one that matters for a
 *   host: the host element is the whole monitor and never resizes, so a button
 *   appearing inside it is invisible to the `ResizeObserver`. The comparator
 *   does not need one because it declares two elements it holds references to.
 *
 * All three funnel into one `requestAnimationFrame`, so a burst of reactive
 * changes costs at most one measurement per frame, and [`nextHotRectCalls`]
 * suppresses the IPC entirely when nothing moved — which is the common case,
 * since the host re-renders on every SSOT poll.
 *
 * # Why this also declares `has_content`
 *
 * Hot rects alone claim nothing. `overlay_hook::hit_test` skips a window whose
 * `has_content` is false BEFORE it looks at a single rect, and a window's flag
 * starts false (`HookedWindow::new`) — so a widget host that only ever called
 * `set_overlay_hot_rects` would have every rect it declared ignored, and its
 * buttons would be as dead as if it had declared none. The five older overlays
 * each set the flag from their own content rule; a host has no content rule of
 * its own, because for a widget window "drawing something clickable" and
 * "claiming a rectangle" are the same statement. So the flag follows the rects,
 * and [`nextHotRectCalls`] — which owns the whole what-to-invoke decision — is
 * what keeps that from becoming an IPC call per animation frame.
 */
import { invoke } from '@tauri-apps/api/core';
import { hotRectsEqual, physicalHotRect, type HotRect } from '../hot-rects';

/** What the action needs from the host on every run. */
export interface HotRectsParams {
	/** The window label to declare against — a module's overlay window is
	 *  labelled with the module id. */
	module: string;
	/** The window's Tauri `scaleFactor()`. Zero until it resolves, and
	 *  `physicalHotRect` declines every rect while it is, so nothing is claimed
	 *  before the conversion is trustworthy. */
	scaleFactor: number;
}

/** The elements a host claims clicks in. */
const HOT_SELECTOR = '[data-hot]';

/** What a measurement decides to send. An absent field is a call NOT made. */
export interface HotRectCalls {
	/** The rects to declare through `set_overlay_hot_rects`. */
	rects?: HotRect[];
	/** The value to write through `set_overlay_has_content`. */
	hasContent?: boolean;
}

/**
 * What a fresh measurement has to invoke, given what was last sent.
 *
 * The whole decision, so the action around it is measurement and IPC and
 * nothing else — an overlay window has no test harness in this app, and both
 * halves of this fail invisibly: a rect re-declared on every animation frame
 * costs an IPC call per frame, and a `has_content` flag left false has
 * `overlay_hook::hit_test` skip the window BEFORE it looks at a single rect, so
 * every button the host draws is swallowed by the game with nothing reporting a
 * failure.
 *
 * `prev` is `null` when nothing has been sent yet — which is also what teardown
 * resets to, so the withdrawal is never suppressed as a no-change.
 *
 * Two rules:
 *
 * - Nothing at all when the rects are unchanged. That is the common case by a
 *   wide margin: the host re-measures on every frame a mutation touches, and a
 *   host re-renders on every SSOT poll.
 * - The flag moves only when the EMPTINESS of the list changed. It is a boolean
 *   and the rects are a list, so a button that moved a pixel has changed its
 *   rects and not its content.
 */
export function nextHotRectCalls(prev: HotRect[] | null, next: HotRect[]): HotRectCalls {
	if (prev && hotRectsEqual(prev, next)) return {};
	const had = (prev?.length ?? 0) > 0;
	const has = next.length > 0;
	return had === has ? { rects: next } : { rects: next, hasContent: has };
}

export function useHotRects(node: HTMLElement, params: HotRectsParams) {
	let current = params;
	let sent: HotRect[] | null = null;
	let frame = 0;

	function measure() {
		frame = 0;
		const rects: HotRect[] = [];
		// Document order, which is the hook's tie-break when two rects overlap.
		for (const el of node.querySelectorAll<HTMLElement>(HOT_SELECTOR)) {
			const rect = physicalHotRect(el.getBoundingClientRect(), current.scaleFactor);
			if (rect) rects.push(rect);
		}
		declare(rects);
	}

	function setHasContent(hasContent: boolean) {
		invoke('set_overlay_has_content', { label: current.module, hasContent }).catch((e) =>
			console.warn(`[overlay] set_overlay_has_content failed for '${current.module}':`, e)
		);
	}

	function declare(rects: HotRect[]) {
		const calls = nextHotRectCalls(sent, rects);
		if (!calls.rects) return;
		sent = calls.rects;
		invoke('set_overlay_hot_rects', { label: current.module, rects: calls.rects }).catch((e) =>
			console.warn(`[overlay] set_overlay_hot_rects failed for '${current.module}':`, e)
		);
		// After the rects, not before: the flag is what makes the hook look at
		// them, so arming it first opens a window in which the hook would consume a
		// click against the PREVIOUS declaration.
		if (calls.hasContent !== undefined) setHasContent(calls.hasContent);
	}

	function schedule() {
		if (frame) return;
		frame = requestAnimationFrame(measure);
	}

	const resizes = new ResizeObserver(schedule);
	resizes.observe(node);
	const mutations = new MutationObserver(schedule);
	mutations.observe(node, { childList: true, subtree: true, attributes: true });
	schedule();

	return {
		update(next: HotRectsParams) {
			current = next;
			schedule();
		},
		destroy() {
			resizes.disconnect();
			mutations.disconnect();
			if (frame) {
				cancelAnimationFrame(frame);
				frame = 0;
			}
			// Not `declare([])` — the equality test would skip the call when the
			// window had never claimed anything, and a window being torn down
			// must leave nothing behind either way.
			sent = null;
			declare([]);
			// Unconditional, unlike the transition inside `declare`: resetting
			// `sent` above erases what the flag was last set to, and a hook left
			// believing a destroyed page still has content would keep consuming
			// clicks in whatever rectangles it last heard about.
			setHasContent(false);
		}
	};
}
