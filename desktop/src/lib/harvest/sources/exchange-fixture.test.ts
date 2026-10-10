import { describe, it, expect } from 'vitest';
import { createExchangeFixture } from './exchange-fixture';

const NOW = new Date('2026-09-10T00:00:00Z');
const load = createExchangeFixture(() => NOW);

const FINE_DELIRIUM = 'Metadata/Items/Currency/CurrencyAfflictionOrbCurrency';

describe('exchange fixture adapter', () => {
	it('dates the prices updatedMinutesAgo before the injected clock', async () => {
		const read = await load('day');
		expect(read.lastUpdated).toBe('2026-09-09T23:54:00.000Z');
	});

	it('serves the fixture divine rate', async () => {
		expect((await load('day')).divineChaosRate).toBe(360.07);
	});

	it('serves the fixture league', async () => {
		expect((await load('day')).league).toBe('Allflame');
	});

	it('serves the fixture price hour', async () => {
		expect((await load('day')).priceHour).toBe('2026-09-09T23:00:00Z');
	});

	it('reports the exchange as warm', async () => {
		expect((await load('day')).warm).toBe(true);
	});

	it('serves Wild lifeforce with its chaos price and per-divine amount', async () => {
		const read = await load('day');
		expect(read.lifeforce.Wild).toEqual({
			itemId: 'Metadata/Items/Currency/HarvestSeedRed',
			colour: 'purple',
			chaos: 0.031,
			perDivine: 9905
		});
	});

	it('serves Primal lifeforce as the blue colour', async () => {
		expect((await load('day')).lifeforce.Primal.colour).toBe('blue');
	});

	it('serves the Primal lifeforce chaos price essences pay', async () => {
		expect((await load('day')).lifeforce.Primal.chaos).toBe(0.049);
	});

	it('prices Fine Delirium Orb by its itemId', async () => {
		const read = await load('day');
		expect(Object.hasOwn(read.prices, FINE_DELIRIUM)).toBe(true);
		expect(read.prices[FINE_DELIRIUM]).toEqual({ chaos: 16.69, divine: 0.0407, lastSeenChaos: null });
	});

	it('carries no last seen price for any item', async () => {
		const read = await load('day');
		const lastSeen = Object.values(read.prices).map((p) => p.lastSeenChaos);
		expect(lastSeen).toHaveLength(88);
		expect(lastSeen.every((v) => v === null)).toBe(true);
	});

	it('answers a Recent 6h request with the requested horizon', async () => {
		expect((await load('recent')).horizon).toBe('recent');
	});

	it('answers a Recent 6h request with the same prices as Day 24h', async () => {
		const day = await load('day');
		const recent = await load('recent');
		expect(recent.prices).toEqual(day.prices);
	});

	it('hands out fresh prices so a caller mutating one read cannot change the next', async () => {
		const first = await load('day');
		first.prices[FINE_DELIRIUM].chaos = 1;
		const second = await load('day');
		expect(second.prices[FINE_DELIRIUM].chaos).toBe(16.69);
	});

	it('hands out fresh lifeforce so a caller mutating one read cannot change the next', async () => {
		const first = await load('day');
		first.lifeforce.Wild.perDivine = 1;
		const second = await load('day');
		expect(second.lifeforce.Wild.perDivine).toBe(9905);
	});
});
