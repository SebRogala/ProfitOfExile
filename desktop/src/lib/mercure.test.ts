import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from 'vitest';

// api.ts pulls in Tauri + the status store at module load; neither is needed for
// the connection lifecycle under test.
vi.mock('@tauri-apps/api/app', () => ({ getVersion: async () => '0.0.0-test' }));
vi.mock('$lib/stores/status.svelte', () => ({ store: { status: { server_url: 'https://hub.test' } } }));

const { connectMercure, CURRENCY_EXCHANGE_UPDATED_TOPIC, HARVEST_UPDATED_TOPIC } = await import('./api');

const TOKEN_RESPONSE = {
	ok: true,
	json: async () => ({ token: 'tok', url: 'https://hub.test/.well-known/mercure' }),
};

/** Every EventSource the module opened during a test. */
let opened: FakeEventSource[] = [];

class FakeEventSource {
	url: string;
	closeCalls = 0;
	onopen: (() => void) | null = null;
	onmessage: ((e: MessageEvent) => void) | null = null;
	onerror: (() => void) | null = null;

	constructor(url: string) {
		this.url = url;
		opened.push(this);
	}

	close() {
		this.closeCalls++;
	}
}

/** A promise plus its resolver, so a test can hold the token fetch mid-flight. */
function deferred<T>() {
	let resolve!: (v: T) => void;
	const promise = new Promise<T>((r) => { resolve = r; });
	return { promise, resolve };
}

/**
 * Open a connection with every per-topic callback wired and hand back a
 * `publish` that feeds one JSON payload into the live stream. The dispatch
 * tests below then differ only in the payload, so "the publish reached the
 * wrong callback" is the visible difference rather than the setup.
 */
async function connectedWithAllCallbacks() {
	globalThis.fetch = vi.fn(async () => TOKEN_RESPONSE) as unknown as typeof fetch;
	const onUpdate = vi.fn();
	const onLayoutUpdate = vi.fn();
	const onCurrencyExchangeUpdate = vi.fn();
	const onHarvestUpdate = vi.fn();

	connectMercure({ onUpdate, onLayoutUpdate, onCurrencyExchangeUpdate, onHarvestUpdate });
	await vi.advanceTimersByTimeAsync(0);

	const publish = (payload: unknown) =>
		opened[0].onmessage!({ data: JSON.stringify(payload) } as MessageEvent);
	return { publish, onUpdate, onLayoutUpdate, onCurrencyExchangeUpdate, onHarvestUpdate };
}

let originalFetch: typeof globalThis.fetch;
let originalEventSource: typeof globalThis.EventSource;

beforeEach(() => {
	opened = [];
	originalFetch = globalThis.fetch;
	originalEventSource = globalThis.EventSource;
	globalThis.EventSource = FakeEventSource as unknown as typeof globalThis.EventSource;
	vi.useFakeTimers();
	// The backoff applies equal jitter (half..full), so attempt counts over a
	// fixed window are a distribution, not a number — and the outage-bound test
	// below straddles its own threshold. Pinning Math.random to the worst case
	// (full delay -> fewest attempts is 1.0; most attempts is 0.0) makes the
	// assertion deterministic. 0.0 is chosen deliberately: it is the fastest
	// ladder, so the upper bound is tested against the hardest case.
	vi.spyOn(Math, 'random').mockReturnValue(0);
});

afterEach(() => {
	vi.useRealTimers();
	globalThis.fetch = originalFetch;
	globalThis.EventSource = originalEventSource;
	vi.restoreAllMocks();
});

