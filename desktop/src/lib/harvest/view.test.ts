import { describe, it, expect } from 'vitest';
import {
	applyPick,
	combineOwnerState,
	formatHarvestChaos,
	formatHarvestCount,
	formatHarvestGain,
	formatHarvestPercent,
	formatHarvestRolls,
	moveTier,
	pageView,
	parseHarvestHorizon,
	parsePicks,
	resetFamily,
	serializePicks,
	type FamilyBody,
	type HarvestPicks,
	type PageInput,
	type PageView
} from './view';
import { createExchangeFixture } from './sources/exchange-fixture';
import { loadHarvestFamilies } from './sources/harvest-fixture';
import type { ExchangePriceRead, HarvestFamilyData } from './seam';
import type { CurrencyExchangeHorizon } from '$lib/exchange/view';

/**
 * Read models come from the WI-5 fixture adapters; every expected string is a
 * literal from the reference screen named on the case (01–08), never a re-call
 * of a helper the projection itself uses.
 */

const NOW = new Date('2026-09-10T00:00:00Z');

async function readModels(horizon: CurrencyExchangeHorizon = 'day') {
	const exchange = await createExchangeFixture(() => NOW)(horizon);
	const harvest = await loadHarvestFamilies(horizon);
	return { exchange, harvest };
}

function itemId(harvest: HarvestFamilyData, familyId: string, shortName: string): string {
	const t = harvest.families.find((f) => f.id === familyId)?.weights?.types.find((x) => x.shortName === shortName);
	if (!t) throw new Error(`no ${shortName} in ${familyId}`);
	return t.itemId;
}

async function input(overrides: Partial<PageInput> = {}): Promise<PageInput> {
	const { exchange, harvest } = await readModels(overrides.horizon ?? 'day');
	return {
		exchange,
		harvest,
		lastFetchedAt: NOW,
		lastError: null,
		now: NOW,
		familyId: 'fossil',
		picks: {},
		horizon: 'day',
		divineChaosRate: exchange.divineChaosRate,
		...overrides
	};
}

function family(view: PageView): FamilyBody {
	if (view.body?.kind !== 'family') throw new Error(`expected a family body, got ${view.body?.kind}`);
	return view.body;
}

function card(body: FamilyBody, side: 'feeders' | 'keepers', tier: string) {
	const found = body[side].cards.find((c) => c.tier === tier);
	if (!found) throw new Error(`no ${tier} card among ${side}`);
	return found;
}

/** Test-local exchange data: Primal at 8/30c makes a 30-Primal reroll cost 8c (the WI-1 not-worth-it input). */
function atEightChaosPerReroll(exchange: ExchangePriceRead): ExchangePriceRead {
	return {
		...exchange,
		lifeforce: { ...exchange.lifeforce, Primal: { ...exchange.lifeforce.Primal, chaos: 8 / 30 } }
	};
}

function withPrice(
	exchange: ExchangePriceRead,
	id: string,
	chaos: number | null,
	lastSeenChaos: number | null
): ExchangePriceRead {
	return { ...exchange, prices: { ...exchange.prices, [id]: { chaos, divine: null, lastSeenChaos } } };
}

// ---------------------------------------------------------------- formats --

describe('formatHarvestChaos', () => {
	it('prints one decimal below 100: 9.1 → 9.1c', () => {
		expect(formatHarvestChaos(9.1)).toBe('9.1c');
	});

	it('prints whole, comma-grouped chaos from 100: 1344 → 1,344c', () => {
		expect(formatHarvestChaos(1344)).toBe('1,344c');
	});

	it('rounds the 19.35 half-tie up: 19.4c', () => {
		expect(formatHarvestChaos(19.35)).toBe('19.4c');
	});

	it('rounds a value just below the tie down: 19.3499 → 19.3c', () => {
		expect(formatHarvestChaos(19.3499)).toBe('19.3c');
	});

	it('rounds a value just above the tie up: 19.3501 → 19.4c', () => {
		expect(formatHarvestChaos(19.3501)).toBe('19.4c');
	});

	it('rounds the float form of the tie up: 65.67 − 3 × 1.47 − 41.91 → 19.4c', () => {
		expect(formatHarvestChaos(65.67 - 3 * 1.47 - 41.91)).toBe('19.4c');
	});
});

describe('formatHarvestGain', () => {
	it('signs a gain: 24.3 → +24.3c', () => {
		expect(formatHarvestGain(24.3)).toBe('+24.3c');
	});

	it('signs a loss with U+2212: −0.3 → −0.3c', () => {
		expect(formatHarvestGain(-0.3)).toBe('−0.3c');
	});

	it('prints |x| ≤ 0.05 unsigned: 0.04 → 0.0c', () => {
		expect(formatHarvestGain(0.04)).toBe('0.0c');
	});

	it('prints Hysteria’s exact 19.35 loop EV as +19.4c', () => {
		expect(formatHarvestGain(19.35)).toBe('+19.4c');
	});

	it('rounds a negative half-tie away from zero like its positive twin: −19.35 → −19.4c', () => {
		expect(formatHarvestGain(-19.35)).toBe('−19.4c');
	});
});

