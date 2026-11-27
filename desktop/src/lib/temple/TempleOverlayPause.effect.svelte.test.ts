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
import { applySnapshot, ssot, type SsotSnapshot } from '$lib/stores/ssot.svelte';
import { templeSliceDefault, type CaptureRect, type TempleSlice } from '$lib/temple/slice';
import Page from '../../routes/overlay/temple/+page.svelte';

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

/** A read that settled a current room: `temple()` with the room's diamond
 *  (the shape only has to be drawable) and the panel's room name. */
function drawable(readAt: number): TempleSlice {
	const slice = temple(readAt);
	return {
		...slice,
		layout: {
			...slice.layout!,
			diamond: {
				corners: [[2, 0], [0, 2], [-2, 0], [0, -2]],
				seals: [
					{ neighbour: 'C2', edge: 'C1-C2', pos: [2, 0] },
					{ neighbour: 'C0', edge: 'C0-C1', pos: [-2, 0] }
				],
				topIcon: [1, -1],
				bottomIcon: [-1, 1]
			}
		},
		panel: { ...slice.panel!, room: 'Chamber of Iron' }
	} as TempleSlice;
}

function seed(timeoutSeconds: number, slice: TempleSlice = temple(Date.now())) {
	applySnapshot({
		temple: slice,
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

async function render(timeoutSeconds: number, query = '', slice?: TempleSlice) {
	window.history.replaceState(null, '', `/overlay/temple${query}`);
	seed(timeoutSeconds, slice);
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
	it('a forwarded click on Pause keeps the offers past their timeout', async () => {
		const target = await render(10);
		expect(target.querySelector('[data-action="temple.keep"]')?.textContent?.trim()).toBe('Pause');
		await clickKeep(target);
		vi.advanceTimersByTime(11000);
		await settle();
		const layer = offersLayer(target)!;
		expect(layer.querySelectorAll('.box')).toHaveLength(2);
		expect(layer.querySelector('.keepwin .lb')?.textContent).toBe('Timer stopped');
		expect(layer.querySelector('.keepwin')?.textContent).not.toMatch(/Esc|Key/);
		expect(layer.querySelector('[data-action="temple.keep"]')).toBeNull();
		expect(layer.querySelector('[data-action="temple.close"]')).not.toBeNull();
		expect(layer.querySelector('.kw-keep:not([data-action])')?.textContent?.trim()).toBe('Paused');
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

	// keep:regression
	it('clear advice then re-read counts down', async () => {
		const target = await render(10);
		await clickKeep(target);
		// An area change: `slice::clear_advice` drops the advice and leaves the status and read stamp.
		vi.advanceTimersByTime(1000);
		applySnapshot({ temple: { ...ssot.temple, advice: null } } as unknown as SsotSnapshot);
		await settle();
		vi.advanceTimersByTime(1000);
		applySnapshot({ temple: temple(Date.now()) } as unknown as SsotSnapshot);
		await settle();
		expect(offersLayer(target)?.querySelectorAll('.box')).toHaveLength(2);
		vi.advanceTimersByTime(11000);
		await settle();
		expect(target.querySelectorAll('.box')).toHaveLength(0);
		expect(target.querySelector('.keepwin')).toBeNull();
	});

	// keep:happy — POE-460 AC-1/AC-2 on the rendered route: the door diamond
	// stands for as long as Rust keeps the advice, and goes with it.
	it('the door diamond lives exactly as long as the advice', async () => {
		const readAt = Date.now();
		const target = await render(10);
		applySnapshot({ temple: drawable(readAt) } as unknown as SsotSnapshot);
		await settle();
		const door = () => target.querySelector<HTMLElement>('.widget[data-widget="temple.door"]');
		const drawn = () => ({
			room: door()?.querySelector('.room')?.textContent ?? null,
			seals: door()?.querySelectorAll('svg.shape .seal').length ?? 0
		});
		expect(drawn()).toEqual({ room: 'Chamber of Iron', seals: 2 });

		// The portal's START, then the temple's entry and Alva's banter: each
		// publish moves the cycle flag and keeps the advice and the read stamp.
		for (const waitingForPanel of [true, false, false]) {
			vi.advanceTimersByTime(1000);
			applySnapshot({ temple: { ...ssot.temple, waitingForPanel } } as unknown as SsotSnapshot);
			await settle();
			expect(drawn()).toEqual({ room: 'Chamber of Iron', seals: 2 });
		}

		// A natural clear (`slice::clear_advice`): advice and mode go, the
		// status and the read stamp stay.
		applySnapshot({ temple: { ...ssot.temple, advice: null, mode: null, waitingForPanel: false } } as unknown as SsotSnapshot);
		await settle();
		expect(door()?.querySelector('svg.shape')).toBeNull();
		expect(door()?.querySelector('.room')).toBeNull();
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

const CRADLE = 'Cradle of Creation';
const PRIORITY = { architectIndex: 0, reason: 'priority: Cradle of Creation — kill this architect so Xipocado builds it' };
/** The first block's top in `temple()`, CSS px at scale factor 1. */
const FIRST_BLOCK_TOP = 133;

function roomValue(total: number) {
	return { total, priced: 'market', guessed: false, league: 'Allflame', asOf: Date.now(), scaledFromTier3: null, drivers: [] };
}

/**
 * `temple()` with box 0 on the Queen's line and `advice.priority` naming it:
 * a Cradle of Creation tier-3 kill beside the fixture's other-line box 1.
 * `over` replaces box 0's fields (a Hall of Lords tier-2 variant) and
 * `advice` the advice's (the other box as the pick, or no pick at all).
 */
function alerted(
	readAt: number,
	over: Record<string, unknown> = {},
	advice: Partial<NonNullable<TempleSlice['advice']>> = {}
): TempleSlice {
	const slice = temple(readAt);
	const [first, second] = slice.panel!.offers;
	return {
		...slice,
		panel: {
			...slice.panel!,
			offers: [
				{ ...first, architectName: 'Xipocado', displayName: CRADLE, builtTier: 3, lineTop: CRADLE, value: roomValue(96), ...over },
				{ ...second, value: roomValue(40) }
			]
		},
		advice: { ...slice.advice!, priority: PRIORITY, ...advice }
	} as TempleSlice;
}

/** The host's height CSS px, so `free` px are left under the first block. */
function hostWithFree(free: number) {
	patch(window, 'innerHeight', { get: () => FIRST_BLOCK_TOP + free });
}

/** What one rendered box says about the alert: its classes and the banner's
 *  position and three parts, null where it draws no banner. */
function alertOf(box: Element) {
	const prio = box.querySelector(':scope > .prio');
	return {
		alert: box.classList.contains('alert'),
		pick: box.classList.contains('pick'),
		compact: box.classList.contains('compact'),
		banner: prio
			? {
					first: box.firstElementChild === prio,
					beforeTitle: prio.nextElementSibling?.classList.contains('head-title') ?? false,
					bang: prio.querySelector('.bang')?.textContent ?? null,
					call: prio.querySelector('.call')?.textContent ?? null,
					why: prio.querySelector('.why')?.textContent ?? null
				}
			: null
	};
}

function boxesOf(target: HTMLElement) {
	return [...(offersLayer(target)?.querySelectorAll('.box') ?? [])];
}

const GO_WITH_CRADLE = 'always go with Cradle';
const BANNER = (why: string) => ({ first: true, beforeTitle: true, bang: '!', call: 'JUST DO IT!', why });

describe('temple priority alert (POE-465)', () => {
	it('keep:happy draws the banner first on the alerted full box and nothing on the other', async () => {
		const target = await render(10, '', alerted(Date.now()));

		expect(boxesOf(target).map(alertOf)).toEqual([
			{ alert: true, pick: true, compact: false, banner: BANNER(GO_WITH_CRADLE) },
			{ alert: false, pick: false, compact: false, banner: null }
		]);
	});

	it('keep:happy draws both boxes compact with the banner kept at 607 px free under an alerted board', async () => {
		hostWithFree(607);
		const target = await render(10, '', alerted(Date.now()));

		expect(boxesOf(target).map(alertOf)).toEqual([
			{ alert: true, pick: true, compact: true, banner: BANNER(GO_WITH_CRADLE) },
			{ alert: false, pick: false, compact: true, banner: null }
		]);
	});

	it('keep:happy keeps an alerted board full at 608 px free', async () => {
		hostWithFree(608);
		const target = await render(10, '', alerted(Date.now()));

		expect(boxesOf(target).map(alertOf)).toEqual([
			{ alert: true, pick: true, compact: false, banner: BANNER(GO_WITH_CRADLE) },
			{ alert: false, pick: false, compact: false, banner: null }
		]);
	});

	it('keep:happy keeps an unalerted board full at 588 px free with no banner', async () => {
		hostWithFree(588);
		const target = await render(10);

		expect(boxesOf(target).map(alertOf)).toEqual([
			{ alert: false, pick: true, compact: false, banner: null },
			{ alert: false, pick: false, compact: false, banner: null }
		]);
	});

	it('keep:happy tells a forced Hall of Lords tier-2 kill to go with Cradle, beside its four-digit value', async () => {
		const target = await render(
			10,
			'',
			alerted(Date.now(), { displayName: 'Hall of Lords', builtTier: 2, value: roomValue(1240) }, { forcedKill: true })
		);
		const [upper] = boxesOf(target);

		expect(alertOf(upper).banner).toEqual(BANNER(GO_WITH_CRADLE));
		expect(upper.querySelector('.forced')?.textContent).toBe('(only architect read)');
		expect(upper.querySelector('.val .num')?.textContent).toBe('1240c');
	});

	it('keep:happy keeps the cyan pick on the other box while the alerted alternative carries the banner', async () => {
		const target = await render(
			10,
			'',
			alerted(Date.now(), {}, { recommendations: [{ ...temple(0).advice!.recommendations[0], architectIndex: 1 }] })
		);

		expect(boxesOf(target).map(alertOf)).toEqual([
			{ alert: true, pick: false, compact: false, banner: BANNER(GO_WITH_CRADLE) },
			{ alert: false, pick: true, compact: false, banner: null }
		]);
	});

	it('keep:happy alerts the higher-value Queen-line box on a board with no pick and no priority', async () => {
		const readAt = Date.now();
		const slice = alerted(readAt, { value: roomValue(64) }, { recommendations: [], priority: null });
		const [first, second] = slice.panel!.offers;
		const board = {
			...slice,
			panel: { ...slice.panel!, offers: [first, { ...second, displayName: CRADLE, builtTier: 3, lineTop: CRADLE, value: roomValue(96) }] }
		} as TempleSlice;
		const target = await render(10, '', board);

		expect(boxesOf(target).map((box) => alertOf(box).banner?.why ?? null)).toEqual([null, GO_WITH_CRADLE]);
	});

	it('keep:happy shows both boxes measured and placed after a read that gains the alert', async () => {
		const target = await render(10);
		vi.advanceTimersByTime(1000);
		applySnapshot({ temple: alerted(Date.now()) } as unknown as SsotSnapshot);
		await settle();
		const boxes = boxesOf(target) as HTMLElement[];

		expect(boxes.map((box) => box.classList.contains('alert'))).toEqual([true, false]);
		expect(boxes.map((box) => box.style.visibility)).toEqual(['', '']);
	});

	it('keep:happy hides an alerted comparator on a forwarded Close', async () => {
		const target = await render(10, '', alerted(Date.now()));
		expect(boxesOf(target).filter((box) => box.classList.contains('alert'))).toHaveLength(1);

		await clickClose(target);

		expect(target.querySelectorAll('.box')).toHaveLength(0);
		expect(target.querySelector('.keepwin')).toBeNull();
	});

	it('keep:happy hides an alerted comparator at its timeout, unpaused by the alert', async () => {
		const target = await render(10, '', alerted(Date.now()));
		expect(boxesOf(target).filter((box) => box.classList.contains('alert'))).toHaveLength(1);
		expect(target.querySelector('[data-action="temple.keep"]')?.textContent?.trim()).toBe('Pause');

		vi.advanceTimersByTime(11000);
		await settle();

		expect(target.querySelectorAll('.box')).toHaveLength(0);
		expect(target.querySelector('.keepwin')).toBeNull();
	});

	it('keep:happy keeps an alerted comparator, banner and all, past its timeout once paused', async () => {
		const target = await render(10, '', alerted(Date.now()));
		await clickKeep(target);
		vi.advanceTimersByTime(11000);
		await settle();
		const layer = offersLayer(target)!;

		expect(boxesOf(target).map((box) => alertOf(box).alert)).toEqual([true, false]);
		expect(layer.querySelector('.keepwin .lb')?.textContent).toBe('Timer stopped');
		expect(layer.querySelector('.kw-keep:not([data-action])')?.textContent?.trim()).toBe('Paused');
	});
});
