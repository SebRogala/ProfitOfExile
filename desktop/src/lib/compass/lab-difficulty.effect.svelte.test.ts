// @vitest-environment happy-dom
/**
 * The lab session, the compass widget, the run recorder and the Planner on a
 * difficulty switch, mounted in the client runtime under happy-dom. Rust is
 * the stubbed IPC (`invoke` records every call, `listen` is one shared event
 * bus) and the server is the stubbed `fetch`, which serves fixture layouts
 * and answers 404 for a difficulty with none, as the layout handler does.
 * The layout loader, the session and the recorder are the real modules.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import uberFixture from './__fixtures__/poelab-prod-2026-08-02-uber.json';
import mercilessFixture from './__fixtures__/poelab-prod-2026-07-31-merciless.json';

type Handler = (event: { payload: unknown }) => void;

const ipc = vi.hoisted(() => ({
	calls: [] as [string, unknown][],
	settings: {} as Record<string, unknown>,
	listeners: new Map<string, Set<(event: { payload: unknown }) => void>>()
}));

vi.mock('@tauri-apps/api/core', () => ({
	invoke: vi.fn(async (cmd: string, args?: unknown) => {
		ipc.calls.push([cmd, args]);
		switch (cmd) {
			case 'get_compass_settings':
				return { ...ipc.settings };
			case 'get_lab_catchup':
				return { events: [] };
			case 'get_status':
				return { device_id: 'dev-test', app_version: '0.0.0-test' };
			case 'get_privacy_state':
				return { accepted: true };
			default:
				return null;
		}
	})
}));
vi.mock('@tauri-apps/api/event', () => ({
	listen: vi.fn(async (event: string, handler: (event: { payload: unknown }) => void) => {
		const set = ipc.listeners.get(event) ?? new Set();
		set.add(handler);
		ipc.listeners.set(event, set);
		return () => {
			set.delete(handler);
		};
	})
}));

const SERVER = 'http://lab-server.test';

/** Published layouts by difficulty; any other difficulty answers 404. */
const PUBLISHED = new Map<string, unknown>([
	['Uber', uberFixture],
	['Merciless', mercilessFixture]
]);

let { flushSync, mount, unmount } = await import('svelte');
let labSession: typeof import('./lab-session.svelte');
let LabCompassWidget: typeof import('./LabCompassWidget.svelte').default;
let PlannerPage: typeof import('$lib/pages/PlannerPage.svelte').default;
let startRunRecorder: typeof import('$lib/run-recorder').startRunRecorder;

/** Difficulty segment of every GET /api/lab/layout/<difficulty>, in order. */
let layoutRequests: string[];
/** Bodies POSTed to /api/lab/runs. */
let runPosts: Record<string, unknown>[];
let originalFetch: typeof globalThis.fetch;

function respond(status: number, body: unknown) {
	return { ok: status >= 200 && status < 300, status, json: async () => body, text: async () => JSON.stringify(body) };
}

beforeEach(async () => {
	vi.useFakeTimers();
	// The recorder, the server base and the session keep module state; fresh
	// modules per case keep one case's layout or listeners out of the next.
	vi.resetModules();
	({ flushSync, mount, unmount } = await import('svelte'));
	const serverBase = await import('$lib/stores/server-base.svelte');
	serverBase.applyServerSlice({ url: SERVER, defaultUrl: SERVER });
	labSession = await import('./lab-session.svelte');
	LabCompassWidget = (await import('./LabCompassWidget.svelte')).default;
	PlannerPage = (await import('$lib/pages/PlannerPage.svelte')).default;
	({ startRunRecorder } = await import('$lib/run-recorder'));

	layoutRequests = [];
	runPosts = [];
	originalFetch = globalThis.fetch;
	globalThis.fetch = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
		const url = String(input);
		if (url === `${SERVER}/api/lab/runs` && init?.method === 'POST') {
			runPosts.push(JSON.parse(String(init.body)));
			return respond(200, { run_id: 1 });
		}
		const layoutPrefix = `${SERVER}/api/lab/layout/`;
		if (url.startsWith(layoutPrefix) && (init?.method ?? 'GET') === 'GET') {
			const difficulty = url.slice(layoutPrefix.length);
			layoutRequests.push(difficulty);
			const layout = PUBLISHED.get(difficulty);
			return layout ? respond(200, layout) : respond(404, { error: 'no layout for difficulty' });
		}
		return respond(500, { error: `unexpected request ${init?.method ?? 'GET'} ${url}` });
	}) as unknown as typeof fetch;
});

let app: ReturnType<typeof mount> | null = null;
let releaseSession: (() => void) | null = null;

afterEach(() => {
	if (app) unmount(app);
	app = null;
	releaseSession?.();
	releaseSession = null;
	document.body.innerHTML = '';
	globalThis.fetch = originalFetch;
	vi.useRealTimers();
	ipc.calls.length = 0;
	ipc.settings = {};
	ipc.listeners.clear();
});

