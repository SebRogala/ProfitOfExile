import { describe, it, expect } from 'vitest';
import { solveFamily, familyTypes, type FamilyInput, type FamilyResult, type Pick } from './engine';
import exchange from './__fixtures__/exchange.json';
import harvest from './__fixtures__/harvest.json';
import derived from './__fixtures__/derived.json';

/**
 * Inputs come from the per-owner fixture copies; the expected keeper sets come
 * from `derived.json`, which only tests read (the handoff's oracle). Every other
 * expected number is a literal from the reference screens named on the case.
 */

type FamilyId = 'delirium' | 'essence' | 'corrupt' | 'fossil' | 'astrolabe';
type FixtureFamily = (typeof harvest.families)[number];
type LifeforceName = keyof typeof exchange.lifeforce;

function family(id: FamilyId): FixtureFamily {
	const found = harvest.families.find((f) => f.id === id);
	if (!found) throw new Error(`no fixture family ${id}`);
	return found;
}

interface InputOverrides {
	rerollCostChaos?: number;
	lifeforcePerReroll?: number;
	picks?: Record<string, Pick>;
	prices?: Record<string, number | null>;
	lastSeen?: Record<string, number | null>;
}

/** Arrange an engine input from the fixtures the way the page will: chaos price per itemId, cost = rerollCost × lifeforce chaos. */
function inputFor(id: FamilyId, overrides: InputOverrides = {}): FamilyInput {
	const f = family(id);
	const lifeforce = f.lifeforce ? exchange.lifeforce[f.lifeforce as LifeforceName] : null;
	const prices = new Map<string, number | null>();
	const lastSeen = new Map<string, number | null>();
	for (const [itemId, price] of Object.entries(exchange.prices)) {
		prices.set(itemId, price.chaos);
		lastSeen.set(itemId, null);
	}
	for (const [itemId, chaos] of Object.entries(overrides.prices ?? {})) prices.set(itemId, chaos);
	for (const [itemId, chaos] of Object.entries(overrides.lastSeen ?? {})) lastSeen.set(itemId, chaos);
	return {
		types: familyTypes(f.weights),
		prices,
		lastSeen,
		rerollCostChaos:
			overrides.rerollCostChaos ?? (f.rerollCost ?? 0) * (lifeforce ? lifeforce.chaos : 0),
		lifeforcePerReroll: overrides.lifeforcePerReroll ?? f.rerollCost ?? 0,
		picks: new Map(Object.entries(overrides.picks ?? {})),
		lifeforcePerDivine: lifeforce ? lifeforce.perDivine : 0,
		divineChaosRate: exchange.divineChaosRate
	};
}

function idOf(id: FamilyId, shortName: string): string {
	const t = family(id).weights?.types.find((x) => x.shortName === shortName);
	if (!t) throw new Error(`no ${shortName} in ${id}`);
	return t.itemId;
}

function shortNamesOf(id: FamilyId, itemIds: string[]): string[] {
	const byId = new Map((family(id).weights?.types ?? []).map((t) => [t.itemId, t.shortName]));
	return itemIds.map((itemId) => byId.get(itemId) ?? itemId);
}

function ok(result: FamilyResult): Extract<FamilyResult, { kind: 'ok' }> {
	if (result.kind !== 'ok') throw new Error(`expected an ok result, got ${result.kind}`);
	return result;
}

/** Every number reachable in a result, with its path, so a NaN or ±Infinity anywhere is named. */
function numbersIn(value: unknown, path = 'result'): [string, number][] {
	if (typeof value === 'number') return [[path, value]];
	if (value instanceof Map) return [...value].flatMap(([k, v]) => numbersIn(v, `${path}.${k}`));
	if (Array.isArray(value)) return value.flatMap((v, i) => numbersIn(v, `${path}[${i}]`));
	if (value && typeof value === 'object')
		return Object.entries(value).flatMap(([k, v]) => numbersIn(v, `${path}.${k}`));
	return [];
}

function sorted(xs: string[]): string[] {
	return [...xs].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
}

