/**
 * What an `/api/device/me` answer is allowed to turn into, and what the store
 * does until one arrives (POE-203).
 *
 * The store's whole safety property is that only a well-formed answer grants
 * anything: every other outcome — no answer, a partial one, one whose fields
 * arrive as the wrong type — has to land on stable with no features, because
 * that is what every non-beta device in the world gets. The second property is
 * that "no answer yet" is a state the store keeps ASKING about: a single-shot
 * fetch that lost to a network which was not up at launch would strand an
 * entitled device on stable for the whole session.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { normalizeEntitlements } from './entitlements.svelte';

/**
 * The two modules `loadEntitlements` reaches DYNAMICALLY (it is imported by the
 * status store, so it cannot import either statically). Built through
 * `vi.hoisted` so `vi.resetModules()` — which re-runs the factories — hands the
 * reloaded store the same objects these tests write to.
 */
const mocks = vi.hoisted(() => ({
	deviceStatus: { status: null as { device_id: string } | null },
	fetchDeviceMe: vi.fn()
}));
vi.mock('$lib/stores/status.svelte', () => ({ store: mocks.deviceStatus }));
vi.mock('$lib/api', () => ({ fetchDeviceMe: mocks.fetchDeviceMe }));

describe('normalizeEntitlements', () => {
	it('takes the beta channel and its features from a well-formed answer', () => {
		expect(normalizeEntitlements({ role: 'editor', channel: 'beta', features: ['merc'] })).toEqual({
			role: 'editor',
			channel: 'beta',
			features: ['merc']
		});
	});

	it('leaves a plain user on stable with no features', () => {
		expect(normalizeEntitlements({ role: 'user', channel: 'stable', features: [] })).toEqual({
			role: 'user',
			channel: 'stable',
			features: []
		});
	});

	it('falls back to stable for a channel name this build does not know', () => {
		// A newer server naming a third channel must not read as beta.
		expect(normalizeEntitlements({ role: 'editor', channel: 'nightly', features: [] }).channel).toBe(
			'stable'
		);
	});

	it('falls back to stable when the answer names no channel', () => {
		expect(normalizeEntitlements({ role: 'editor' }).channel).toBe('stable');
	});

	it('grants nothing when features arrives as something other than a list', () => {
		expect(normalizeEntitlements({ role: 'editor', channel: 'beta', features: 'merc' }).features).toEqual(
			[]
		);
	});

	it('drops the non-string entries out of a features list', () => {
		expect(
			normalizeEntitlements({ channel: 'beta', features: ['merc', 7, null, 'temple'] }).features
		).toEqual(['merc', 'temple']);
	});

	it('reports an empty role when the answer carries none', () => {
		expect(normalizeEntitlements({ channel: 'beta', features: ['merc'] }).role).toBe('');
	});

	it('grants nothing for a body that is not an object at all', () => {
		// A proxy or error page answering 200 with a string body.
		expect(normalizeEntitlements('Service Unavailable')).toEqual({
			role: '',
			channel: 'stable',
			features: []
		});
	});

	it('grants nothing for a null body', () => {
		expect(normalizeEntitlements(null)).toEqual({ role: '', channel: 'stable', features: [] });
	});

	it('hands each caller its own features array rather than a shared one', () => {
		// The store assigns this array onto a rune, so a shared one would let a
		// mutation downstream rewrite what every later failure falls back to.
		const first = normalizeEntitlements(null);
		first.features.push('merc');
		expect(normalizeEntitlements(null).features).toEqual([]);
	});
});

/**
 * `loadEntitlements` carries module-level state — the in-flight chain — and
 * writes a module-level rune, so every test re-imports the store to get its own
 * instance. Fake timers throughout: a failed attempt schedules the next one, so
 * a test on real timers would either hang or leak a retry loop into the next.
 */
