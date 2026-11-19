// @vitest-environment happy-dom
/**
 * CurrencyExchangePage mounted in the client runtime under happy-dom, so its
 * effects run and the no-offers chip is clicked for real. The transport is the
 * stubbed `fetchCurrencyExchangePlays`; every call it records is a sent request.
 */
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
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
			return transport.responses.shift() ?? transport.responses.at(-1);
		})
	};
});
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => ({})) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

import CurrencyExchangePage from './CurrencyExchangePage.svelte';
import { setNumberFormat } from '$lib/stores/number-format.svelte';

beforeAll(() => setNumberFormat('comma'));

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
	localStorage.clear();
	transport.calls.length = 0;
	transport.responses.length = 0;
});

/** The page mounted and settled on `first`, then the chip clicked with `second` as the next answer. */
async function clickChip(first: string[], second: string[]) {
	transport.responses.push(answer(first), answer(second));
	app = mount(CurrencyExchangePage, { target: document.body });
	await settle();
	const chip = document.querySelector<HTMLButtonElement>('button.no-offers-chip');
	if (!chip) throw new Error('the no-offers chip did not render');
	chip.click();
	flushSync();
	await settle();
}

/** Item names in the rendered table body, one per row, in row order. */
function rowNames(names: string[]): string[] {
	return [...document.querySelectorAll('tbody tr')]
		.map((row) => names.find((name) => row.textContent?.includes(name)))
		.filter((name): name is string => name !== undefined);
}

describe('CurrencyExchangePage no offers chip click', () => {
	it('no offers chip click sends a request with noOffers=only', async () => {
		await clickChip(['TwoSidedOrb'], ['SellBlockedOrb']);

		expect(transport.calls.at(-1)?.params.noOffers).toBe('only');
	});

	it('no offers chip click sends the same sort as the request before it', async () => {
		await clickChip(['TwoSidedOrb'], ['SellBlockedOrb']);

		const filtered = transport.calls.find((call) => call.params.noOffers === 'only');

		expect(filtered?.params.sort).toBe(transport.calls[0].params.sort);
	});

	it('no offers rows keep the order the filtered response answered with', async () => {
		await clickChip(['TwoSidedOrb'], ['SellBlockedOrb', 'BuyBlockedOrb']);

		expect(rowNames(['SellBlockedOrb', 'BuyBlockedOrb', 'TwoSidedOrb'])).toEqual([
			'SellBlockedOrb',
			'BuyBlockedOrb'
		]);
	});
});
