// @vitest-environment happy-dom
/**
 * TempleOfferBoxes mounted in the client runtime under happy-dom, fed the
 * boxes `offerBoxes` builds from Rust's projected offers (POE-466). Each offer
 * comes from `fixtures/tier-offers.json`, which slice.rs keeps equal to the
 * projector's output, so the description drawn is the shipped text for the
 * tier the offer builds.
 *
 * happy-dom has no layout, so this file supplies the measurements the box
 * stack reads, as `TempleOverlayPause.effect.svelte.test.ts` does: a box size
 * through `offsetWidth` / `offsetHeight` and a `ResizeObserver` that reports
 * once on observe. It proves the rendered text of a placed box, not browser
 * CSS layout.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';
import { templeSliceDefault, type OfferView, type TempleSlice } from './slice';
import tierOffers from './fixtures/tier-offers.json';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => ({})) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

const OFFERS = tierOffers as unknown as OfferView[];
const BOX = { w: 300, h: 216 };

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
});

afterEach(() => {
	if (mounted) unmount(mounted);
	mounted = null;
	document.body.innerHTML = '';
	restores.splice(0).reverse().forEach((restore) => restore());
});

async function settle() {
	for (let i = 0; i < 6; i++) {
		flushSync();
		await tick();
		await Promise.resolve();
	}
	flushSync();
}

/** A read whose panel carries the one fixture offer printed at `target`. */
function sliceWith(target: string): TempleSlice {
	const offer = OFFERS.find((o) => o.printedTarget === target);
	if (offer === undefined) throw new Error(`tier-offers.json has no offer printed at ${target}`);
	return {
		...templeSliceDefault(),
		status: 'read',
		panel: { room: null, roomRect: null, incursionsRemaining: null, offers: [offer] },
		advice: {
			recommendations: [{
				headline: `upgrade → ${offer.displayName}`, doorsLabel: 'C1-C2', doors: ['C1-C2'],
				architectIndex: offer.index, ev: 1, risk: null, reasons: []
			}],
			gambles: [], mapAction: 'continue', warnings: [], forcedKill: false
		}
	} as TempleSlice;
}

/** The mounted box for `target`'s offer, once its measurement has placed it. */
async function placedBox(target: string): Promise<HTMLElement> {
	const { offerBoxes } = await import('./view');
	const { default: TempleOfferBoxes } = await import('./TempleOfferBoxes.svelte');
	const host = document.createElement('div');
	document.body.appendChild(host);
	mounted = mount(TempleOfferBoxes, {
		target: host,
		props: {
			boxes: offerBoxes(sliceWith(target)),
			blocks: [{ x: 1300, y: 140, w: 280, h: 43 }],
			panel: { x: 1300, y: 100, w: 300, h: 400 },
			obstacles: [{ x: 1100, y: 40, w: 500, h: 400 }],
			host: { width: 1920, height: 1080 },
			width: BOX.w
		}
	});
	await settle();
	const boxes = [...host.querySelectorAll<HTMLElement>('.box')];
	expect(boxes).toHaveLength(1);
	expect(boxes[0].style.visibility, `${target} box was never placed`).not.toBe('hidden');
	return boxes[0];
}

describe('TempleOfferBoxes tier description', () => {
	// keep:happy — POE-466 AC-2/AC-3/AC-4, rendered: each box draws the
	// description of the tier its offer builds.
	it.each([
		['Workshop', 'Extra item drops; +20% atlas drops'],
		['Engineering Department', 'Extra item drops; +40% atlas drops'],
		['Factory', 'Extra item drops; +60% atlas drops'],
		['Guardhouse', 'Rare monster packs; +10% pack size'],
		['Barracks', 'Rare monster packs; +20% pack size'],
		['Hall of War', 'Rare monster packs; +30% pack size'],
		["Surveyor's Study", 'Map drops; 1 map corrupt device uses'],
		['Office of Cartography', 'Map drops; 3 map corrupt device uses'],
		['Atlas of Worlds', 'Map drops; 5 map corrupt device uses']
	])('keep:happy — an offer built at %s draws "%s"', async (target, text) => {
		const box = await placedBox(target);
		expect(box.querySelector('.content')?.textContent?.trim()).toBe(text);
	});
});
