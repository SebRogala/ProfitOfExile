// @vitest-environment happy-dom
/**
 * The temple overlay route mounted with the real WidgetHost, WidgetActions and
 * TempleOfferBoxes, the Tauri boundary mocked (POE-439 variant C keep window).
 *
 * happy-dom has no layout, so this file supplies the measurements the route
 * reads: the offer boxes' committed-frame size through `offsetWidth` /
 * `offsetHeight` and `ResizeObserver`, a scale factor of 1, and an
 * `elementFromPoint` that resolves the keep window's rect from its own style.
 * It proves the forwarded-click dispatch and the timer wiring, not browser CSS
 * hit testing.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';
import { applySnapshot, type SsotSnapshot } from '$lib/stores/ssot.svelte';
import { templeSliceDefault, type CaptureRect, type TempleSlice } from '$lib/temple/slice';

const calls: [string, unknown][] = [];
const emissions: [string, unknown][] = [];
const busHandlers: Record<string, (event: { payload: unknown }) => void> = {};
const windowHandlers: Record<string, (event: { payload: unknown }) => void> = {};

vi.mock('@tauri-apps/api/core', () => ({
	invoke: vi.fn(async (command: string, args?: unknown) => {
		calls.push([command, args ?? null]);
		if (command === 'get_widget_geometries') return [];
		if (command === 'get_overlay_config_mode') return false;
		return undefined;
	})
}));
vi.mock('@tauri-apps/api/event', () => ({
	emit: vi.fn(async (event: string, payload?: unknown) => { emissions.push([event, payload ?? null]); }),
	listen: vi.fn(async (event: string, handler: (event: { payload: unknown }) => void) => {
		busHandlers[event] = handler;
		return () => { if (busHandlers[event] === handler) delete busHandlers[event]; };
	})
}));
vi.mock('@tauri-apps/api/webviewWindow', () => ({
	getCurrentWebviewWindow: () => ({
		scaleFactor: async () => 1,
		emit: async (event: string, payload?: unknown) => { emissions.push([event, payload ?? null]); },
		listen: async (event: string, handler: (event: { payload: unknown }) => void) => {
			windowHandlers[event] = handler;
			return () => { if (windowHandlers[event] === handler) delete windowHandlers[event]; };
		}
	})
}));

/** The upper offer box on the committed frame: 300 wide (the registry's
 *  width), 216 tall (POE-439-C NOTES.md). */
const BOX = { w: 300, h: 216 };
const KEEP = { w: 164, h: 66 };
const esc = { key: 27, shift: false, ctrl: false, alt: false };

function offer(index: number, rect: CaptureRect) {
	return {
		index, architectName: index === 0 ? 'Puhuarte' : 'Tacati', kind: 'upgrade',
		printedTarget: "Sadist's Den", displayName: 'Torment Cells', builtTier: 2,
		grade: null, lineTop: null, rect
	};
}

function temple(readAt: number): TempleSlice {
	return {
		...templeSliceDefault(),
		status: 'read',
		lastReadAt: readAt,
		layout: {
			slots: [], doors: [], uncertain: [], unresolvedIncident: [], markerError: null, current: null,
			scale: 1, ncc: 0.95, confidence: 'high', origin: [0, 0],
			centres: Array.from({ length: 13 }, () => [0, 0]) as never,
			rois: [
				{ kind: 'panel', of: null, rect: [1131, 4, 544, 454] },
				{ kind: 'plate', of: 'D0', rect: [556, 554, 172, 77] }
			],
			diamond: null, cradle: false
		},
		panel: {
			room: null, roomRect: null, incursionsRemaining: null,
			offers: [offer(0, [1484, 133, 154, 54]), offer(1, [1188, 289, 160, 39])]
		},
		advice: {
			recommendations: [{
				headline: 'upgrade → Torment Cells', doorsLabel: 'C1-C2', doors: ['C1-C2'],
				architectIndex: 0, ev: 12, risk: null, reasons: []
			}],
			gambles: [], mapAction: 'continue', warnings: [], forcedKill: false
		}
	} as TempleSlice;
}