describe('formatHarvestPercent / Rolls / Count', () => {
	it('prints one decimal from 1%: 15.7 → 15.7%', () => {
		expect(formatHarvestPercent(15.7)).toBe('15.7%');
	});

	it('prints two decimals below 1%: 0.11 → 0.11%', () => {
		expect(formatHarvestPercent(0.11)).toBe('0.11%');
	});

	it('rounds a tie that binary stores low half-up: 0.145 → 0.15% (0.145 × 100 = 14.4999…)', () => {
		expect(formatHarvestPercent(0.145)).toBe('0.15%');
	});

	it('prints rolls with a tilde and one decimal: 6 → ~6.0', () => {
		expect(formatHarvestRolls(6)).toBe('~6.0');
	});

	it('groups counts: 9905 → 9,905', () => {
		expect(formatHarvestCount(9905)).toBe('9,905');
	});
});

// ----------------------------------------------------------- status line --

describe('status line', () => {
	it('Fossils ready reads updated · prices · weights (reference 01)', async () => {
		const view = pageView(await input());
		expect(view.status?.text).toBe(
			"updated 6 min ago · prices: Currency Exchange, Day 24h · 1 div = 360c · weights: HarvestForge log · 5,271 rolls · 2026-10-09 · one player's sample"
		);
	});

	it('Corrupted Essences say there is no logged sample (reference 04)', async () => {
		const view = pageView(await input({ familyId: 'corrupt' }));
		expect(view.status?.segments.at(-1)).toBe(
			'weights: no logged sample · assumed uniform, closed pool of four'
		);
	});

	it('names the selected horizon: Recent 6h', async () => {
		const view = pageView(await input({ horizon: 'recent' }));
		expect(view.status?.segments[1]).toBe('prices: Currency Exchange, Recent 6h · 1 div = 360c');
	});

	it('reads the rate it is given, not the exchange read model’s', async () => {
		const view = pageView(await input({ divineChaosRate: 400 }));
		expect(view.status?.segments[1]).toBe('prices: Currency Exchange, Day 24h · 1 div = 400c');
	});

	it('Delirium Orbs name their logged sample (reference 02)', async () => {
		const view = pageView(await input({ familyId: 'delirium' }));
		expect(view.status?.segments.at(-1)).toBe("weights: HarvestForge log · 5,122 rolls · 2026-10-09 · one player's sample");
	});

	it('Deafening Essences read uniform ~5% (reference 03)', async () => {
		const view = pageView(await input({ familyId: 'essence' }));
		expect(view.status?.segments.at(-1)).toBe('weights: HarvestForge log · 5,012 rolls · 2026-10-09 · uniform ~5%');
	});
});

// ----------------------------------------------------- other families --

describe('Delirium Orbs (reference 02)', () => {
	const body = async () => family(pageView(await input({ familyId: 'delirium' })));

	it('names Jeweller’s Delirium Orb at 10.3c as the cheapest feeder', async () => {
		expect((await body()).verdict.reason).toBe(
			"Buy Jeweller's Delirium Orb (10.3c, cheapest feeder) and reroll until a keeper."
		);
	});

	it('prints the divine line in Primal', async () => {
		expect((await body()).verdict.divineLine).toBe('1 div profit ≈ 17 feeders · ~229 rerolls · ~6,872 Primal');
	});

	it('feeds one LOW card', async () => {
		expect((await body()).feeders.cards.map((c) => c.tier)).toEqual(['LOW']);
	});

	it('prints LOW as 10 types · 10.3c–16.7c', async () => {
		expect(card(await body(), 'feeders', 'LOW').meta).toBe('10 types · 10.3c–16.7c');
	});

	it('keeps one HIGH card', async () => {
		expect((await body()).keepers.cards.map((c) => c.tier)).toEqual(['HIGH']);
	});

	it('prints HIGH as 2 types · 41.5c–54.5c', async () => {
		expect(card(await body(), 'keepers', 'HIGH').meta).toBe('2 types · 41.5c–54.5c');
	});

	it('keeps Diviner’s and Skittering in HIGH', async () => {
		expect(card(await body(), 'keepers', 'HIGH').chips.map((c) => c.shortName)).toEqual(["Diviner's", 'Skittering']);
	});
});

describe('Deafening Essences (reference 03)', () => {
	const body = async () => family(pageView(await input({ familyId: 'essence' })));

	it('names Torment at 5.0c as the cheapest feeder', async () => {
		expect((await body()).verdict.reason).toBe(
			'Buy Deafening Essence of Torment (5.0c, cheapest feeder) and reroll until a keeper.'
		);
	});

	it('prints the loop EV +6.3c', async () => {
		expect((await body()).verdict.evText).toBe('+6.3c');
	});

	it('prints the divine line', async () => {
		expect((await body()).verdict.divineLine).toBe('1 div profit ≈ 58 feeders · ~184 rerolls · ~5,510 Primal');
	});

	it('feeds MID-HIGH then LOW', async () => {
		expect((await body()).feeders.cards.map((c) => c.tier)).toEqual(['MID-HIGH', 'LOW']);
	});

	it('prints MID-HIGH as 4 types · 9.6c–10.1c', async () => {
		expect(card(await body(), 'feeders', 'MID-HIGH').meta).toBe('4 types · 9.6c–10.1c');
	});

	it('prints LOW as 10 types · 5.0c–6.2c', async () => {
		expect(card(await body(), 'feeders', 'LOW').meta).toBe('10 types · 5.0c–6.2c');
	});

	it('keeps TOP then HIGH', async () => {
		expect((await body()).keepers.cards.map((c) => c.tier)).toEqual(['TOP', 'HIGH']);
	});

	it('prints TOP as 1 type · 26.9c', async () => {
		expect(card(await body(), 'keepers', 'TOP').meta).toBe('1 type · 26.9c');
	});

	it('prints HIGH as 5 types · 12.1c–15.9c', async () => {
		expect(card(await body(), 'keepers', 'HIGH').meta).toBe('5 types · 12.1c–15.9c');
	});
});