/** Let pending IPC, fetches and effects run without advancing the poll clock. */
async function settle(): Promise<void> {
	for (let i = 0; i < 5; i++) {
		await vi.advanceTimersByTimeAsync(0);
		flushSync();
	}
}

/** What Rust's lab-nav emit delivers to every window's listener. */
function emitLabNav(payload: unknown): void {
	for (const handler of [...(ipc.listeners.get('lab-nav') ?? [])] as Handler[]) handler({ payload });
}

/**
 * Pick `value` in a select as a user does. Svelte's select binding reads the
 * pick through `:checked`, which happy-dom 21 matches on INPUT only (observed in
 * its SelectorItem), so the binding would fall back to the first option; this
 * select answers `:checked` with its selected option, as a browser does.
 */
function pickOption(select: HTMLSelectElement, value: string): void {
	const index = [...select.options].findIndex((option) => option.value === value);
	if (index < 0) throw new Error(`the select offers no ${value} option`);
	select.selectedIndex = index;
	const query = select.querySelector.bind(select);
	select.querySelector = ((selector: string) =>
		selector === ':checked' ? select.options[select.selectedIndex] : query(selector)) as typeof select.querySelector;
	select.dispatchEvent(new Event('change', { bubbles: true }));
}

const emittedLabNav = () => ipc.calls.filter(([cmd]) => cmd === 'emit_lab_nav').map(([, args]) => (args as { eventJson: unknown }).eventJson);

describe('lab difficulty selection', () => {
	it('keep:regression a LayoutChanged to a difficulty the server has no layout for leaves no stale Uber layout behind', async () => {
		ipc.settings = { difficulty: 'Uber', mode: 'minimap' };
		startRunRecorder();
		const session = labSession.createLabSession();
		app = mount(LabCompassWidget, {
			target: document.body,
			props: { configMode: false, label: 'Compass' },
			context: new Map([[labSession.LAB_SESSION, session]])
		});
		await settle();
		emitLabNav({ type: 'PlazaEntered' });
		emitLabNav({ type: 'RoomChanged', name: 'Estate Path' });
		await settle();
		// The stale state this case starts from: today's Uber layout drives a visible compass.
		expect(session.layoutLoaded).toBe(true);
		expect(session.navState.layout?.difficulty).toBe('Uber');
		expect(document.querySelector('.compass-content')).not.toBeNull();

		emitLabNav({ type: 'LayoutChanged', difficulty: 'Cruel' });
		await settle();
		// Still in the lab, so only the missing layout can take the compass down.
		expect(layoutRequests).toContain('Cruel');
		expect({
			layoutLoaded: session.layoutLoaded,
			layout: session.navState.layout?.difficulty ?? null,
			rooms: [...session.navState.roomById.values()].map((room) => room.name)
		}).toEqual({ layoutLoaded: false, layout: null, rooms: [] });
		expect(document.querySelector('.compass-content')).toBeNull();

		// The run goes on in the Cruel lab, through a room name today's Uber layout also has.
		emitLabNav({ type: 'RoomChanged', name: 'Sepulchre Atrium' });
		await vi.advanceTimersByTimeAsync(5000);
		emitLabNav({ type: 'LabFinished' });
		emitLabNav({ type: 'LabExited' });
		await settle();

		expect(runPosts).toHaveLength(1);
		expect({ difficulty: runPosts[0].difficulty, has_golden_door: runPosts[0].has_golden_door }).toEqual({
			difficulty: 'Cruel',
			has_golden_door: false
		});
	});

	it('keep:regression a difficulty selected after the first lock reaches the lab overlays even without a layout', async () => {
		ipc.settings = { difficulty: 'Uber' };
		const session = labSession.createLabSession();
		releaseSession = session.acquire({});
		await settle();
		// The lock this case starts from: the saved Uber setting.
		expect(session.lockedDifficulty).toBe('Uber');

		ipc.settings = { difficulty: 'Merciless' };
		await vi.advanceTimersByTimeAsync(2000);
		await settle();
		expect(session.lockedDifficulty).toBe('Merciless');
		expect(layoutRequests).toContain('Merciless');

		// The next tick carries the same saved value; it neither re-locks nor refetches.
		await vi.advanceTimersByTimeAsync(2000);
		await settle();
		expect({
			locked: session.lockedDifficulty,
			layout: session.navState.layout?.difficulty ?? null,
			mercilessRequests: layoutRequests.filter((d) => d === 'Merciless').length
		}).toEqual({ locked: 'Merciless', layout: 'Merciless', mercilessRequests: 1 });

		app = mount(PlannerPage, { target: document.body });
		await settle();
		const difficultySelect = document.querySelector<HTMLSelectElement>('.header-controls select');
		if (!difficultySelect) throw new Error('the Planner difficulty select did not render');
		pickOption(difficultySelect, 'Cruel');
		flushSync();
		await settle();

		expect(layoutRequests.at(-1)).toBe('Cruel');
		expect(emittedLabNav().at(-1)).toEqual({ type: 'LayoutChanged', difficulty: 'Cruel' });
		expect(document.querySelector('.empty-message')?.textContent).toContain("today's Cruel lab");
	});
});
