import { describe, it, expect } from 'vitest';
import { loadHarvestFamilies } from './harvest-fixture';

const NO_DATA_NOTE =
	"No HarvestForge log yet → the tab shows 'no data' and the 'reroll weights not logged yet' panel (reference 06). Never hide the tab.";

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

	it.each(['astrolabe', 'oil', 'catalyst'])('serves %s without weights', async (id) => {
		expect((await family(id)).weights).toBeNull();
	});

	it.each(['astrolabe', 'oil', 'catalyst'])('serves %s without tiers', async (id) => {
		expect((await family(id)).tiers).toBeNull();
	});

	it.each(['astrolabe', 'oil', 'catalyst'])('serves %s without a lifeforce colour', async (id) => {
		expect((await family(id)).lifeforce).toBeNull();
	});

	it.each(['astrolabe', 'oil', 'catalyst'])('serves %s without a reroll cost', async (id) => {
		expect((await family(id)).rerollCost).toBeNull();
	});

	it.each(['astrolabe', 'oil', 'catalyst'])('carries the fixture no-data note for %s', async (id) => {
		expect((await family(id)).note).toBe(NO_DATA_NOTE);
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

	it('stamps every logged family tier set with the exchange price hour', async () => {
		const data = await loadHarvestFamilies('day');
		const logged = data.families.filter((f) => f.weights !== null);
		expect(logged.map((f) => f.tiers?.priceHour)).toEqual(Array(4).fill('2026-09-09T23:00:00Z'));
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
