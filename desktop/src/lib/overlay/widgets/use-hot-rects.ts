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
 * changes costs at most one measurement per frame, and `hotRectsEqual`
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
 * and [`hasContentTransition`] is what keeps that from becoming an IPC call per
 * animation frame.
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

/**
 * Whether a declaration has to move the window's `has_content` flag, and where
 * to — `null` when it does not.
 *
 * The flag is a boolean and the rects are a list, so only the EMPTINESS of the
 * list matters: a host that redraws a button one pixel over has changed its
 * rects and not its content, and re-asserting the flag on every such frame
 * would put an IPC call behind every animation frame the overlay runs.
 */
export function hasContentTransition(prevCount: number, nextCount: number): boolean | null {
	const had = prevCount > 0;
	const has = nextCount > 0;
	if (had === has) return null;
	return has;
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
		if (sent && hotRectsEqual(sent, rects)) return;
		const flip = hasContentTransition(sent?.length ?? 0, rects.length);
		sent = rects;
		invoke('set_overlay_hot_rects', { label: current.module, rects }).catch((e) =>
			console.warn(`[overlay] set_overlay_hot_rects failed for '${current.module}':`, e)
		);
		// After the rects, not before: the flag is what makes the hook look at
		// them, so arming it first opens a window in which the hook would consume a
		// click against the PREVIOUS declaration.
		if (flip !== null) setHasContent(flip);
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
