import { afterEach, describe, expect, it, vi } from 'vitest';
import { mount, unmount, flushSync, tick } from 'svelte';
import { SvelteMap } from 'svelte/reactivity';
import Comparator from './Comparator.svelte';

const name = 'Kinetic Blast of Clustering';
const compareRow = {
	name, variant: '20/20', color: 'BLUE', confidence: 'OK', roi: 80, roiPercent: 400,
	weightedRoi: 70, signal: 'STABLE', cv: 0, windowSignal: 'HOLD', advancedSignal: '',
	liquidityTier: 'HIGH', transListings: 30, transVelocity: 0, baseListings: 20,
	baseVelocity: 0, basePrice: 20, transPrice: 100, sparkline: [], sparklineListings: [],
	signalHistory: [], priceTier: 'TOP', lowConfidence: false, tierAction: '', sellUrgency: '',
	sellReason: '', sellability: 80, sellabilityLabel: 'SAFE', low7d: 0, high7d: 0, histPosition: 0,
	sellConfidence: '', tradeConfidenceNote: '', gcpRecipeCost: 10, gcpRecipeBase: 5, gcpRecipeSaves: 10,
	baseName: '', recommendation: 'BEST', transfiguredName: name, gemColor: 'BLUE', transfiguredPrice: 100
};

/** A cache hit whose only listing prices the gem at 123c — the rendered marker of a written trade result. */
function tradeHit(league: string) {
	return Response.json({
		gem: name, variant: '20/20', league, total: 1, priceFloor: 123, priceCeiling: 123, priceSpread: 0,
		medianTop10: 123, medianTop3: 123, divinePrice: 200, tradeUrl: '', fetchedAt: new Date().toISOString(),
		signals: {}, listings: [{ price: 123, currency: 'chaos', chaosPrice: 123,indexedAt: new Date().toISOString(), gemLevel: 20, gemQuality: 20 }]
	});
}
const cacheMiss = () => new Response(null, { status: 204 });

/** Each cache-only trade lookup takes the next queued answer; a pending one resolves when the test says so. */
let lookups: Array<Promise<Response>> = [];
function serve() {
	vi.stubGlobal('fetch', vi.fn(async (input: string | URL | Request) => {
		const url = new URL(String(input), window.location.origin);
		switch (url.pathname) {
			case '/api/analysis/gems/names': return Response.json({ names: [name] });
			case '/api/analysis/compare': return Response.json({ data: [compareRow] });
			case '/api/trade/lookup': {
				const next = lookups.shift();
				if (!next) throw new Error('Unplanned trade lookup');
				return next;
			}
			default:
				if (url.pathname.includes('history')) return Response.json({ data: [] });
				throw new Error(`Unexpected request ${url.pathname}`);
		}
	}));
}
function pending() {
	let resolve!: (r: Response) => void;
	lookups.push(new Promise<Response>((r) => { resolve = r; }));
	return resolve;
}

const mounted: ReturnType<typeof mount>[] = [];
/** Mounts with a reactive `league` prop: the getter reads a SvelteMap, so `setLeague` re-runs the component's effects. */
function attach(league: string) {
	const props = new SvelteMap([['league', league]]);
	const target = document.createElement('div');
	document.body.append(target);
	flushSync(() => { mounted.push(mount(Comparator, { target, props: { get league() { return props.get('league')!; } } })); });
	return { root: target, setLeague: (next: string) => flushSync(() => { props.set('league', next); }) };
}
async function settle() {
	await new Promise((resolve) => setTimeout(resolve, 0));
	await tick();
	flushSync();
}
async function selectGem(root: Element) {
	const input = root.querySelector<HTMLInputElement>('input.search-input')!;
	input.value = 'Kinetic';
	input.dispatchEvent(new Event('input', { bubbles: true }));
	await new Promise((resolve) => setTimeout(resolve, 130));
	await settle();
	const suggestion = root.querySelector('.dropdown-item');
	expect(suggestion).not.toBeNull();
	suggestion!.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
	await settle();
	await vi.waitFor(() => expect(root.querySelector('.trade-section')).not.toBeNull());
}
const selectedChip = (root: Element) => root.textContent?.includes('No trade data available') || root.querySelector('.trade-data') !== null;
const tradePrice = (root: Element) => root.querySelector('.trade-data .trade-floor-price')?.textContent ?? null;

afterEach(async () => {
	for (const component of mounted.splice(0)) await unmount(component);
	document.body.replaceChildren();
	localStorage.clear();
	vi.unstubAllGlobals();
	lookups = [];
});

describe('Comparator league fence', () => {
	it('clears the selected gem when the league prop changes', async () => {
		serve();
		lookups.push(Promise.resolve(cacheMiss()));
		const { root, setLeague } = attach('A');
		await selectGem(root);
		setLeague('B');
		await settle();
		expect(selectedChip(root)).toBe(false);
	});

	it('keeps the selected gem when the league prop is set to the same league', async () => {
		serve();
		lookups.push(Promise.resolve(cacheMiss()));
		const { root, setLeague } = attach('A');
		await selectGem(root);
		setLeague('A');
		await settle();
		expect(selectedChip(root)).toBe(true);
	});

	describe('cache-only lookup', () => {
		it('renders the trade price of a current-league result', async () => {
			serve();
			lookups.push(Promise.resolve(tradeHit('A')));
			const { root } = attach('A');
			await selectGem(root);
			await vi.waitFor(() => expect(tradePrice(root)).toBe('123c'));
		});

		it('renders no trade price for a foreign-league result', async () => {
			serve();
			lookups.push(Promise.resolve(tradeHit('B')));
			const { root } = attach('A');
			await selectGem(root);
			await settle();
			expect(tradePrice(root)).toBeNull();
			expect(root.textContent).toContain('No trade data available');
		});

		it('renders no trade price on a 204 cache miss without throwing', async () => {
			serve();
			const warn = vi.spyOn(console, 'warn');
			lookups.push(Promise.resolve(cacheMiss()));
			const { root } = attach('A');
			await selectGem(root);
			await settle();
			expect(tradePrice(root)).toBeNull();
			expect(root.textContent).toContain('No trade data available');
			expect(warn).not.toHaveBeenCalled();
		});
	});

	describe('pending cache-only lookup across a league rollover', () => {
		it.each(['A', 'B'])('drops the old league-A lookup answering with a %s payload after the gem is re-selected in B', async (payloadLeague) => {
			serve();
			const resolveOld = pending();
			lookups.push(Promise.resolve(cacheMiss()));
			const { root, setLeague } = attach('A');
			await selectGem(root);
			setLeague('B');
			await settle();
			await selectGem(root);
			resolveOld(tradeHit(payloadLeague));
			await settle();
			await settle();
			expect(tradePrice(root)).toBeNull();
			expect(root.textContent).toContain('No trade data available');
		});
	});
});
