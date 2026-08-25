import { describe, it, expect, vi, afterEach } from 'vitest';
import {
	CREATE_TIMEOUT_MS,
	GAVE_UP_NOTE,
	MAX_CREATE_ATTEMPTS,
	moduleOverlayBegin,
	moduleOverlayCreateWithTimeout,
	moduleOverlayDesired,
	moduleOverlayDriver,
	moduleOverlayInit,
	moduleOverlayNextAction,
	moduleOverlayRetryDelayMs,
	moduleOverlaySettle,
	type ModuleOverlayLifecycle
} from './module-lifecycle';

/**
 * Run one full action the way the layout does: ask, begin, settle.
 *
 * The layout's `pump` is exactly this plus the Tauri work in the middle, so
 * driving the machine through the same three steps is what makes these tests
 * about the real sequence rather than about three functions in isolation.
 */
function step(state: ModuleOverlayLifecycle, ok = true): ModuleOverlayLifecycle {
	const action = moduleOverlayNextAction(state);
	if (action === 'none') return state;
	return moduleOverlaySettle(moduleOverlayBegin(state, action), ok);
}

describe('the module-coupled overlay lifecycle', () => {
	describe('deciding what to do next', () => {
		it('does nothing before the first SSOT poll reports the flag', () => {
			expect(moduleOverlayNextAction(moduleOverlayInit())).toBe('none');
		});

		it('creates the window when the flag comes back on', () => {
			expect(moduleOverlayNextAction(moduleOverlayDesired(moduleOverlayInit(), true))).toBe('create');
		});

		it('does nothing when the flag comes back off with no window built', () => {
			// The startup case: the module is disabled and there is nothing to
			// tear down. A `destroy` here would run a five-round close/destroy
			// sweep against a label that was never created, on every launch.
			expect(moduleOverlayNextAction(moduleOverlayDesired(moduleOverlayInit(), false))).toBe('none');
		});

		it('destroys the window when the flag goes off after one was built', () => {
			const built = step(moduleOverlayDesired(moduleOverlayInit(), true));
			expect(moduleOverlayNextAction(moduleOverlayDesired(built, false))).toBe('destroy');
		});

		it('does nothing while a creation is already in flight', () => {
			// The whole point of the machine. This state still reads "wanted but
			// not built" — which is exactly what asks for a create — and it must
			// not ask again, or a second `new WebviewWindow` would run against a
			// label Tauri is still building.
			const creating = moduleOverlayBegin(moduleOverlayDesired(moduleOverlayInit(), true), 'create');
			expect(creating.desired).toBe(true);
			expect(creating.actual).toBe(false);
			expect(moduleOverlayNextAction(creating)).toBe('none');
		});

		it('does nothing while a destroy is already in flight', () => {
			const built = step(moduleOverlayDesired(moduleOverlayInit(), true));
			const destroying = moduleOverlayBegin(moduleOverlayDesired(built, false), 'destroy');
			expect(destroying.actual).toBe(true);
			expect(moduleOverlayNextAction(destroying)).toBe('none');
		});
	});

	describe('ordering a burst of toggles', () => {
		it('ends with the window destroyed after off, on, off', () => {
			// The race the serialisation exists for. Each flag change lands
			// while the previous action is still in flight, and the settle that
			// ends it is what re-asks — so the LAST flag wins rather than the
			// last action started.
			let s = moduleOverlayDesired(moduleOverlayInit(), false);
			s = moduleOverlayDesired(s, true);
			s = moduleOverlayBegin(s, 'create');
			s = moduleOverlayDesired(s, false); // flipped back mid-create
			s = moduleOverlaySettle(s, true); // the window did get built
			expect(moduleOverlayNextAction(s)).toBe('destroy');
			s = step(s);
			expect(s.actual).toBe(false);
			expect(moduleOverlayNextAction(s)).toBe('none');
		});

		it('ends with the window built after on, off, on', () => {
			let s = moduleOverlayDesired(moduleOverlayInit(), true);
			s = step(s);
			s = moduleOverlayDesired(s, false);
			s = moduleOverlayBegin(s, 'destroy');
			s = moduleOverlayDesired(s, true); // flipped back mid-destroy
			s = moduleOverlaySettle(s, true);
			expect(moduleOverlayNextAction(s)).toBe('create');
			s = step(s);
			expect(s.actual).toBe(true);
			expect(moduleOverlayNextAction(s)).toBe('none');
		});

		it('asks for nothing more once the window matches the flag', () => {
			const built = step(moduleOverlayDesired(moduleOverlayInit(), true));
			expect(moduleOverlayNextAction(built)).toBe('none');
		});
	});

	describe('a creation that fails', () => {
		it('asks for another attempt rather than treating the failure as terminal', () => {
			const failed = step(moduleOverlayDesired(moduleOverlayInit(), true), false);
			expect(failed.actual).toBe(false);
			expect(moduleOverlayNextAction(failed)).toBe('create');
		});

		it('stops asking once the attempt budget is spent', () => {
			let s = moduleOverlayDesired(moduleOverlayInit(), true);
			for (let i = 0; i < MAX_CREATE_ATTEMPTS; i++) s = step(s, false);
			expect(s.attempts).toBe(MAX_CREATE_ATTEMPTS);
			expect(s.gaveUp).toBe(true);
			expect(moduleOverlayNextAction(s)).toBe('none');
		});

		it('spends exactly the budget before giving up', () => {
			// One short of the budget must still be asking, or a wrong
			// comparison could cut the retries to two and nothing above would
			// notice.
			let s = moduleOverlayDesired(moduleOverlayInit(), true);
			for (let i = 0; i < MAX_CREATE_ATTEMPTS - 1; i++) s = step(s, false);
			expect(s.gaveUp).toBe(false);
			expect(moduleOverlayNextAction(s)).toBe('create');
		});

		it('takes the module being toggled as permission to try again', () => {
			let s = moduleOverlayDesired(moduleOverlayInit(), true);
			for (let i = 0; i < MAX_CREATE_ATTEMPTS; i++) s = step(s, false);
			s = moduleOverlayDesired(s, false);
			s = moduleOverlayDesired(s, true);
			expect(s.attempts).toBe(0);
			expect(moduleOverlayNextAction(s)).toBe('create');
		});

		it('does not refresh the budget when the poll repeats the same flag', () => {
			// `ssot.modules` is polled every few seconds. If re-reporting `true`
			// cleared the counter, a window that can never be built would retry
			// for as long as the app runs.
			let s = moduleOverlayDesired(moduleOverlayInit(), true);
			s = step(s, false);
			s = moduleOverlayDesired(s, true);
			expect(s.attempts).toBe(1);
		});

		it('clears the failures a later successful creation followed', () => {
			// So a failure hours later gets the full budget rather than the
			// remainder of an old one.
			let s = moduleOverlayDesired(moduleOverlayInit(), true);
			s = step(s, false);
			s = step(s, true);
			expect(s.attempts).toBe(0);
			expect(s.actual).toBe(true);
		});

		it('backs off further with each attempt', () => {
			expect(moduleOverlayRetryDelayMs(1)).toBe(500);
			expect(moduleOverlayRetryDelayMs(2)).toBe(1000);
			expect(moduleOverlayRetryDelayMs(3)).toBe(2000);
		});
	});

	describe('a destroy that fails', () => {
		it('records the window as gone instead of queueing the destroy again', () => {
			// `destroyTempleWindow` already retries five times internally, so a
			// reported failure is the end of what this scheduler can usefully
			// ask for — and holding `actual` true would repeat that sweep for
			// as long as the module stays off.
			const built = step(moduleOverlayDesired(moduleOverlayInit(), true));
			const s = step(moduleOverlayDesired(built, false), false);
			expect(s.actual).toBe(false);
			expect(moduleOverlayNextAction(s)).toBe('none');
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
		let s = moduleOverlayBegin(moduleOverlayDesired(moduleOverlayInit(), true), 'create');

		const raced = moduleOverlayCreateWithTimeout(neverSettles, onTimeout);
		await vi.advanceTimersByTimeAsync(CREATE_TIMEOUT_MS);
		s = moduleOverlaySettle(s, await raced);

		expect(onTimeout).toHaveBeenCalledTimes(1);
		expect(s.pending).toBe('none');
		// `pending` free is what makes the machine answerable again: a stuck
		// `'create'` also reads `'none'` from `moduleOverlayNextAction`, so the
		// discriminating question is whether a later toggle gets acted on.
		s = moduleOverlayDesired(s, false);
		s = moduleOverlayDesired(s, true);
		expect(moduleOverlayNextAction(s)).toBe('create');
	});

	it('reports the timeout as a failed attempt, not as a built window', async () => {
		// A timeout means "we do not know what got built". Recording it as a
		// success would leave `actual` true against a window that may not
		// exist; recording it as a failure sends the pump back through a
		// retry, whose first step is the destroy sweep that cleans up whatever
		// the slow creation eventually produced.
		vi.useFakeTimers();
		let s = moduleOverlayBegin(moduleOverlayDesired(moduleOverlayInit(), true), 'create');

		const raced = moduleOverlayCreateWithTimeout(neverSettles, () => {});
		await vi.advanceTimersByTimeAsync(CREATE_TIMEOUT_MS);
		s = moduleOverlaySettle(s, await raced);

		expect(s.actual).toBe(false);
		expect(s.attempts).toBe(1);
		expect(moduleOverlayNextAction(s)).toBe('create');
	});

	it('leaves a creation that finishes inside the deadline alone', async () => {
		// The negative half: the deadline must not overrule a real answer, or
		// every slow-but-successful build would be torn down and retried.
		vi.useFakeTimers();
		const onTimeout = vi.fn();

		const raced = moduleOverlayCreateWithTimeout(
			() => new Promise<boolean>((r) => setTimeout(() => r(true), CREATE_TIMEOUT_MS - 1)),
			onTimeout
		);
		await vi.advanceTimersByTimeAsync(CREATE_TIMEOUT_MS * 2);

		expect(await raced).toBe(true);
		expect(onTimeout).not.toHaveBeenCalled();
	});

	it('passes a creation failure through as the failure it is', async () => {
		vi.useFakeTimers();
		const onTimeout = vi.fn();

		const raced = moduleOverlayCreateWithTimeout(async () => false, onTimeout);
		await vi.advanceTimersByTimeAsync(CREATE_TIMEOUT_MS * 2);

		expect(await raced).toBe(false);
		expect(onTimeout).not.toHaveBeenCalled();
	});

	it('re-throws a creation that rejects rather than reporting it as a timeout', async () => {
		// The layout's own catch logs which step threw; folding a rejection
		// into `false` here would take that message away.
		vi.useFakeTimers();
		const onTimeout = vi.fn();

		const raced = moduleOverlayCreateWithTimeout(async () => {
			throw new Error('window label already in use');
		}, onTimeout);

		await expect(raced).rejects.toThrow('window label already in use');
		expect(onTimeout).not.toHaveBeenCalled();
	});
});

/**
 * The driver, with the Tauri calls faked.
 *
 * These assert on WHICH effects ran and in what order, which is the exception
 * the outcome-over-path rule names: ordering Tauri window calls IS this code's
 * contract. The window it would have built is not observable from here.
 */
describe('driving one module-coupled overlay', () => {
	afterEach(() => {
		vi.useRealTimers();
	});

	/**
	 * `createResults` is what each successive `create` does, in order: `true`
	 * built it, `false` reported failure, an `Error` THREW — the third being a
	 * real case (`new WebviewWindow` on a label Tauri has not finished freeing)
	 * that the contract says cannot happen, which is why the driver has to
	 * survive it anyway.
	 */
	function harness(createResults: (boolean | Error)[] = []) {
		const calls: string[] = [];
		const logs: string[] = [];
		let attempt = 0;
		const driver = moduleOverlayDriver(
			{ label: 'mercenary', moduleId: 'mercenary' },
			{
				create: async () => {
					calls.push('create');
					const result = createResults[attempt++] ?? true;
					if (result instanceof Error) throw result;
					return result;
				},
				destroy: async () => {
					calls.push('destroy');
					return true;
				},
				log: (msg) => logs.push(msg)
			}
		);
		return { driver, calls, logs };
	}

	/** Let the driver's promise chain run to quiescence. */
	async function settle(): Promise<void> {
		for (let i = 0; i < 10; i++) await Promise.resolve();
	}

	it('builds the window when the module flag comes back on', async () => {
		const { driver, calls } = harness();
		driver.setDesired(true);
		await settle();
		expect(calls).toEqual(['create']);
		expect(driver.built()).toBe(true);
	});

	it('builds nothing when the flag comes back off with no window standing', async () => {
		// The startup case: a destroy here would run a five-round close/destroy
		// sweep against a label that was never created, on every launch.
		const { driver, calls } = harness();
		driver.setDesired(false);
		await settle();
		expect(calls).toEqual([]);
	});

	it('tears the window down when the module is switched off', async () => {
		const { driver, calls } = harness();
		driver.setDesired(true);
		await settle();
		driver.setDesired(false);
		await settle();
		expect(calls).toEqual(['create', 'destroy']);
		expect(driver.built()).toBe(false);
	});

	/**
	 * The edge case the machine exists for (POE-199): a module toggled
	 * off→on→off faster than a window can be built must not strand a
	 * transparent always-on-top window over the game.
	 */
	it('ends with no window after an off, on, off burst', async () => {
		const { driver, calls, logs } = harness();
		driver.setDesired(false);
		driver.setDesired(true);
		driver.setDesired(false);
		await settle();
		expect(calls).toEqual(['create', 'destroy']);
		expect(driver.built()).toBe(false);
		expect(logs).toEqual([]);
	});

	it('ends with a window after an on, off, on burst', async () => {
		const { driver, calls } = harness();
		driver.setDesired(true);
		driver.setDesired(false);
		driver.setDesired(true);
		await settle();
		expect(driver.built()).toBe(true);
		expect(calls.at(-1)).toBe('create');
	});

	it('ignores the poll repeating the flag it already acted on', async () => {
		const { driver, calls } = harness();
		driver.setDesired(true);
		await settle();
		driver.setDesired(true);
		driver.setDesired(true);
		await settle();
		expect(calls).toEqual(['create']);
	});

	it('retries a failed creation after the backoff', async () => {
		vi.useFakeTimers();
		const { driver, calls, logs } = harness([false]);
		driver.setDesired(true);
		await vi.advanceTimersByTimeAsync(0);
		expect(calls).toEqual(['create']);
		expect(logs.some((line) => line.includes('retrying in 500 ms'))).toBe(true);

		await vi.advanceTimersByTimeAsync(500);
		expect(calls).toEqual(['create', 'create']);
		expect(driver.built()).toBe(true);
	});

	it('stops retrying once the budget is spent, and says so', async () => {
		vi.useFakeTimers();
		const { driver, calls, logs } = harness([false, false, false]);
		driver.setDesired(true);
		await vi.advanceTimersByTimeAsync(10_000);

		expect(calls).toHaveLength(MAX_CREATE_ATTEMPTS);
		// A silent give-up is the failure mode the note exists for: the module
		// reads as on and no window was ever built. The line has to name the
		// MODULE too — "until the module is toggled" is not actionable without
		// saying which toggle, and the caller's prefix only names the window.
		const gaveUp = logs.find((line) => line.includes(GAVE_UP_NOTE));
		expect(gaveUp).toBeDefined();
		expect(gaveUp).toContain('module mercenary');
	});

	/**
	 * Fix for the review finding: a `create` that REJECTS used to settle the
	 * action and stop there — `desired` true, no window, no retry scheduled —
	 * so the module read as on with nothing on screen for the life of the
	 * process, and only a manual toggle could recover it.
	 */
	it('retries a creation that throws, instead of leaving the module on with no window', async () => {
		vi.useFakeTimers();
		const { driver, calls, logs } = harness([new Error('window label already in use')]);
		driver.setDesired(true);
		await vi.advanceTimersByTimeAsync(0);
		expect(calls).toEqual(['create']);
		expect(logs.some((line) => line.includes('window label already in use'))).toBe(true);

		await vi.advanceTimersByTimeAsync(500);

		expect(calls).toEqual(['create', 'create']);
		expect(driver.built()).toBe(true);
	});

	/**
	 * The other half: a throwing DESTROY must re-ask rather than stop. The
	 * machine records the window as gone either way, so what matters is that a
	 * module switched back ON while the failed destroy was in flight still gets
	 * its window.
	 */
	it('re-asks after a destroy that throws', async () => {
		const calls: string[] = [];
		const driver = moduleOverlayDriver(
			{ label: 'mercenary', moduleId: 'mercenary' },
			{
				create: async () => {
					calls.push('create');
					return true;
				},
				destroy: async () => {
					calls.push('destroy');
					throw new Error('close() failed');
				},
				log: () => {}
			}
		);

		driver.setDesired(true);
		await settle();
		driver.setDesired(false);
		driver.setDesired(true);
		await settle();

		expect(calls).toEqual(['create', 'destroy', 'create']);
		expect(driver.built()).toBe(true);
	});

	/**
	 * The poll repeats the module flag every few seconds, and a repeat must not
	 * be mistaken for a toggle: `moduleOverlayDesired` clears the retry budget
	 * on a CHANGE, so a driver that forwarded every reading would hand a
	 * permanently failing creation an unlimited one — and, before the backoff
	 * elapsed, would pump a second attempt on top of the scheduled one.
	 */
	it('does not act on the poll repeating the flag while a retry is pending', async () => {
		vi.useFakeTimers();
		const { driver, calls } = harness([false, false]);
		driver.setDesired(true);
		await vi.advanceTimersByTimeAsync(0);
		expect(calls).toEqual(['create']);

		// The poll answers `true` again, mid-backoff.
		driver.setDesired(true);
		await vi.advanceTimersByTimeAsync(400);

		// Still only the first attempt: the repeat was ignored, and the retry
		// the driver DID schedule is still 100 ms away.
		expect(calls).toEqual(['create']);
	});

	it('builds no window from a pending retry once the module is switched off', async () => {
		vi.useFakeTimers();
		const { driver, calls } = harness([false]);
		driver.setDesired(true);
		await vi.advanceTimersByTimeAsync(0);
		expect(calls).toEqual(['create']);

		driver.setDesired(false);
		await vi.advanceTimersByTimeAsync(5000);

		// Two things stop it and either alone would do: the toggle cancels the
		// timer, and a retry that fired anyway goes through the machine, which
		// answers `'none'` for a module nobody wants. What must not happen is a
		// transparent always-on-top window appearing seconds after the user
		// switched the module off — that is what this asserts, whichever guard
		// a future edit breaks first.
		expect(calls).toEqual(['create']);
		expect(driver.built()).toBe(false);
	});
});
