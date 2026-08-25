/**
 * A MODULE-COUPLED overlay window's create/destroy scheduler: a pure state
 * machine, plus the driver that runs it.
 *
 * Keyed by (window label, module id) — the overlays that live and die with a
 * module flag rather than with an overlay setting of their own. Two callers
 * today, the temple board (POE-171) and the merc verdict strip (POE-199), and
 * they differ only in those two strings and in what their `create` closure
 * builds.
 *
 * It lives outside `routes/(app)/+layout.svelte` for the reason the rest of this
 * feature's logic does: a `.svelte` file has no unit-test harness in this app,
 * and the interesting part here is not the Tauri calls — it is the ORDERING.
 * The module flag can flip faster than a window can be built, and the layout's
 * only defence used to be a promise chain plus a boolean that the create path
 * set from an event handler nobody awaited, so an off→on→off burst could leave
 * a transparent always-on-top window standing with the module switched off.
 *
 * So this file owns the decision and the caller owns the effects:
 *
 * - `desired` — what the module flag says. `undefined` until the first SSOT
 *   poll answers, and deliberately NOT treated as `false`: tearing a window
 *   down on a value nobody has reported would fight the startup poll.
 * - `actual` — whether a window is believed to exist.
 * - `pending` — the action in flight, or `'none'`. One at a time, always.
 * - `attempts` / `gaveUp` — the bounded retry. A window that cannot be built is
 *   built again a few times and then left alone, because retrying forever
 *   writes a log line per attempt for as long as the app runs.
 *
 * Every state function is total and returns a NEW state — no mutation, no
 * clock, no Tauri. Two things at the foot of the file are not pure and say so:
 * `moduleOverlayCreateWithTimeout`, which owns a clock and nothing else, and
 * `moduleOverlayDriver`, which owns the mutable state, the serialisation and
 * the retry timer — everything the layout used to hold in three module-level
 * variables per overlay. Both live here rather than in the layout for the same
 * reason the machine does: a `.svelte` file has no unit-test harness.
 * `module-lifecycle.test.ts` drives all three through the real sequences.
 */

/** What the driver should do next. */
export type ModuleOverlayAction = 'none' | 'create' | 'destroy';

/** The scheduler's whole state. */
export interface ModuleOverlayLifecycle {
	/** The module flag, or `undefined` before the first poll answers. */
	desired: boolean | undefined;
	/** Whether a window is believed to exist. */
	actual: boolean;
	/** The action in flight, `'none'` when idle. */
	pending: ModuleOverlayAction;
	/** Consecutive failed creations since the flag last moved. */
	attempts: number;
	/** The retry budget is spent; only a flag change clears it. */
	gaveUp: boolean;
}

/**
 * How many times a failed creation is retried before the module is left alone.
 *
 * Three because the failures this covers are transient by nature — Tauri label
 * cleanup is asynchronous and has produced "already exists" on a fast toggle
 * (`docs/OVERLAY-GUIDE.md` guard 4). A failure that survives three attempts with
 * backoff is not a race, and repeating it costs a log line a second forever.
 */
export const MAX_CREATE_ATTEMPTS = 3;

/** What the driver logs once the budget is spent. */
export const GAVE_UP_NOTE = 'giving up until the module is toggled';

/** Backoff before retry number `attempt` (1-based): 500 ms, 1 s, 2 s. */
export function moduleOverlayRetryDelayMs(attempt: number): number {
	return 500 * 2 ** Math.max(0, attempt - 1);
}

/** The state before the first poll: nothing wanted, nothing built. */
export function moduleOverlayInit(): ModuleOverlayLifecycle {
	return { desired: undefined, actual: false, pending: 'none', attempts: 0, gaveUp: false };
}

/**
 * Record what the module flag now says.
 *
 * A CHANGE clears the retry budget — that is what "until the module is toggled"
 * means. Re-reporting the same value does not, or a poll every three seconds
 * would hand a permanently failing creation an unlimited budget.
 */
export function moduleOverlayDesired(state: ModuleOverlayLifecycle, desired: boolean): ModuleOverlayLifecycle {
	if (state.desired === desired) return state;
	return { ...state, desired, attempts: 0, gaveUp: false };
}

/**
 * What to do next, given only the state.
 *
 * Nothing is decided while an action is in flight: the settle that ends it
 * re-asks, so a flag that moved mid-flight is acted on then rather than
 * interleaved with the work already running.
 */
