import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import type { LabLayout } from './navigation';
import {
	fetchLabLayout,
	DEFAULT_DIFFICULTY_ORDER,
	MAX_STATUS_ATTEMPTS,
	STATUS_RETRY_MS,
} from './layout-loader';

// The loader only reaches Rust through get_status; nothing else in the Tauri
// core module is needed, and the real one cannot load outside a webview.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);

const SERVER = 'http://127.0.0.1:8080';

const LAYOUT: LabLayout = {
	difficulty: 'Uber',
	date: '2026-08-05',
	weapon: 'sword',
	phase1: '',
	phase2: '',
	trap1: '',
	trap2: '',
	rooms: [],
};

function layoutResponse() {
	return { ok: true, json: async () => LAYOUT };
}

// Carries a body, deliberately: a stub that only sets ok=false lets a loader
// which ignores the status blow up on the missing json() and reach the catch,
// so the "no layout published" test would go green for the wrong reason.
const NOT_FOUND = { ok: false, status: 404, json: async () => ({ error: 'no layout for difficulty' }) };

/** Difficulty segment of a `/api/lab/layout/<difficulty>` URL. */
function requestedDifficulties(fetchMock: ReturnType<typeof vi.fn>): string[] {
	return fetchMock.mock.calls.map((call) => String(call[0]).split('/').pop() as string);
}

/** Long enough for every sleep the attempt budget allows. */
const WHOLE_BUDGET_MS = STATUS_RETRY_MS * (MAX_STATUS_ATTEMPTS + 1);

let originalFetch: typeof globalThis.fetch;

beforeEach(() => {
	originalFetch = globalThis.fetch;
	vi.useFakeTimers();
});

afterEach(() => {
	vi.useRealTimers();
	globalThis.fetch = originalFetch;
	vi.restoreAllMocks();
	invokeMock.mockReset();
});

