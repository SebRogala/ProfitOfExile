import { afterEach, describe, expect, it, vi } from 'vitest';
import { mount, unmount, flushSync, tick, type Component } from 'svelte';
import { fetchStatus, type GemPlay } from '$lib/api';
import BestPlays from './BestPlays.svelte';
import Comparator from './Comparator.svelte';
import FontEVCompare from './FontEVCompare.svelte';

const name = 'Kinetic Blast of Clustering';
const gem = {
	name, variant: '20/20', color: 'BLUE', confidence: 'OK', roi: 80, roiPercent: 400,
	weightedRoi: 70, signal: 'STABLE', cv: 0, windowSignal: 'HOLD', advancedSignal: '',
	liquidityTier: 'HIGH', transListings: 30, transVelocity: 0, baseListings: 20,
	baseVelocity: 0, basePrice: 20, transPrice: 100, sparkline: [], sparklineListings: [],
	signalHistory: [], priceTier: 'TOP', lowConfidence: false, tierAction: '', sellUrgency: '',
	sellReason: '', sellability: 80, sellabilityLabel: 'SAFE', low7d: 0, high7d: 0, histPosition: 0,
	sellConfidence: '', tradeConfidenceNote: '', gcpRecipeCost: 10, gcpRecipeBase: 5, gcpRecipeSaves: 10,
	baseName: '', recommendation: 'BEST'
};
const color = { color: 'BLUE', ev: 50, evRaw: 50, winners: 1, pool: 10, pWin: 30, fontsToHit: 3, avgWin: 100, jackpotGems: [{ name, chaos: 100, gcpRecipeCost: 10, gcpRecipeBase: 5, gcpRecipeSaves: 10 }] };

function serve(league: string | undefined) {
	vi.stubGlobal('fetch', vi.fn(async (input: string | URL | Request) => {
		const url = new URL(String(input), window.location.origin);
		let body: unknown;
		switch (url.pathname) {
			case '/api/analysis/status':
				return league === undefined ? new Response('', { status: 503 }) : Response.json({ league });
			case '/api/analysis/font': body = { safe: [color], premium: [color], jackpot: [color] }; break;
			case '/api/analysis/dedication': body = { variant: url.searchParams.get('variant'), skills: { safe: [color] }, transfigured: { safe: [color] } }; break;
			case '/api/analysis/gems/names': body = { names: [name] }; break;
			case '/api/analysis/compare': body = { data: [{ ...gem, transfiguredName: name, gemColor: 'BLUE', transfiguredPrice: 100 }] }; break;
			case '/api/trade/lookup': body = { gem: name, listings: [], total: 0, signals: {}, fetchedAt: new Date().toISOString() }; break;
			default:
				if (url.pathname.includes('history')) body = { data: [] };
				else throw new Error(`Unexpected request ${url.pathname}`);
		}
		return Response.json(body);
	}));
}

const mounted: ReturnType<typeof mount>[] = [];
function attach<Props extends Record<string, unknown>>(component: Component<Props>, props: Props) {
	const target = document.createElement('div');
	document.body.append(target);
	flushSync(() => { mounted.push(mount(component, { target, props })); });
	return target;
}
async function settle() {
	await new Promise(resolve => setTimeout(resolve, 0));
	await tick();
	flushSync();
}
function click(el: Element | null) {
	expect(el).not.toBeNull();
	el!.dispatchEvent(new MouseEvent('click', { bubbles: true }));
	flushSync();
}
function checkLinks(root: Element, resolved: boolean, count: number) {
	const links = [...root.querySelectorAll<HTMLAnchorElement>('a[href]')];
	expect(links).toHaveLength(resolved ? count : 0);
	for (const a of links) expect(new URL(a.href).pathname).toBe('/trade/search/New%20League');
	expect(root.innerHTML).not.toMatch(/href="(?:null|undefined|)"|Mirage/);
}

afterEach(async () => {
	for (const component of mounted.splice(0)) await unmount(component);
	document.body.replaceChildren();
	localStorage.clear();
	vi.unstubAllGlobals();
});

// Use fetchStatus's real failure result, as the lab page does, while analysis
// fixtures remain available so the link-bearing UI actually renders.
for (const league of [undefined, ' \t ', 'New League']) {
	const resolved = league === 'New League';
	describe(`trade links with status league ${JSON.stringify(league)}`, () => {
		it('renders BestPlays base and GCP labels with the resolved availability', async () => {
			serve(league);
			const status = await fetchStatus();
			const root = attach(BestPlays, { plays: [gem as GemPlay], league: status.league });
			click(root.querySelector('tbody tr'));
			await settle();
			expect(root.textContent).toContain('Buy Base');
			expect(root.textContent).toContain('Buy 20/0');
			checkLinks(root, resolved, 2);
			if (!resolved) expect(root.querySelectorAll('[aria-disabled="true"]')).toHaveLength(2);
		});
		it('renders Comparator base availability after selecting a gem', async () => {
			serve(league);
			const status = await fetchStatus();
			const root = attach(Comparator, { league: status.league });
			const input = root.querySelector<HTMLInputElement>('input.search-input')!;
			input.value = 'Kinetic';
			input.dispatchEvent(new Event('input', { bubbles: true }));
			await new Promise(resolve => setTimeout(resolve, 130));
			await settle();
			const suggestion = root.querySelector('.dropdown-item');
			expect(suggestion).not.toBeNull();
			suggestion!.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
			await settle();
			await vi.waitFor(() => expect(root.textContent).toContain('Buy Base'));
			checkLinks(root, resolved, 1);
			if (!resolved) expect(root.querySelector('[aria-disabled="true"]')?.textContent).toBe('Buy Base');
		});
		it('renders Normal and jackpot tooltip links through the production HTML path', async () => {
			serve(league);
			const status = await fetchStatus();
			const root = attach(FontEVCompare, { league: status.league });
			await settle();
			await vi.waitFor(() => expect(root.querySelector('td.buy-col')).not.toBeNull());
			checkLinks(root, resolved, 6);
			if (!resolved) expect(root.querySelectorAll('.buy-col [aria-disabled="true"]')).toHaveLength(6);
			const jackpot = root.querySelector('.t-jackpot')!.closest('.tier-row')!;
			click(jackpot.querySelector('.info-icon'));
			const panel = jackpot.querySelector('.info-panel')!;
			expect(panel.textContent).toContain(name);
			expect(panel.textContent).toContain('Buy Base');
			expect(panel.textContent).toContain('Buy 20/0');
			checkLinks(panel, resolved, 2);
			if (!resolved) expect(panel.querySelectorAll('[aria-disabled="true"]')).toHaveLength(2);
			else {
				const recipe = [...panel.querySelectorAll<HTMLAnchorElement>('a')].find(a => a.textContent === 'Buy 20/0')!;
				const q = JSON.parse(new URL(recipe.href).searchParams.get('q')!);
				expect(q.query.filters.misc_filters.filters).toEqual({ gem_level: { min: 20 }, corrupted: { option: 'false' } });
			}
		});
		it('renders Dedication color labels with the resolved availability', async () => {
			serve(league);
			const status = await fetchStatus();
			const root = attach(FontEVCompare, { league: status.league, labMode: 'dedication' });
			await settle();
			await vi.waitFor(() => expect(root.querySelector('td.buy-col')).not.toBeNull());
			checkLinks(root, resolved, 12);
			if (!resolved) expect(root.querySelectorAll('.buy-col [aria-disabled="true"]')).toHaveLength(12);
		});
	});
}