describe('loadEntitlements', () => {
	/** A grant that would be visible in the store if it were written through. */
	const betaGrant = { role: 'editor', channel: 'beta', features: ['merc'], alias: 'Test PC' };
	let warn: ReturnType<typeof vi.spyOn>;

	beforeEach(() => {
		vi.resetModules();
		vi.useFakeTimers();
		mocks.deviceStatus.status = null;
		mocks.fetchDeviceMe.mockReset();
		// Every unresolved attempt logs on purpose; keep the run readable.
		warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
	});

	afterEach(() => {
		vi.useRealTimers();
		warn.mockRestore();
	});

	it('does not ask the server before the status store has a device id', async () => {
		// `X-Device-ID` IS the question: the server answers 200 for an anonymous
		// device, and that answer would settle the store on "entitled to nothing".
		const mod = await import('./entitlements.svelte');

		void mod.loadEntitlements();
		await vi.advanceTimersByTimeAsync(0);

		expect(mocks.fetchDeviceMe).not.toHaveBeenCalled();
	});

	it('asks again once the device id arrives', async () => {
		mocks.fetchDeviceMe.mockResolvedValue(betaGrant);
		const mod = await import('./entitlements.svelte');

		void mod.loadEntitlements();
		await vi.advanceTimersByTimeAsync(0);
		mocks.deviceStatus.status = { device_id: 'device-1' };
		await vi.advanceTimersByTimeAsync(mod.RETRY_DELAYS_MS[0]);

		expect(mod.entitlements.features).toEqual(['merc']);
	});

	it('grants nothing while the server cannot be reached', async () => {
		mocks.deviceStatus.status = { device_id: 'device-1' };
		mocks.fetchDeviceMe.mockRejectedValue(new Error('offline'));
		const mod = await import('./entitlements.svelte');

		void mod.loadEntitlements();
		await vi.advanceTimersByTimeAsync(0);

		expect(mod.entitlements).toEqual({ role: '', channel: 'stable', features: [] });
	});

	it('asks again after a failed attempt and takes the answer the retry brings', async () => {
		mocks.deviceStatus.status = { device_id: 'device-1' };
		mocks.fetchDeviceMe
			.mockRejectedValueOnce(new Error('network not up yet'))
			.mockResolvedValueOnce(betaGrant);
		const mod = await import('./entitlements.svelte');

		void mod.loadEntitlements();
		await vi.advanceTimersByTimeAsync(mod.RETRY_DELAYS_MS[0]);

		expect(mod.entitlements.channel).toBe('beta');
	});

	it('stops asking once an answer has landed', async () => {
		mocks.deviceStatus.status = { device_id: 'device-1' };
		mocks.fetchDeviceMe.mockResolvedValue(betaGrant);
		const mod = await import('./entitlements.svelte');

		void mod.loadEntitlements();
		await vi.advanceTimersByTimeAsync(0);
		// Well past every rung of the backoff schedule.
		await vi.advanceTimersByTimeAsync(30 * 60 * 1000);

		expect(mocks.fetchDeviceMe).toHaveBeenCalledTimes(1);
	});

	it('puts a well-formed answer into the store', async () => {
		mocks.deviceStatus.status = { device_id: 'device-1' };
		mocks.fetchDeviceMe.mockResolvedValue(betaGrant);
		const mod = await import('./entitlements.svelte');

		await mod.loadEntitlements();

		expect(mod.entitlements).toEqual({ role: 'editor', channel: 'beta', features: ['merc'] });
	});

	it('writes the answer through the normalizer rather than storing it raw', async () => {
		// A channel this build does not know and a non-string feature: assigning
		// the body straight onto the rune would leak both into every gate.
		mocks.deviceStatus.status = { device_id: 'device-1' };
		mocks.fetchDeviceMe.mockResolvedValue({
			role: 'editor',
			channel: 'nightly',
			features: ['merc', 7],
			alias: null
		});
		const mod = await import('./entitlements.svelte');

		await mod.loadEntitlements();

		expect(mod.entitlements).toEqual({ role: 'editor', channel: 'stable', features: ['merc'] });
	});

	it('keeps a grant that already landed when a later refresh fails', async () => {
		// The 30-minute tick re-runs this; a refresh that could not reach the
		// server must not revoke the module from a device that is entitled to it.
		mocks.deviceStatus.status = { device_id: 'device-1' };
		mocks.fetchDeviceMe.mockResolvedValueOnce(betaGrant);
		const mod = await import('./entitlements.svelte');
		await mod.loadEntitlements();

		mocks.fetchDeviceMe.mockRejectedValue(new Error('server down'));
		void mod.loadEntitlements();
		await vi.advanceTimersByTimeAsync(0);

		expect(mod.entitlements).toEqual({ role: 'editor', channel: 'beta', features: ['merc'] });
	});

	it('withdraws a landed grant on resetEntitlements', async () => {
		// The one path that revokes without a server saying so: a server switch.
		mocks.deviceStatus.status = { device_id: 'device-1' };
		mocks.fetchDeviceMe.mockResolvedValue(betaGrant);
		const mod = await import('./entitlements.svelte');
		await mod.loadEntitlements();

		mod.resetEntitlements();

		expect(mod.entitlements).toEqual({ role: '', channel: 'stable', features: [] });
	});

	it('starts a chain on refreshEntitlements when none is running', async () => {
		mocks.deviceStatus.status = { device_id: 'device-1' };
		mocks.fetchDeviceMe.mockResolvedValue(betaGrant);
		const mod = await import('./entitlements.svelte');

		await mod.refreshEntitlements();

		expect(mod.entitlements.channel).toBe('beta');
	});

	it('ends a sleeping backoff early on refreshEntitlements', async () => {
		// The chain is a rung into the schedule against a server that was down;
		// a server switch cannot wait the rest of that rung out.
		mocks.deviceStatus.status = { device_id: 'device-1' };
		mocks.fetchDeviceMe
			.mockRejectedValueOnce(new Error('local server down'))
			.mockResolvedValueOnce(betaGrant);
		const mod = await import('./entitlements.svelte');
		void mod.loadEntitlements();
		await vi.advanceTimersByTimeAsync(0);
		expect(mocks.fetchDeviceMe).toHaveBeenCalledTimes(1);

		void mod.refreshEntitlements();
		await vi.advanceTimersByTimeAsync(0);

		expect(mocks.fetchDeviceMe).toHaveBeenCalledTimes(2);
		expect(mod.entitlements.channel).toBe('beta');
	});

	it('drops an answer that was in flight when the refresh was asked for, and asks again', async () => {
		// The request left for the previous server; its grant must not land
		// after the switch and pass for the new server's.
		mocks.deviceStatus.status = { device_id: 'device-1' };
		let answerFirst!: (body: unknown) => void;
		mocks.fetchDeviceMe
			.mockImplementationOnce(() => new Promise((resolve) => { answerFirst = resolve; }))
			.mockResolvedValueOnce({ role: 'user', channel: 'stable', features: [], alias: null });
		const mod = await import('./entitlements.svelte');
		void mod.loadEntitlements();
		await vi.advanceTimersByTimeAsync(0);

		const refreshed = mod.refreshEntitlements();
		answerFirst(betaGrant);
		await refreshed;

		expect(mocks.fetchDeviceMe).toHaveBeenCalledTimes(2);
		expect(mod.entitlements).toEqual({ role: 'user', channel: 'stable', features: [] });
	});

	it('returns the running chain from refreshEntitlements rather than a second one', async () => {
		vi.useRealTimers();
		mocks.deviceStatus.status = { device_id: 'device-1' };
		mocks.fetchDeviceMe.mockResolvedValue(betaGrant);
		const mod = await import('./entitlements.svelte');

		const first = mod.loadEntitlements();
		expect(mod.refreshEntitlements()).toBe(first);
		await first;
	});

	it('shares one chain between concurrent callers', async () => {
		// The 30-minute refresh can fire while a startup load is still running;
		// a second chain would double the request rate on every tick.
		// Real timers: the success path schedules no retry, and awaiting both
		// chains out is what makes a SECOND chain observable at all.
		vi.useRealTimers();
		mocks.deviceStatus.status = { device_id: 'device-1' };
		mocks.fetchDeviceMe.mockResolvedValue(betaGrant);
		const mod = await import('./entitlements.svelte');

		await Promise.all([mod.loadEntitlements(), mod.loadEntitlements()]);

		expect(mocks.fetchDeviceMe).toHaveBeenCalledTimes(1);
	});
});

