import { describe, it, expect, vi, afterEach } from 'vitest';
import {
	MAX_CREATE_ATTEMPTS,
	TEMPLE_CREATE_TIMEOUT_MS,
	templeBegin,
	templeCreateWithTimeout,
	templeDesired,
	templeLifecycleInit,
	templeNextAction,
	templeRetryDelayMs,
	templeSettle,
	type TempleLifecycle
} from './temple-lifecycle';

/**
 * Run one full action the way the layout does: ask, begin, settle.
 *
 * The layout's `pump` is exactly this plus the Tauri work in the middle, so
 * driving the machine through the same three steps is what makes these tests
 * about the real sequence rather than about three functions in isolation.
 */
function step(state: TempleLifecycle, ok = true): TempleLifecycle {
	const action = templeNextAction(state);
	if (action === 'none') return state;
	return templeSettle(templeBegin(state, action), ok);
}

describe('the temple overlay lifecycle', () => {
	describe('deciding what to do next', () => {
		it('does nothing before the first SSOT poll reports the flag', () => {
			expect(templeNextAction(templeLifecycleInit())).toBe('none');
		});

		it('creates the window when the flag comes back on', () => {
			expect(templeNextAction(templeDesired(templeLifecycleInit(), true))).toBe('create');
		});

		it('does nothing when the flag comes back off with no window built', () => {
			// The startup case: the module is disabled and there is nothing to
			// tear down. A `destroy` here would run a five-round close/destroy
			// sweep against a label that was never created, on every launch.
			expect(templeNextAction(templeDesired(templeLifecycleInit(), false))).toBe('none');
		});

		it('destroys the window when the flag goes off after one was built', () => {
			const built = step(templeDesired(templeLifecycleInit(), true));
			expect(templeNextAction(templeDesired(built, false))).toBe('destroy');
		});

		it('does nothing while a creation is already in flight', () => {
			// The whole point of the machine. This state still reads "wanted but
			// not built" — which is exactly what asks for a create — and it must
			// not ask again, or a second `new WebviewWindow` would run against a
			// label Tauri is still building.
			const creating = templeBegin(templeDesired(templeLifecycleInit(), true), 'create');
			expect(creating.desired).toBe(true);
			expect(creating.actual).toBe(false);
			expect(templeNextAction(creating)).toBe('none');
		});

		it('does nothing while a destroy is already in flight', () => {
			const built = step(templeDesired(templeLifecycleInit(), true));
			const destroying = templeBegin(templeDesired(built, false), 'destroy');
			expect(destroying.actual).toBe(true);
			expect(templeNextAction(destroying)).toBe('none');
		});
	});

	describe('ordering a burst of toggles', () => {
		it('ends with the window destroyed after off, on, off', () => {
			// The race the serialisation exists for. Each flag change lands
			// while the previous action is still in flight, and the settle that
			// ends it is what re-asks — so the LAST flag wins rather than the
			// last action started.
			let s = templeDesired(templeLifecycleInit(), false);
			s = templeDesired(s, true);
			s = templeBegin(s, 'create');
			s = templeDesired(s, false); // flipped back mid-create
			s = templeSettle(s, true); // the window did get built
			expect(templeNextAction(s)).toBe('destroy');
			s = step(s);
			expect(s.actual).toBe(false);
			expect(templeNextAction(s)).toBe('none');
		});

		it('ends with the window built after on, off, on', () => {
			let s = templeDesired(templeLifecycleInit(), true);
			s = step(s);
			s = templeDesired(s, false);
			s = templeBegin(s, 'destroy');
			s = templeDesired(s, true); // flipped back mid-destroy
			s = templeSettle(s, true);
			expect(templeNextAction(s)).toBe('create');
			s = step(s);
			expect(s.actual).toBe(true);
			expect(templeNextAction(s)).toBe('none');
		});

		it('asks for nothing more once the window matches the flag', () => {
			const built = step(templeDesired(templeLifecycleInit(), true));
			expect(templeNextAction(built)).toBe('none');
		});
	});

	describe('a creation that fails', () => {
		it('asks for another attempt rather than treating the failure as terminal', () => {
			const failed = step(templeDesired(templeLifecycleInit(), true), false);
			expect(failed.actual).toBe(false);
			expect(templeNextAction(failed)).toBe('create');
		});

		it('stops asking once the attempt budget is spent', () => {
			let s = templeDesired(templeLifecycleInit(), true);
			for (let i = 0; i < MAX_CREATE_ATTEMPTS; i++) s = step(s, false);
			expect(s.attempts).toBe(MAX_CREATE_ATTEMPTS);
			expect(s.gaveUp).toBe(true);
			expect(templeNextAction(s)).toBe('none');
		});

		it('spends exactly the budget before giving up', () => {
			// One short of the budget must still be asking, or a wrong
			// comparison could cut the retries to two and nothing above would
			// notice.
			let s = templeDesired(templeLifecycleInit(), true);
			for (let i = 0; i < MAX_CREATE_ATTEMPTS - 1; i++) s = step(s, false);
			expect(s.gaveUp).toBe(false);
			expect(templeNextAction(s)).toBe('create');
		});

		it('takes the module being toggled as permission to try again', () => {
			let s = templeDesired(templeLifecycleInit(), true);
			for (let i = 0; i < MAX_CREATE_ATTEMPTS; i++) s = step(s, false);
			s = templeDesired(s, false);
			s = templeDesired(s, true);
			expect(s.attempts).toBe(0);
			expect(templeNextAction(s)).toBe('create');
		});

		it('does not refresh the budget when the poll repeats the same flag', () => {
			// `ssot.modules` is polled every few seconds. If re-reporting `true`
			// cleared the counter, a window that can never be built would retry
			// for as long as the app runs.
			let s = templeDesired(templeLifecycleInit(), true);
			s = step(s, false);
			s = templeDesired(s, true);
			expect(s.attempts).toBe(1);
		});

		it('clears the failures a later successful creation followed', () => {
			// So a failure hours later gets the full budget rather than the
			// remainder of an old one.
			let s = templeDesired(templeLifecycleInit(), true);
			s = step(s, false);
			s = step(s, true);
			expect(s.attempts).toBe(0);
			expect(s.actual).toBe(true);
		});

		it('backs off further with each attempt', () => {
			expect(templeRetryDelayMs(1)).toBe(500);
			expect(templeRetryDelayMs(2)).toBe(1000);
			expect(templeRetryDelayMs(3)).toBe(2000);
		});
	});

	describe('a destroy that fails', () => {
		it('records the window as gone instead of queueing the destroy again', () => {
			// `destroyTempleWindow` already retries five times internally, so a
			// reported failure is the end of what this scheduler can usefully
			// ask for — and holding `actual` true would repeat that sweep for
			// as long as the module stays off.
			const built = step(templeDesired(templeLifecycleInit(), true));
			const s = step(templeDesired(built, false), false);
			expect(s.actual).toBe(false);
			expect(templeNextAction(s)).toBe('none');
		});
	});
});