export function moduleOverlayNextAction(state: ModuleOverlayLifecycle): ModuleOverlayAction {
	if (state.pending !== 'none') return 'none';
	if (state.desired === undefined) return 'none';
	if (state.desired === state.actual) return 'none';
	if (!state.desired) return 'destroy';
	return state.gaveUp ? 'none' : 'create';
}

/** Mark an action started. */
export function moduleOverlayBegin(
	state: ModuleOverlayLifecycle,
	action: Exclude<ModuleOverlayAction, 'none'>
): ModuleOverlayLifecycle {
	return { ...state, pending: action };
}

/**
 * Mark the in-flight action finished.
 *
 * A failed destroy still records the window as gone: the caller's `destroy`
 * carries its own retry loop, so by the time it reports failure there is
 * nothing further this scheduler could usefully ask for, and holding `actual`
 * true would queue a destroy that repeats forever. The next enable starts with
 * a destroy sweep anyway, which is where a genuinely surviving window is
 * cleaned up.
 */
export function moduleOverlaySettle(state: ModuleOverlayLifecycle, ok: boolean): ModuleOverlayLifecycle {
	if (state.pending === 'none') return state;
	if (state.pending === 'destroy') {
		return { ...state, actual: false, pending: 'none' };
	}
	if (ok) {
		return { ...state, actual: true, pending: 'none', attempts: 0, gaveUp: false };
	}
	const attempts = state.attempts + 1;
	return {
		...state,
		actual: false,
		pending: 'none',
		attempts,
		gaveUp: attempts >= MAX_CREATE_ATTEMPTS
	};
}

/**
 * How long a creation may sit unsettled before it is called failed.
 *
 * A caller's `create` resolves from `tauri://created` / `tauri://error` —
 * neither of which is guaranteed to arrive. If both are lost, that promise
 * never settles, `pending` stays `'create'` for the life of the process, and
 * `moduleOverlayNextAction` answers `'none'` to every later flag change: a module-off
 * can no longer tear the window down. Ten seconds because it must sit well
 * above a slow-but-real WebView2 boot (a cold first overlay is seconds, not
 * tens of seconds) and well below the patience of someone who just toggled a
 * module off and is watching a window that will not go away.
 */
export const CREATE_TIMEOUT_MS = 10_000;

/**
 * Race a creation against that deadline.
 *
 * A timeout is reported as an ordinary failed creation (`false`), which is what
 * makes it safe: `moduleOverlaySettle` frees `pending` and asks for a retry, and the
 * retry's FIRST step is the destroy sweep — so a window the slow creation
 * eventually produced is cleaned up there rather than left standing.
 *
 * The late promise is deliberately not cancelled; nothing can cancel a Tauri
 * window constructor. It is simply no longer listened to. A rejection is
 * re-thrown rather than folded into `false` so the driver's own catch still
 * logs which step threw.
 */
export function moduleOverlayCreateWithTimeout(
	create: () => Promise<boolean>,
	onTimeout: () => void,
	timeoutMs: number = CREATE_TIMEOUT_MS
): Promise<boolean> {
	return new Promise<boolean>((resolve, reject) => {
		let settled = false;
		const finish = (ok: boolean) => {
			if (settled) return;
			settled = true;
			clearTimeout(timer);
			resolve(ok);
		};
		const timer = setTimeout(() => {
			onTimeout();
			finish(false);
		}, timeoutMs);
		create().then(finish, (e) => {
			if (settled) return;
			settled = true;
			clearTimeout(timer);
			reject(e);
		});
	});
}

// ------------------------------------------------------------- the driver --

/** Which window, and which module flag owns it. */
export interface ModuleOverlayKey {
	/** The Tauri window label — also the `/overlay/<label>` route segment, and an
	 *  entry `src-tauri/capabilities/default.json` must carry (guide guard 1). */
	label: string;
	/** The `src-tauri/src/modules.rs` registry id `ssot.modules` is keyed by. */
	moduleId: string;
}

/** The Tauri work the driver orders. Both report failure as `false`, never throw. */
export interface ModuleOverlayEffects {
	/** Build the window. `true` only once it is positioned, sized and click-through. */
	create: () => Promise<boolean>;
	/** Tear it down. `true` when the label is free afterwards. */
	destroy: () => Promise<boolean>;
	/**
	 * Where a failure is reported — the app log, not just the console.
	 *
	 * The CALLER identifies the window in this channel (its own prefix), so
	 * nothing here adds the label on top: the caller logs its own failures
	 * through the same function, and a prefix added in both places would read
	 * `[merc-overlay] mercenary: …`.
	 */
	log: (msg: string) => void;
}

