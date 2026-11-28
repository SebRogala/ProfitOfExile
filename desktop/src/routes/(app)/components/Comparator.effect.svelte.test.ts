// @vitest-environment happy-dom
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import Comparator from './Comparator.svelte';
import wi9ComparatorSource from './Comparator.svelte?raw';
import { applySnapshot } from '$lib/stores/ssot.svelte';
import { applyServerSlice, resetServerBaseForTests } from '$lib/stores/server-base.svelte';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

const mounted: ReturnType<typeof mount>[] = [];
beforeEach(() => {
	applyServerSlice({ url: 'https://server.test', defaultUrl: 'https://server.test' });
	applySnapshot({ league: { name: 'Probe League' }, normalVariant: '20/20' });
});
afterEach(async () => {
	for (const component of mounted.splice(0)) await unmount(component);
	document.body.replaceChildren();
	vi.unstubAllGlobals();
	resetServerBaseForTests();
});

it.each(
[
	{ tag: 'keep:critical', tier: 'UNRATED', color: 'rgb(234, 179, 8)' },
	{ tag: 'keep:happy', tier: 'MID-HIGH', color: 'rgb(192, 132, 252)' },
	{ tag: 'keep:happy', tier: 'TOP', color: 'rgb(251, 191, 36)' },
	{ tag: 'keep:happy', tier: 'FLOOR', color: 'rgb(71, 85, 105)' },
	{ tag: 'keep:happy', tier: 'CHAOTIC', color: 'rgb(248, 113, 113)' }
])('$tag WI-9 Comparator: selected $tier preserves its badge instead of a silent LOW valuation', async ({ tier, color }) => {
	const name = 'Ice Nova of Frostbolts';
	vi.stubGlobal('fetch', vi.fn(async (input: string | URL | Request) => {
		const url = new URL(String(input));
		if (url.pathname.endsWith('/gems/names')) return Response.json({ names: [name] });
		if (url.pathname.endsWith('/compare')) return Response.json({ data: [{
			name, variant: '20/20', color: 'BLUE', confidence: 'OK', roi: 31, roiPercent: 282,
			weightedRoi: 28, signal: 'STABLE', cv: 0, windowSignal: 'HOLD', advancedSignal: '',
			liquidityTier: 'MED', transListings: 9, transVelocity: 0, baseListings: 20,
			baseVelocity: 0, basePrice: 11, transPrice: 42, sparkline: [], sparklineListings: [],
			signalHistory: [], priceTier: tier, lowConfidence: false, tierAction: '', sellUrgency: '',
			sellReason: '', sellability: 48, sellabilityLabel: 'MODERATE', low7d: 38, high7d: 45, histPosition: 0,
			sellConfidence: 'FAIR', tradeConfidenceNote: '', gcpRecipeCost: 0, gcpRecipeBase: 0,
			gcpRecipeSaves: 0, baseName: 'Ice Nova', recommendation: 'OK'
		}] });
		if (url.pathname.includes('history')) return Response.json({ data: [] });
		throw new Error('Unexpected request ' + url.pathname);
	}));
	const root = document.body.appendChild(document.createElement('div'));
	flushSync(() => mounted.push(mount(Comparator, { target: root })));
	const input = root.querySelector<HTMLInputElement>('input.search-input')!;
	input.value = 'Ice Nova';
	input.dispatchEvent(new Event('input', { bubbles: true }));
	await vi.waitFor(() => { flushSync(); expect(root.querySelector('.dropdown-item')?.textContent).toContain(name); });
	root.querySelector('.dropdown-item')!.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
	await vi.waitFor(() => { flushSync(); expect(root.querySelector('.compare-card .tier-badge')?.textContent).toBe(tier); });

		const badge = root.querySelector<HTMLElement>('.compare-card .tier-badge')!;
		expect(badge?.textContent).toBe(tier);
		expect(badge?.classList.contains('tier-' + tier.toLowerCase())).toBe(true);
		expect(badge?.classList.contains('tier-low')).toBe(false);
		const style = document.createElement('style');
		style.textContent = ':root { --color-lab-yellow: #eab308; }' + (wi9ComparatorSource.match(/<style[^>]*>([\s\S]*?)<\/style>/)?.[1] ?? '');
		document.head.append(style);
		try {
			expect(getComputedStyle(badge).color).toBe(color);
			if (tier === 'UNRATED') {
				const css = getComputedStyle(badge);
				expect(css.backgroundColor).toMatch(/^(transparent|rgba\(0, 0, 0, 0\))$/);
				expect(css.borderTopWidth).toBe('1px');
				expect(css.borderTopStyle).toBe('solid');
				expect(css.borderTopColor).toBe('rgba(234, 179, 8, 0.6)');
				expect(css.padding).toBe('1px 7px');
			}
			if (tier === 'CHAOTIC') expect(getComputedStyle(badge).backgroundColor).toBe('rgba(248, 113, 113, 0.15)');
		} finally { style.remove(); }

});

