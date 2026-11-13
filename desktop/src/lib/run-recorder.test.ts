import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { resetServerBaseForTests } from '$lib/stores/server-base.svelte';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));
// No layout: the run is still posted, with has_golden_door=false.
vi.mock('$lib/compass/layout-loader', () => ({ fetchLabLayout: vi.fn(async () => null) }));

const CARRIER = 'http://carrier.test:8080';
const STATUS_URL = 'http://status.test:9090';

const invokeMock = vi.mocked(invoke);
const listenMock = vi.mocked(listen);

let handlers: Record<string, (e: { payload: unknown }) => void>;
let fetchMock: ReturnType<typeof vi.fn>;
let originalFetch: typeof globalThis.fetch;

beforeEach(() => {
	vi.resetModules();
	vi.useFakeTimers();
	resetServerBaseForTests();
	handlers = {};
	listenMock.mockImplementation(async (name: string, fn: any) => {
		handlers[name] = fn;
		return () => {};
	});
	invokeMock.mockImplementation(async (cmd: string) => {
		if (cmd === 'get_status') return { server_url: STATUS_URL, device_id: 'dev-1', app_version: '9.9.9' };
		return null; // get_compass_settings, get_lab_catchup, app_log_from_frontend
	});
	originalFetch = globalThis.fetch;
	fetchMock = vi.fn(async () => ({ ok: true, json: async () => ({ run_id: 1 }) }));
	globalThis.fetch = fetchMock as unknown as typeof fetch;
});

afterEach(() => {
	vi.useRealTimers();
	globalThis.fetch = originalFetch;
	invokeMock.mockReset();
	listenMock.mockReset();
	resetServerBaseForTests();
});

/** Start the recorder and play one run, plaza to exit, with a 5 s kill. */
async function playFinishedRun(): Promise<[string, RequestInit]> {
	// The store module the recorder sees must be the same instance the test seeds.
	const { applyServerSlice: seed } = await import('$lib/stores/server-base.svelte');
	seed({ url: CARRIER, defaultUrl: CARRIER });
	const { startRunRecorder } = await import('$lib/run-recorder');
	startRunRecorder();
	await vi.advanceTimersByTimeAsync(0);

	const nav = (payload: unknown) => handlers['lab-nav']({ payload });
	nav({ type: 'PlazaEntered' });
	nav({ type: 'RoomChanged', name: 'Estate Walkways' });
	await vi.advanceTimersByTimeAsync(5000);
	nav({ type: 'LabFinished' });
	nav({ type: 'LabExited' });
	await vi.advanceTimersByTimeAsync(0);

	const post = fetchMock.mock.calls.find(([url]) => String(url).endsWith('/api/lab/runs'));
	expect(post).toBeDefined();
	return post as [string, RequestInit];
}

describe('run recorder submission', () => {
	it("a finished run is posted to the carrier's server", async () => {
		const [url] = await playFinishedRun();
		expect(url).toBe(`${CARRIER}/api/lab/runs`);
	});

	it('a finished run keeps its device identity headers', async () => {
		const [, init] = await playFinishedRun();
		const headers = init.headers as Record<string, string>;
		expect(headers['X-Device-ID']).toBe('dev-1');
		expect(headers['X-App-Version']).toBe('9.9.9');
	});

	it('posts nothing while the server url is unknown', async () => {
		const { startRunRecorder } = await import('$lib/run-recorder');
		startRunRecorder();
		await vi.advanceTimersByTimeAsync(0);
		const nav = (payload: unknown) => handlers['lab-nav']({ payload });
		nav({ type: 'PlazaEntered' });
		nav({ type: 'RoomChanged', name: 'Estate Walkways' });
		await vi.advanceTimersByTimeAsync(5000);
		nav({ type: 'LabFinished' });
		nav({ type: 'LabExited' });
		await vi.advanceTimersByTimeAsync(0);

		expect(fetchMock).not.toHaveBeenCalled();
	});
});
