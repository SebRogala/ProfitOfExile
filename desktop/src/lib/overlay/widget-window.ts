import { invoke } from '@tauri-apps/api/core';
import {
	moduleOverlayDriver,
	type ModuleOverlayDriver
} from './module-lifecycle';
import {
	builtOnStaleMonitor,
	chooseMonitor,
	gameMonitorAfterBuild,
	monitorNoticeAction,
	type GameMonitorInfo
} from './monitor-choice';

export interface WidgetWindowOptions {
	/** The window label — also the `WidgetSpec.module` and the `/overlay/<label>` route segment. */
	label: string;
	/** The driver key's module id (`ModuleOverlayKey.moduleId`). */
	moduleId: string;
	/** The whole desired-state rule (see `moduleOverlayWanted`). Read by the two rebuild sites. */
	wanted: () => boolean;
	/** Whether a widget-config session is live for this window. */
	configLive: () => boolean;
	/** Whether to build the window with `?debug`. */
	debug: () => boolean;
}

export interface WidgetWindow {
	readonly label: string;
	/** The create/destroy driver (`setDesired`, `built`). */
	readonly driver: ModuleOverlayDriver;
	/** The entry `WIDGET_CONFIG_TARGETS` holds for this window. */
	readonly configTarget: { label: string; built: () => boolean };
	/** Handle one `game-monitor-changed` notice. The layout owns the single listener. */
	onGameMonitorChanged(notice: GameMonitorInfo): void;
	/** Guard 6's channel for this window. */
	log(msg: string): void;
}

// --- Monitor-sized widget window orchestration (POE-171) ---
//
// Display-only, and coupled to the module's MODULE flag rather than to an
// overlay setting of its own: one switch governs the capture loop AND this
// window, because a module's window with no reader behind it can only ever
// show a stale board. `docs/OVERLAY-GUIDE.md` guards, and where each is met:
//
//  1. capabilities — the window label is in capabilities/default.json.
//  2/3. physical vs logical — the window IS the GAME's monitor (POE-237;
//    the primary one until something has seen the game window), so the
//    monitor's physical position and size are divided by ITS scale factor
//    for the constructor, which takes LOGICAL pixels, and the exact
//    `PhysicalPosition`/`PhysicalSize` are applied in `tauri://created`.
//    `window.devicePixelRatio` is not used.
//  4. move, not recreate — this window is never repositioned WITHIN a
//    display, so there is no destroy/recreate cycle to avoid there. It is
//    built and torn down on the module flag's transitions, on the bounded
//    creation retry, and — POE-237 — when the game moves to another
//    monitor, which is a genuinely different canvas rather than a move:
//    the size, the scale factor and every widget's coordinate space change
//    with it. All of them go through `module-lifecycle.ts`, so two never
//    overlap.
//  5. settings survival — the WINDOW persists nothing, because it is a whole
//    monitor and there is nothing about it a user could choose. What
//    IS persisted is where the user put each WIDGET inside it
//    (`Settings.widgets`, keyed `"<label>.<widget>"`), and that map is owned
//    by an `AppState` mutex, so it travels through `from_state` and is
//    covered by the round-trip tests in `settings.rs` rather than by
//    `persist_overlay_settings`.
//  6. error visibility — every failure below goes through `log`, so it
//    reaches `app_log_from_frontend` (the LOGS channel, and the only one
//    readable in a shipped build) with the console as a second copy. Nothing
//    here swallows a failure, and a creation that cannot be completed tears
//    down what it half-built rather than leaving it standing.
//
// Visibility with the game is NOT decided here. The Rust focus poller shows
// and hides the window on the game-focus transition, exactly as it
// does the comparator; the route decides only whether it has a board worth
// drawing.


