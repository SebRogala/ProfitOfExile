/**
 * Harvest server data fixture adapter — the mock behind `LoadHarvestFamilies`.
 * Going live replaces this file with a fetch of the server's read model.
 *
 * Tiers are derived from exchange prices, so the server keys them to the
 * price hour they were computed on; the adapter stamps the exchange fixture's
 * `priceHour`. Both horizons serve the same fixture. Families without logged
 * weights stay in the list with `weights: null`.
 */
import exchange from '../__fixtures__/exchange.json';
import harvest from '../__fixtures__/harvest.json';
import type { HarvestFamily, HarvestFamilyData, LifeforceColour, LoadHarvestFamilies } from '../seam';

interface FixtureFamily {
	id: string;
	label: string;
	lifeforce?: string;
	rerollCost?: number;
	weights: HarvestFamily['weights'];
	/**
	 * `byItem` admits `undefined` because TypeScript widens the JSON families
	 * array to one union whose members carry each other's keys as optional.
	 */
	tiers?: Omit<NonNullable<HarvestFamily['tiers']>, 'priceHour' | 'byItem'> & {
		byItem: Record<string, string | undefined>;
	};
	_note?: string;
}

function toFamily(f: FixtureFamily): HarvestFamily {
	return {
		id: f.id,
		label: f.label,
		lifeforce: (f.lifeforce as LifeforceColour | undefined) ?? null,
		rerollCost: f.rerollCost ?? null,
		weights: f.weights
			? { sample: { ...f.weights.sample }, types: f.weights.types.map((t) => ({ ...t })) }
			: null,
		tiers: f.tiers
			? {
					boundariesChaos: [...f.tiers.boundariesChaos],
					names: [...f.tiers.names],
					topBoundaryChaos: f.tiers.topBoundaryChaos,
					byItem: Object.fromEntries(
						Object.entries(f.tiers.byItem).filter((e): e is [string, string] => e[1] !== undefined)
					),
					priceHour: exchange.priceHour
				}
			: null,
		note: f._note ?? null
	};
}

export const loadHarvestFamilies: LoadHarvestFamilies = async () => {
	const data: HarvestFamilyData = {
		warm: true,
		families: harvest.families.map(toFamily)
	};
	return data;
};
