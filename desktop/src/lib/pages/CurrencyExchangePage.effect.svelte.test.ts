// @vitest-environment happy-dom
/**
 * CurrencyExchangePage mounted in the client runtime under happy-dom, so its
 * effects run and the no-offers chip is clicked for real. The transport is the
 * stubbed `fetchCurrencyExchangePlays`; every call it records is a sent request.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { CurrencyExchangeResponse, ExchangePageParams } from '$lib/api';

const transport = vi.hoisted(() => ({
	calls: [] as { mode: string; horizon: string; params: ExchangePageParams }[],
	responses: [] as unknown[]
}));

vi.mock('$lib/api', async () => {
	const actual = await vi.importActual<typeof import('$lib/api')>('$lib/api');
	return {
		...actual,
		fetchCurrencyExchangePlays: vi.fn(async (mode: string, horizon: string, params: ExchangePageParams) => {
			transport.calls.push({ mode, horizon, params });
			// [unfiltered, filtered]: mount and prefs hydration each send an unfiltered request.
			return transport.responses[params.noOffers === 'only' ? 1 : 0];
		})
	};
});
/** The prefs map the stubbed Rust `get_ui_prefs` answers with: a nondefault sort, chip off. */
const storedPrefs = vi.hoisted(() => ({
	currencyExchangeSort: 'roi',
	currencyExchangeNoOffersOnly: 'false'
}));
vi.mock('@tauri-apps/api/core', () => ({
	invoke: vi.fn(async (cmd: string) => (cmd === 'get_ui_prefs' ? { ...storedPrefs } : {}))
}));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

let CurrencyExchangePage: typeof import('./CurrencyExchangePage.svelte').default;
let { flushSync, mount, unmount } = await import('svelte');
// Transform the page graph once at collection, outside any hook's timeout;
// resetModules below only re-evaluates the cached transforms per case.
await import('./CurrencyExchangePage.svelte');
await import('$lib/stores/number-format.svelte');

// prefs.svelte.ts caches the loaded map at module level; fresh modules per case
// keep one case's chip toggle from hydrating the next case's mount.
beforeEach(async () => {
	vi.resetModules();
	({ flushSync, mount, unmount } = await import('svelte'));
	CurrencyExchangePage = (await import('./CurrencyExchangePage.svelte')).default;
	(await import('$lib/stores/number-format.svelte')).setNumberFormat('comma');
});

const leg = (item: string, action: 'buy' | 'sell') => ({
	action,
	item,
	quote: 'Metadata/Items/Currency/CurrencyRerollRare',
	price: 10,
	priceItemQty: 1,
	priceQuoteQty: 10,
	fair: 10,
	fairOk: true,
	tick: 0.01,
	volume: 100,
	stock: 10,
	suspect: false,
	itemName: item,
	itemIcon: null,
	itemCategory: 'Currency',
	quoteName: 'Chaos Orb',
	quoteIcon: null,
	quoteCategory: 'Currency'
});

/** A settled page carrying one play whose item name is `name`. */
function page(name: string): CurrencyExchangeResponse {
	return {
		league: 'Mirage',
		lastUpdated: '2026-11-13T10:00:00Z',
		from: null,
		to: null,
		hours: 6,
		warm: true,
		mode: 'all',
		horizon: 'recent',
		divineChaosRate: 200,
		count: 1,
		plays: [
			{
				key: `direct:${name}`,
				mode: 'direct',
				legs: [leg(name, 'buy'), leg(name, 'sell')],
				roiPct: 0.1,
				edge: 0.1,
				roiPctRaw: 0.12,
				roi: 1,
				investment: 10,
				expectedRoi: 0.5,
				expectedRoiPct: 0.05,
				simEntries: 20,
				lowCoverage: false,
				turnover: 1000,
				tick: 0.01,
				depth: 100,
				suspect: false,
				hoursSeen: 6,
				lastHour: '2026-11-13T10:00:00Z',
				display: {
					units: 1,
					basis: 'posting',
					money: { investment: 10, roi: 1, expectedRoi: 0.5 },
					expectedPrinted: true,
					scale: { flips: 200, gain: 100, investment: 2000, hours: 2 },
					runInvestment: 2000,
					sortKeys: { expected: 0.5, roi: 1, fastestHours: 2 }
				}
			}
		],
		categories: ['Currency'],
		total: 1,
		afterRules: 1,
		afterGates: 1,
		matched: 1,
		showConvert: false,
		itemUniverse: []
	};
}