function seed(timeoutSeconds: number) {
	applySnapshot({
		temple: temple(Date.now()),
		overlayDismiss: { timeoutSeconds, shortcut: esc, overlays: {}, pending: [] }
	} as unknown as SsotSnapshot);
}

let mounted: ReturnType<typeof mount> | null = null;
const restores: (() => void)[] = [];

function patch<T extends object, K extends keyof T>(target: T, key: K, descriptor: PropertyDescriptor) {
	const original = Object.getOwnPropertyDescriptor(target, key);
	Object.defineProperty(target, key, { configurable: true, ...descriptor });
	restores.push(() => {
		if (original) Object.defineProperty(target, key, original);
		else delete (target as Record<PropertyKey, unknown>)[key as PropertyKey];
	});
}

beforeEach(() => {
	vi.useFakeTimers();
	calls.length = 0;
	emissions.length = 0;
	const size = (axis: 'w' | 'h') => function (this: HTMLElement) {
		return this.classList.contains('box') ? BOX[axis] : 0;
	};
	patch(HTMLElement.prototype, 'offsetWidth', { get: size('w') });
	patch(HTMLElement.prototype, 'offsetHeight', { get: size('h') });
	patch(globalThis, 'ResizeObserver', {
		writable: true,
		value: class {
			constructor(private readonly callback: ResizeObserverCallback) {}
			observe(target: Element) {
				queueMicrotask(() => this.callback([{ target } as unknown as ResizeObserverEntry], this as never));
			}
			unobserve() {}
			disconnect() {}
		}
	});
	patch(globalThis, 'requestAnimationFrame', {
		writable: true,
		value: (callback: FrameRequestCallback) => { queueMicrotask(() => callback(0)); return 1; }
	});
	patch(globalThis, 'cancelAnimationFrame', { writable: true, value: () => {} });
	patch(document, 'elementFromPoint', {
		writable: true,
		value: (x: number, y: number) => {
			const win = document.querySelector<HTMLElement>('.keepwin');
			if (!win) return document.body;
			const left = parseFloat(win.style.left);
			const top = parseFloat(win.style.top);
			const inside = x >= left && x < left + KEEP.w && y >= top && y < top + KEEP.h;
			if (!inside) return document.body;
			// Keep spans x 10-82 and Close x 88-154 inside the window (border 2 + padding 8, gap 6).
			const action = x - left < 85 ? 'temple.keep' : 'temple.close';
			return win.querySelector(`[data-action="${action}"]`) ?? win;
		}
	});
});

afterEach(() => {
	if (mounted) unmount(mounted);
	mounted = null;
	document.body.innerHTML = '';
	restores.splice(0).reverse().forEach((restore) => restore());
	vi.useRealTimers();
});

async function settle() {
	for (let i = 0; i < 6; i++) {
		flushSync();
		await tick();
		await Promise.resolve();
	}
	flushSync();
}

async function render(timeoutSeconds: number, query = '') {
	window.history.replaceState(null, '', `/overlay/temple${query}`);
	seed(timeoutSeconds);
	const { default: Page } = await import('../../routes/overlay/temple/+page.svelte');
	const target = document.createElement('div');
	document.body.appendChild(target);
	mounted = mount(Page, { target });
	await settle();
	return target;
}

function offersLayer(target: HTMLElement) {
	return target.querySelector<HTMLElement>('.anchored-layer[data-widget="temple.offers"]');
}

async function clickButton(target: HTMLElement, centre: number) {
	const win = target.querySelector<HTMLElement>('.keepwin');
	expect(win).not.toBeNull();
	// The upper box was placed, so the window stands left of it.
	expect({ left: win!.style.left, top: win!.style.top }).toEqual({ left: '242px', top: '208px' });
	const x = parseFloat(win!.style.left) + centre;
	const y = parseFloat(win!.style.top) + KEEP.h - 8 - 11;
	windowHandlers['overlay-click']({ payload: { label: 'temple', x, y } });
	await settle();
}
const clickKeep = (target: HTMLElement) => clickButton(target, 10 + 36);
const clickClose = (target: HTMLElement) => clickButton(target, 88 + 33);

