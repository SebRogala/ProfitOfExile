// @vitest-environment happy-dom
/**
 * The lab window's real session and the prompt window's real page, joined by
 * one in-memory Tauri event bus (POE-483 N1). Rust is the stubbed IPC
 * (`invoke`) and the shared bus (`listen` / `emit` / `emitTo` reach every
 * window's listeners, as Tauri's global events do); the server is the stubbed
 * `fetch`, which answers 404 for a difficulty with no published layout. The
 * layout loader, the lab session, the prompt presentation and its dismissals
 * are the real modules; only WidgetHost placement is doubled.
 *
 * The prompt area holds two slots: the lab-layout notice (N1) in
 * `[data-slot="layout"]` above the read prompt in `[data-slot="read"]`.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import uberFixture from '../compass/__fixtures__/poelab-prod-2026-08-02-uber.json';
import mercilessFixture from '../compass/__fixtures__/poelab-prod-2026-07-31-merciless.json';

type Handler = (event: { event: string; id: number; payload: unknown }) => void;

const bus = vi.hoisted(() => {
	const listeners = new Map<string, Set<(event: { event: string; id: number; payload: unknown }) => void>>();
	return {
		listeners,
		settings: {} as Record<string, unknown>,
		/** Tauri delivers an emit asynchronously, to every window's listeners. */
		deliver: (event: string, payload: unknown): Promise<void> => Promise.resolve().then(() => {
			for (const handler of [...(listeners.get(event) ?? [])]) handler({ event, id: 0, payload });
		})
	};
});
const deliver = bus.deliver;

vi.mock('@tauri-apps/api/core', () => ({
	invoke: vi.fn(async (command: string) => {
		switch (command) {
			case 'get_compass_settings':
				return { ...bus.settings };
			case 'get_lab_catchup':
				return { events: [] };
			default:
				return null;
		}
	})
}));
vi.mock('@tauri-apps/api/event', () => {
	const send = (event: string, payload?: unknown) => bus.deliver(event, payload ?? null);
	return {
		listen: vi.fn(async (event: string, handler: Handler) => {
			const set = bus.listeners.get(event) ?? new Set();
			set.add(handler);
			bus.listeners.set(event, set);
			return () => { set.delete(handler); };
		}),
		emit: vi.fn(async (event: string, payload?: unknown) => send(event, payload)),
		emitTo: vi.fn(async (_target: unknown, event: string, payload?: unknown) => send(event, payload))
	};
});
vi.mock('$lib/overlay/widgets/WidgetHost.svelte', async () => ({
	default: (await import('./fixtures/WidgetHostDouble.svelte')).default
}));

// No vi.resetModules(): after a reset the mocked WidgetHost double and the page
// resolve different Svelte runtime instances (see prompt-page.effect.svelte.test.ts).
const { flushSync, mount, unmount } = await import('svelte');
const PromptPage = (await import('../../routes/overlay/prompt/+page.svelte')).default;
const { ssot } = await import('$lib/stores/ssot.svelte');
const { applyServerSlice } = await import('$lib/stores/server-base.svelte');
const { createLabSession } = await import('$lib/compass/lab-session.svelte');

const SERVER = 'http://lab-server.test';
const esc = { key: 27, shift: false, ctrl: false, alt: false };

/** Published layouts by difficulty; any other difficulty answers 404. */
let published: Map<string, unknown>;
/** Difficulty segment of every GET /api/lab/layout/<difficulty>, with its status. */
let layoutRequests: string[];
let originalFetch: typeof globalThis.fetch;
let originalFrame: typeof globalThis.requestAnimationFrame;
let mounted: ReturnType<typeof mount> | null = null;
let release: (() => void) | null = null;
let savedSsot: Pick<typeof ssot, 'readPrompt' | 'readHotkey' | 'overlayDismiss'>;

function respond(status: number, body: unknown) {
	return { ok: status >= 200 && status < 300, status, json: async () => body, text: async () => JSON.stringify(body) };
}

beforeEach(() => {
	vi.useFakeTimers();
	savedSsot = { readPrompt: ssot.readPrompt, readHotkey: ssot.readHotkey, overlayDismiss: ssot.overlayDismiss };
	applyServerSlice({ url: SERVER, defaultUrl: SERVER });
	bus.settings = { difficulty: 'Uber' };
	published = new Map<string, unknown>([['Uber', uberFixture]]);
	layoutRequests = [];
	originalFetch = globalThis.fetch;
	globalThis.fetch = vi.fn(async (input: string | URL | Request) => {
		const url = String(input);
		const prefix = `${SERVER}/api/lab/layout/`;
		if (!url.startsWith(prefix)) return respond(500, { error: `unexpected request ${url}` });
		const difficulty = url.slice(prefix.length);
		const layout = published.get(difficulty);
		layoutRequests.push(`${difficulty} ${layout ? 200 : 404}`);
		return layout ? respond(200, layout) : respond(404, { error: 'no layout for difficulty' });
	}) as unknown as typeof fetch;
	originalFrame = globalThis.requestAnimationFrame;
	globalThis.requestAnimationFrame = ((callback: FrameRequestCallback) => {
		queueMicrotask(() => callback(0));
		return 1;
	}) as typeof requestAnimationFrame;
});

function teardown(): void {
	if (mounted) unmount(mounted);
	mounted = null;
	release?.();
	release = null;
	document.body.innerHTML = '';
	bus.listeners.clear();
}

afterEach(() => {
	teardown();
	globalThis.fetch = originalFetch;
	globalThis.requestAnimationFrame = originalFrame;
	Object.assign(ssot, savedSsot);
	vi.useRealTimers();
});

