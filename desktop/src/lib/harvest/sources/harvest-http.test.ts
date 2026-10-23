import { describe, it, expect, vi } from 'vitest';
import { createHarvestHttp } from './harvest-http';
import type { HarvestFamily, LifeforcePrice } from '../seam';
import type { HarvestResponse } from '$lib/api';
import type { CurrencyExchangeHorizon } from '$lib/exchange/view';

const ITEM = 'Metadata/Items/Currency/CurrencyUpgradeToRare';

function lifeforce(colour: string, itemId: string): LifeforcePrice {
	return { itemId, colour, chaos: 0.05, perDivine: 4000 };
}

function family(id: string): HarvestFamily {
	return { id, label: id, lifeforce: null, rerollCost: null, weights: null, tiers: null, note: null };
}

/** A snapshot whose price horizon and family id both name the horizon it answers. */
function responseFor(horizon: CurrencyExchangeHorizon): HarvestResponse {
	return {
		prices: {
			league: 'Allflame',
			horizon,
			lastUpdated: '2026-09-09T23:54:00Z',
			priceHour: '2026-09-09T23:00:00Z',
			warm: true,
			divineChaosRate: 360,
			prices: { [ITEM]: { chaos: 2.5, divine: 0.007, lastSeenChaos: null } },
			lifeforce: {
				Primal: lifeforce('blue', 'Metadata/Items/Currency/HarvestSeedBlue'),
				Vivid: lifeforce('yellow', 'Metadata/Items/Currency/HarvestSeedGreen'),
				Wild: lifeforce('purple', 'Metadata/Items/Currency/HarvestSeedRed')
			}
		},
		families: { warm: true, families: [family(`${horizon}-family`)] }
	};
}

describe('harvest HTTP adapter', () => {

	it('returns the new hour for a newer generation while an older read is pending', async () => {
		let finishOld!: (value: HarvestResponse) => void;
		const old = new Promise<HarvestResponse>((resolve) => { finishOld = resolve; });
		const fresh = responseFor('day');
		fresh.prices.priceHour = '2026-09-10T00:00:00Z';
		fresh.families.families = [family('new-hour-family')];
		const fetchOne = vi.fn().mockReturnValueOnce(old).mockResolvedValueOnce(fresh);
		const http = createHarvestHttp(fetchOne);
		const oldPrices = http.loadExchangePrices('day', 1);
		const newPrices = http.loadExchangePrices('day', 2);
		const newFamilies = http.loadHarvestFamilies('day', 2);

		// Resolve the old read too, so a regression fails on the value instead of timing out.
		finishOld(responseFor('day'));
		expect((await newPrices).priceHour).toBe('2026-09-10T00:00:00Z');
		expect((await newFamilies).families[0].id).toBe('new-hour-family');
		expect((await oldPrices).priceHour).toBe('2026-09-09T23:00:00Z');
		expect(fetchOne).toHaveBeenCalledTimes(2);
	});

	it('keeps a newer pending read shareable when an older request settles', async () => {
		let finishOld!: (value: HarvestResponse) => void;
		let finishNew!: (value: HarvestResponse) => void;
		const fetchOne = vi.fn(async () => responseFor('day'))
			.mockReturnValueOnce(new Promise<HarvestResponse>((resolve) => { finishOld = resolve; }))
			.mockReturnValueOnce(new Promise<HarvestResponse>((resolve) => { finishNew = resolve; }));
		const http = createHarvestHttp(fetchOne);
		const old = http.loadExchangePrices('day', 1);
		const fresh = http.loadExchangePrices('day', 2);
		finishOld(responseFor('day'));
		await old;
		const families = http.loadHarvestFamilies('day', 2);
		const response = responseFor('day');
		response.families.families = [family('pending-new-family')];
		finishNew(response);

		expect((await families).families[0].id).toBe('pending-new-family');
		await fresh;
		expect(fetchOne).toHaveBeenCalledTimes(2);
	});

	it('reads one snapshot for both ports when they load in the same tick', async () => {
		// Prices and tiers must come from one response, or a recompute between two
		// requests could pair a price hour with tiers from another.
		const fetchOne = vi.fn(async (h: CurrencyExchangeHorizon) => responseFor(h));
		const http = createHarvestHttp(fetchOne);

		const prices = http.loadExchangePrices('day');
		const families = http.loadHarvestFamilies('day');

		expect((await prices).prices[ITEM].chaos).toBe(2.5);
		expect((await families).families[0].id).toBe('day-family');
		expect(fetchOne).toHaveBeenCalledTimes(1);
	});

	it('fetches again for a pair that loads after the previous pair settled', async () => {
		// The shared read is only in flight; a later refetch (harvest-updated)
		// must return the second snapshot, not the one from before the update.
		const first = responseFor('day');
		const second: HarvestResponse = {
			prices: { ...responseFor('day').prices, priceHour: '2026-09-10T00:00:00Z' },
			families: { warm: true, families: [family('refreshed-family')] }
		};
		const fetchOne = vi
			.fn<(h: CurrencyExchangeHorizon) => Promise<HarvestResponse>>()
			.mockResolvedValueOnce(first)
			.mockResolvedValueOnce(second);
		const http = createHarvestHttp(fetchOne);

		const [firstPrices, firstFamilies] = await Promise.all([
			http.loadExchangePrices('day'),
			http.loadHarvestFamilies('day')
		]);
		const [secondPrices, secondFamilies] = await Promise.all([
			http.loadExchangePrices('day'),
			http.loadHarvestFamilies('day')
		]);

		expect(fetchOne).toHaveBeenCalledTimes(2);
		expect(firstPrices.priceHour).toBe('2026-09-09T23:00:00Z');
		expect(firstFamilies.families[0].id).toBe('day-family');
		expect(secondPrices.priceHour).toBe('2026-09-10T00:00:00Z');
		expect(secondFamilies.families[0].id).toBe('refreshed-family');
	});

	it('gives each port the response for its own horizon when two horizons load in one tick', async () => {
		const fetchOne = vi.fn(async (h: CurrencyExchangeHorizon) => responseFor(h));
		const http = createHarvestHttp(fetchOne);

		const prices = http.loadExchangePrices('recent');
		const families = http.loadHarvestFamilies('day');

		expect((await prices).horizon).toBe('recent');
		expect((await families).families[0].id).toBe('day-family');
		expect(fetchOne).toHaveBeenCalledTimes(2);
	});

	it('rejects both ports with the error when the request fails', async () => {
		const error = new Error('API /harvest: 500 Internal Server Error');
		const fetchOne = vi.fn(async (): Promise<HarvestResponse> => {
			throw error;
		});
		const http = createHarvestHttp(fetchOne);

		const prices = http.loadExchangePrices('day');
		const families = http.loadHarvestFamilies('day');

		await expect(prices).rejects.toBe(error);
		await expect(families).rejects.toBe(error);
		expect(fetchOne).toHaveBeenCalledTimes(1);
	});
});
