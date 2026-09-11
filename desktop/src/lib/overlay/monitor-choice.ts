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

export type MonitorNoticeAction = 'known' | 'record' | 'learn-id' | 'defer' | 'unwanted' | 'rebuild';

/**
 * What one `game-monitor-changed` notice means for a widget window (POE-237,
 * POE-245). `widget-window.ts`'s `onGameMonitorChanged` performs the side
 * effect for each answer; the order below is the old listener's.
 *
 * TWO notices do not rebuild: one naming the display the window is already ON
 * (only the id was unknown), and one that arrives while the user is arranging
 * widgets — the window a rebuild would destroy is the surface that session is
 * running on. `docs/OVERLAY-GUIDE.md` states both with the rest of the rebuild
 * rule.
 *
 * - `'record'` — no settled window, so there is nothing to rebuild HERE — but
 *   `built()` is also false while a create is IN FLIGHT, and this notice is the
 *   only one Rust will send (`remember_game_monitor` emits on a CHANGE, and the
 *   change is now recorded). Dropping it is what stranded the overlay on the
 *   display the game had left until the module was toggled (POE-245).
 * - `'learn-id'` — already the right canvas: the window was BUILT on the
 *   display this notice names, and only the id was unknown (a build made
 *   before anything had seen the game window records `0`). Learning the id is
 *   the whole change; rebuilding would take a correct overlay off screen for a
 *   destroy/create to put it back where it already was.
 * - `'defer'` — a rebuild is a destroy, and this window is the surface the user
 *   is dragging widgets on: taking it down mid-session drops them into a window
 *   that no longer exists and leaves Settings on `Configuring…` waiting for a
 *   host that was destroyed.
 * - `'unwanted'` — same guard as `reconcileMonitor` (POE-245): a notice can land
 *   in the gap between the user switching the module off and the driver
 *   finishing the teardown, and the unconditional off/on would put the window
 *   straight back up.
 */
export function monitorNoticeAction(input: {
	notice: GameMonitorInfo;
	/** The id the window was built on (`0` = not built on a known display). */
	builtId: number;
	/** The corner the window was built at, or null when none is up. */
	builtAt: { x: number; y: number } | null;
	/** The driver's settled marker. */
	built: boolean;
	configLive: boolean;
	wanted: boolean;
}): MonitorNoticeAction {
	if (input.notice.id === input.builtId) return 'known';
	if (!input.built) return 'record';
	if (
		input.builtAt &&
		input.builtAt.x === input.notice.x &&
		input.builtAt.y === input.notice.y
	) return 'learn-id';
	if (input.configLive) return 'defer';
	if (!input.wanted) return 'unwanted';
	return 'rebuild';
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

/**
 * Whether the window was built on a display the game has since left (POE-245).
 *
 * The gap this closes: `widget-window.ts`'s `create` asks Rust which display the game
 * is on, and the answer can change while the window is still being built — the
 * create awaits `set_overlay_clickthrough`, which alone sleeps ~1 s waiting for
 * the WebView2 HWND. Rust DOES send a `game-monitor-changed` in that window,
 * but the layout's handler drops a notice that arrives before the driver has
 * settled the create (there is no window yet to rebuild), and
 * `remember_game_monitor` emits only on a CHANGE — so that notice is the only
 * one there will ever be and the overlay stays on the wrong display until the
 * module is toggled. The owner's report of a temple module that reads as
 * enabled while nothing is on screen is that: the widgets are on the monitor
 * they are not looking at.
 *
 * So the layout asks the question a second time once the window is up, and a
 * `true` there asks the driver for its own off/on — the SAME rebuild the notice
 * handler performs, so there is one monitor-follow mechanism rather than two.
 * Deliberately not reported as a failed creation: the window is built and
 * usable, and three stale answers would exhaust the create budget and leave the
 * module with NO overlay, which is worse than one on the wrong screen.
 *
 * `builtAt` is the top-left of the display the window ACTUALLY went on, in the
 * virtual-desktop physical px both enumerations share — the same corner
 * [`chooseMonitor`] matches on, and for the same reason: the two id spaces are
 * not comparable.
 *
 * FALSE whenever there is nothing to compare: no window (`builtAt` null), or no
 * game monitor (nothing has seen PoE, or the lookup failed). Both are the
 * pre-POE-237 state, in which the window is on the primary ON PURPOSE, and
 * calling that stale would fail every creation made before the game was ever in
 * the foreground — the module would retry three times and give up.
 *
 * `couldNotReachTheGame` is the third false, and the one that is not about
 * missing information: the build KNEW where the game was and [`chooseMonitor`]
 * still fell back to the primary, because the webview does not list the display
 * Rust named. Retrying that lands on the primary again every time.
 */
export function builtOnStaleMonitor(
	builtAt: { x: number; y: number } | null,
	game: GameMonitorInfo | null | undefined,
	couldNotReachTheGame: boolean
): boolean {
	// A build that KNEW where the game was and could not go there — Rust named a
	// display this webview's own enumeration does not list — is not stale; it is
	// POE-237's soft failure, and the retry would land on the primary again,
	// three times, and then give the module up. That case is already logged as
	// the real disagreement it is. It is NOT the same as a build made before
	// anything had seen PoE, which also lands on the primary and IS the case
	// this function exists for.
	if (couldNotReachTheGame) return false;
	if (!builtAt || !game) return false;
	return builtAt.x !== game.x || builtAt.y !== game.y;
}

/**
 * The display the game is on once the window is up — the answer the post-build
 * re-check compares against (POE-245).
 *
 * TWO sources, because either one alone has a hole. `queried` is a fresh
 * `get_game_monitor`, and it is preferred: Rust writes `AppState.game_monitor`
 * BEFORE it emits (`capture.rs`), so a query made after any notice sees at
 * least what that notice carried, and it is also the only source that reflects
 * a game that moved away and back again during the build. `recorded` is the
 * `game-monitor-changed` the layout captured while the create was in flight,
 * and it is the fallback for the one case the query cannot cover: a lookup that
 * FAILED, which the caller turns into `null`. Without it a failed query would
 * read as "nothing to correct" and the window would stay on the display the
 * notice already said the game had left.
 *
 * `null` when neither answered, which the caller reads as nothing to compare —
 * the pre-POE-237 state, in which the window is on the primary on purpose.
 */
export function gameMonitorAfterBuild(
	recorded: GameMonitorInfo | null,
	queried: GameMonitorInfo | null | undefined
): GameMonitorInfo | null {
	return queried ?? recorded;
}