describe('Corrupted Essences (reference 04)', () => {
	const body = async () => family(pageView(await input({ familyId: 'corrupt' })));

	it('prints the divine line in Primal', async () => {
		expect((await body()).verdict.divineLine).toBe('1 div profit ≈ 19 feeders · ~57 rerolls · ~1,710 Primal');
	});

	it('feeds MID-HIGH then LOW', async () => {
		expect((await body()).feeders.cards.map((c) => c.tier)).toEqual(['MID-HIGH', 'LOW']);
	});

	it('prints MID-HIGH as 1 type · 52.5c', async () => {
		expect(card(await body(), 'feeders', 'MID-HIGH').meta).toBe('1 type · 52.5c');
	});

	it('prints LOW as 2 types · 41.9c–42.6c', async () => {
		expect(card(await body(), 'feeders', 'LOW').meta).toBe('2 types · 41.9c–42.6c');
	});

	it('prints HIGH as 1 type · 65.7c', async () => {
		expect(card(await body(), 'keepers', 'HIGH').meta).toBe('1 type · 65.7c');
	});

	it('keeps Horror alone in HIGH', async () => {
		expect(card(await body(), 'keepers', 'HIGH').chips.map((c) => c.shortName)).toEqual(['Horror']);
	});

	it('gives Horror the weight share 25.0%', async () => {
		expect(card(await body(), 'keepers', 'HIGH').chips[0].shareText).toBe('25.0%');
	});

	it('cheapest feeder row reads Essence of Hysteria · 41.9c', async () => {
		expect((await body()).ev!.rows[0].value).toBe('Essence of Hysteria · 41.9c');
	});
});

// ------------------------------------------------------------------ cards --

describe('Fossil tier cards (reference 01)', () => {
	it('feeds MID then LOW', async () => {
		const body = family(pageView(await input()));
		expect(body.feeders.cards.map((c) => c.tier)).toEqual(['MID', 'LOW']);
	});

	it('keeps TOP, HIGH then MID', async () => {
		const body = family(pageView(await input()));
		expect(body.keepers.cards.map((c) => c.tier)).toEqual(['TOP', 'HIGH', 'MID']);
	});

	it('marks the feeder MID card split', async () => {
		expect(card(family(pageView(await input())), 'feeders', 'MID').split).toBe(true);
	});

	it('prints the feeder MID card with its whole-tier count and range: 7 types · 30.0c–109c', async () => {
		expect(card(family(pageView(await input())), 'feeders', 'MID').meta).toBe('7 types · 30.0c–109c');
	});

	it('gives a feeder card ← Keep tier', async () => {
		expect(card(family(pageView(await input())), 'feeders', 'MID').moveLabel).toBe('← Keep tier');
	});

	it('does not mark LOW split', async () => {
		expect(card(family(pageView(await input())), 'feeders', 'LOW').split).toBe(false);
	});

	it('prints LOW as 11 types · 9.1c–17.4c', async () => {
		expect(card(family(pageView(await input())), 'feeders', 'LOW').meta).toBe('11 types · 9.1c–17.4c');
	});

	it('prints a one-type tier with one price: TOP 1 type · 402c', async () => {
		expect(card(family(pageView(await input())), 'keepers', 'TOP').meta).toBe('1 type · 402c');
	});

	it('prints HIGH as 1 type · 192c', async () => {
		expect(card(family(pageView(await input())), 'keepers', 'HIGH').meta).toBe('1 type · 192c');
	});

	it('marks the keeper MID card split', async () => {
		expect(card(family(pageView(await input())), 'keepers', 'MID').split).toBe(true);
	});

	it('prints the keeper MID card with the same whole-tier range', async () => {
		expect(card(family(pageView(await input())), 'keepers', 'MID').meta).toBe('7 types · 30.0c–109c');
	});

	it('gives a keeper card Feed tier →', async () => {
		expect(card(family(pageView(await input())), 'keepers', 'MID').moveLabel).toBe('Feed tier →');
	});

	it('orders chips by price, high to low', async () => {
		const low = card(family(pageView(await input())), 'feeders', 'LOW');
		expect(low.chips.map((c) => c.shortName)).toEqual([
			'Fundamental', 'Jagged', 'Pristine', 'Deft', 'Bloodstained', 'Aberrant',
			'Scorched', 'Bound', 'Metallic', 'Frigid', 'Lucent'
		]);
	});

	const lucent = async () => card(family(pageView(await input())), 'feeders', 'LOW').chips.at(-1)!;
	const hollow = async () => card(family(pageView(await input())), 'keepers', 'TOP').chips[0];

	it('prices a feeder chip: Lucent 9.1c', async () => {
		expect((await lucent()).priceText).toBe('9.1c');
	});

	it('gives a feeder chip its signed loop EV: +25.7c', async () => {
		expect((await lucent()).evText).toBe('+25.7c');
	});

	it('gives a feeder chip no weight share', async () => {
		expect((await lucent()).shareText).toBeNull();
	});

	it('prices a keeper chip: Hollow 402c', async () => {
		expect([(await hollow()).shortName, (await hollow()).priceText]).toEqual(['Hollow', '402c']);
	});

	it('gives a keeper chip its weight share: 0.02%', async () => {
		expect((await hollow()).shareText).toBe('0.02%');
	});

	it('gives a keeper chip no loop EV', async () => {
		expect((await hollow()).evText).toBeNull();
	});
});