/** What the layout holds onto: one call per module-flag reading. */
export interface ModuleOverlayDriver {
	/**
	 * Report what the module flag now says. The same value twice is a no-op, so
	 * a poll every three seconds costs nothing and never refreshes a spent retry
	 * budget.
	 */
	setDesired(enabled: boolean): void;
	/** Whether a window is believed to exist. */
	built(): boolean;
}

/**
 * Run one module-coupled overlay: the machine, the serialisation and the
 * bounded retry, with the Tauri calls injected.
 *
 * The mutable state is deliberately NOT a rune. Nothing renders from it, and
 * the layout's effect must depend on the module flag alone — a reactive
 * lifecycle would re-enter itself on every settle.
 *
 * Every step is appended to one promise chain, so the Tauri calls happen one at
 * a time however fast the flag moves, and the settle that ends an action is
 * what re-asks — which is how the LAST flag wins rather than the last action
 * started.
 */
export function moduleOverlayDriver(
	key: ModuleOverlayKey,
	effects: ModuleOverlayEffects
): ModuleOverlayDriver {
	let state = moduleOverlayInit();
	let work: Promise<void> = Promise.resolve();
	let retryTimer: ReturnType<typeof setTimeout> | null = null;

	function cancelRetry(): void {
		if (retryTimer === null) return;
		clearTimeout(retryTimer);
		retryTimer = null;
	}

	/**
	 * Queue another creation attempt, or say out loud that we have stopped.
	 *
	 * A failed creation used to be terminal and silent — the module read as on,
	 * the window was never built, and nothing said so.
	 */
	function scheduleRetry(): void {
		if (state.gaveUp) {
			// The module id, not the window label: the caller's own prefix already
			// says which window, and "until the module is toggled" is useless
			// without naming the toggle it means.
			effects.log(
				`creation failed ${state.attempts} times — ${GAVE_UP_NOTE} (module ${key.moduleId})`
			);
			return;
		}
		if (moduleOverlayNextAction(state) !== 'create') return;
		const delay = moduleOverlayRetryDelayMs(state.attempts);
		effects.log(`creation attempt ${state.attempts} failed — retrying in ${delay} ms`);
		cancelRetry();
		retryTimer = setTimeout(() => {
			retryTimer = null;
			pump();
		}, delay);
	}

	function pump(): void {
		const action = moduleOverlayNextAction(state);
		if (action === 'none') return;
		state = moduleOverlayBegin(state, action);
		work = work
			.then(async () => {
				const ok =
					action === 'create'
						? // Bounded, not awaited forever: `create` settles from a
							// Tauri event, and an event that never arrives would
							// leave `pending` set to `'create'` for the life of the
							// process — after which no module-off could tear the
							// window down. See the constant.
							await moduleOverlayCreateWithTimeout(effects.create, () =>
								effects.log(
									`creation did not settle within ${CREATE_TIMEOUT_MS} ms — counting it as failed`
								)
							)
						: await effects.destroy();
				state = moduleOverlaySettle(state, ok);
				if (action === 'create' && !ok) {
					scheduleRetry();
					return;
				}
				pump();
			})
			.catch((e) => {
				// Neither effect throws by contract — both report failure as
				// `false`. If one ever does, the action still has to be settled
				// or `pending` would stay set and no window would be built again.
				state = moduleOverlaySettle(state, false);
				effects.log(`lifecycle step '${action}' threw: ${e}`);
				// …and settling alone is not enough. A throwing CREATE leaves
				// `desired` true against no window, and nothing re-asks: the
				// module reads as on with no overlay and no retry, for the life
				// of the process. This is the same continuation the non-throwing
				// path takes — retry a creation, re-ask after anything else — so
				// a `create` that throws costs a backoff, not the feature.
				if (action === 'create') scheduleRetry();
				else pump();
			});
	}

	return {
		setDesired(enabled: boolean): void {
			if (state.desired === enabled) return;
			state = moduleOverlayDesired(state, enabled);
			// A toggle is the one thing that clears a spent retry budget, so a
			// scheduled attempt from the previous flag value has nothing left to do.
			cancelRetry();
			pump();
		},
		built(): boolean {
			return state.actual;
		}
	};
}
