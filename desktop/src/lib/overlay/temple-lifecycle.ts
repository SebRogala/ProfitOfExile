/**
 * The temple overlay window's create/destroy scheduler, as a pure state machine.
 *
 * It lives outside `routes/(app)/+layout.svelte` for the reason the rest of this
 * feature's logic does: a `.svelte` file has no unit-test harness in this app,
 * and the interesting part here is not the Tauri calls — it is the ORDERING.
 * The module flag can flip faster than a window can be built, and the layout's
 * only defence used to be a promise chain plus a boolean that the create path
 * set from an event handler nobody awaited, so an off→on→off burst could leave
 * a transparent always-on-top window standing with the module switched off.
 *
 * So this file owns the decision and the layout owns the effects:
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
 * clock, no Tauri. The one exception is `templeCreateWithTimeout` at the foot
 * of the file: it owns a clock and nothing else, and lives here rather than in
 * the layout for the same reason the machine does — a `.svelte` file has no
 * unit-test harness. `temple-lifecycle.test.ts` drives both through the real
 * sequences.
 */

/** What the layout should do next. */
export type TempleLifecycleAction = 'none' | 'create' | 'destroy';

/** The scheduler's whole state. */
export interface TempleLifecycle {
	/** The module flag, or `undefined` before the first poll answers. */
	desired: boolean | undefined;
	/** Whether a window is believed to exist. */
	actual: boolean;
	/** The action in flight, `'none'` when idle. */
	pending: TempleLifecycleAction;
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

/** What the layout logs once the budget is spent. */
export const TEMPLE_GAVE_UP_NOTE = 'giving up until the module is toggled';

/** Backoff before retry number `attempt` (1-based): 500 ms, 1 s, 2 s. */
export function templeRetryDelayMs(attempt: number): number {
	return 500 * 2 ** Math.max(0, attempt - 1);
}

/** The state before the first poll: nothing wanted, nothing built. */
export function templeLifecycleInit(): TempleLifecycle {
	return { desired: undefined, actual: false, pending: 'none', attempts: 0, gaveUp: false };
}

/**
 * Record what the module flag now says.
 *
 * A CHANGE clears the retry budget — that is what "until the module is toggled"
 * means. Re-reporting the same value does not, or a poll every three seconds
 * would hand a permanently failing creation an unlimited budget.
 */
export function templeDesired(state: TempleLifecycle, desired: boolean): TempleLifecycle {
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
export function templeNextAction(state: TempleLifecycle): TempleLifecycleAction {
	if (state.pending !== 'none') return 'none';
	if (state.desired === undefined) return 'none';
	if (state.desired === state.actual) return 'none';
	if (!state.desired) return 'destroy';
	return state.gaveUp ? 'none' : 'create';
}

/** Mark an action started. */
export function templeBegin(
	state: TempleLifecycle,
	action: Exclude<TempleLifecycleAction, 'none'>
): TempleLifecycle {
	return { ...state, pending: action };
}

/**
 * Mark the in-flight action finished.
 *
 * A failed destroy still records the window as gone: `destroyTempleWindow`
 * carries its own retry loop, so by the time it reports failure there is
 * nothing further this scheduler could usefully ask for, and holding `actual`
 * true would queue a destroy that repeats forever. The next enable starts with
 * a destroy sweep anyway, which is where a genuinely surviving window is
 * cleaned up.
 */
export function templeSettle(state: TempleLifecycle, ok: boolean): TempleLifecycle {
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
 * `createTempleOverlay` resolves from `tauri://created` / `tauri://error` —
 * neither of which is guaranteed to arrive. If both are lost, that promise
 * never settles, `pending` stays `'create'` for the life of the process, and
 * `templeNextAction` answers `'none'` to every later flag change: a module-off
 * can no longer tear the window down. Ten seconds because it must sit well
 * above a slow-but-real WebView2 boot (a cold first overlay is seconds, not
 * tens of seconds) and well below the patience of someone who just toggled a
 * module off and is watching a window that will not go away.
 */
export const TEMPLE_CREATE_TIMEOUT_MS = 10_000;

/**
 * Race a creation against that deadline.
 *
 * A timeout is reported as an ordinary failed creation (`false`), which is what
 * makes it safe: `templeSettle` frees `pending` and asks for a retry, and the
 * retry's FIRST step is the destroy sweep — so a window the slow creation
 * eventually produced is cleaned up there rather than left standing.
 *
 * The late promise is deliberately not cancelled; nothing can cancel a Tauri
 * window constructor. It is simply no longer listened to. A rejection is
 * re-thrown rather than folded into `false` so the layout's own catch still
 * logs which step threw.
 */
export function templeCreateWithTimeout(
	create: () => Promise<boolean>,
	onTimeout: () => void,
	timeoutMs: number = TEMPLE_CREATE_TIMEOUT_MS
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