// ---------------------------------------------------------------- verdict --

describe('verdict', () => {
	describe('Fossils (reference 01)', () => {
		const v = async () => family(pageView(await input())).verdict;

		it('reads Yes — feed the cheap tiers', async () => {
			expect((await v()).headline).toBe('Yes — feed the cheap tiers');
		});

		it('names Lucent Fossil at 9.1c as the cheapest feeder', async () => {
			expect((await v()).reason).toBe('Buy Lucent Fossil (9.1c, cheapest feeder) and reroll until a keeper.');
		});

		it('prints the loop EV +25.7c', async () => {
			expect((await v()).evText).toBe('+25.7c');
		});

		it('prints per Lucent Fossil', async () => {
			expect((await v()).per).toBe('per Lucent Fossil');
		});

		it('prints the divine line', async () => {
			expect((await v()).divineLine).toBe('1 div profit ≈ 15 feeders · ~378 rerolls · ~11,349 Wild');
		});
	});

	it('Corrupted Essences print Hysteria’s exact 19.35 as +19.4c (Supervisor ruling)', async () => {
		const v = family(pageView(await input({ familyId: 'corrupt' }))).verdict;
		expect(v.evText).toBe('+19.4c');
	});

	describe('not worth it: Torment picked reroll at 8c a reroll (reference 08 §6)', () => {
		const notWorth = async () => {
			const base = await input({ familyId: 'essence' });
			const torment = itemId(base.harvest!, 'essence', 'Torment');
			return pageView({
				...base,
				exchange: atEightChaosPerReroll(base.exchange!),
				picks: { essence: { [torment]: 'reroll' } }
			});
		};

		it('reads No — not at these prices', async () => {
			expect(family(await notWorth()).verdict.headline).toBe('No — not at these prices');
		});

		it('gives the not-worth-it reason', async () => {
			expect(family(await notWorth()).verdict.reason).toBe(
				'Even the cheapest feeder sells for more than the loop returns.'
			);
		});

		it('prints the EV with U+2212', async () => {
			expect(family(await notWorth()).verdict.evText?.startsWith('−')).toBe(true);
		});

		it('shows no divine line', async () => {
			expect(family(await notWorth()).verdict.divineLine).toBeNull();
		});

		it('keeps both columns listed', async () => {
			const body = family(await notWorth());
			expect([body.feeders.cards.length > 0, body.keepers.cards.length > 0]).toEqual([true, true]);
		});
	});

	describe('the same 8c without picks: every tier kept', () => {
		const allKept = async () => {
			const base = await input({ familyId: 'essence' });
			return family(pageView({ ...base, exchange: atEightChaosPerReroll(base.exchange!) }));
		};

		it('reads No — nothing to flip', async () => {
			expect((await allKept()).verdict.headline).toBe('No — nothing to flip');
		});

		it('says No feeders: every tier is kept. in place of the feeder column', async () => {
			expect((await allKept()).feeders.empty).toBe('No feeders: every tier is kept.');
		});

		it('shows no divine line', async () => {
			expect((await allKept()).verdict.divineLine).toBeNull();
		});
	});

	describe('every type picked reroll: nothing to flip (reference 08 §7)', () => {
		const allFed = async () => {
			const base = await input();
			const fossil = base.harvest!.families.find((f) => f.id === 'fossil')!;
			const picks: HarvestPicks = {
				fossil: Object.fromEntries(fossil.weights!.types.map((t) => [t.itemId, 'reroll' as const]))
			};
			return family(pageView({ ...base, picks }));
		};

		it('reads No — nothing to flip', async () => {
			expect((await allFed()).verdict.headline).toBe('No — nothing to flip');
		});

		it('gives the empty-keep-set reason', async () => {
			expect((await allFed()).verdict.reason).toBe('The keep set is empty: pick a tier to keep.');
		});

		it('prints no EV', async () => {
			expect((await allFed()).verdict.evText).toBeNull();
		});

		it('shows no divine line', async () => {
			expect((await allFed()).verdict.divineLine).toBeNull();
		});

		it('says No keepers: nothing to stop on. in place of the keeper column', async () => {
			const body = await allFed();
			expect([body.keepers.empty, body.keepers.cards]).toEqual(['No keepers: nothing to stop on.', []]);
		});

		it('still lists every type as a feeder', async () => {
			const body = await allFed();
			expect(body.feeders.cards.reduce((n, c) => n + c.chips.length, 0)).toBe(20);
		});
	});
});

// ------------------------------------------------------- split row, picks --