export function createWidgetWindow(options: WidgetWindowOptions): WidgetWindow {
	const { label } = options;
	/**
	 * Guard 6's channel.
	 *
	 * `console` alone is a message nobody can read: a shipped build has no
	 * devtools, so an overlay that failed to build would fail invisibly. The app
	 * log is the surface the user can actually open, and the console copy is
	 * kept for `npm run dev`.
	 */
	function log(msg: string): void {
		console.warn(`[overlay] ${label}: ${msg}`);
		invoke('app_log_from_frontend', { msg: `[${label}-overlay] ${msg}` })
			.catch(e => console.error(`[overlay] ${label}: app log unreachable:`, e));
	}

	/**
	 * The display the window was last BUILT on, as Rust's
	 * `get_game_monitor` reported it (POE-237). `0` means "not built on a known
	 * display" — either nothing had seen the game window yet or the lookup
	 * failed — and every real id differs from it, so the first notice after such
	 * a build rebuilds onto the display the game is actually on.
	 *
	 * A plain `let`, not a rune: nothing renders from it, and making it reactive
	 * would re-enter the lifecycle effect on every build.
	 */
	let monitorId = 0;

	/**
	 * WHERE the window was last built — the top-left of the monitor it
	 * was actually constructed on, in the same virtual-desktop physical px Rust
	 * reports (POE-237). `null` when no window is up.
	 *
	 * The id cannot answer this on its own. A build that happened before
	 * anything had seen the game window records id `0` and lands on the primary,
	 * so the first notice — the game being on that very primary — differs from
	 * `0` and would rebuild an already-correct window, taking the overlay away
	 * for the length of a destroy/create. The corner is what the two
	 * enumerations share (`overlay/monitor-choice.ts`), so it is what can say
	 * "that is the display this window is already on".
	 */
	let monitorAt: { x: number; y: number } | null = null;

	/**
	 * A `game-monitor-changed` that arrived while a create was IN FLIGHT
	 * (POE-245).
	 *
	 * The notice handler cannot act on one of those — there is no settled window
	 * to rebuild — and it used to drop it. That was the whole bug:
	 * `remember_game_monitor` emits only when the answer CHANGES, so the dropped
	 * notice is the only one there will ever be and the overlay stays on the
	 * display the game left until the module is toggled.
	 *
	 * So it is RECORDED here instead, and `reconcileMonitor` consumes it
	 * once the driver has settled the create. Cleared at the start of every
	 * build, so one build's reconcile only ever sees notices from its own
	 * lifetime.
	 *
	 * A plain `let` for the same reason the two above are: nothing renders from
	 * it, and a rune would re-enter the lifecycle effect.
	 */
	let pendingGameMonitor: GameMonitorInfo | null = null;

	/** The create/destroy ordering, and the bounded retry — see
	 *  `$lib/overlay/module-lifecycle`. The driver owns the mutable state; this
	 *  file owns the Tauri work it orders. */
	const driver = moduleOverlayDriver(
		{ label, moduleId: options.moduleId },
		{ create, destroy, log }
	);

	/** The entry `WIDGET_CONFIG_TARGETS` holds for this window. */
	const configTarget = {
		label,
		// The driver's own settled marker. The LABEL appearing is not the same
		// thing: `getByLabel` answers the moment the constructor returns, while
		// `tauri://created` is still running — and that handler ends by hiding
		// the window when the game is not focused, which during a config
		// session it never is. Waiting for `built()` is waiting for that
		// handler to have finished.
		built: () => driver.built()
	};

	/**
	 * Build the window. Resolves true only once it is positioned, sized and
	 * click-through — a half-built one resolves false and is torn down.
	 *
	 * The promise settles from `tauri://created` / `tauri://error` rather than
	 * from the constructor returning, which is what makes the driver's
	 * serialisation real: the previous code resolved as soon as the constructor
	 * had been called, so an off toggle could run its destroy sweep before the
	 * window it was meant to remove existed.
	 */
	async function create(): Promise<boolean> {
		const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
		const { PhysicalPosition, PhysicalSize } = await import('@tauri-apps/api/dpi');
		const { availableMonitors, currentMonitor, primaryMonitor } = await import(
			'@tauri-apps/api/window'
		);

		await destroy();

		// The window IS one monitor (POE-225): one fullscreen, click-through
		// canvas per module, with the widgets placed inside it. WHICH monitor is
		// the game's, since POE-237 — a window on the primary while PoE is
		// fullscreen on the second display draws every widget where the player
		// is not looking, and Rust's capture reads the wrong screen for the same
		// reason. `currentMonitor()` is the primary's fallback rather than a
		// constant, because a constant would be a guess at a resolution — and a
		// window sized wrong puts every widget's persisted physical coordinate
		// in the wrong place.
		const primary =
			(await primaryMonitor().catch((e: any) => {
				log(`primaryMonitor failed, trying currentMonitor: ${e}`);
				return null;
			})) ??
			(await currentMonitor().catch((e: any) => {
				log(`currentMonitor failed too: ${e}`);
				return null;
			}));
		// Both lookups fail SOFT to the pre-POE-237 behaviour (the primary):
		// a module with its window on the wrong display is still usable, and one
		// with no window at all is not.
		const game = await invoke<GameMonitorInfo | null>('get_game_monitor').catch((e: any) => {
			log(`get_game_monitor failed, building on the primary monitor: ${e}`);
			return null;
		});
		const listed = await availableMonitors().catch((e: any) => {
			log(`availableMonitors failed, building on the primary monitor: ${e}`);
			return [];
		});
		const monitor = chooseMonitor(game, listed, primary);
		if (!monitor) {
			// No monitor, no canvas. Refusing is what the bounded retry in
			// `module-lifecycle.ts` is for; guessing a size would place the
			// widgets somewhere the player never put them.
			log('no monitor to build the overlay on — not creating it');
			return false;
		}
		// Did the corner match actually land on the game's display, or did
		// `chooseMonitor` fall back to the primary? Both the log below and the
		// id recorded for the rebuild guard turn on this one answer.
		const onGameMonitor =
			!!game && monitor.position.x === game.x && monitor.position.y === game.y;
		if (game && !onGameMonitor) {
			// A real disagreement between the two enumerations, worth saying out
			// loud: Rust placed the game window on a display this webview does
			// not list, so the overlay is going on the primary and the widgets
			// will be on the wrong screen until the next rebuild.
			log(
				`the game is on a monitor at ${game.x},${game.y} that this window cannot see — building on the primary`
			);
		}
		// What a `game-monitor-changed` is compared against: the id of the display
		// this window was ACTUALLY built on, and that display's corner. `0` is
		// "not built on a known display" — recorded whenever the build fell back
		// to the primary, either because nothing had seen PoE yet or because the
		// webview could not list the display Rust named. Recording the game's id
		// there would be a lie the guard believes: the very next notice carries
		// that same id, the `notice.id === builtId` line in `monitorNoticeAction`
		// answers `'known'`, and the overlay is left on the wrong screen for the
		// rest of the session. With `0` the notice falls through to the corner
		// comparison, which rebuilds (corners differ) or just learns the id (the
		// window is already there).
		// A LOCAL as well as the module variable (POE-245). The re-check at the
		// end of this build must compare against the display this build actually
		// went on, and the module variable is written by the notice handler in
		// between — reading it back there is how the first version of this fix
		// defeated itself.
		const builtAt = { x: monitor.position.x, y: monitor.position.y };
		monitorId = onGameMonitor ? game.id : 0;
		monitorAt = builtAt;
		// This build's window for recording notices starts here.
		pendingGameMonitor = null;
		// Constructor dimensions are LOGICAL; a monitor reports PHYSICAL.
		const sf = monitor.scaleFactor > 0 ? monitor.scaleFactor : 1;
		const win = new WebviewWindow(label, {
			url: `/overlay/${label}${options.debug() ? '?debug' : ''}`,
			transparent: true,
			decorations: false,
			alwaysOnTop: true,
			// Nothing drags or resizes this window: it is the monitor. A
			// resizable frame would only offer an edge that desyncs every
			// widget's persisted position from the canvas it was measured in.
			resizable: false,
			shadow: false,
			skipTaskbar: true,
			// Built without stealing the foreground. Tauri focuses a new window
			// by default, and this one is created while the game may be in
			// front — taking focus would pull the player out of PoE for the
			// second before the click-through thread below runs.
			focus: false,
			x: Math.round(monitor.position.x / sf),
			y: Math.round(monitor.position.y / sf),
			width: Math.round(monitor.size.width / sf),
			height: Math.round(monitor.size.height / sf),
		});

		return await new Promise<boolean>((resolve) => {
			let settled = false;
			const finish = (ok: boolean) => {
				if (settled) return;
				settled = true;
				resolve(ok);
			};

			win.once('tauri://created', async () => {
				try {
					await win.setPosition(new PhysicalPosition(monitor.position.x, monitor.position.y));
					await win.setSize(new PhysicalSize(monitor.size.width, monitor.size.height));
					// Registration, not interactivity: the window declares no hot
					// rects of its own, and `WidgetHost.svelte` declares one per
					// `[data-hot]` element its widgets draw. Registering is what
					// lets the hook repair the WS_EX_TRANSPARENT WebView2 strips.
					//
					// AWAITED, and it is the gate this creation turns on. The
					// command spends ~1 s waiting for the WebView2 HWND (the
					// guide's runtime-earned observations) and then reports
					// whether the window is actually click-through; it used to
					// return before doing any of that, so this try/catch could
					// not see a failure and a window that never became
					// click-through was kept. THIS window is the size of the
					// monitor: an invisible, always-on-top one that is not
					// click-through swallows every click on the screen until it
					// is destroyed. Which is what the catch does — and the
					// `false` sends the failure back to `module-lifecycle.ts`,
					// whose bounded retry builds it again.
					await invoke('set_overlay_clickthrough', {
						label: label,
					});
				} catch (e) {
					// A window that is transparent, always-on-top and NOT
					// click-through eats clicks over the game with nothing
					// visible to explain why. Half-built is worse than absent,
					// so it goes.
					log(`setup failed, destroying the half-built window: ${e}`);
					await destroy();
					finish(false);
					return;
				}
				// Soft step: the focus poller only acts on transitions, so a
				// window built while PoE is not in the foreground would sit on
				// the desktop until the next alt-tab. Failing this leaves a
				// visible window, not a broken one — logged, not fatal.
				//
				// SKIPPED while the user is arranging widgets (POE-226). A window
				// built for a config session is built BECAUSE Settings asked for
				// it, and the user is in Settings, so the game is never focused
				// then — hiding here would race the `show()` that follows the
				// build and leave the session on an invisible window about half
				// the time. One owner for the decision: while a session is live,
				// the config flow decides visibility, not this check.
				if (options.configLive()) {
					log('built during a widget-config session — leaving it visible');
				} else {
					try {
						const status = await invoke<any>('get_status');
						if (!status?.game_focused) await win.hide();
					} catch (e) {
						log(`initial focus check failed, window left visible: ${e}`);
					}
				}
				// The monitor question, asked a SECOND time (POE-245) — but AFTER
				// the driver has settled this create, which is what the timeout
				// buys. The first ask was before the constructor; everything
				// since — the ~1 s `set_overlay_clickthrough` spends waiting for
				// the WebView2 HWND most of all — is time in which the player can
				// alt-tab into a game on another display, and the notice Rust
				// sends for that is one the handler below can only record.
				//
				// A macrotask and not a microtask: `finish(true)` resolves this
				// creation, and the driver's settle (`moduleOverlaySettle`, then
				// `pump`) runs in the microtasks that follow it. The rebuild the
				// reconcile may ask for is `setDesired(false)/setDesired(true)`,
				// which the driver ignores while `pending` is still `'create'` —
				// so it has to run after the settle, not merely after this
				// handler.
				setTimeout(() => void reconcileMonitor(builtAt, !!game && !onGameMonitor), 0);
				finish(true);
			}).catch(e => {
				log(`could not listen for tauri://created: ${e}`);
				finish(false);
			});

			win.once('tauri://error', (e: any) => {
				log(`creation failed: ${JSON.stringify(e?.payload ?? e)}`);
				finish(false);
			}).catch(e => {
				log(`could not listen for tauri://error: ${e}`);
				finish(false);
			});
		});
	}

	/**
	 * Is the window on the display the game is on? Asked once, after a build has
	 * settled (POE-245).
	 *
	 * The gap this closes: `create` reads `get_game_monitor` before
	 * the constructor, and the answer can move while the window is still being
	 * built. Rust DOES send a `game-monitor-changed` for that, but the handler
	 * below cannot act on a notice that arrives before the driver has settled —
	 * there is no window to rebuild yet — and `remember_game_monitor` emits only
	 * on a CHANGE, so that notice is the only one there will ever be. Dropping it
	 * stranded the overlay on the display the game had left until the module was
	 * toggled, which is the workaround the owner found.
	 *
	 * `builtAt` is the corner this build actually went on, passed in as a LOCAL.
	 * Reading `monitorAt` here instead is what defeated the first version
	 * of this fix: the notice handler writes that variable, and it had already
	 * run by the time this ran.
	 *
	 * A stale answer asks the DRIVER for its own off/on — the same rebuild the
	 * notice handler performs, so this is one monitor-follow mechanism rather
	 * than two, and the create budget is untouched. Reporting the build as
	 * FAILED instead would spend that budget three times over and leave the
	 * module with no overlay at all, which is worse than one on the wrong screen.
	 *
	 * It does NOT honour the `widgetConfigLive` deferral the notice handler
	 * applies, and that is deliberate: the deferral works there because "the next
	 * notice still rebuilds", and here there is no next notice — deferring would
	 * strand the correction for the session. What the deferral protects is a user
	 * mid-drag, and this runs before a session's `set_overlay_config_mode` has
	 * been sent, so there is nothing to drag yet. A session opening across the
	 * rebuild waits on `waitForWidgetConfigWindow`, whose `WIDGET_WINDOW_WAIT_MS`
	 * budget covers a destroy and a create, and then arranges widgets on the
	 * right display.
	 */
	async function reconcileMonitor(
		builtAt: { x: number; y: number },
		couldNotReachTheGame: boolean,
		retry = false
	): Promise<void> {
		// The create may have been undone between the schedule and now — a
		// module-off, or a settle that recorded a failure. Nothing to reconcile,
		// and the next build asks all of this again.
		//
		// It would ALSO read this way if the driver ever grew a macrotask hop
		// between the create promise resolving and `moduleOverlaySettle`, which
		// is what the `setTimeout(…, 0)` above is timed against. One more turn of
		// the event loop covers that without changing today's behaviour, and the
		// record is untouched on this path so the retry still finds it. Bounded
		// to a single retry on purpose: a window that is genuinely gone never
		// comes back, and an unbounded version would re-arm a timer for the life
		// of the process.
		if (!driver.built()) {
			if (!retry) {
				setTimeout(() => void reconcileMonitor(builtAt, couldNotReachTheGame, true), 0);
			}
			return;
		}
		const recorded = pendingGameMonitor;
		pendingGameMonitor = null;
		const queried = await invoke<GameMonitorInfo | null>('get_game_monitor').catch(
			(e: any) => {
				// The recorded notice is what covers this: without it a failed
				// lookup reads as "nothing to correct".
				log(`could not re-check the game monitor after the build: ${e}`);
				return null;
			}
		);
		const now = gameMonitorAfterBuild(recorded, queried);
		if (!builtOnStaleMonitor(builtAt, now, couldNotReachTheGame)) {
			// Re-arm both guards against the display the window is actually on.
			// The notice handler zeroes the id while a create is in flight, so
			// without this an id we already know would read as unknown and the
			// next notice would rebuild a window that is already correct.
			monitorAt = builtAt;
			if (now && now.x === builtAt.x && now.y === builtAt.y) monitorId = now.id;
			return;
		}
		if (!options.wanted()) {
			// The module was switched off (or the grant withdrawn) while this
			// window was being built. The desired-state effect has already asked
			// for the teardown; rebuilding here would put the window back up
			// against the user's own toggle.
			log(
				'built on a display the game has since left, but nothing wants the window any more — leaving it to the module flag'
			);
			return;
		}
		log(
			`built on a display the game has since left — rebuilding on the one at ${now?.x},${now?.y}`
		);
		// Cleared rather than pointed at the new display: the rebuild's own
		// create records where it lands, and a guard armed against a window that
		// does not exist yet would make the next notice return early.
		monitorId = 0;
		monitorAt = null;
		driver.setDesired(false);
		driver.setDesired(true);
	}

	/** Tear the window down. Returns whether the label is gone afterwards. */
	async function destroy(): Promise<boolean> {
		const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
		for (let i = 0; i < 5; i++) {
			const existing = await WebviewWindow.getByLabel(label).catch(e => {
				log(`lookup during destroy failed: ${e}`);
				return null;
			});
			if (!existing) return true;
			// Both are attempted: `close` is the polite path and `destroy` the
			// one that actually frees the label. Tauri's cleanup is async, hence
			// the re-check rather than a single call.
			try { await existing.close(); } catch (e) { log(`close attempt ${i + 1} failed: ${e}`); }
			try { await existing.destroy(); } catch (e) { log(`destroy attempt ${i + 1} failed: ${e}`); }
			await new Promise(r => setTimeout(r, 100));
		}
		log('window still present after 5 close/destroy rounds — giving the label up');
		return false;
	}

	/**
	 * The game moved to another display: rebuild the window there (POE-237).
	 *
	 * A REBUILD, not a move. The window is the monitor, so the new display
	 * brings a different size, a different scale factor and a different physical
	 * coordinate space for every widget inside it — none of which a
	 * `setPosition` would fix, and all of which the constructor path already
	 * gets right. Guard 4's "move, not recreate" is about repositioning a window
	 * on ONE display; this is a different canvas.
	 *
	 * Through the driver's own off/on, which is the same path a module toggle
	 * takes (and the one a `?debug` rebuild uses): the driver serialises the
	 * destroy and the create, so this cannot race a build the module flag
	 * started. Nothing is queued when no window is up — `setDesired` is
	 * value-guarded and the flag's own effect re-asserts `true` on its next poll
	 * anyway.
	 */
	function onGameMonitorChanged(notice: GameMonitorInfo): void {
		const action = monitorNoticeAction({
			notice,
			builtId: monitorId,
			builtAt: monitorAt,
			built: driver.built(),
			configLive: options.configLive(),
			wanted: options.wanted()
		});
		switch (action) {
			case 'known':
				return;
			case 'record':
				// So it is RECORDED and `reconcileMonitor` consumes it once the
				// driver has settled the create. A record left over from a build
				// that never completed costs nothing: every build clears it on the
				// way in.
				pendingGameMonitor = notice;
				// The id, but NOT the corner. The id guard must not believe a
				// display the window was never on; the corner is rewritten by
				// every build before the window is constructed, so it can only be
				// read here after a build that wrote it — and the reconcile needs
				// it left alone to re-arm.
				monitorId = 0;
				return;
			case 'learn-id':
				monitorId = notice.id;
				return;
			case 'defer':
				// The record is deliberately NOT updated, so the next notice still
				// rebuilds; and when the session was the only thing holding the
				// window up, its end tears the window down and the next build asks
				// `get_game_monitor` afresh.
				log(
					`the game moved to monitor ${notice.id} while widgets are being arranged — deferring the rebuild until the config session ends`
				);
				return;
			case 'unwanted':
				log(
					`the game moved to monitor ${notice.id}, but nothing wants the window any more — leaving it to the module flag`
				);
				return;
			case 'rebuild':
				log(`the game moved to monitor ${notice.id} — rebuilding the overlay there`);
				driver.setDesired(false);
				driver.setDesired(true);
				return;
		}
	}

	return { label, driver, configTarget, onGameMonitorChanged, log };
}
