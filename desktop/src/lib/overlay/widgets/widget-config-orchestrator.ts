/**
 * The widget-config flow (POE-226, rescoped by POE-241; moved out of
 * `routes/(app)/+layout.svelte` in POE-346): Settings emits
 * `widget-config-start {module}`, and this runs the three ordered steps of the
 * "Config-mode ordering contract" in `docs/OVERLAY-GUIDE.md`, the deadline that
 * floors a session the host never ends, and the abandon that takes a window back
 * out of config mode.
 *
 * Tauri-free: every effect goes through `WidgetConfigPorts`, built in the layout
 * (which also keeps the two `listen` registrations and the sessions rune).
 */
import {
	widgetConfigEnd,
	widgetConfigStart,
	type WidgetConfigSessions
} from './widget-config-session';

/** The slice of a Tauri webview window this flow drives. */
export interface ConfigWindow {
	isVisible(): Promise<boolean>;
	show(): Promise<void>;
	hide(): Promise<void>;
}

/** The overlay window behind a module that has widgets. */
export interface WidgetConfigTarget {
	label: string;
	built: () => boolean;
}

export type TimerHandle = unknown;

export type WidgetConfigPorts = {
	/** May reject; a rejection is logged and read as "no window". */
	getWindowByLabel: (label: string) => Promise<ConfigWindow | null>;
	invoke: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
	/** Webview-scoped emit to one window. */
	emitTo: (label: string, event: string, payload: unknown) => Promise<void>;
	/** Emit from the main webview (what Settings listens on). */
	emitToMain: (event: string, payload: unknown) => Promise<void>;
	getSessions: () => WidgetConfigSessions;
	setSessions: (next: WidgetConfigSessions) => void;
	/** Read at call time, keyed by window label. */
	targets: () => Record<string, WidgetConfigTarget>;
	setTimer: (callback: () => void, ms: number) => TimerHandle;
	clearTimer: (timer: TimerHandle) => void;
	wait: (ms: number) => Promise<void>;
	now: () => number;
	warn: (...args: unknown[]) => void;
	error: (...args: unknown[]) => void;
};

export interface WidgetConfigOrchestrator {
	start(module: string): Promise<void>;
	end(module: string): Promise<void>;
}

/**
 * How long a session gets to produce its window.
 *
 * Recording the session is what BUILDS the window: `widgetConfigSessions` is
 * a rune, the layout's overlay effect ORs it into the driver's desired state,
 * and the driver creates. That path is bounded by the driver's own
 * `CREATE_TIMEOUT_MS` (10 s) plus a retry, so waiting the same 10 s here
 * covers a first attempt that succeeds slowly without waiting out a window
 * that is never coming.
 */
export const WIDGET_WINDOW_WAIT_MS = 10_000;
export const WIDGET_WINDOW_POLL_MS = 150;

/**
 * How long an OPEN config session may run before this orchestrator ends it itself.
 *
 * Not the same clock as Settings' opening deadline, which bounds the OPENING
 * only and stands down on `widget-config-open`. This one starts where that
 * one stops, and it exists because the host can now decline to end a session:
 * `WidgetHost.svelte` keeps config mode when `set_overlay_config_mode(label,
 * false)` refuses, which is right for the user in front of the window and
 * leaves everyone else waiting — Settings on `Configuring…`, the window
 * force-shown, and a window that only the session record is holding up.
 * Without a floor under it, a window whose exit can never succeed leaves a
 * monitor-sized INTERACTIVE rectangle over the game until the app is
 * restarted: the mouse hook deliberately skips a window in config mode, so
 * every click where the widgets are stops reaching PoE.
 *
 * Ten minutes because it must sit far above real arranging — the session ends
 * when the user presses Save or Cancel, however long they take, and dragging
 * widgets around a board is minutes, not seconds — and far below "for the
 * rest of the session". A user still arranging at ten minutes loses the
 * frames and presses Configure again; a stuck one gets their clicks back.
 */
export const WIDGET_CONFIG_SESSION_MAX_MS = 10 * 60_000;