describe('split row', () => {
	const unmoved = async () => family(pageView(await input())).split;
	const densePicked = async () => {
		const base = await input();
		const dense = itemId(base.harvest!, 'fossil', 'Dense');
		return { dense, body: family(pageView({ ...base, picks: { fossil: { [dense]: 'keep' } } })) };
	};
	const denseEvRow = async (label: string) => {
		const ev = (await densePicked()).body.ev!;
		return [...ev.rows, ...ev.yield].find((r) => r.label === label)?.value;
	};
	const denseChip = async () => {
		const { dense, body } = await densePicked();
		return card(body, 'keepers', 'MID').chips.find((c) => c.itemId === dense)!;
	};

	it('reads computed from prices when nothing was moved', async () => {
		expect((await unmoved()).label).toBe('computed from prices');
	});

	it('is not marked moved when nothing was moved', async () => {
		expect((await unmoved()).moved).toBe(false);
	});

	it('keeps Reset inert when nothing was moved', async () => {
		expect((await unmoved()).resetEnabled).toBe(false);
	});

	it('reads 1 type moved by you after the Dense pick (reference 05)', async () => {
		expect((await densePicked()).body.split.label).toBe('1 type moved by you');
	});

	it('is marked moved after the Dense pick', async () => {
		expect((await densePicked()).body.split.moved).toBe(true);
	});

	it('enables Reset after the Dense pick', async () => {
		expect((await densePicked()).body.split.resetEnabled).toBe(true);
	});

	it('marks the picked chip as picked', async () => {
		expect((await denseChip()).picked).toBe(true);
	});

	it('titles a kept pick as your pick', async () => {
		expect((await denseChip()).pickTitle).toBe('Your pick: kept. Click to return it to the computed side.');
	});

	it('moves the headline to Lucent Fossil +24.3c after the Dense pick (reference 05)', async () => {
		expect((await densePicked()).body.verdict.evText).toBe('+24.3c');
	});

	it('prints the divine line after the Dense pick (reference 05)', async () => {
		expect((await densePicked()).body.verdict.divineLine).toBe(
			'1 div profit ≈ 15 feeders · ~90 rerolls · ~2,686 Wild'
		);
	});

	it('EV panel after the Dense pick — Cheapest feeder: Lucent Fossil · 9.1c (reference 05)', async () => {
		expect(await denseEvRow('Cheapest feeder')).toBe('Lucent Fossil · 9.1c');
	});

	it('EV panel after the Dense pick — Loop EV per feeder: +24.3c (reference 05)', async () => {
		expect(await denseEvRow('Loop EV per feeder')).toBe('+24.3c');
	});

	it('EV panel after the Dense pick — Keeper hit per roll: 15.7% (reference 05)', async () => {
		expect(await denseEvRow('Keeper hit per roll')).toBe('15.7%');
	});

	it('EV panel after the Dense pick — Rerolls per keeper: ~6.0 (reference 05)', async () => {
		expect(await denseEvRow('Rerolls per keeper')).toBe('~6.0');
	});

	it('EV panel after the Dense pick — Lifeforce per keeper: ~179 Wild · 5.6c (reference 05)', async () => {
		expect(await denseEvRow('Lifeforce per keeper')).toBe('~179 Wild · 5.6c');
	});

	it('EV panel after the Dense pick — 1 div of lifeforce yields: ~55.3 keepers (reference 05)', async () => {
		expect(await denseEvRow('1 div of lifeforce yields')).toBe('~55.3 keepers');
	});

	it('EV panel after the Dense pick — …worth after inputs: +1,344c (reference 05)', async () => {
		expect(await denseEvRow('…worth after inputs')).toBe('+1,344c');
	});

	it('keeps the feeder MID card split after the Dense pick (reference 05)', async () => {
		expect(card((await densePicked()).body, 'feeders', 'MID').split).toBe(true);
	});

	it('leaves Corroded alone in the feeder MID card after the Dense pick (reference 05)', async () => {
		expect(card((await densePicked()).body, 'feeders', 'MID').chips.map((c) => c.shortName)).toEqual(['Corroded']);
	});

	it('gives Corroded the loop EV +3.5c after the Dense pick (reference 05)', async () => {
		expect(card((await densePicked()).body, 'feeders', 'MID').chips[0].evText).toBe('+3.5c');
	});

	it('gives the picked Dense chip its keeper share 11.7% (reference 05)', async () => {
		expect((await denseChip()).shareText).toBe('11.7%');
	});

	it('counts the feeders regex after the Dense pick: 49 / 250 characters (reference 05)', async () => {
		expect((await densePicked()).body.regex.feeders.count).toBe('49 / 250 characters');
	});

	it('counts picks of the active family only', async () => {
		const base = await input();
		const blacksmith = itemId(base.harvest!, 'delirium', "Blacksmith's");
		const split = family(pageView({ ...base, picks: { delirium: { [blacksmith]: 'keep' } } })).split;
		expect(split.label).toBe('computed from prices');
	});
});

// -------------------------------------------------------------- cost, EV --

describe('cost panel (reference 01)', () => {
	it('prints 0.9c = 30 × Wild lifeforce (purple)', async () => {
		expect(family(pageView(await input())).cost.line).toBe('0.9c = 30 × Wild lifeforce (purple)');
	});

	it('prints 1 div → 9,905 lifeforce → 330 rerolls', async () => {
		expect(family(pageView(await input())).cost.perDivine).toBe('1 div → 9,905 lifeforce → 330 rerolls');
	});

	it('names both markets in the caption', async () => {
		expect(family(pageView(await input())).cost.caption).toBe(
			'chaos side 0.031c each; divine side from the lifeforce/divine market'
		);
	});

	it('Delirium Orbs pay Primal: 1.5c = 30 × Primal lifeforce (blue)', async () => {
		expect(family(pageView(await input({ familyId: 'delirium' }))).cost.line).toBe(
			'1.5c = 30 × Primal lifeforce (blue)'
		);
	});

	it('Delirium Orbs buy 248 rerolls a divine', async () => {
		expect(family(pageView(await input({ familyId: 'delirium' }))).cost.perDivine).toBe(
			'1 div → 7,445 lifeforce → 248 rerolls'
		);
	});
});