describe('connectMercure', () => {
	it('opens one EventSource once the token fetch resolves', async () => {
		// Positive control for the two close() tests below: it proves this
		// arrangement does reach the EventSource construction, so their empty
		// expectation is the guard working and not a harness that never got there.
		globalThis.fetch = vi.fn(async () => TOKEN_RESPONSE) as unknown as typeof fetch;

		connectMercure({ onUpdate: () => {} });
		await vi.advanceTimersByTimeAsync(0);

		expect(opened).toHaveLength(1);
		expect(opened[0].url).toContain('authorization=tok');
	});

	it('opens no EventSource when close() lands inside the token fetch', async () => {
		// The orphan bug (POE-151): the caller drops its reference at close(), so an
		// EventSource created after that point can never be closed by anyone. It then
		// holds a subscriber slot on the hub until the server-side write_timeout
		// (480-600s) expires.
		const token = deferred<typeof TOKEN_RESPONSE>();
		globalThis.fetch = vi.fn(() => token.promise) as unknown as typeof fetch;

		const connection = connectMercure({ onUpdate: () => {} });
		connection.close();
		token.resolve(TOKEN_RESPONSE);
		await vi.advanceTimersByTimeAsync(0);

		expect(opened).toHaveLength(0);
	});

	it('leaves no timer armed when close() lands inside the token fetch', async () => {
		// The second half of the orphan: the abandoned connection re-arms its own
		// 25-minute token refresh, so it keeps reconnecting with nobody holding it.
		const token = deferred<typeof TOKEN_RESPONSE>();
		globalThis.fetch = vi.fn(() => token.promise) as unknown as typeof fetch;

		const connection = connectMercure({ onUpdate: () => {} });
		connection.close();
		token.resolve(TOKEN_RESPONSE);
		await vi.advanceTimersByTimeAsync(0);

		expect(vi.getTimerCount()).toBe(0);
	});

	it('stops retrying after close(), whatever stage the retry loop is in', async () => {
		const fetchMock = vi.fn(async () => { throw new Error('hub down'); });
		globalThis.fetch = fetchMock as unknown as typeof fetch;

		const connection = connectMercure({ onUpdate: () => {} });
		// Let the first attempt fail and a retry be scheduled.
		await vi.advanceTimersByTimeAsync(5000);
		const attemptsBeforeClose = fetchMock.mock.calls.length;
		expect(attemptsBeforeClose).toBeGreaterThan(0);

		connection.close();
		await vi.advanceTimersByTimeAsync(10 * 60 * 1000);

		expect(fetchMock).toHaveBeenCalledTimes(attemptsBeforeClose);
	});

	it('bounds a sustained outage to roughly one attempt per minute', async () => {
		// Before the slow lane the backoff capped at 10s with no ceiling, so a hub
		// that stays down cost 360 token fetches an hour per client, forever.
		//
		// The bound asserted here is the WORST case, not the typical one, because
		// Math.random is pinned to 0 above — equal jitter takes half..full, so 0
		// yields the fastest ladder the policy allows. That matters: it means the
		// 60s slow lane has an effective floor of 30s, so the honest worst case is
		// ~150 token fetches an hour, not the ~60 the ticket claimed. Still a 2.4x
		// improvement on the 360 it replaced, and the assertion is written against
		// the number the code actually guarantees rather than the number we hoped
		// for. Typical (random ~0.5) lands near 17.
		//
		// 10 minutes of failure: 5 fast attempts (2/4/8/10/10s) then the slow lane.
		const fetchMock = vi.fn(async () => { throw new Error('hub down'); });
		globalThis.fetch = fetchMock as unknown as typeof fetch;

		connectMercure({ onUpdate: () => {} });
		await vi.advanceTimersByTimeAsync(10 * 60 * 1000);

		expect(fetchMock.mock.calls.length).toBeLessThan(30);
		// And it must still be retrying — a bound that stops recovering is a
		// permanently dead client, since nothing re-invokes connectMercure.
		expect(fetchMock.mock.calls.length).toBeGreaterThan(5);
	});

	it('reloads on a well-formed publish', async () => {
		// Positive control for the unparseable-publish test below: without it that
		// test passes against a handler that never calls onUpdate at all.
		globalThis.fetch = vi.fn(async () => TOKEN_RESPONSE) as unknown as typeof fetch;
		const onUpdate = vi.fn();

		connectMercure({ onUpdate });
		await vi.advanceTimersByTimeAsync(0);
		opened[0].onmessage!({ data: '{"type":"analysis"}' } as MessageEvent);

		expect(onUpdate).toHaveBeenCalledTimes(1);
	});

	it('does not reload on an unparseable publish', async () => {
		// The handler used to call onUpdate() from the catch, firing the whole
		// loadAll() fan-out on bytes it could not read and dropping the parse error.
		// Every legitimate publish is a marshalled JSON object
		// (internal/lab/throttler.go marshals a map[string]string), so unparseable
		// data is not an update. 542f171.
		globalThis.fetch = vi.fn(async () => TOKEN_RESPONSE) as unknown as typeof fetch;
		const onUpdate = vi.fn();
		const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
		const raw = '<html>502 Bad Gateway</html>';

		connectMercure({ onUpdate });
		await vi.advanceTimersByTimeAsync(0);
		opened[0].onmessage!({ data: raw } as MessageEvent);

		expect(onUpdate).not.toHaveBeenCalled();
		// The error is reported rather than swallowed — the silent discard was half
		// of what made the original bug invisible. What has to survive a refactor is
		// that the parse error AND the bytes that caused it both reach the log; the
		// wording of the label does not, so the label positions are matched by shape.
		// Pinning the literals made rewording a log string a red test with no
		// behaviour change, which teaches the next reader to edit the assertion.
		expect(warn).toHaveBeenCalledWith(
			expect.any(String),
			expect.any(SyntaxError),
			expect.anything(),
			raw,
		);
	});
});

