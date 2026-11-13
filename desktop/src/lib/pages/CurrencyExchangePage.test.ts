/**
 * CurrencyExchangePage server-rendered from a captured real controller, as
 * `HarvestFlippingPage.test.ts` does: the factory mock hands the page a
 * controller this test drove first. `render()` runs no `$effect`, so the page's
 * own `start()` and selection sync stay off and the test owns every request.
 */
import { beforeAll, describe, expect, it, vi } from 'vitest';
import { render } from 'svelte/server';
import type { CurrencyExchangeResponse } from '$lib/api';
import type { ExchangeQuery } from '$lib/exchange/controller.svelte';

const pageHarness = vi.hoisted(() => ({ controller: null as unknown }));

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => ({})) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

vi.mock('$lib/exchange/controller.svelte', async () => {
	const actual = await vi.importActual<typeof import('$lib/exchange/controller.svelte')>(
		'$lib/exchange/controller.svelte'
	);
	return {
		...actual,
		createExchangeController: () => {
			if (!pageHarness.controller) throw new Error('the test builds the controller before render');
			return pageHarness.controller;
		}
	};
});

const { createExchangeController } = await vi.importActual<
	typeof import('$lib/exchange/controller.svelte')
>('$lib/exchange/controller.svelte');

import CurrencyExchangePage from './CurrencyExchangePage.svelte';
import { setNumberFormat } from '$lib/stores/number-format.svelte';

beforeAll(() => setNumberFormat('comma'));

const SEARCH_INPUT = 'aria-label="Search plays by item name"';

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

function deferred<T>() {
	let resolve!: (value: T) => void;
	let reject!: (error: Error) => void;
	const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
	return { promise, resolve, reject };
}

async function settle(): Promise<void> {
	for (let i = 0; i < 5; i++) await Promise.resolve();
}

/** A page settled on the old query, then a changed query whose request is still open. */
async function changedQueryPending() {
	const calls: ReturnType<typeof deferred<CurrencyExchangeResponse>>[] = [];
	const query: ExchangeQuery = { mode: 'all', horizon: 'recent', params: { pageSize: '50' } };
	const controller = createExchangeController({
		selection: () => query,
		load: () => { const d = deferred<CurrencyExchangeResponse>(); calls.push(d); return d.promise; },
		listen: async () => () => {}
	});
	void controller.load();
	calls[0].resolve(page('OldQueryOrb'));
	await settle();
	query.params = { pageSize: '50', search: 'o' };
	controller.syncSelection();
	pageHarness.controller = controller;
	return { calls };
}

describe('CurrencyExchangePage during a changed-query fetch', () => {
	it('keeps the filter bar mounted while the new page is pending', async () => {
		await changedQueryPending();
		expect(render(CurrencyExchangePage).body).toContain(SEARCH_INPUT);
	});

	it('shows no row of the previous query while the new page is pending', async () => {
		await changedQueryPending();
		expect(render(CurrencyExchangePage).body).not.toContain('OldQueryOrb');
	});

	it('keeps the filter bar mounted after the changed-query request is rejected', async () => {
		const { calls } = await changedQueryPending();
		calls[1].reject(new Error('offline'));
		await settle();
		expect(render(CurrencyExchangePage).body).toContain(SEARCH_INPUT);
	});

	it('shows no row of the previous query after the changed-query request is rejected', async () => {
		const { calls } = await changedQueryPending();
		calls[1].reject(new Error('offline'));
		await settle();
		const body = render(CurrencyExchangePage).body;
		expect([body.includes('OldQueryOrb'), body.includes('Couldn')]).toEqual([false, true]);
	});
});