describe('EV panel (reference 01)', () => {
	const rowOf = async (label: string) => {
		const ev = family(pageView(await input())).ev!;
		return [...ev.rows, ...ev.yield].find((r) => r.label === label)?.value;
	};

	it('lists its rows in reference order', async () => {
		const ev = family(pageView(await input())).ev!;
		expect([...ev.rows, ...ev.yield].map((r) => r.label)).toEqual([
			'Cheapest feeder',
			'Loop EV per feeder',
			'Keeper hit per roll',
			'Rerolls per keeper',
			'Lifeforce per keeper',
			'1 div of lifeforce yields',
			'…worth after inputs'
		]);
	});

	it('Cheapest feeder: Lucent Fossil · 9.1c', async () => {
		expect(await rowOf('Cheapest feeder')).toBe('Lucent Fossil · 9.1c');
	});

	it('Loop EV per feeder: +25.7c', async () => {
		expect(await rowOf('Loop EV per feeder')).toBe('+25.7c');
	});

	it('Keeper hit per roll: 3.7%', async () => {
		expect(await rowOf('Keeper hit per roll')).toBe('3.7%');
	});

	it('Rerolls per keeper: ~25.2', async () => {
		expect(await rowOf('Rerolls per keeper')).toBe('~25.2');
	});

	it('Lifeforce per keeper: ~757 Wild · 23.5c', async () => {
		expect(await rowOf('Lifeforce per keeper')).toBe('~757 Wild · 23.5c');
	});

	it('1 div of lifeforce yields: ~13.1 keepers', async () => {
		expect(await rowOf('1 div of lifeforce yields')).toBe('~13.1 keepers');
	});

	it('…worth after inputs: +336c', async () => {
		expect(await rowOf('…worth after inputs')).toBe('+336c');
	});

	it('carries no unpriced note when every type is priced', async () => {
		expect(family(pageView(await input())).ev!.unpricedNote).toBeNull();
	});
});

describe('unpriced Hollow (reference 08 §5, test-local price)', () => {
	const unpriced = async () => {
		const base = await input();
		const hollow = itemId(base.harvest!, 'fossil', 'Hollow');
		return { hollow, body: family(pageView({ ...base, exchange: withPrice(base.exchange!, hollow, null, 402) })) };
	};

	it('prints the floor EV +23.7c (floor ruling)', async () => {
		expect((await unpriced()).body.ev!.rows[1].value).toBe('+23.7c');
	});

	it('notes the floor and the EV at the last seen price', async () => {
		expect((await unpriced()).body.ev!.unpricedNote).toBe(
			'1 unpriced type counted as 0c, so this EV is a floor. At its last seen 402c it would read +25.7c.'
		);
	});

	it('prints its card as 1 type · —', async () => {
		const { body } = await unpriced();
		expect(card(body, 'keepers', 'TOP').meta).toBe('1 type · —');
	});

	it('prints its chip as — with the unpriced mark', async () => {
		const { body, hollow } = await unpriced();
		const chip = card(body, 'keepers', 'TOP').chips.find((c) => c.itemId === hollow)!;
		expect([chip.priceText, chip.unpriced]).toEqual(['—', true]);
	});

	it('prints no weight share on its chip, as drawn', async () => {
		const { body, hollow } = await unpriced();
		expect(card(body, 'keepers', 'TOP').chips.find((c) => c.itemId === hollow)!.shareText).toBeNull();
	});
});

// ------------------------------------------------------------------ regex --

describe('regex boxes', () => {
	it('builds the Fossil keepers regex (reference 01)', async () => {
		expect(family(pageView(await input())).regex.keepers.text).toBe('"shu|san|gil|fac|fra|hol|gly"');
	});

	it('counts it: 29 / 250 characters, within the limit', async () => {
		const box = family(pageView(await input())).regex.keepers;
		expect([box.count, box.overLimit]).toEqual(['29 / 250 characters', false]);
	});

	it('follows the picks: Dense kept leads the keepers regex (reference 05)', async () => {
		const base = await input();
		const dense = itemId(base.harvest!, 'fossil', 'Dense');
		const box = family(pageView({ ...base, picks: { fossil: { [dense]: 'keep' } } })).regex.keepers;
		expect(box.text).toBe('"den|shu|san|gil|fac|fra|hol|gly"');
	});

	it('counts the picked keepers regex: 33 / 250 characters', async () => {
		const base = await input();
		const dense = itemId(base.harvest!, 'fossil', 'Dense');
		const box = family(pageView({ ...base, picks: { fossil: { [dense]: 'keep' } } })).regex.keepers;
		expect(box.count).toBe('33 / 250 characters');
	});

	it('flags a regex over 250 characters with the long count (reference 08 §8)', async () => {
		// Test-local: 70 extra unpriced Fossil types named "z" + two letters. Unpriced types are
		// kept (C2), and each adds a unique 3-letter fragment, so the keepers regex passes 250.
		const base = await input();
		const letters = 'efghijklmnop';
		const extra = Array.from({ length: 70 }, (_, i) => {
			const name = `z${letters[Math.floor(i / 12)]}${letters[i % 12]}`;
			return { itemId: `test/${name}`, name, shortName: name, loggedRolls: 1 };
		});
		const harvest = {
			...base.harvest!,
			families: base.harvest!.families.map((f) =>
				f.id === 'fossil' ? { ...f, weights: { ...f.weights!, types: [...f.weights!.types, ...extra] } } : f
			)
		};
		const box = family(pageView({ ...base, harvest })).regex.keepers;
		expect([box.overLimit, box.count]).toEqual([true, `${box.length} / 250 characters: too long for one stash search`]);
		expect(box.length).toBeGreaterThan(250);
	});

	it('captions both boxes', async () => {
		expect(family(pageView(await input())).regex.caption).toBe(
			'Paste into the stash search. Shortest fragment unique within this family; follows your tier picks.'
		);
	});
});