describe('solveFamily', () => {
	describe('Fossil default (reference 01)', () => {
		const r = () => ok(solveFamily(inputFor('fossil')));

		it('keeps the derived keepers', () => {
			expect(sorted(shortNamesOf('fossil', r().keepers))).toEqual(sorted(derived.fossil.keepers));
		});

		it('heads with Lucent Fossil at 9.1c, the cheapest feeder', () => {
			expect(r().headline.name).toBe(derived.fossil.cheapestFeeder);
			expect(r().headline.priceChaos.toFixed(1)).toBe('9.1');
		});

		it('pays +25.7c per Lucent loop', () => {
			expect(r().headline.loopEvChaos.toFixed(1)).toBe(String(derived.fossil.loopEvChaos));
		});

		it('rounds the divine line to 15 feeders, 378 rerolls, 11,349 Wild (C4)', () => {
			expect(r().divineLine).toEqual({ feeders: 15, rerolls: 378, lifeforce: 11349 });
		});

		it('hits a keeper 3.7% of rolls', () => {
			expect((r().headline.keeperHitPerRoll * 100).toFixed(1)).toBe('3.7');
		});

		it('takes ~25.2 rerolls per keeper', () => {
			expect(r().headline.rerollsPerKeeper.toFixed(1)).toBe('25.2');
		});

		it('spends ~757 Wild per keeper', () => {
			expect(Math.round(r().headline.lifeforcePerKeeper)).toBe(757);
		});

		it('spends 23.5c of lifeforce per keeper', () => {
			expect(r().headline.lifeforceChaosPerKeeper.toFixed(1)).toBe('23.5');
		});

		it('yields ~13.1 keepers per divine of lifeforce', () => {
			expect(r().headline.keepersPerDivine.toFixed(1)).toBe('13.1');
		});

		it('makes +336c from those keepers', () => {
			expect(Math.round(r().headline.keepersPerDivineWorthChaos)).toBe(336);
		});

		it('costs 0.93c a reroll', () => {
			expect(r().rerollCostChaos).toBeCloseTo(0.93, 9);
		});

		it('buys 330 rerolls per divine', () => {
			expect(Math.round(r().rerollsPerDivine)).toBe(330);
		});
	});

	it('Fossil weight share divides by the sum of logged rolls (4,720), not the sample size', () => {
		const r = ok(solveFamily(inputFor('fossil')));
		const share = (s: string) => r.types.find((t) => t.itemId === idOf('fossil', s))!.weightShare;
		expect((share('Shuddering') * 100).toFixed(1)).toBe('2.8');
		expect((share('Faceted') * 100).toFixed(2)).toBe('0.11');
		expect((share('Hollow') * 100).toFixed(2)).toBe('0.02');
		expect(share('Hollow')).toBeCloseTo(1 / 4720, 12);
	});

	describe('Delirium default (reference 02)', () => {
		const r = () => ok(solveFamily(inputFor('delirium')));

		it('keeps Diviner\u2019s and Skittering', () => {
			expect(sorted(shortNamesOf('delirium', r().keepers))).toEqual(sorted(derived.delirium.keepers));
		});

		it('heads with Jeweller\u2019s at 10.3c', () => {
			expect(r().headline.shortName).toBe("Jeweller's");
			expect(r().headline.priceChaos.toFixed(1)).toBe('10.3');
		});

		it('pays +22.4c per loop', () => {
			expect(r().headline.loopEvChaos.toFixed(1)).toBe('22.4');
		});

		it('rounds the divine line to 17 feeders, 229 rerolls, 6,872 Primal', () => {
			expect(r().divineLine).toEqual({ feeders: 17, rerolls: 229, lifeforce: 6872 });
		});

		it('hits a keeper 7.7% of rolls', () => {
			expect((r().headline.keeperHitPerRoll * 100).toFixed(1)).toBe('7.7');
		});

		it('takes ~13.5 rerolls per keeper', () => {
			expect(r().headline.rerollsPerKeeper.toFixed(1)).toBe('13.5');
		});

		it('spends ~404 Primal per keeper', () => {
			expect(Math.round(r().headline.lifeforcePerKeeper)).toBe(404);
		});

		it('spends 19.8c of lifeforce per keeper', () => {
			expect(r().headline.lifeforceChaosPerKeeper.toFixed(1)).toBe('19.8');
		});

		it('yields ~18.4 keepers per divine of lifeforce', () => {
			expect(r().headline.keepersPerDivine.toFixed(1)).toBe('18.4');
		});

		it('makes +412c from those keepers', () => {
			expect(Math.round(r().headline.keepersPerDivineWorthChaos)).toBe(412);
		});

		it('costs 1.5c a reroll', () => {
			expect(r().rerollCostChaos.toFixed(1)).toBe('1.5');
		});

		it('buys 248 rerolls per divine', () => {
			expect(Math.round(r().rerollsPerDivine)).toBe(248);
		});
	});

	it('Deafening Essences: Torment feeds at +6.3c with the six derived keepers (reference 03)', () => {
		const r = ok(solveFamily(inputFor('essence')));
		expect(sorted(shortNamesOf('essence', r.keepers))).toEqual(sorted(derived.essence.keepers));
		expect(r.headline.name).toBe(derived.essence.cheapestFeeder);
		expect(r.headline.loopEvChaos.toFixed(1)).toBe(String(derived.essence.loopEvChaos));
	});

	describe('Corrupted Essences, uniform weights (reference 04)', () => {
		const r = () => ok(solveFamily(inputFor('corrupt')));

		it('keeps Horror', () => {
			expect(shortNamesOf('corrupt', r().keepers)).toEqual(derived.corrupt.keepers);
		});

		it('heads with Hysteria at 41.9c', () => {
			expect(r().headline.shortName).toBe('Hysteria');
			expect(r().headline.priceChaos.toFixed(1)).toBe('41.9');
		});

		// Closed form (one keeper, symmetric feeders): 65.67 − 3 × 1.47 − 41.91 = 19.35, a half-tie
		// that reference 04 prints +19.3c and the ticket +19.4c. Which way it displays is the
		// formatter's (WI-6), so the engine is pinned to the exact value.
		it('pays 19.35c per loop', () => {
			expect(r().headline.loopEvChaos).toBeCloseTo(19.35, 9);
		});

		it('rounds the divine line to 19 feeders, 57 rerolls, 1,710 Primal', () => {
			expect(r().divineLine).toEqual({ feeders: 19, rerolls: 57, lifeforce: 1710 });
		});

		it('hits a keeper 33.3% of rolls', () => {
			expect((r().headline.keeperHitPerRoll * 100).toFixed(1)).toBe('33.3');
		});

		it('takes ~3.0 rerolls per keeper', () => {
			expect(r().headline.rerollsPerKeeper.toFixed(1)).toBe('3.0');
		});
	});

	it('Pick: Dense picked keep is kept, Corroded stays fed, and Lucent re-solves to +24.3c (reference 05)', () => {
		const dense = idOf('fossil', 'Dense');
		const r = ok(solveFamily(inputFor('fossil', { picks: { [dense]: 'keep' } })));
		expect(r.keepers).toContain(dense);
		expect(r.feeders).toContain(idOf('fossil', 'Corroded'));
		expect(r.headline.shortName).toBe('Lucent');
		expect(r.headline.loopEvChaos.toFixed(1)).toBe('24.3');
	});

	// Reference 08 §5 prints +23.8c; that is the EV with Hollow FED at 0c (23.789). The rule it
	// illustrates keeps an unpriced type (README § Engine, C2), which gives 23.743 → +23.7c.
	// Disputed in the WI-1 return; the rule wins until ruled otherwise.
	it('Unpriced: Hollow with no price is kept, the EV is a +23.7c floor and reads +25.7c at its last seen 402c (reference 08 §5)', () => {
		const hollow = idOf('fossil', 'Hollow');
		const r = ok(
			solveFamily(
				inputFor('fossil', {
					prices: { [hollow]: null },
					lastSeen: { [hollow]: 402 }
				})
			)
		);
		expect(r.keepers).toContain(hollow);
		expect(r.unpriced).toEqual([hollow]);
		expect(r.headline.loopEvChaos.toFixed(1)).toBe('23.7');
		expect(r.headline.loopEvAtLastSeenChaos?.toFixed(1)).toBe('25.7');
	});

	it('Unpriced beats a pick: Hollow picked reroll with no price stays a keeper (C2)', () => {
		const hollow = idOf('fossil', 'Hollow');
		const r = ok(
			solveFamily(inputFor('fossil', { prices: { [hollow]: null }, picks: { [hollow]: 'reroll' } }))
		);
		expect(r.keepers).toContain(hollow);
		expect(r.types.find((t) => t.itemId === hollow)!.picked).toBeNull();
	});

	it('Unpriced without a last seen price reports no last-seen EV', () => {
		const hollow = idOf('fossil', 'Hollow');
		const r = ok(solveFamily(inputFor('fossil', { prices: { [hollow]: null } })));
		expect(r.headline.loopEvAtLastSeenChaos).toBeNull();
	});

	it('Not worth it: Torment picked reroll at 8c a reroll stays the cheapest feeder with a negative EV and no divine line (reference 08 §6)', () => {
		const torment = idOf('essence', 'Torment');
		const r = ok(
			solveFamily(inputFor('essence', { rerollCostChaos: 8, picks: { [torment]: 'reroll' } }))
		);
		expect(r.headline.itemId).toBe(torment);
		expect(r.headline.loopEvChaos).toBeLessThan(0);
		expect(r.divineLine).toBeNull();
	});

	it('High cost without picks: 8c a reroll keeps every type, so there is nothing to feed', () => {
		expect(solveFamily(inputFor('essence', { rerollCostChaos: 8 })).kind).toBe('no-feeders');
	});

	// Closest literals from the all-kept closed form R(i) = Σ_{j≠i} p(j)/19 − c (uniform
	// weights, computed by hand from the fixture prices), minus p(i).
	it('Nothing to feed at 8c names Torment as the closest type', () => {
		const r = solveFamily(inputFor('essence', { rerollCostChaos: 8 }));
		expect(r.kind === 'no-feeders' && r.closest?.itemId).toBe(idOf('essence', 'Torment'));
	});

	it('Nothing to feed at 8c gives Torment its loop EV −3.26c', () => {
		const r = solveFamily(inputFor('essence', { rerollCostChaos: 8 }));
		expect(r.kind === 'no-feeders' ? r.closest?.loopEvChaos : null).toBeCloseTo(-3.2605, 4);
	});

	it('An unpriced type is never the closest: Torment unpriced at 8c hands it to Suffering', () => {
		const torment = idOf('essence', 'Torment');
		const r = solveFamily(inputFor('essence', { rerollCostChaos: 8, prices: { [torment]: null } }));
		expect(r.kind === 'no-feeders' && r.closest?.itemId).toBe(idOf('essence', 'Suffering'));
	});

	const allKeptFossil = () =>
		solveFamily(
			inputFor('fossil', {
				picks: Object.fromEntries(family('fossil').weights!.types.map((t) => [t.itemId, 'keep' as Pick]))
			})
		);

	it('Every type picked keep names Frigid as the closest', () => {
		const r = allKeptFossil();
		expect(r.kind === 'no-feeders' ? r.closest?.itemId : null).toBe(idOf('fossil', 'Frigid'));
	});

	it('Every type picked keep gives Frigid its all-kept loop EV +7.26c', () => {
		const r = allKeptFossil();
		expect(r.kind === 'no-feeders' ? r.closest?.loopEvChaos : null).toBeCloseTo(7.256, 3);
	});

	describe('the family\'s rerollCost, not a constant 30 (C12)', () => {
		// Every fixture family costs 30, so 60 is test-local; the chaos cost is left at the fixture's,
		// so only the lifeforce count per reroll changes.
		const at60 = () => ok(solveFamily(inputFor('fossil', { lifeforcePerReroll: 60 })));

		it('buys 9,905 / 60 = 165 rerolls per divine', () => {
			expect(Math.round(at60().rerollsPerDivine)).toBe(165);
		});

		it('spends 60 lifeforce for every reroll of a keeper', () => {
			const h = at60().headline;
			expect(h.lifeforcePerKeeper / h.rerollsPerKeeper).toBeCloseTo(60, 9);
		});

		it('halves the keepers a divine of lifeforce yields: ~6.5', () => {
			expect(at60().headline.keepersPerDivine.toFixed(1)).toBe('6.5');
		});

		it('counts 60 lifeforce per reroll in the divine line', () => {
			const line = at60().divineLine!;
			// Both are rounded counts, so 60 within rounding (a constant 30 would read ~30).
			expect(line.lifeforce / line.rerolls).toBeCloseTo(60, 0);
		});

		it('a zero rerollCost gives no-data', () => {
			expect(solveFamily(inputFor('fossil', { lifeforcePerReroll: 0 })).kind).toBe('no-data');
		});
	});

	it('Degenerate: every type picked reroll gives no-keepers', () => {
		const picks = Object.fromEntries(
			family('fossil').weights!.types.map((t) => [t.itemId, 'reroll' as Pick])
		);
		expect(solveFamily(inputFor('fossil', { picks })).kind).toBe('no-keepers');
	});

	it('Degenerate: every type picked keep gives no-feeders', () => {
		const picks = Object.fromEntries(
			family('fossil').weights!.types.map((t) => [t.itemId, 'keep' as Pick])
		);
		expect(solveFamily(inputFor('fossil', { picks })).kind).toBe('no-feeders');
	});

	it('Degenerate: Astrolabes without weights give no-data', () => {
		expect(solveFamily(inputFor('astrolabe')).kind).toBe('no-data');
	});

	// Expected literals from a closed form, not the engine: with Hollow the only keeper,
	// Lucent's EV = 401.63 − c·N − 9.12 and N = 1 − w(L) + Σ_{j fed} w(j)(1 − w(j)) / w(Hollow)
	// = 4238.151483 (scratch oracle.py over the fixture weights).
	describe('Sparse picks: only Hollow kept, every other Fossil picked reroll', () => {
		const sparse = (rerollCostChaos?: number) => {
			const picks = Object.fromEntries(
				family('fossil').weights!.types.map((t) => [t.itemId, (t.shortName === 'Hollow' ? 'keep' : 'reroll') as Pick])
			);
			return ok(solveFamily(inputFor('fossil', { picks, rerollCostChaos })));
		};

		it('solves Lucent to its exact fixed point at the fixture cost: \u22123,548.97c', () => {
			expect(sparse().headline.loopEvChaos).toBeCloseTo(-3548.970879, 4);
		});

		it('solves rerolls per keeper exactly: 4,238.15', () => {
			expect(sparse().headline.rerollsPerKeeper).toBeCloseTo(4238.151483, 4);
		});

		it('stays a loss at 0.1c a reroll: \u221231.31c', () => {
			expect(sparse(0.1).headline.loopEvChaos).toBeCloseTo(-31.305148, 4);
		});

		it('shows no divine line at 0.1c a reroll', () => {
			expect(sparse(0.1).divineLine).toBeNull();
		});
	});

	it('Unreachable keeper: the only kept type has zero weight, so there is nothing to stop on', () => {
		const input = inputFor('fossil', {
			picks: Object.fromEntries(
				family('fossil').weights!.types.map((t) => [t.itemId, (t.shortName === 'Hollow' ? 'keep' : 'reroll') as Pick])
			)
		});
		input.types = input.types.map((t) => (t.shortName === 'Hollow' ? { ...t, weight: 0 } : t));
		expect(solveFamily(input).kind).toBe('no-keepers');
	});

	it('A non-finite lifeforce rate gives no-data', () => {
		expect(solveFamily({ ...inputFor('fossil'), lifeforcePerDivine: Infinity }).kind).toBe('no-data');
	});

	it('A NaN lifeforce rate gives no-data', () => {
		expect(solveFamily({ ...inputFor('fossil'), lifeforcePerDivine: Number.NaN }).kind).toBe('no-data');
	});

	it('A non-finite divine rate gives no-data', () => {
		expect(solveFamily({ ...inputFor('fossil'), divineChaosRate: Infinity }).kind).toBe('no-data');
	});

	it('A negative lifeforce rate gives no-data', () => {
		expect(solveFamily({ ...inputFor('fossil'), lifeforcePerDivine: -9905 }).kind).toBe('no-data');
	});

	it('A negative divine rate gives no-data', () => {
		expect(solveFamily({ ...inputFor('fossil'), divineChaosRate: -360.07 }).kind).toBe('no-data');
	});

	it('A finite divine rate that overflows the divine line gives no-data', () => {
		expect(solveFamily({ ...inputFor('fossil'), divineChaosRate: Number.MAX_VALUE }).kind).toBe('no-data');
	});

	it('No result field is NaN or ±Infinity on any measured input', () => {
		const torment = idOf('essence', 'Torment');
		const hollow = idOf('fossil', 'Hollow');
		const results = [
			...(['fossil', 'delirium', 'essence', 'corrupt', 'astrolabe'] as const).map((id) =>
				solveFamily(inputFor(id))
			),
			solveFamily(inputFor('essence', { rerollCostChaos: 8, picks: { [torment]: 'reroll' } })),
			solveFamily(inputFor('fossil', { prices: { [hollow]: null } }))
		];
		const bad = results.flatMap((r) => numbersIn(r)).filter(([, n]) => !Number.isFinite(n));
		expect(bad).toEqual([]);
	});
});
