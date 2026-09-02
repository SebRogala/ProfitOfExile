/**
 * Which display a module's overlay window is built on (POE-237).
 *
 * Before this, every widget window and every capture went to the PRIMARY
 * monitor. With PoE fullscreen on a second display that put the whole overlay
 * on the monitor the player was not looking at, while focus detection happily
 * reported the game as foreground — and Rust's capture read the wrong screen
 * for the same reason (`src-tauri/src/capture.rs`).
 *
 * Rust answers "which display is the game on" from the game window's own HWND
 * (`get_game_monitor`); this file answers the follow-up the webview has to ask,
 * "which of MY monitors is that". The two enumerations are different APIs —
 * Win32 `GetMonitorInfoW` on one side, Tauri's `availableMonitors()` on the
 * other — and their ids are not comparable, so the match is made on POSITION.
 *
 * Pure, and separate from `routes/(app)/+layout.svelte` for the reason every
 * other overlay rule is: an overlay window has no test harness in this app, and
 * a window built on the wrong display looks exactly like a window that was
 * never built. Same split as `widgets/widget-geometry.ts`.
 */

/** Rust's `capture::GameMonitor`, as `get_game_monitor` serialises it. */
export interface GameMonitorInfo {
	/** Win32's `HMONITOR` truncated to 32 bits. NOT comparable with anything
	 *  Tauri reports — see the module note; the position is what matches. */
	id: number;
	/** Physical px, virtual-desktop coordinates. */
	x: number;
	y: number;
	width: number;
	height: number;
}

/*
 * No scale factor here, deliberately: the caller has a TAURI monitor by the
 * time it needs one — the one this rule matched — and it is THAT `scaleFactor`
 * its logical-vs-physical maths has to agree with. Rust reporting a second one
 * from a third API could only disagree with it.
 */

/**
 * The shape of `availableMonitors()` / `primaryMonitor()` this rule reads.
 *
 * Structural rather than Tauri's own `Monitor`, so the rule is testable without
 * a Tauri runtime. The real type has more fields; extra ones are carried
 * through untouched because the caller gets its own object back.
 */
export interface PositionedMonitor {
	position: { x: number; y: number };
}

/**
 * The monitor to build on: the one the game is on when the webview can see it,
 * the primary otherwise.
 *
 * **Matched on the top-left corner, exactly.** Two displays cannot share one,
 * and both APIs report it in the same virtual-desktop physical pixels, so the
 * corner is an identity that survives the id spaces not lining up. Size is
 * deliberately not part of the key: a mismatch there would mean the two APIs
 * disagree about the same display, and falling back to the primary because of
 * it would be worse than building on the display the player is looking at.
 *
 * **Every failure falls back to the primary**, which is exactly the behaviour
 * that shipped before POE-237: no game monitor yet (nothing has seen PoE in the
 * foreground), or a game monitor the webview's own enumeration does not list (a
 * display unplugged between the two calls). The caller logs the second case —
 * it is a real disagreement — but a null here would leave the module with no
 * window at all, which is strictly worse than the pre-POE-237 status quo.
 *
 * `primary` may itself be null when the caller could not resolve one; that is
 * passed straight back, because the caller's answer to "no monitor at all" is
 * to refuse to build and let the bounded retry ask again.
 */
export function chooseMonitor<M extends PositionedMonitor>(
	game: GameMonitorInfo | null | undefined,
	available: readonly M[],
	primary: M | null
): M | null {
	if (!game) return primary;
	const found = available.find(
		(m) => m.position.x === game.x && m.position.y === game.y
	);
	return found ?? primary;
}