/**
 * One SSE stream serves the whole app (the mercure store in stores/mercure.svelte.ts owns it), so every page
 * that wants server pushes depends on two things this describe pins: the
 * subscribe URL naming its topic, and the message handler routing that topic to
 * its own callback instead of the generic reload.
 */
describe('connectMercure topic routing', () => {
	/** Topics as the hub will read them, not as the query string spells them. */
	function subscribedTopics(): string[] {
		return new URL(opened[0].url).searchParams.getAll('topic');
	}

	it('subscribes once to each app topic', async () => {
		// A payload the server publishes on a topic nobody subscribed to never
		// arrives at all, so the URL is the first half of the feature.
		globalThis.fetch = vi.fn(async () => TOKEN_RESPONSE) as unknown as typeof fetch;

		connectMercure({ onUpdate: () => {} });
		await vi.advanceTimersByTimeAsync(0);

		expect(subscribedTopics()).toEqual(
			expect.arrayContaining([
				'poe/analysis/updated',
				'poe/lab/layout',
				'poe/currency-exchange/updated',
				'poe/harvest/updated',
				'poe/league/active',
			]),
		);
		// Exactly these five topics, each subscribed once.
		expect(subscribedTopics()).toHaveLength(5);
	});

	it('hands a currency exchange publish to the currency exchange callback', async () => {
		const { publish, onCurrencyExchangeUpdate } = await connectedWithAllCallbacks();
		const payload = { topic: CURRENCY_EXCHANGE_UPDATED_TOPIC, league: 'Mirage', hours: 24 };

		publish(payload);

		// The parsed payload, not the raw MessageEvent. The page ignores the
		// payload today, but the mercure store forwards this object through a Tauri `emit`,
		// which a MessageEvent would not survive — and a consumer that later
		// wants to filter (on `league`, say) needs the fields to be there.
		expect(onCurrencyExchangeUpdate.mock.calls).toEqual([[payload]]);
	});

	it('does not reload the lab dashboard on a currency exchange publish', async () => {
		// onUpdate reaches LabPage's loadAll() fan-out — half a dozen requests. A
		// Currency Exchange hour closing must not drag it through them, which is
		// what the early `return` in the topic branch is for.
		const { publish, onUpdate } = await connectedWithAllCallbacks();

		publish({ topic: CURRENCY_EXCHANGE_UPDATED_TOPIC, league: 'Mirage' });

		expect(onUpdate).not.toHaveBeenCalled();
	});

	it('still hands a layout publish to the layout callback', async () => {
		// The currency exchange branch was inserted after this one; a branch added
		// above it, or a fallthrough, would silently re-route lab layout pushes.
		const { publish, onLayoutUpdate, onCurrencyExchangeUpdate } =
			await connectedWithAllCallbacks();
		const payload = { topic: 'poe/lab/layout', section: 2 };

		publish(payload);

		expect(onLayoutUpdate.mock.calls).toEqual([[payload]]);
		expect(onCurrencyExchangeUpdate).not.toHaveBeenCalled();
	});

	it('still reloads on a publish that carries no dedicated topic', async () => {
		const { publish, onUpdate, onCurrencyExchangeUpdate } = await connectedWithAllCallbacks();

		publish({ topic: 'poe/analysis/updated' });

		expect(onUpdate).toHaveBeenCalledTimes(1);
		expect(onCurrencyExchangeUpdate).not.toHaveBeenCalled();
	});

	it('drops a currency exchange publish for a caller that passed no currency exchange handler', async () => {
		// Every existing caller is three-argument; the new topic is on the shared
		// subscribe URL, so their handler now receives payloads they never asked
		// for. Dropping is the contract — throwing would kill the stream, and
		// falling through to onUpdate would reload the lab on every hour.
		globalThis.fetch = vi.fn(async () => TOKEN_RESPONSE) as unknown as typeof fetch;
		const onUpdate = vi.fn();

		connectMercure({ onUpdate, onLayoutUpdate: () => {} });
		await vi.advanceTimersByTimeAsync(0);
		opened[0].onmessage!({
			data: JSON.stringify({ topic: CURRENCY_EXCHANGE_UPDATED_TOPIC }),
		} as MessageEvent);

		expect(onUpdate).not.toHaveBeenCalled();
	});
});