describe('a creation that never settles', () => {
	afterEach(() => {
		vi.useRealTimers();
	});

	/** A creation whose Tauri events never arrive: the promise stays pending. */
	function neverSettles(): Promise<boolean> {
		return new Promise<boolean>(() => {});
	}

	it('frees the machine to act on the next flag change', async () => {
		// The whole point of the deadline. `createTempleOverlay` resolves from
		// `tauri://created` / `tauri://error`; with neither delivered the
		// awaited promise never returns, `pending` stays `'create'`, and every
		// later flag change is answered with `'none'` — the module reads as off
		// while a transparent always-on-top window sits over the game with no
		// way left to remove it.
		vi.useFakeTimers();
		const onTimeout = vi.fn();
		let s = templeBegin(templeDesired(templeLifecycleInit(), true), 'create');

		const raced = templeCreateWithTimeout(neverSettles, onTimeout);
		await vi.advanceTimersByTimeAsync(TEMPLE_CREATE_TIMEOUT_MS);
		s = templeSettle(s, await raced);

		expect(onTimeout).toHaveBeenCalledTimes(1);
		expect(s.pending).toBe('none');
		// `pending` free is what makes the machine answerable again: a stuck
		// `'create'` also reads `'none'` from `templeNextAction`, so the
		// discriminating question is whether a later toggle gets acted on.
		s = templeDesired(s, false);
		s = templeDesired(s, true);
		expect(templeNextAction(s)).toBe('create');
	});

	it('reports the timeout as a failed attempt, not as a built window', async () => {
		// A timeout means "we do not know what got built". Recording it as a
		// success would leave `actual` true against a window that may not
		// exist; recording it as a failure sends the pump back through a
		// retry, whose first step is the destroy sweep that cleans up whatever
		// the slow creation eventually produced.
		vi.useFakeTimers();
		let s = templeBegin(templeDesired(templeLifecycleInit(), true), 'create');

		const raced = templeCreateWithTimeout(neverSettles, () => {});
		await vi.advanceTimersByTimeAsync(TEMPLE_CREATE_TIMEOUT_MS);
		s = templeSettle(s, await raced);

		expect(s.actual).toBe(false);
		expect(s.attempts).toBe(1);
		expect(templeNextAction(s)).toBe('create');
	});

	it('leaves a creation that finishes inside the deadline alone', async () => {
		// The negative half: the deadline must not overrule a real answer, or
		// every slow-but-successful build would be torn down and retried.
		vi.useFakeTimers();
		const onTimeout = vi.fn();

		const raced = templeCreateWithTimeout(
			() => new Promise<boolean>((r) => setTimeout(() => r(true), TEMPLE_CREATE_TIMEOUT_MS - 1)),
			onTimeout
		);
		await vi.advanceTimersByTimeAsync(TEMPLE_CREATE_TIMEOUT_MS * 2);

		expect(await raced).toBe(true);
		expect(onTimeout).not.toHaveBeenCalled();
	});

	it('passes a creation failure through as the failure it is', async () => {
		vi.useFakeTimers();
		const onTimeout = vi.fn();

		const raced = templeCreateWithTimeout(async () => false, onTimeout);
		await vi.advanceTimersByTimeAsync(TEMPLE_CREATE_TIMEOUT_MS * 2);

		expect(await raced).toBe(false);
		expect(onTimeout).not.toHaveBeenCalled();
	});

	it('re-throws a creation that rejects rather than reporting it as a timeout', async () => {
		// The layout's own catch logs which step threw; folding a rejection
		// into `false` here would take that message away.
		vi.useFakeTimers();
		const onTimeout = vi.fn();

		const raced = templeCreateWithTimeout(async () => {
			throw new Error('window label already in use');
		}, onTimeout);

		await expect(raced).rejects.toThrow('window label already in use');
		expect(onTimeout).not.toHaveBeenCalled();
	});
});