describe('fetchLabLayout', () => {
	it('returns the layout the server publishes', async () => {
		// Positive control for the null-returning tests below: it proves this
		// arrangement does reach the layout request, so their null expectation is
		// the guard working rather than a harness that never got that far.
		invokeMock.mockResolvedValue({ server_url: SERVER });
		globalThis.fetch = vi.fn(async () => layoutResponse()) as unknown as typeof fetch;

		expect(await fetchLabLayout({ log: () => {} })).toEqual(LAYOUT);
	});

	it('waits for a server_url that arrives late, then fetches against it', async () => {
		invokeMock
			.mockResolvedValueOnce({ server_url: '' })
			.mockResolvedValueOnce({ server_url: '' })
			.mockResolvedValue({ server_url: SERVER });
		const fetchMock = vi.fn(async (_url: string) => layoutResponse());
		globalThis.fetch = fetchMock as unknown as typeof fetch;

		const pending = fetchLabLayout({ log: () => {} });
		await vi.advanceTimersByTimeAsync(STATUS_RETRY_MS * 2);

		expect(await pending).toEqual(LAYOUT);
		// Against the URL it waited for, not the empty one it saw first.
		expect(fetchMock.mock.calls[0][0]).toBe(`${SERVER}/api/lab/layout/Uber`);
	});

	it('gives up after the attempt budget of status checks', async () => {
		// The bound is what stops a server_url that never arrives from leaving a
		// 2s poll running for the life of the window.
		invokeMock.mockResolvedValue({ server_url: '' });
		const fetchMock = vi.fn(async () => layoutResponse());
		globalThis.fetch = fetchMock as unknown as typeof fetch;

		const pending = fetchLabLayout({ log: () => {} });
		await vi.advanceTimersByTimeAsync(WHOLE_BUDGET_MS);

		expect(await pending).toBeNull();
		expect(invokeMock).toHaveBeenCalledTimes(MAX_STATUS_ATTEMPTS);
		expect(fetchMock).not.toHaveBeenCalled();
	});

	it('reports the give-up through the log', async () => {
		// An exhausted loader renders nothing at all — no compass, no path strip,
		// no message on screen — so the log is the only trace it leaves. What has
		// to survive a refactor is that exhaustion is reported distinctly from the
		// routine retries; the wording is not pinned.
		invokeMock.mockResolvedValue({ server_url: '' });
		const logs: string[] = [];

		const pending = fetchLabLayout({ log: (msg) => logs.push(msg) });
		await vi.advanceTimersByTimeAsync(WHOLE_BUDGET_MS);
		await pending;

		expect(logs.at(-1)).toMatch(/giv(e|ing) up/i);
	});

	it('announces the wait once rather than once per poll', async () => {
		// Three overlay windows run this loader and the app log's UI buffer holds
		// 50 entries, so a line per poll per window evicts everything else during
		// the slow server start it is meant to help diagnose.
		invokeMock.mockResolvedValue({ server_url: '' });
		const logs: string[] = [];

		const pending = fetchLabLayout({ log: (msg) => logs.push(msg) });
		await vi.advanceTimersByTimeAsync(WHOLE_BUDGET_MS);
		await pending;

		// The wait and the give-up, and nothing in between.
		expect(logs).toHaveLength(2);
	});

	it('spends a fresh attempt budget on every call', async () => {
		// The budget used to be a module-scoped counter that nothing ever reset,
		// so it was a lifetime allowance for the window rather than a timeout for
		// one load. A window that spent it during a slow server start — and then
		// loaded a layout perfectly well — would refuse to retry a later transient
		// outage even once, and render nothing for the rest of its life.
		invokeMock.mockResolvedValue({ server_url: '' });
		globalThis.fetch = vi.fn(async () => layoutResponse()) as unknown as typeof fetch;

		const exhausting = fetchLabLayout({ log: () => {} });
		await vi.advanceTimersByTimeAsync(WHOLE_BUDGET_MS);
		expect(await exhausting).toBeNull();

		// The later outage this window has to survive: server_url is empty for one
		// poll before it comes back. The second call therefore has to still own a
		// retry — an already-spent lifetime budget leaves it none, and a call that
		// merely succeeds on its first poll cannot tell the two apart, because the
		// exhausted version still made that one poll.
		invokeMock
			.mockResolvedValueOnce({ server_url: '' })
			.mockResolvedValue({ server_url: SERVER });

		const recovering = fetchLabLayout({ log: () => {} });
		await vi.advanceTimersByTimeAsync(STATUS_RETRY_MS);

		expect(await recovering).toEqual(LAYOUT);
	});

	it('prefers the difficulty the triggering event named over the locked one', async () => {
		// LayoutChanged carries the difficulty the planner just switched to, so it
		// is newer than whatever this window settled on earlier.
		invokeMock.mockResolvedValue({ server_url: SERVER });
		const fetchMock = vi.fn(async () => layoutResponse());
		globalThis.fetch = fetchMock as unknown as typeof fetch;

		await fetchLabLayout({ preferredDifficulty: 'Cruel', lockedDifficulty: 'Uber', log: () => {} });

		expect(requestedDifficulties(fetchMock)).toEqual(['Cruel']);
	});

	it('walks the default difficulty order and stops at the first one published', async () => {
		invokeMock.mockResolvedValue({ server_url: SERVER });
		const fetchMock = vi.fn(async (url: string) =>
			url.endsWith('/Cruel') ? layoutResponse() : NOT_FOUND,
		);
		globalThis.fetch = fetchMock as unknown as typeof fetch;

		expect(await fetchLabLayout({ log: () => {} })).toEqual(LAYOUT);
		expect(requestedDifficulties(fetchMock)).toEqual(DEFAULT_DIFFICULTY_ORDER.slice(0, 3));
	});

	it('returns null when no difficulty has a published layout', async () => {
		invokeMock.mockResolvedValue({ server_url: SERVER });
		globalThis.fetch = vi.fn(async () => NOT_FOUND) as unknown as typeof fetch;

		expect(await fetchLabLayout({ log: () => {} })).toBeNull();
	});

	it('returns null rather than rejecting when the status IPC fails', async () => {
		// Callers do not await this from anywhere that could handle a rejection —
		// it runs from event handlers — so a throw would surface as an unhandled
		// promise rejection and nothing else.
		invokeMock.mockRejectedValue(new Error('IPC closed'));
		const logs: string[] = [];

		expect(await fetchLabLayout({ log: (msg) => logs.push(msg) })).toBeNull();
		expect(logs.at(-1)).toContain('IPC closed');
	});
});