export function createWidgetConfigOrchestrator(ports: WidgetConfigPorts): WidgetConfigOrchestrator {
	/**
	 * The armed deadlines, by module. Also the record of which sessions this
	 * orchestrator has seen OPEN and not yet seen end — which is what the timer
	 * needs and the sessions record cannot answer, because a session that forced
	 * nothing still has to be reported to Settings.
	 */
	const widgetConfigDeadlines = new Map<string, TimerHandle>();

	function clearWidgetConfigDeadline(module: string): void {
		if (!widgetConfigDeadlines.has(module)) return;
		ports.clearTimer(widgetConfigDeadlines.get(module));
		widgetConfigDeadlines.delete(module);
	}

	/** Arm the deadline for a session that is now open. Re-arming on a second
	 *  Configure press restarts the clock, which is the honest reading: the user
	 *  just told us they are still working on this module. */
	function armWidgetConfigDeadline(module: string): void {
		clearWidgetConfigDeadline(module);
		widgetConfigDeadlines.set(
			module,
			ports.setTimer(() => {
				widgetConfigDeadlines.delete(module);
				logWidgetConfig(
					module,
					`still in configuration after ${Math.round(WIDGET_CONFIG_SESSION_MAX_MS / 60_000)} min — ending the session from here`
				);
				void abandonWidgetConfig(module);
			}, WIDGET_CONFIG_SESSION_MAX_MS)
		);
	}

	/** The starts currently in flight, by module. A second Configure press during
	 *  the wait above would re-derive the session against a half-built world,
	 *  fail, and abandon the FIRST press — disabling the module under it. */
	const widgetConfigStarting = new Set<string>();

	function logWidgetConfig(module: string, msg: string): void {
		ports.warn(`[widget-config] ${module}: ${msg}`);
		ports
			.invoke('app_log_from_frontend', { msg: `[widget-config] ${module}: ${msg}` })
			.catch((e) => ports.error('[widget-config] app log unreachable:', e));
	}

	async function widgetConfigWindow(label: string): Promise<ConfigWindow | null> {
		return await ports.getWindowByLabel(label).catch((e) => {
			logWidgetConfig(label, `window lookup failed: ${e}`);
			return null;
		});
	}

	/**
	 * Wait for a module's window to be BUILT, not merely to exist.
	 *
	 * Polling rather than an event because the creation belongs to the driver,
	 * which reports to itself; `built()` is the marker it settles, and the window
	 * lookup is the second half because a create that failed leaves `built()`
	 * false and a destroy leaves the label gone.
	 */
	async function waitForWidgetConfigWindow(target: WidgetConfigTarget): Promise<ConfigWindow | null> {
		const deadline = ports.now() + WIDGET_WINDOW_WAIT_MS;
		for (;;) {
			if (target.built()) {
				const win = await widgetConfigWindow(target.label);
				if (win) return win;
			}
			if (ports.now() >= deadline) return null;
			await ports.wait(WIDGET_WINDOW_POLL_MS);
		}
	}

	async function startWidgetConfig(module: string): Promise<void> {
		// The guard covers the whole function, the multi-second wait included: a
		// second press then would re-derive against a half-built world, throw, and
		// abandon the press that is still working. It is also what makes a
		// duplicated listener (an HMR reload registers a second one; the layout
		// never unmounts, so nothing removes the first) harmless.
		if (widgetConfigStarting.has(module)) {
			logWidgetConfig(module, 'a config session is already opening — ignoring the second press');
			return;
		}
		widgetConfigStarting.add(module);
		try {
			await openWidgetConfig(module);
		} finally {
			widgetConfigStarting.delete(module);
		}
	}

	async function openWidgetConfig(module: string): Promise<void> {
		// "I have the request and I am working on it." Settings' opening deadline
		// distinguishes a start that is merely SLOW — the window wait alone runs
		// to 10 s — from one that nothing ever picked up, and only the second is
		// safe to abandon: tearing the session down under a start still in flight
		// would leave that start setting config mode on a window just torn down.
		await ports
			.emitToMain('widget-config-opening', { module })
			.catch((e: any) => logWidgetConfig(module, `could not report the start to Settings: ${e}`));
		const target = ports.targets()[module];
		if (!target) {
			logWidgetConfig(module, 'no overlay window is registered for this module');
			await abandonWidgetConfig(module);
			return;
		}
		let win = await widgetConfigWindow(target.label);
		const pre = { shown: win ? await win.isVisible().catch(() => false) : false };
		const started = widgetConfigStart(ports.getSessions(), module, pre);
		// The record IS the request for the window (POE-241): the layout's overlay
		// effect ORs `widgetConfigLive` into the driver's desired state, so this
		// assignment is what builds a window for a module whose flag is off — and
		// it starts no module work, so nothing here turns on a capture loop or an
		// OCR pass. It also has to land before `widget-window.ts`'s `create` runs, which
		// reads the same record: a window built for this session must not run its
		// not-focused-so-hide step.
		ports.setSessions(started.sessions);
		try {
			// Whenever there is no window yet — not only when the module was off. A
			// module that reads as on can still be mid-build (a restart, a retry
			// after a failed create), and abandoning immediately would take the
			// session down with it.
			if (!win) win = await waitForWidgetConfigWindow(target);
			if (!win) throw new Error(`the ${target.label} window did not appear`);
			// Step 1: on screen. Unconditional rather than gated on
			// `actions.showWindow`, because a window built during the wait ran its
			// own focus check on the way up; on an already-visible window this is a
			// no-op. The focus poller will not undo it — it acts on transitions,
			// and Rust now also leaves a window in config mode alone.
			await win.show();
			// Step 2: the Rust flag BEFORE the event, so the mouse hook is already
			// leaving the window alone — and so the host's catch-up query cannot
			// answer false for a window created a moment ago.
			await ports.invoke('set_overlay_config_mode', { label: target.label, on: true });
			// Step 3: webview-scoped, so a second module's host never hears it.
			await ports.emitTo(target.label, 'widget-config', { module, on: true });
			// The acknowledgement Settings' deadline waits on. Without it, a
			// listener that never registered (or a start that never ran) leaves the
			// button on "Configuring…" with nothing ever answering.
			await ports
				.emitToMain('widget-config-open', { module })
				.catch((e: any) => logWidgetConfig(module, `could not confirm config mode to Settings: ${e}`));
			// Armed where Settings' opening deadline stands down, so exactly one
			// clock is running on this module at any moment.
			armWidgetConfigDeadline(module);
		} catch (e) {
			// Settings is sitting on "Configuring...", and nothing else will
			// answer it: the host that emits `widget-config-end` is in the window
			// that failed to open.
			logWidgetConfig(module, `could not open config mode: ${e}`);
			await abandonWidgetConfig(module);
		}
	}

	async function endWidgetConfig(module: string): Promise<void> {
		// First, and unconditionally: this runs for an end that arrived normally,
		// for one this orchestrator's own deadline produced, and for a module that
		// never had a session at all. A timer left armed past any of them would
		// abandon the NEXT session ten minutes in.
		clearWidgetConfigDeadline(module);
		const target = ports.targets()[module];
		// The state of the world NOW, not when the session opened: if the game is
		// back in front, the poller has already shown this window and wants it
		// shown, and hiding it would cost the player their overlay until two more
		// focus transitions. A status read that fails is treated as "not focused",
		// which restores the pre-session state — the conservative answer, since the
		// session only ever forced a show when the game was NOT focused.
		const status = (await ports.invoke('get_status').catch((e: any) => {
			logWidgetConfig(module, `could not read game focus, assuming unfocused: ${e}`);
			return null;
		})) as { game_focused?: boolean } | null;
		const ended = widgetConfigEnd(ports.getSessions(), module, status?.game_focused === true);
		if (target && ended.actions.hideWindow) {
			const win = await widgetConfigWindow(target.label);
			if (win) await win.hide().catch((e: any) => logWidgetConfig(module, `hide failed: ${e}`));
		}
		// LAST, because dropping the record is what lowers the window's desired
		// state: for a module whose flag is off, the driver tears the window down
		// on this assignment. Hiding first therefore hides a window that still
		// exists, rather than racing a teardown into a "hide failed" line — the
		// same ordering the force-disable used to need, for the same reason.
		ports.setSessions(ended.sessions);
	}

	/**
	 * End the session from THIS side: take the window out of config mode, undo
	 * what the session forced, and tell Settings.
	 *
	 * Two callers with the same need. A start that never got as far as a host
	 * (no window, or a command that failed) — and the deadline above, which fires
	 * on a session the host has declined to end. The second is why the window
	 * half exists at all: the host listens for `widget-config` and nothing else,
	 * so restoring the visibility and the button without telling the window would
	 * leave Rust's `config_mode` set — the re-assert in `WidgetHost.exitConfig`
	 * puts it back on a refused exit — and with it a monitor-sized interactive
	 * rectangle the mouse hook deliberately skips, with no Settings button left
	 * to say so.
	 *
	 * The window is dealt with BEFORE the restore, because the restore may hide
	 * or tear the window down, and the three steps are in this order:
	 *
	 * 1. `widget-config {on: false}` to the window. The HOST owns the Rust call
	 *    — its handler runs `exitConfig` — so the normal path keeps one owner and
	 *    the host clears its frames, its draft and its bar itself.
	 * 2. The belt, direct to Rust. It covers a host that is unreachable (no
	 *    listener, a window mid-teardown) and a host whose own exit REFUSED, in
	 *    which case it has just re-asserted config mode and would otherwise leave
	 *    the flag set behind us. On an abandon the layout wins: the session is
	 *    being taken down whether the window agrees or not.
	 * 3. The same event again. Free when the host has already exited (its
	 *    `configMode` guard drops it), and it is what closes the one gap step 2
	 *    opens: a host that refused in step 1 is still drawing frames over a
	 *    window that is click-through again, and this repeat is the retry that
	 *    now succeeds, because step 2 has already put Rust where the host needs
	 *    it. Without it the user is left with painted frames and dead buttons.
	 *
	 * The echo the final `widget-config-end` produces on our own listener is a
	 * no-op — the session is already closed by then.
	 */
	async function abandonWidgetConfig(module: string): Promise<void> {
		const target = ports.targets()[module];
		if (target) {
			await ports.emitTo(target.label, 'widget-config', { module, on: false }).catch((e: any) =>
				logWidgetConfig(module, `could not ask the window to leave config mode: ${e}`)
			);
			// Expected to fail when there is no window — this is also the path a
			// start that never built one takes — so it is a note, not an alarm.
			await ports.invoke('set_overlay_config_mode', { label: target.label, on: false }).catch(
				(e: any) => logWidgetConfig(module, `direct config-mode clear did not apply: ${e}`)
			);
			await ports.emitTo(target.label, 'widget-config', { module, on: false }).catch((e: any) =>
				logWidgetConfig(module, `could not repeat the leave request: ${e}`)
			);
		}
		await endWidgetConfig(module);
		await ports
			.emitToMain('widget-config-end', { module })
			.catch((e: any) => logWidgetConfig(module, `could not report the failure to Settings: ${e}`));
	}

	return { start: startWidgetConfig, end: endWidgetConfig };
}
