import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

// api.ts pulls in Tauri + the status store at module load; neither is needed for
// the connection lifecycle under test.
vi.mock('@tauri-apps/api/app', () => ({ getVersion: async () => '0.0.0-test' }));
vi.mock('$lib/stores/status.svelte', () => ({ store: { status: { server_url: 'https://hub.test' } } }));

const { connectMercure } = await import('./api');

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

		connectMercure(() => {});
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

		const connection = connectMercure(() => {});
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

		const connection = connectMercure(() => {});
		connection.close();
		token.resolve(TOKEN_RESPONSE);
		await vi.advanceTimersByTimeAsync(0);

		expect(vi.getTimerCount()).toBe(0);
	});

	it('stops retrying after close(), whatever stage the retry loop is in', async () => {
		const fetchMock = vi.fn(async () => { throw new Error('hub down'); });
		globalThis.fetch = fetchMock as unknown as typeof fetch;

		const connection = connectMercure(() => {});
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

		connectMercure(() => {});
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

		connectMercure(onUpdate);
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

		connectMercure(onUpdate);
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
