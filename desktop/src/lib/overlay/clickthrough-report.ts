/**
 * How a failed `set_overlay_clickthrough` is reported (POE-227).
 *
 * The command awaits its own setup and returns a reason now, and the five
 * per-window overlays REPORT that reason rather than tearing the window down.
 * Which leaves one decision worth getting right: not every failure means the
 * player has a click-eating window on screen.
 *
 * A window destroyed inside the command's ~1 s HWND wait is ORDINARY — it is
 * exactly what an overlay toggled off mid-creation looks like — and there is
 * nothing left to catch a click. Reporting that as "this window may be
 * swallowing your clicks" would cry wolf on every fast toggle and teach the
 * owner to ignore the line that matters. Rust marks that one case with a
 * prefix; everything else is a live window that may be opaque to the mouse.
 *
 * The decision lives here rather than in `routes/(app)/+layout.svelte` because
 * a `.svelte` file has no unit-test harness in this app — the same split
 * `module-lifecycle.ts` and `widget-config-exit.ts` use.
 */

/**
 * The marker Rust puts on the "window was already gone" failure.
 *
 * Must match `CLICKTHROUGH_WINDOW_GONE` in `desktop/src-tauri/src/lib.rs`. Both
 * ends are pinned by a test — `clickthrough-report.test.ts` here, and
 * `a_vanished_window_is_reported_with_the_marker_the_caller_matches` there — so
 * a rename on either side fails a gate instead of silently downgrading every
 * genuine warning to an info line.
 */
export const CLICKTHROUGH_WINDOW_GONE = 'window-gone';

/** How loudly to say it. `info` is a fact; `error` is something to act on. */
export type ClickthroughReportLevel = 'info' | 'error';

/** What to write, and where in the register to write it. */
export interface ClickthroughReport {
	level: ClickthroughReportLevel;
	/** The line for the app log — a shipped build has no devtools. */
	message: string;
}

/**
 * Turn a rejected `set_overlay_clickthrough` into the line to log.
 *
 * `reason` is whatever the invoke rejected with; it is stringified here rather
 * than by the caller so an `Error` and a Tauri string reach the same wording.
 */
export function clickthroughReport(label: string, reason: unknown): ClickthroughReport {
	const text = String(reason);
	if (text.startsWith(CLICKTHROUGH_WINDOW_GONE)) {
		return {
			level: 'info',
			message: `[${label}-overlay] click-through setup skipped — the window was gone before it ran: ${text}`
		};
	}
	return {
		level: 'error',
		message: `[${label}-overlay] click-through setup failed — the window may be catching clicks meant for the game: ${text}`
	};
}