/** Let pending IPC, bus deliveries, fetches and effects run without moving the clock. */
async function settle(): Promise<void> {
	for (let i = 0; i < 10; i++) {
		await vi.advanceTimersByTimeAsync(0);
		flushSync();
	}
}

/** A fresh prompt window and a fresh lab window, sharing the bus, after the saved Uber layout loads. */
async function openWindows(): Promise<HTMLElement> {
	teardown();
	const target = document.createElement('div');
	document.body.appendChild(target);
	mounted = mount(PromptPage, { target });
	release = createLabSession().acquire({});
	await settle();
	await deliver('lab-nav', { type: 'PlazaEntered' });
	await settle();
	return target;
}

/** Which slot each rendered notice sits in, top to bottom. */
function slots(target: HTMLElement): (string | null)[] {
	return Array.from(target.querySelectorAll('[data-kind]'), (notice) =>
		notice.closest('[data-slot]')?.getAttribute('data-slot') ?? null);
}

function layoutNotice(target: HTMLElement) {
	const notice = target.querySelector('[data-slot="layout"] [data-kind]');
	return notice && {
		kind: notice.getAttribute('data-kind'),
		header: notice.querySelector('[data-part="header"]')?.textContent,
		body: notice.querySelector('[data-part="body"]')?.textContent
	};
}

function readHeader(target: HTMLElement): string | null | undefined {
	return target.querySelector('[data-slot="read"] [data-kind] [data-part="header"]')?.textContent;
}

function showNoMatch(seq: number): void {
	ssot.readPrompt = {
		mechanic: null, noMatch: true, recalibrating: false, recalibrateOffer: true, seq,
		gemUncertain: [false, false, false]
	};
}

const MERCILESS_NOTICE = { kind: 'warning', header: 'No layout for Merciless lab', body: 'Import a layout in the Planner' };

describe('missing-layout notice', () => {
	it('keep:critical a selected-difficulty 404 in the lab window shows the missing-layout notice in the prompt window, which a loaded layout, its own timeout and the shared dismiss key clear (else the compass vanishes with no explanation)', async () => {
		ssot.readHotkey = { shortcut: { key: 65, shift: false, ctrl: true, alt: false }, delayMs: 0, recalibrateSeconds: 30 };
		ssot.overlayDismiss = { timeoutSeconds: 10, shortcut: esc, overlays: {}, pending: [] };
		ssot.readPrompt = { mechanic: null, noMatch: false, recalibrating: false, recalibrateOffer: false, seq: 100, gemUncertain: [false, false, false] };

		// Delivery: the saved Uber layout draws nothing in the prompt; Merciless answers 404.
		let target = await openWindows();
		expect(slots(target)).toEqual([]);
		await deliver('lab-nav', { type: 'LayoutChanged', difficulty: 'Merciless' });
		await settle();
		// The real loader saw the 404, so an absent notice is a delivery failure, not a harness one.
		expect(layoutRequests).toEqual(['Uber 200', 'Merciless 404']);
		expect(layoutNotice(target)).toEqual(MERCILESS_NOTICE);

		// A layout imported in the Planner loads through the server push and clears the notice.
		published.set('Merciless', mercilessFixture);
		await deliver('lab-layout-updated', { difficulty: 'Merciless' });
		await settle();
		expect(slots(target)).toEqual([]);
		published.delete('Merciless');

		// AC-6: one dismiss-key press hides both N1 and R1.
		showNoMatch(101);
		target = await openWindows();
		await deliver('lab-nav', { type: 'LayoutChanged', difficulty: 'Merciless' });
		await settle();
		expect(slots(target)).toEqual(['layout', 'read']);
		expect(layoutNotice(target)).toEqual(MERCILESS_NOTICE);
		await deliver('overlay-dismiss-key', { gen: 1001, shortcut: esc });
		await settle();
		expect(slots(target)).toEqual([]);

		// AC-6: N1's global 10 s timeout hides only N1; R1 moves to the top slot.
		showNoMatch(101);
		target = await openWindows();
		await deliver('lab-nav', { type: 'LayoutChanged', difficulty: 'Merciless' });
		await settle();
		expect(slots(target)).toEqual(['layout', 'read']);
		await vi.advanceTimersByTimeAsync(9999);
		flushSync();
		expect(slots(target)).toEqual(['layout', 'read']);
		await vi.advanceTimersByTimeAsync(1);
		flushSync();
		expect(slots(target)).toEqual(['read']);
		expect(readHeader(target)).toBe('No mechanics detected');

		// A new read result shows only R1; a hidden N1 stays hidden.
		showNoMatch(102);
		await settle();
		expect(slots(target)).toEqual(['read']);

		// A different missing difficulty shows N1 again, above R1.
		await deliver('lab-nav', { type: 'LayoutChanged', difficulty: 'Cruel' });
		await settle();
		expect(slots(target)).toEqual(['layout', 'read']);
		expect(layoutNotice(target)).toEqual({ ...MERCILESS_NOTICE, header: 'No layout for Cruel lab' });

		// AC-6: R1 ending before N1's deadline removes only R1.
		showNoMatch(103);
		target = await openWindows();
		await deliver('lab-nav', { type: 'LayoutChanged', difficulty: 'Merciless' });
		await settle();
		expect(slots(target)).toEqual(['layout', 'read']);
		await vi.advanceTimersByTimeAsync(5000);
		ssot.readPrompt = { mechanic: null, noMatch: false, recalibrating: false, recalibrateOffer: false, seq: 103, gemUncertain: [false, false, false] };
		await settle();
		expect(slots(target)).toEqual(['layout']);
		expect(layoutNotice(target)).toEqual(MERCILESS_NOTICE);
	});
});