/** A page answering with one direct play per name, in the order given. */
function answer(names: string[]): CurrencyExchangeResponse {
	const base = page(names[0]);
	const plays = names.map((name) => ({ ...page(name).plays[0], noOffers: true }));
	return { ...base, plays, count: plays.length, total: plays.length, matched: plays.length };
}

async function settle(): Promise<void> {
	for (let i = 0; i < 10; i++) await Promise.resolve();
	flushSync();
}

let app: ReturnType<typeof mount> | null = null;

afterEach(() => {
	if (app) unmount(app);
	app = null;
	document.body.innerHTML = '';
	transport.calls.length = 0;
	transport.responses.length = 0;
});

/**
 * The page mounted and settled on `first`, then the chip clicked with `second`
 * as the next answer. Returns how many requests were sent before the click.
 */
async function clickChip(first: string[], second: string[]): Promise<number> {
	transport.responses.push(answer(first), answer(second));
	app = mount(CurrencyExchangePage, { target: document.body });
	await settle();
	const chip = document.querySelector<HTMLButtonElement>('button.no-offers-chip');
	if (!chip) throw new Error('the no-offers chip did not render');
	const before = transport.calls.length;
	chip.click();
	flushSync();
	await settle();
	return before;
}

/** Item names in the rendered table body, one per row, in row order. */
function rowNames(names: string[]): string[] {
	return [...document.querySelectorAll('tbody tr')]
		.map((row) => names.find((name) => row.textContent?.includes(name)))
		.filter((name): name is string => name !== undefined);
}

describe('CurrencyExchangePage no offers chip click', () => {
	it('no offers chip click sends a request with noOffers=only', async () => {
		const before = await clickChip(['TwoSidedOrb'], ['SellBlockedOrb']);

		expect(transport.calls[before]?.params.noOffers).toBe('only');
	});

	it('no offers chip click sends the same sort as the request before it', async () => {
		const before = await clickChip(['TwoSidedOrb'], ['SellBlockedOrb']);

		expect(transport.calls[before - 1].params.sort).toBe('roi');
		expect(transport.calls[before]?.params.sort).toBe('roi');
	});

	it('no offers rows keep the order the filtered response answered with', async () => {
		await clickChip(['TwoSidedOrb'], ['SellBlockedOrb', 'BuyBlockedOrb']);

		expect(rowNames(['SellBlockedOrb', 'BuyBlockedOrb', 'TwoSidedOrb'])).toEqual([
			'SellBlockedOrb',
			'BuyBlockedOrb'
		]);
	});
});

describe('CurrencyExchangePage column tooltips', () => {
	// keep:happy
	it('hovering the Investment header shows the exchange Investment tooltip', async () => {
		const { EXCHANGE_TOOLTIPS } = await import('$lib/exchange/tooltips');
		transport.responses.push(answer(['TwoSidedOrb']), answer(['TwoSidedOrb']));
		app = mount(CurrencyExchangePage, { target: document.body });
		await settle();
		const trigger = [...document.querySelectorAll<HTMLElement>('thead .tooltip-wrap')].find(
			(wrap) => wrap.textContent?.trim() === 'Investment'
		);
		if (!trigger) throw new Error('the Investment header tooltip trigger did not render');

		trigger.dispatchEvent(new MouseEvent('mouseenter'));
		flushSync();

		const shown = [...document.querySelectorAll<HTMLElement>('[role="tooltip"]')].find(
			(tip) => tip.style.visibility === 'visible'
		);
		expect(shown?.textContent).toBe(EXCHANGE_TOOLTIPS.Investment);
	});
});
