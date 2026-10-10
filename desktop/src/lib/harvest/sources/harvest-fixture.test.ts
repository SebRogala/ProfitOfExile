import { describe, it, expect, vi } from 'vitest';
import { loadHarvestFamilies } from './harvest-fixture';

const NO_DATA_NOTE = 'No HarvestForge log yet.';

/**
 * Every fixture family now has a log, so the no-data mapping runs over a
 * test-local fixture: the adapter re-imported with `harvest.json` replaced.
 */
async function unloggedFamily() {
	vi.resetModules();
	vi.doMock('../__fixtures__/harvest.json', () => ({
		default: { families: [{ id: 'unlogged', label: 'Unlogged Orbs', weights: null, _note: NO_DATA_NOTE }] }
	}));
	try {
		const adapter = await import('./harvest-fixture');
		return (await adapter.loadHarvestFamilies('day')).families[0];
	} finally {
		vi.doUnmock('../__fixtures__/harvest.json');
		vi.resetModules();
	}
}

async function family(id: string) {
	const data = await loadHarvestFamilies('day');
	const found = data.families.find((f) => f.id === id);
	if (!found) throw new Error(`family ${id} missing`);
	return found;
}

describe('harvest fixture adapter', () => {
	it('returns the seven families in fixture order', async () => {
		const data = await loadHarvestFamilies('day');
		expect(data.families.map((f) => f.id)).toEqual([
			'delirium',
			'essence',
			'corrupt',
			'fossil',
			'astrolabe',
			'oil',
			'catalyst'
		]);
	});

	it('reports the Harvest data as warm', async () => {
		expect((await loadHarvestFamilies('day')).warm).toBe(true);
	});

	it('serves a family without a log without weights', async () => {
		expect((await unloggedFamily()).weights).toBeNull();
	});

	it('serves a family without a log without tiers', async () => {
		expect((await unloggedFamily()).tiers).toBeNull();
	});

	it('serves a family without a log without a lifeforce colour', async () => {
		expect((await unloggedFamily()).lifeforce).toBeNull();
	});

	it('serves a family without a log without a reroll cost', async () => {
		expect((await unloggedFamily()).rerollCost).toBeNull();
	});

	it('carries the fixture note of a family without a log', async () => {
		expect((await unloggedFamily()).note).toBe(NO_DATA_NOTE);
	});

	it.each(['astrolabe', 'oil', 'catalyst'])('serves %s without tiers', async (id) => {
		expect((await family(id)).tiers).toBeNull();
	});

	it('serves Delirium Orbs as twelve types', async () => {
		expect((await family('delirium')).weights!.types).toHaveLength(12);
	});

	it('leaves the Obscured Delirium Orb out of Delirium Orbs', async () => {
		const names = (await family('delirium')).weights!.types.map((t) => t.name);
		expect(names).not.toContain('Obscured Delirium Orb');
	});

	it('identifies Fine Delirium Orb by its itemId', async () => {
		const f = await family('delirium');
		const fine = f.weights!.types.find((t) => t.itemId === 'Metadata/Items/Currency/CurrencyAfflictionOrbCurrency');
		expect(fine?.name).toBe('Fine Delirium Orb');
		expect(fine?.loggedRolls).toBe(937);
	});

	it('marks Corrupted Essences weights as assumed uniform', async () => {
		expect((await family('corrupt')).weights!.sample.uniform).toBe(true);
	});

	it('serves Corrupted Essences with no logged sample', async () => {
		expect((await family('corrupt')).weights!.sample.rolls).toBeNull();
	});

	it('serves Corrupted Essences as the closed pool of four', async () => {
		const types = (await family('corrupt')).weights!.types.map((t) => t.shortName);
		expect(types).toEqual(['Hysteria', 'Insanity', 'Horror', 'Delirium']);
	});

	it.each(['essence', 'corrupt'])('costs %s in Primal lifeforce', async (id) => {
		expect((await family(id)).lifeforce).toBe('Primal');
	});

	it.each(['essence', 'corrupt'])('costs %s 30 lifeforce per reroll', async (id) => {
		expect((await family(id)).rerollCost).toBe(30);
	});

	it('costs Fossils in Wild lifeforce', async () => {
		expect((await family('fossil')).lifeforce).toBe('Wild');
	});

	it('stamps every served tier set with the exchange price hour', async () => {
		const data = await loadHarvestFamilies('day');
		const tiered = data.families.filter((f) => f.tiers !== null);
		expect(tiered.map((f) => f.tiers?.priceHour)).toEqual(Array(4).fill('2026-09-09T23:00:00Z'));
	});

	it('serves the fossil tier names from the fixture', async () => {
		expect((await family('fossil')).tiers!.names).toEqual(['HIGH', 'MID', 'LOW', 'FLOOR']);
	});

	it('serves the fossil top boundary from the fixture', async () => {
		expect((await family('fossil')).tiers!.topBoundaryChaos).toBe(401.63);
	});

	it.each(['delirium', 'corrupt'])('keeps the null top boundary of %s', async (id) => {
		expect((await family(id)).tiers!.topBoundaryChaos).toBeNull();
	});

	it('serves the per-item fossil tier from the fixture', async () => {
		const byItem = (await family('fossil')).tiers!.byItem;
		expect(byItem['Metadata/Items/Currency/CurrencyDelveCraftingDefences']).toBe('MID');
	});

	it('hands out a fresh family list so a caller mutating one read cannot change the next', async () => {
		const first = await loadHarvestFamilies('day');
		first.families.pop();
		const second = await loadHarvestFamilies('day');
		expect(second.families).toHaveLength(7);
	});

	it('hands out fresh weight types so a caller mutating one read cannot change the next', async () => {
		const first = await loadHarvestFamilies('day');
		first.families[0].weights!.types.pop();
		const second = await loadHarvestFamilies('day');
		expect(second.families[0].weights!.types).toHaveLength(12);
	});
});
