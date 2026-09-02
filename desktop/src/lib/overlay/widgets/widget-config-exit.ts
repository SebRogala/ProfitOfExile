/**
 * What the widget host does when the window refuses to leave config mode
 * (POE-227).
 *
 * The host's way out of a config session is `set_overlay_config_mode(label,
 * false)`, and that call can fail. It used to clear local `configMode` and the
 * draft BEFORE invoking it and then merely LOG a rejection — which removed the
 * Save/Cancel bar, the only controls that window has, while the window itself
 * was still interactive. A monitor-sized, always-on-top rectangle eating every
 * click over the game, with nothing on it to press.
 *
 * So the order is inverted: ask Rust first, and keep config mode — the frames
 * AND the bar — until it confirms.
 *
 * # Why a refusal needs a RE-ASSERT, not just a message
 *
 * Rust's exit path is deliberately asymmetric (`lib.rs`,
 * `set_overlay_config_mode`): on the way out it clears its own registry flag and
 * re-applies `WS_EX_NOACTIVATE` even when `set_ignore_cursor_events(true)`
 * failed, and returns the error afterwards. Leaving the flag set would strand
 * the window, because the mouse hook skips a config-mode window and would never
 * repair it.
 *
 * That is right for Rust and it costs the host its retry. With the flag
 * cleared, the hook resumes repairing `WS_EX_TRANSPARENT` on the next mouse
 * move — so within one twitch of the cursor the window is click-through again
 * and the Save/Cancel bar the host just decided to keep is no longer clickable.
 * "The buttons stay pressable because the window is still interactive" is true
 * for a fraction of a second and then false.
 *
 * So the host RE-ASSERTS config mode (`on: true`) before showing the error, and
 * the wording turns on whether that landed: re-asserted means "press Save or
 * Cancel again", and a re-assert that failed too means the window cannot be
 * recovered from the inside and the way back is Configure in Settings.
 *
 * The decision lives out here because a `.svelte` file has no unit-test harness
 * in this app. Note what that does and does not cover: `widget-config-exit.test.ts`
 * pins THIS mapping. The ORDERING it exists to serve — invoke first, keep the
 * bar, re-assert, emit `widget-config-end` only on the Ok — is in
 * `WidgetHost.svelte` and is a Windows smoke check.
 *
 * Not to be confused with `widget-config-session.ts`, which is the LAYOUT's
 * record of what a session forced (module flag, visibility) so it can be undone.
 * This one lives in the overlay window and decides one thing: whether the user
 * is still in config mode.
 */

/** What a `set_overlay_config_mode` call answered. */
export type ConfigExitOutcome = { ok: true } | { ok: false; error: unknown };

/** What the host does about it. */
export interface ConfigExitDecision {
	/** Stay in config mode: the widget frames, the Save/Cancel bar, and the
	 *  draft rectangles the user arranged all remain. `false` is the only value
	 *  that authorises clearing the draft and emitting `widget-config-end`. */
	keepConfigMode: boolean;
	/** What the bar says. Empty exactly when the exit landed. */
	error: string;
}

/**
 * Decide from the exit's answer and, when it refused, the re-assert's.
 *
 * `reassert` is `null` when none was attempted — which is correct only on the
 * `ok` path. A refusal with no re-assert is treated as unrecoverable, because
 * that is what it is: the flag is cleared in Rust and nothing has put it back.
 *
 * A refusal keeps the session open rather than forcing it shut, because the two
 * failures are not symmetric: leaving the user in config mode costs them one
 * more press, while dropping out of it on a window that may still be
 * interactive leaves them a click-eating rectangle with no controls on it.
 */
export function configExitDecision(
	exit: ConfigExitOutcome,
	reassert: ConfigExitOutcome | null
): ConfigExitDecision {
	if (exit.ok) return { keepConfigMode: false, error: '' };
	const reason = String(exit.error);
	if (reassert?.ok) {
		return {
			keepConfigMode: true,
			error: `Could not hand the window back to the game — nothing was closed. Press Save or Cancel again. (${reason})`
		};
	}
	// No re-assert, or one that failed too. The hook is free to make this window
	// click-through again on the next mouse move, so the bar may stop responding
	// — say where the way back is instead of promising a retry that may not be
	// pressable.
	return {
		keepConfigMode: true,
		error: `Could not hand the window back to the game, and could not reopen configuration either. If these buttons stop responding, press Configure in Settings again. (${reason})`
	};
}
