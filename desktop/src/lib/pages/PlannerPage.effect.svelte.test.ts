// @vitest-environment happy-dom
/**
 * The Planner tab against the lab difficulty Rust detects, mounted in the
 * client runtime under happy-dom. Rust is the stubbed IPC (`invoke` records
 * every call, `listen` is one shared event bus) and the server is the stubbed
 * `fetch`, which serves fixture layouts by difficulty.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import uberFixture from '$lib/compass/__fixtures__/poelab-prod-2026-08-02-uber.json';
import mercilessFixture from '$lib/compass/__fixtures__/poelab-prod-2026-07-31-merciless.json';

type Handler = (event: { payload: unknown }) => void;

const ipc = vi.hoisted(() => ({
	calls: [] as [string, unknown][],
	settings: {} as Record<string, unknown>,
	catchup: { events: [] } as { events: unknown[]; in_lab?: boolean; layout_difficulty?: string | null },
	listeners: new Map<string, Set<(event: { payload: unknown }) => void>>()
}));

vi.mock('@tauri-apps/api/core', () => ({
	invoke: vi.fn(async (cmd: string, args?: unknown) => {
		ipc.calls.push([cmd, args]);
		switch (cmd) {
			case 'get_compass_settings':
				return { ...ipc.settings };
			case 'get_lab_catchup':
				return ipc.catchup;
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

const PUBLISHED = new Map<string, unknown>([
	['Uber', uberFixture],
	['Merciless', mercilessFixture]
]);

let { flushSync, mount, unmount } = await import('svelte');
let PlannerPage: typeof import('$lib/pages/PlannerPage.svelte').default;

/** Difficulty segment of every GET /api/lab/layout/<difficulty>, in order. */
let layoutRequests: string[];
let originalFetch: typeof globalThis.fetch;
let app: ReturnType<typeof mount> | null = null;

function respond(status: number, body: unknown) {
	return { ok: status >= 200 && status < 300, status, json: async () => body, text: async () => JSON.stringify(body) };
}

beforeEach(async () => {
	vi.useFakeTimers();
	vi.resetModules();
	({ flushSync, mount, unmount } = await import('svelte'));
	const serverBase = await import('$lib/stores/server-base.svelte');
	serverBase.applyServerSlice({ url: SERVER, defaultUrl: SERVER });
	PlannerPage = (await import('$lib/pages/PlannerPage.svelte')).default;

	layoutRequests = [];
	originalFetch = globalThis.fetch;
	globalThis.fetch = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
		const url = String(input);
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

afterEach(async () => {
	if (app) await unmount(app);
	app = null;
	document.body.innerHTML = '';
	globalThis.fetch = originalFetch;
	vi.clearAllTimers();
	vi.useRealTimers();
	ipc.calls.length = 0;
	ipc.settings = {};
	ipc.catchup = { events: [] };
	ipc.listeners.clear();
});

/** Let pending IPC, fetches and effects run. */
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

/** Pick `value` as a user does; see lab-difficulty.effect.svelte.test.ts for the `:checked` shim. */
function pickOption(select: HTMLSelectElement, value: string): void {
	const index = [...select.options].findIndex((option) => option.value === value);
	if (index < 0) throw new Error(`the select offers no ${value} option`);
	select.selectedIndex = index;
	const query = select.querySelector.bind(select);
	select.querySelector = ((selector: string) =>
		selector === ':checked' ? select.options[select.selectedIndex] : query(selector)) as typeof select.querySelector;
	select.dispatchEvent(new Event('change', { bubbles: true }));
}

function difficultySelect(): HTMLSelectElement {
	const select = document.querySelector<HTMLSelectElement>('.header-controls select');
	if (!select) throw new Error('the Planner difficulty select did not render');
	return select;
}

/** The difficulty the user sees picked, and the layout the planner last loaded for it. */
function shown() {
	const select = difficultySelect();
	return { picked: select.options[select.selectedIndex]?.textContent?.trim(), layout: layoutRequests.at(-1) };
}

const UBER_SHOWN = { picked: 'Uber Lab', layout: 'Uber' };
const MERCILESS_SHOWN = { picked: 'Merciless', layout: 'Merciless' };

const callsOf = (cmd: string) => ipc.calls.filter(([c]) => c === cmd).map(([, args]) => args);
const emittedLabNav = () => callsOf('emit_lab_nav').map((args) => (args as { eventJson: unknown }).eventJson);

async function mountPlanner(): Promise<void> {
	app = mount(PlannerPage, { target: document.body });
	await settle();
}

describe('POE-525 planner follows the detected lab difficulty', () => {
	beforeEach(() => {
		ipc.settings = { difficulty: 'Merciless', mode: 'minimap' };
	});

	it('keep:regression POE-525 an in-lab catch-up with an Uber layout shows Uber over the saved Merciless', async () => {
		ipc.catchup = { in_lab: true, layout_difficulty: 'Uber', events: [] };
		await mountPlanner();
		expect(shown()).toEqual(UBER_SHOWN);
	});

	it.each(['Uber', 'Eternal'])(
		'keep:regression POE-525 a detected LayoutChanged to %s shows Uber over the saved Merciless',
		async (detected) => {
			await mountPlanner();
			// The state this case starts from: no lab yet, so the saved difficulty.
			expect(shown()).toEqual(MERCILESS_SHOWN);

			emitLabNav({ type: 'LayoutChanged', difficulty: detected });
			await settle();
			expect(shown()).toEqual(UBER_SHOWN);
		}
	);

	it('keep:regression POE-525 a catch-up outside a lab keeps the saved Merciless', async () => {
		ipc.catchup = { in_lab: false, layout_difficulty: 'Uber', events: [] };
		await mountPlanner();
		expect(shown()).toEqual(MERCILESS_SHOWN);
	});

	// Silent: a pick that never reaches the overlays leaves them guiding through another lab's layout.
	it('keep:critical POE-525 a manual pick saves the difficulty and tells the overlays', async () => {
		await mountPlanner();
		pickOption(difficultySelect(), 'Uber');
		flushSync();
		await settle();

		expect(callsOf('set_compass_difficulty')).toEqual([{ difficulty: 'Uber' }]);
		expect(emittedLabNav().at(-1)).toEqual({ type: 'LayoutChanged', difficulty: 'Uber' });
	});

	// Irreversible + silent: a detection written back would overwrite the player's saved
	// difficulty, and a planner emit fighting the detection would switch every overlay
	// to the saved lab's layout and bounce back to the planner as another LayoutChanged.
	it('keep:critical POE-525 an in-lab catch-up neither saves the detected difficulty nor tells the overlays another one', async () => {
		ipc.catchup = { in_lab: true, layout_difficulty: 'Uber', events: [] };
		await mountPlanner();

		expect(callsOf('set_compass_difficulty')).toEqual([]);
		const detected = JSON.stringify({ type: 'LayoutChanged', difficulty: 'Uber' });
		expect(emittedLabNav().filter((event) => JSON.stringify(event) !== detected)).toEqual([]);
	});

	it.each(['Uber', 'Eternal'])(
		'keep:critical POE-525 a detected LayoutChanged to %s neither saves the difficulty nor echoes to the overlays',
		async (detected) => {
			await mountPlanner();
			const emitsBefore = emittedLabNav().length;

			emitLabNav({ type: 'LayoutChanged', difficulty: detected });
			await settle();

			expect(callsOf('set_compass_difficulty')).toEqual([]);
			expect(emittedLabNav().slice(emitsBefore)).toEqual([]);
		}
	);
});