// ------------------------------------------------------------------ states --

describe('page states (reference 08 §1–4, §9)', () => {
	it('loading: Loading… and no verdict', () => {
		const view = pageView({
			exchange: null, harvest: null, lastFetchedAt: null, lastError: null, now: NOW,
			familyId: 'fossil', picks: {}, horizon: 'day', divineChaosRate: null
		});
		expect([view.state, view.stateLine, view.body]).toEqual(['loading', 'Loading…', null]);
	});

	const warming = async () => {
		const base = await input();
		return pageView({ ...base, exchange: { ...base.exchange!, warm: false, lastUpdated: null } });
	};

	it('warming: waits for the first exchange hour', async () => {
		const view = await warming();
		expect([view.state, view.stateLine]).toEqual(['warming', 'Waiting for the first Currency Exchange hour…']);
	});

	it('warming: the verdict panel says No prices yet', async () => {
		expect((await warming()).body).toEqual({
			kind: 'cold',
			title: 'No prices yet',
			text: 'The server has not read an hour of the exchange. This fills in on its own.'
		});
	});

	it('unreachable: Couldn’t reach the server', () => {
		const view = pageView({
			exchange: null, harvest: null, lastFetchedAt: null, lastError: 'boom', now: NOW,
			familyId: 'fossil', picks: {}, horizon: 'day', divineChaosRate: null
		});
		expect([view.state, view.stateLine, view.body]).toEqual(['unreachable', "Couldn't reach the server", null]);
	});

	it('stale: prints stale since HH:MM — server unreachable', async () => {
		const fetched = new Date(2026, 8, 9, 21, 4);
		const view = pageView({ ...(await input()), lastError: 'boom', lastFetchedAt: fetched });
		expect([view.state, view.status?.segments[0]]).toEqual(['stale', 'stale since 21:04 — server unreachable']);
	});

	it('stale: prints prices from HH:MM with their age (reference 08 §3)', async () => {
		const base = await input();
		const view = pageView({
			...base,
			exchange: { ...base.exchange!, lastUpdated: new Date(2026, 8, 9, 21, 0).toISOString() },
			lastError: 'boom',
			now: new Date(2026, 8, 9, 23, 0)
		});
		expect(view.status?.segments[1]).toBe('prices from 21:00 (2 h ago)');
	});

	it('stale keeps every number: the verdict EV still reads +25.7c', async () => {
		const view = pageView({ ...(await input()), lastError: 'boom' });
		expect(family(view).verdict.evText).toBe('+25.7c');
	});

	it('no-data family: Astrolabes say their weights are not logged yet', async () => {
		const view = pageView(await input({ familyId: 'astrolabe' }));
		expect(view.body).toEqual({
			kind: 'no-data',
			title: 'Astrolabes: reroll weights not logged yet',
			text: 'The EV needs how often each type comes out of a reroll. Nobody has sent a HarvestForge log for this family yet, so there is nothing to compute. The tab fills in when one arrives.'
		});
	});

	it('no-data family keeps the status line (reference 06)', async () => {
		const view = pageView(await input({ familyId: 'astrolabe' }));
		expect(view.status?.text).toBe(
			'updated 6 min ago · prices: Currency Exchange, Day 24h · 1 div = 360c · weights: no log yet'
		);
	});

	it('lists every family as a tab, marking the three without weights', async () => {
		const view = pageView(await input());
		expect(view.tabs.map((t) => [t.label, t.noData, t.active])).toEqual([
			['Delirium Orbs', false, false],
			['Deafening Essences', false, false],
			['Corrupted Essences', false, false],
			['Fossils', false, true],
			['Astrolabes', true, false],
			['Oils', true, false],
			['Catalysts', true, false]
		]);
	});
});

describe('combineOwnerState', () => {
	const owners = async () => readModels();

	it('CX warm + Harvest cold → warming', async () => {
		const { exchange, harvest } = await owners();
		const state = combineOwnerState({
			exchange, harvest: { ...harvest, warm: false }, lastFetchedAt: NOW, lastError: null, now: NOW
		});
		expect(state.kind).toBe('warming');
	});

	it('CX cold + Harvest warm → warming', async () => {
		const { exchange, harvest } = await owners();
		const state = combineOwnerState({
			exchange: { ...exchange, warm: false }, harvest, lastFetchedAt: NOW, lastError: null, now: NOW
		});
		expect(state.kind).toBe('warming');
	});

	it('an error with a prior result → stale', async () => {
		const { exchange, harvest } = await owners();
		expect(combineOwnerState({ exchange, harvest, lastFetchedAt: NOW, lastError: 'x', now: NOW }).kind).toBe('stale');
	});

	it('an error with only one owner loaded → unreachable', async () => {
		const { exchange } = await owners();
		expect(
			combineOwnerState({ exchange, harvest: null, lastFetchedAt: null, lastError: 'x', now: NOW }).kind
		).toBe('unreachable');
	});

	it('both warm, no error → ready with the exchange age', async () => {
		const { exchange, harvest } = await owners();
		const state = combineOwnerState({ exchange, harvest, lastFetchedAt: NOW, lastError: null, now: NOW });
		expect([state.kind, state.updatedAgo]).toEqual(['ready', '6 min ago']);
	});
});