describe('Harvest Mercure routing', () => {
	it('delivers a Harvest publish only to the Harvest callback', async () => {
		const { publish, onHarvestUpdate, onUpdate, onCurrencyExchangeUpdate, onLayoutUpdate } =
			await connectedWithAllCallbacks();
		const payload = { topic: HARVEST_UPDATED_TOPIC, league: 'Mirage' };
		publish(payload);
		expect(onHarvestUpdate.mock.calls).toEqual([[payload]]);
		expect(onUpdate).not.toHaveBeenCalled();
		expect(onCurrencyExchangeUpdate).not.toHaveBeenCalled();
		expect(onLayoutUpdate).not.toHaveBeenCalled();
	});

	it('drops a Harvest publish when the caller has no Harvest callback', async () => {
		globalThis.fetch = vi.fn(async () => TOKEN_RESPONSE) as unknown as typeof fetch;
		const onUpdate = vi.fn();
		connectMercure({ onUpdate });
		await vi.advanceTimersByTimeAsync(0);
		opened[0].onmessage!({ data: JSON.stringify({ topic: HARVEST_UPDATED_TOPIC }) } as MessageEvent);
		expect(onUpdate).not.toHaveBeenCalled();
	});
});

describe('league active hints', () => {
	it('subscribes to league hints on the existing connection', async () => {
		globalThis.fetch = vi.fn(async () => TOKEN_RESPONSE) as unknown as typeof fetch;
		const connection = connectMercure({ onUpdate: vi.fn() });
		await vi.advanceTimersByTimeAsync(0);
		expect(opened).toHaveLength(1);
		expect(new URL(opened[0].url).searchParams.getAll('topic')).toContain('poe/league/active');
		connection.close();
	});

	it('dispatches league hints only to the league callback', async () => {
		globalThis.fetch = vi.fn(async () => TOKEN_RESPONSE) as unknown as typeof fetch;
		const names = ['onUpdate', 'onOpen', 'onLayoutUpdate', 'onCurrencyExchangeUpdate', 'onHarvestUpdate', 'onLeagueActive'] as const;
		const handlers = Object.fromEntries(names.map(name => [name, vi.fn()])) as Record<typeof names[number], Mock<() => void>>;
		const connection = connectMercure(handlers);
		await vi.advanceTimersByTimeAsync(0);
		const payload = { topic: 'poe/league/active', league: 'B', leagueRevision: 7 };
		opened[0].onmessage!({ data: JSON.stringify(payload) } as MessageEvent);
		expect(Object.fromEntries(names.map(name => [name, handlers[name].mock.calls]))).toEqual({
			onUpdate: [], onOpen: [], onLayoutUpdate: [], onCurrencyExchangeUpdate: [], onHarvestUpdate: [],
			onLeagueActive: [[payload]],
		});
		connection.close();
		expect(opened[0].onmessage).toBeNull();
		expect(opened[0].closeCalls).toBe(1);
	});

	it('fires onOpen once per open, and no league or update callback, across a reopen', async () => {
		globalThis.fetch = vi.fn(async () => TOKEN_RESPONSE) as unknown as typeof fetch;
		const onUpdate = vi.fn();
		const onOpen = vi.fn();
		const onLeagueActive = vi.fn();
		const connection = connectMercure({ onUpdate, onOpen, onLeagueActive });
		await vi.advanceTimersByTimeAsync(0);
		opened[0].onopen!();
		opened[0].onerror!();
		await vi.advanceTimersByTimeAsync(2000);
		expect(opened).toHaveLength(2);
		opened[1].onopen!();
		expect(onOpen.mock.calls).toEqual([[], []]);
		expect(onLeagueActive).not.toHaveBeenCalled();
		expect(onUpdate).not.toHaveBeenCalled();
		connection.close();
	});

	it('fires onOpen after onConnectionChange(true) on an open', async () => {
		globalThis.fetch = vi.fn(async () => TOKEN_RESPONSE) as unknown as typeof fetch;
		const order: string[] = [];
		const connection = connectMercure({
			onUpdate: vi.fn(),
			onConnectionChange: (connected) => order.push(`connection:${connected}`),
			onOpen: () => order.push(`open:connected=${connection.connected}`),
		});
		await vi.advanceTimersByTimeAsync(0);
		opened[0].onopen!();
		expect(order).toEqual(['connection:true', 'open:connected=true']);
		connection.close();
	});

	it('fires no onOpen before the stream opens', async () => {
		globalThis.fetch = vi.fn(async () => TOKEN_RESPONSE) as unknown as typeof fetch;
		const onOpen = vi.fn();
		const connection = connectMercure({ onUpdate: vi.fn(), onOpen });
		await vi.advanceTimersByTimeAsync(0);
		expect(opened).toHaveLength(1);
		expect(onOpen).not.toHaveBeenCalled();
		connection.close();
	});
});