describe('temple keep window', () => {
	// keep:happy
	it('a forwarded click on Keep keeps the offers past their timeout', async () => {
		const target = await render(10);
		await clickKeep(target);
		vi.advanceTimersByTime(11000);
		await settle();
		const layer = offersLayer(target)!;
		expect(layer.querySelectorAll('.box')).toHaveLength(2);
		expect(layer.querySelector('.keepwin .lb')?.textContent).toBe('Timer stopped');
		expect(layer.querySelector('.keepwin')?.textContent).not.toMatch(/Esc|Key/);
		expect(layer.querySelector('[data-action="temple.keep"]')).toBeNull();
		expect(layer.querySelector('[data-action="temple.close"]')).not.toBeNull();
	});

	// keep:happy
	it('a forwarded click on Close hides the offers and the keep window, before and after Keep', async () => {
		let target = await render(10);
		await clickClose(target);
		expect(target.querySelectorAll('.box')).toHaveLength(0);
		expect(target.querySelector('.keepwin')).toBeNull();
		unmount(mounted!);
		mounted = null;
		document.body.innerHTML = '';
		target = await render(10);
		await clickKeep(target);
		await clickClose(target);
		expect(target.querySelectorAll('.box')).toHaveLength(0);
		expect(target.querySelector('.keepwin')).toBeNull();
	});

	// keep:happy
	it('the dismissal key hides a kept comparator and its keep window', async () => {
		const target = await render(10);
		await clickKeep(target);
		busHandlers['overlay-dismiss-key']({ payload: { gen: 1, shortcut: esc } });
		await settle();
		await settle();
		expect(target.querySelectorAll('.box')).toHaveLength(0);
		expect(target.querySelector('.keepwin')).toBeNull();
	});

	// keep:critical — a hot rect with no countdown behind it would swallow game clicks for nothing.
	it('no timeout configured draws no keep window', async () => {
		const target = await render(0);
		expect(offersLayer(target)?.querySelectorAll('.box')).toHaveLength(2);
		expect(target.querySelector('.keepwin')).toBeNull();
		expect(target.querySelector('[data-action="temple.keep"]')).toBeNull();
		expect(target.querySelector('[data-action="temple.close"]')).toBeNull();
	});

	// keep:happy
	it('temple probe only logs', async () => {
		const target = await render(10, '?debug');
		const widget = target.querySelector<HTMLElement>('.widget')!;
		expect(widget).not.toBeNull();
		const button = document.createElement('button');
		button.dataset.hot = '';
		button.dataset.action = 'hot-probe';
		widget.append(button);
		patch(document, 'elementFromPoint', { writable: true, value: () => button });
		calls.length = 0;
		windowHandlers['overlay-click']({ payload: { label: 'temple', x: 1, y: 1 } });
		await settle();
		expect(calls.filter(([name]) => name !== 'set_overlay_hot_rects' && name !== 'set_overlay_has_content')).toEqual([
			['app_log_from_frontend', { msg: '[temple-widgets] hot-rect probe clicked — the mouse hook forwarded a click' }]
		]);
	});

	// keep:critical — an action firing in config mode acts in the game while the player arranges widgets.
	it('temple config suppresses widget actions', async () => {
		const target = await render(10, '?debug');
		windowHandlers['widget-config']({ payload: { module: 'temple', on: true } });
		await settle();
		expect(target.querySelector('.config-bar')).not.toBeNull();
		const button = document.createElement('button');
		button.dataset.action = 'hot-probe';
		target.querySelector('.widget')!.append(button);
		patch(document, 'elementFromPoint', { writable: true, value: () => button });
		calls.length = 0;
		emissions.length = 0;
		windowHandlers['overlay-click']({ payload: { label: 'temple', x: 1, y: 1 } });
		button.click();
		await settle();
		expect(emissions).toEqual([]);
		expect(calls.filter(([name]) => ['app_log_from_frontend', 'trade_cancel', 'request_trade_refresh'].includes(name))).toEqual([]);
		target.querySelector('.config-btn.cancel')!.dispatchEvent(new PointerEvent('pointerup', { bubbles: true }));
		await settle();
		expect(target.querySelector('.config-bar')).toBeNull();
		expect(calls.some(([name, args]) => name === 'set_overlay_config_mode' && (args as { on: boolean }).on === false)).toBe(true);
	});
});