// ------------------------------------------------------------------- prefs --

describe('picks prefs', () => {
	const families = async () => (await readModels()).harvest.families;

	it('turns garbage into {}', async () => {
		expect(parsePicks('garbage', await families())).toEqual({});
	});

	it('ignores a constructor key', async () => {
		expect(parsePicks('{"constructor":{}}', await families())).toEqual({});
	});

	it('ignores a __proto__ key without touching the prototype', async () => {
		const picks = parsePicks('{"__proto__":{"x":"keep"}}', await families());
		expect([Object.keys(picks), Object.getPrototypeOf(picks)]).toEqual([[], Object.prototype]);
	});

	it('drops an unknown itemId and keeps a known one', async () => {
		const fs = await families();
		const dense = fs.find((f) => f.id === 'fossil')!.weights!.types.find((t) => t.shortName === 'Dense')!.itemId;
		const raw = JSON.stringify({ fossil: { [dense]: 'keep', 'Metadata/Nope': 'keep' } });
		expect(parsePicks(raw, fs)).toEqual({ fossil: { [dense]: 'keep' } });
	});

	it('drops a value that is not keep or reroll', async () => {
		const fs = await families();
		const dense = fs.find((f) => f.id === 'fossil')!.weights!.types.find((t) => t.shortName === 'Dense')!.itemId;
		expect(parsePicks(JSON.stringify({ fossil: { [dense]: 'maybe' } }), fs)).toEqual({});
	});

	it('round-trips through serializePicks', async () => {
		const fs = await families();
		const dense = fs.find((f) => f.id === 'fossil')!.weights!.types.find((t) => t.shortName === 'Dense')!.itemId;
		const picks: HarvestPicks = { fossil: { [dense]: 'keep' } };
		expect(parsePicks(serializePicks(picks), fs)).toEqual(picks);
	});

	it('resetFamily clears the active family and leaves the others', () => {
		const picks: HarvestPicks = { fossil: { a: 'keep' }, delirium: { b: 'reroll' } };
		expect(resetFamily(picks, 'fossil')).toEqual({ delirium: { b: 'reroll' } });
	});

	it('applyPick sets a pick on the active family only', async () => {
		const { exchange } = await readModels();
		const picks: HarvestPicks = { delirium: { b: 'reroll' } };
		const id = 'Metadata/Items/Currency/CurrencyDelveCraftingDefences';
		expect(applyPick(picks, 'fossil', id, 'keep', exchange.prices)).toEqual({
			delirium: { b: 'reroll' },
			fossil: { [id]: 'keep' }
		});
	});

	it('applyPick with null clears that pick', () => {
		const picks: HarvestPicks = { fossil: { a: 'keep', c: 'reroll' } };
		expect(applyPick(picks, 'fossil', 'a', null, {})).toEqual({ fossil: { c: 'reroll' } });
	});

	it('applyPick drops a reroll pick on an unpriced type (C2)', async () => {
		const { exchange } = await readModels();
		const id = 'Metadata/Items/Currency/CurrencyDelveCraftingAbyss';
		const prices = withPrice(exchange, id, null, 402).prices;
		expect(applyPick({}, 'fossil', id, 'reroll', prices)).toEqual({});
	});

	// The eleven LOW Fossils of fixtures.json fossil tiers.byItem, written out.
	const LOW_FOSSILS = [
		'Lightning', 'Physical', 'Life', 'Cold', 'Fire', 'Chaos', 'Mana', 'MinionsAuras', 'Enchant', 'Sockets', 'Vaal'
	].map((s) => `Metadata/Items/Currency/CurrencyDelveCrafting${s}`);

	it('moveTier keeps exactly the eleven LOW Fossils', async () => {
		const { exchange, harvest } = await readModels();
		const fossil = harvest.families.find((f) => f.id === 'fossil')!;
		const moved = moveTier({}, fossil, 'LOW', 'keep', exchange.prices);
		expect(moved.fossil).toEqual(Object.fromEntries(LOW_FOSSILS.map((id) => [id, 'keep'])));
	});

	it('moveTier leaves another family\u2019s picks in place', async () => {
		const { exchange, harvest } = await readModels();
		const fossil = harvest.families.find((f) => f.id === 'fossil')!;
		const moved = moveTier({ delirium: { b: 'reroll' } }, fossil, 'LOW', 'keep', exchange.prices);
		expect(moved.delirium).toEqual({ b: 'reroll' });
	});

	it('moveTier skips an unpriced type of the tier (C2)', async () => {
		const { exchange, harvest } = await readModels();
		const fossil = harvest.families.find((f) => f.id === 'fossil')!;
		const hollow = fossil.weights!.types.find((t) => t.shortName === 'Hollow')!.itemId;
		const moved = moveTier({}, fossil, 'TOP', 'reroll', withPrice(exchange, hollow, null, 402).prices);
		expect(moved).toEqual({});
	});

	it('parseHarvestHorizon falls back to day (C3)', () => {
		expect(parseHarvestHorizon('nonsense')).toBe('day');
	});

	it('parseHarvestHorizon accepts recent', () => {
		expect(parseHarvestHorizon('recent')).toBe('recent');
	});
});