describe('connection state through an outage', () => {
	async function openConnection() {
		globalThis.fetch = vi.fn(async () => TOKEN_RESPONSE) as unknown as typeof fetch;
		const onConnectionChange = vi.fn();
		const connection = connectMercure({ onUpdate: vi.fn(), onConnectionChange });
		await vi.advanceTimersByTimeAsync(0);
		opened[0].onopen!();
		onConnectionChange.mockClear();
		return { connection, onConnectionChange };
	}

	it('stays connected through an outage shorter than 5s', async () => {
		const { connection, onConnectionChange } = await openConnection();
		opened[0].onerror!();
		await vi.advanceTimersByTimeAsync(4999);
		expect(connection.connected).toBe(true);
		expect(onConnectionChange).not.toHaveBeenCalled();
		connection.close();
	});

	it('reports disconnected once the outage reaches 5s', async () => {
		// Fail every reconnect so the stream cannot reopen inside the window.
		const { connection, onConnectionChange } = await openConnection();
		globalThis.fetch = vi.fn(async () => { throw new Error('hub down'); }) as unknown as typeof fetch;
		opened[0].onerror!();
		await vi.advanceTimersByTimeAsync(5000);
		expect(connection.connected).toBe(false);
		expect(onConnectionChange.mock.calls).toEqual([[false]]);
		connection.close();
	});

	it('reports connected again when the stream reopens after the outage', async () => {
		const { connection, onConnectionChange } = await openConnection();
		globalThis.fetch = vi.fn(async () => { throw new Error('hub down'); }) as unknown as typeof fetch;
		opened[0].onerror!();
		await vi.advanceTimersByTimeAsync(5000);
		globalThis.fetch = vi.fn(async () => TOKEN_RESPONSE) as unknown as typeof fetch;
		await vi.advanceTimersByTimeAsync(60000);
		expect(opened).toHaveLength(2);
		opened[1].onopen!();
		expect(connection.connected).toBe(true);
		expect(onConnectionChange.mock.calls).toEqual([[false], [true]]);
		connection.close();
	});
});