describe('retryDelayMs', () => {
	it('waits longer after each failed attempt', async () => {
		const { retryDelayMs } = await import('./entitlements.svelte');
		expect([retryDelayMs(0), retryDelayMs(1), retryDelayMs(2)]).toEqual([5_000, 15_000, 60_000]);
	});

	it('caps the wait at five minutes however long the server stays down', async () => {
		// An offline session lasts hours; without the cap the schedule would
		// either run off the end of the table or keep polling at a minute.
		const { retryDelayMs } = await import('./entitlements.svelte');
		expect([retryDelayMs(3), retryDelayMs(50)]).toEqual([300_000, 300_000]);
	});
});

/**
 * The server's registration answer for this device, carried on the same chain
 * that loads the entitlements (POE-362). Fixture values are the `deviceMe`
 * records of design/handoffs/poe-362--desktop-app--top-bar--account/fixtures.json.
 */
describe('registration', () => {
	const fx = {
		registered: { role: 'beta', alias: 'Gaming PC', channel: 'beta', features: ['temple', 'merc', 'exchange', 'beta'] },
		registeredNoRole: { role: '', alias: 'Gaming PC', channel: 'stable', features: ['temple'] },
		unregistered: { role: '', alias: null, channel: 'stable', features: ['temple'] },
		unregisteredEmpty: { role: '', alias: '', channel: 'stable', features: ['temple'] },
		unknownOldServer: { role: 'beta', channel: 'beta', features: ['temple', 'merc', 'exchange', 'beta'] }
	};
	let warn: ReturnType<typeof vi.spyOn>;

	beforeEach(() => {
		vi.resetModules();
		vi.useFakeTimers();
		mocks.deviceStatus.status = null;
		mocks.fetchDeviceMe.mockReset();
		warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
	});

	afterEach(() => {
		vi.useRealTimers();
		warn.mockRestore();
	});

	function deferred() {
		let resolve!: (body: unknown) => void;
		let reject!: (e: unknown) => void;
		const promise = new Promise((res, rej) => {
			resolve = res;
			reject = rej;
		});
		return { promise, resolve, reject };
	}

	async function landWith(body: unknown) {
		mocks.deviceStatus.status = { device_id: 'device-1' };
		mocks.fetchDeviceMe.mockResolvedValueOnce(body);
		const mod = await import('./entitlements.svelte');
		void mod.loadEntitlements();
		await vi.advanceTimersByTimeAsync(0);
		return mod;
	}

	it('reads checking before any answer and while there is no device id', async () => {
		const mod = await import('./entitlements.svelte');
		expect(mod.registration.state).toBe('checking');

		void mod.loadEntitlements();
		await vi.advanceTimersByTimeAsync(mod.RETRY_DELAYS_MS[0] + mod.RETRY_DELAYS_MS[1]);

		expect(mocks.fetchDeviceMe).not.toHaveBeenCalled();
		expect(mod.registration.state).toBe('checking');
		expect(warn).toHaveBeenCalledWith(expect.stringContaining('no device id yet'));
	});

	it('reads a registered answer with its alias and role', async () => {
		const mod = await landWith(fx.registered);
		expect({ ...mod.registration }).toEqual({ state: 'registered', alias: 'Gaming PC', role: 'beta' });
		expect(warn).not.toHaveBeenCalled();
	});

	it('reads a registered answer without a role', async () => {
		const mod = await landWith(fx.registeredNoRole);
		expect({ ...mod.registration }).toEqual({ state: 'registered', alias: 'Gaming PC', role: '' });
	});

	it('reads a null alias as unregistered', async () => {
		const mod = await landWith(fx.unregistered);
		expect(mod.registration.state).toBe('unregistered');
		expect(warn).not.toHaveBeenCalled();
	});

	it('reads an empty alias as unregistered', async () => {
		const mod = await landWith(fx.unregisteredEmpty);
		expect(mod.registration.state).toBe('unregistered');
	});

	it('reads an answer with no alias key as unknown', async () => {
		const mod = await landWith(fx.unknownOldServer);
		expect(mod.registration.state).toBe('unknown');
	});

	it('reads a rejected request as unknown', async () => {
		mocks.deviceStatus.status = { device_id: 'device-1' };
		mocks.fetchDeviceMe.mockRejectedValue(new Error('offline'));
		const mod = await import('./entitlements.svelte');
		void mod.loadEntitlements();
		await vi.advanceTimersByTimeAsync(0);
		expect(mod.registration.state).toBe('unknown');
		expect(warn).toHaveBeenCalledWith(
			expect.stringContaining('/api/device/me failed'),
			expect.any(Error)
		);
	});

	it('moves from unknown to registered on the retry after a rejection', async () => {
		mocks.deviceStatus.status = { device_id: 'device-1' };
		mocks.fetchDeviceMe.mockRejectedValueOnce(new Error('offline')).mockResolvedValueOnce(fx.registered);
		const mod = await import('./entitlements.svelte');
		void mod.loadEntitlements();
		await vi.advanceTimersByTimeAsync(0);
		expect(mod.registration.state).toBe('unknown');

		await vi.advanceTimersByTimeAsync(mod.RETRY_DELAYS_MS[0]);

		expect(mocks.fetchDeviceMe).toHaveBeenCalledTimes(2);
		expect(mod.registration.state).toBe('registered');
	});

	it('lands the entitlements of an alias-less answer and asks again for registration', async () => {
		mocks.deviceStatus.status = { device_id: 'device-1' };
		mocks.fetchDeviceMe.mockResolvedValueOnce(fx.unknownOldServer).mockResolvedValueOnce(fx.registered);
		const mod = await import('./entitlements.svelte');
		let settled = false;
		void mod.loadEntitlements().then(() => {
			settled = true;
		});
		await vi.advanceTimersByTimeAsync(0);
		expect(settled).toBe(true);
		expect(mod.entitlements.channel).toBe('beta');
		expect(mod.registration.state).toBe('unknown');

		await vi.advanceTimersByTimeAsync(mod.RETRY_DELAYS_MS[0]);

		expect(mod.registration.state).toBe('registered');
		// Resolved: no further asking.
		await vi.advanceTimersByTimeAsync(30 * 60 * 1000);
		expect(mocks.fetchDeviceMe).toHaveBeenCalledTimes(2);
	});

	it('moves an alias-less answer on to unregistered on the retry', async () => {
		mocks.deviceStatus.status = { device_id: 'device-1' };
		mocks.fetchDeviceMe.mockResolvedValueOnce(fx.unknownOldServer).mockResolvedValueOnce(fx.unregistered);
		const mod = await import('./entitlements.svelte');
		void mod.loadEntitlements();
		await vi.advanceTimersByTimeAsync(0);
		expect(mod.registration.state).toBe('unknown');

		await vi.advanceTimersByTimeAsync(mod.RETRY_DELAYS_MS[0]);

		expect(mod.registration.state).toBe('unregistered');
	});

	it('joins the registration retry rather than starting a second chain', async () => {
		mocks.deviceStatus.status = { device_id: 'device-1' };
		mocks.fetchDeviceMe.mockResolvedValueOnce(fx.unknownOldServer).mockResolvedValueOnce(fx.registered);
		const mod = await import('./entitlements.svelte');
		void mod.loadEntitlements();
		await vi.advanceTimersByTimeAsync(0);

		await mod.loadEntitlements();
		await vi.advanceTimersByTimeAsync(0);
		expect(mocks.fetchDeviceMe).toHaveBeenCalledTimes(1);

		await vi.advanceTimersByTimeAsync(mod.RETRY_DELAYS_MS[0]);
		expect(mocks.fetchDeviceMe).toHaveBeenCalledTimes(2);
		expect(mod.registration.state).toBe('registered');
	});

	it('reads unknown when a later load is rejected, keeping the entitlements', async () => {
		const mod = await landWith(fx.registered);
		expect(mod.registration.state).toBe('registered');

		mocks.fetchDeviceMe.mockRejectedValue(new Error('server down'));
		void mod.loadEntitlements();
		await vi.advanceTimersByTimeAsync(0);

		expect(mod.registration.state).toBe('unknown');
		expect(mod.entitlements.channel).toBe('beta');
	});

	it('ignores the previous server answering after a switch', async () => {
		mocks.deviceStatus.status = { device_id: 'device-1' };
		const serverA = deferred();
		mocks.fetchDeviceMe
			.mockImplementationOnce(() => serverA.promise)
			.mockResolvedValueOnce(fx.unregistered);
		const mod = await import('./entitlements.svelte');
		void mod.loadEntitlements();
		await vi.advanceTimersByTimeAsync(0);

		mod.resetEntitlements();
		void mod.refreshEntitlements();
		expect(mod.registration.state).toBe('checking');
		serverA.resolve(fx.registered);
		await vi.advanceTimersByTimeAsync(0);

		expect(mocks.fetchDeviceMe).toHaveBeenCalledTimes(2);
		expect(mod.registration.state).toBe('unregistered');
		expect(mod.entitlements.channel).toBe('stable');
	});

	it('ignores the previous server failing after a switch and asks the new one at once', async () => {
		mocks.deviceStatus.status = { device_id: 'device-1' };
		const serverA = deferred();
		const serverB = deferred();
		mocks.fetchDeviceMe
			.mockImplementationOnce(() => serverA.promise)
			.mockImplementationOnce(() => serverB.promise);
		const mod = await import('./entitlements.svelte');
		void mod.loadEntitlements();
		await vi.advanceTimersByTimeAsync(0);

		mod.resetEntitlements();
		void mod.refreshEntitlements();
		serverA.reject(new Error('server A gone'));
		await vi.advanceTimersByTimeAsync(0);

		expect(mod.registration.state).toBe('checking');
		expect(mocks.fetchDeviceMe).toHaveBeenCalledTimes(2);

		serverB.resolve(fx.registered);
		await vi.advanceTimersByTimeAsync(0);
		expect(mod.registration.state).toBe('registered');
	});

	it('keeps an identified alias over the stale answers the server caches', async () => {
		mocks.deviceStatus.status = { device_id: 'device-1' };
		const pending = deferred();
		mocks.fetchDeviceMe.mockImplementationOnce(() => pending.promise);
		const mod = await import('./entitlements.svelte');
		void mod.loadEntitlements();
		await vi.advanceTimersByTimeAsync(0);

		mod.markRegistered('Gaming PC');
		expect({ ...mod.registration }).toEqual({ state: 'registered', alias: 'Gaming PC', role: '' });
		pending.resolve({ ...fx.unregistered, role: 'beta' });
		await vi.advanceTimersByTimeAsync(0);
		expect({ ...mod.registration }).toEqual({ state: 'registered', alias: 'Gaming PC', role: 'beta' });

		mocks.fetchDeviceMe.mockResolvedValueOnce(fx.unregistered);
		void mod.loadEntitlements();
		await vi.advanceTimersByTimeAsync(0);
		expect(mod.registration.state).toBe('registered');
		expect(mod.registration.alias).toBe('Gaming PC');

		mocks.fetchDeviceMe.mockResolvedValueOnce(fx.registered).mockResolvedValueOnce(fx.unregistered);
		void mod.loadEntitlements();
		await vi.advanceTimersByTimeAsync(0);
		void mod.loadEntitlements();
		await vi.advanceTimersByTimeAsync(0);
		expect(mocks.fetchDeviceMe).toHaveBeenCalledTimes(4);
		expect(mod.registration.state).toBe('unregistered');
	});

	it('keeps an identified alias when the request in flight is rejected', async () => {
		mocks.deviceStatus.status = { device_id: 'device-1' };
		const pending = deferred();
		mocks.fetchDeviceMe.mockImplementationOnce(() => pending.promise);
		const mod = await import('./entitlements.svelte');
		void mod.loadEntitlements();
		await vi.advanceTimersByTimeAsync(0);

		mod.markRegistered('Gaming PC');
		pending.reject(new Error('offline'));
		await vi.advanceTimersByTimeAsync(0);

		expect({ ...mod.registration }).toEqual({ state: 'registered', alias: 'Gaming PC', role: '' });
	});

	it('keeps an identified alias set while the chain backs off', async () => {
		mocks.deviceStatus.status = { device_id: 'device-1' };
		mocks.fetchDeviceMe.mockRejectedValueOnce(new Error('offline')).mockResolvedValueOnce(fx.unregistered);
		const mod = await import('./entitlements.svelte');
		void mod.loadEntitlements();
		await vi.advanceTimersByTimeAsync(0);
		expect(mod.registration.state).toBe('unknown');

		mod.markRegistered('Gaming PC');
		expect(mod.registration.state).toBe('registered');
		await vi.advanceTimersByTimeAsync(mod.RETRY_DELAYS_MS[0]);

		expect(mocks.fetchDeviceMe).toHaveBeenCalledTimes(2);
		expect({ ...mod.registration }).toEqual({ state: 'registered', alias: 'Gaming PC', role: '' });
	});

	it('drops the identified alias on resetEntitlements', async () => {
		const mod = await landWith(fx.registered);
		mod.markRegistered('Gaming PC');

		mod.resetEntitlements();
		expect({ ...mod.registration }).toEqual({ state: 'checking', alias: '', role: '' });

		mocks.fetchDeviceMe.mockResolvedValueOnce(fx.unregistered);
		void mod.loadEntitlements();
		await vi.advanceTimersByTimeAsync(0);
		expect(mod.registration.state).toBe('unregistered');
	});

	it('settles a load made before a server switch once the new server answers', async () => {
		mocks.deviceStatus.status = { device_id: 'device-1' };
		const serverA = deferred();
		mocks.fetchDeviceMe
			.mockImplementationOnce(() => serverA.promise)
			.mockResolvedValueOnce(fx.registered);
		const mod = await import('./entitlements.svelte');
		let beforeSwitch = false;
		let afterSwitch = false;
		void mod.loadEntitlements().then(() => {
			beforeSwitch = true;
		});
		await vi.advanceTimersByTimeAsync(0);

		mod.resetEntitlements();
		void mod.refreshEntitlements().then(() => {
			afterSwitch = true;
		});
		serverA.resolve(fx.unregistered);
		await vi.advanceTimersByTimeAsync(0);

		expect(mod.registration.state).toBe('registered');
		expect([beforeSwitch, afterSwitch]).toEqual([true, true]);
	});

	it('asks again on a load made right after an awaited one landed', async () => {
		mocks.deviceStatus.status = { device_id: 'device-1' };
		mocks.fetchDeviceMe.mockResolvedValueOnce(fx.registered).mockResolvedValueOnce(fx.unregistered);
		const mod = await import('./entitlements.svelte');

		await mod.loadEntitlements();
		await mod.loadEntitlements();

		expect(mocks.fetchDeviceMe).toHaveBeenCalledTimes(2);
		expect(mod.registration.state).toBe('unregistered');
	});

	it('asks the new server on a reset and refresh right after an awaited load landed', async () => {
		mocks.deviceStatus.status = { device_id: 'device-1' };
		mocks.fetchDeviceMe.mockResolvedValueOnce(fx.registered).mockResolvedValueOnce(fx.unregistered);
		const mod = await import('./entitlements.svelte');
		let refreshed = false;

		await mod.loadEntitlements();
		mod.resetEntitlements();
		void mod.refreshEntitlements().then(() => {
			refreshed = true;
		});
		await vi.advanceTimersByTimeAsync(0);

		expect(mocks.fetchDeviceMe).toHaveBeenCalledTimes(2);
		expect(mod.registration.state).toBe('unregistered');
		expect(refreshed).toBe(true);
	});
});
