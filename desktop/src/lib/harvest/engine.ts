/**
 * Harvest Flipping engine — which types of one family to keep, and what
 * rerolling the rest is worth.
 *
 * Pure: plain data in (types with weights, chaos prices, the reroll cost, the
 * player's picks, the lifeforce and divine rates), a discriminated result out.
 * It reads no store, fetches nothing and imports no seam type, so the page and
 * any later live adapter feed it the same shape the fixtures do.
 *
 * The model (handoff README § Engine): a reroll of type `i` yields type `j ≠ i`
 * with `P(j|i) = w(j) / (1 − w(i))`. Value iteration finds, for every type, the
 * value of rerolling it until a keeper comes out; a type is kept when its price
 * is at least that value, unless the player picked otherwise.
 *
 * Two rules sit on top of the formulas:
 *
 * **An unpriced type is always a keeper and counts 0c as an outcome** (C2: the
 * page must never recommend feeding what it cannot price, so a `'reroll'` pick
 * on it is ignored). The EV is then a floor; `loopEvAtLastSeenChaos` re-solves
 * with its last seen price so the page can say how far that floor moves.
 *
 * **Degenerate sets are results, not numbers.** With no reachable keeper the
 * fixed point does not exist (`I − P_FF` is singular); with no feeder there is no headline. Those return `no-keepers` /
 * `no-feeders` instead of whatever the pass cap left behind, and no field of an
 * `ok` result is NaN or ±Infinity.
 */

export type Pick = 'keep' | 'reroll';

/** One type of a family, with its raw (unnormalised) weight. */
export interface FamilyType {
	itemId: string;
	name: string;
	shortName: string;
	weight: number;
}

export interface FamilyInput {
	types: FamilyType[];
	/** Chaos price per itemId; `null` (or missing) means unpriced. */
	prices: Map<string, number | null>;
	/** Last seen chaos price per itemId, read only for unpriced types. */
	lastSeen: Map<string, number | null>;
	rerollCostChaos: number;
	/** Lifeforce spent per reroll: the family's `rerollCost`. */
	lifeforcePerReroll: number;
	picks: Map<string, Pick>;
	/** Lifeforce of the family's colour bought per 1 divine. */
	lifeforcePerDivine: number;
	divineChaosRate: number;
}

export interface TypeResult {
	itemId: string;
	name: string;
	shortName: string;
	/** `w / Σw` of the family. */
	weightShare: number;
	/** `null` when unpriced; the engine counted it as 0c. */
	priceChaos: number | null;
	kept: boolean;
	/** Set when the player's pick decided this type (an ignored pick on an unpriced type is not one). */
	picked: Pick | null;
	/** `R(i)`: the value of rerolling this type until a keeper. */
	rerollValueChaos: number;
	/** `R(i) − p(i)`: feeder EV of one loop. Meaningful for feeders. */
	loopEvChaos: number;
}

export interface Headline {
	itemId: string;
	name: string;
	shortName: string;
	priceChaos: number;
	loopEvChaos: number;
	/** The same feeder's loop EV with every unpriced type at its last seen price; `null` when nothing is unpriced or a last seen price is missing. */
	loopEvAtLastSeenChaos: number | null;
	keeperHitPerRoll: number;
	rerollsPerKeeper: number;
	lifeforcePerKeeper: number;
	lifeforceChaosPerKeeper: number;
	keepersPerDivine: number;
	keepersPerDivineWorthChaos: number;
}

export interface DivineLine {
	feeders: number;
	rerolls: number;
	lifeforce: number;
}

/** The priced type whose loop EV comes closest to paying, when every type is kept. */
export interface Closest {
	itemId: string;
	name: string;
	loopEvChaos: number;
}

export type FamilyResult =
	| {
			kind: 'ok';
			types: TypeResult[];
			keepers: string[];
			feeders: string[];
			unpriced: string[];
			headline: Headline;
			/** Absent unless the headline loop EV exceeds `DIVINE_LINE_MIN_EV_CHAOS`. */
			divineLine: DivineLine | null;
			rerollCostChaos: number;
			rerollsPerDivine: number;
	  }
	| { kind: 'no-keepers' }
	| {
			kind: 'no-feeders';
			/** `null` when no type is priced. */
			closest: Closest | null;
	  }
	| { kind: 'no-data' };

export const VALUE_ITERATION_TOLERANCE = 1e-9;
export const VALUE_ITERATION_MAX_PASSES = 400;
/** The divine-scale line shows only when the flip pays more than this per feeder. */
export const DIVINE_LINE_MIN_EV_CHAOS = 0.05;
/**
 * Policy-improvement rounds after the value iteration. The 400 passes pick a
 * keep set; a sparse pick set (one rare keeper) can leave `R` far from its
 * fixed point at the cap (Hollow-only on Fossils: −313c after 400 passes vs
 * −3,549c exact), so the chosen policy is evaluated exactly and re-decided
 * until stable. Monotone for optimal stopping; a round per type is ample.
 */
const POLICY_MAX_ROUNDS = 64;
/** A pivot below this means `I − P_FF` is singular: some fed type never reaches a keeper. */
const SINGULAR_PIVOT = 1e-12;

interface WeightsData {
	sample: { uniform?: boolean } | null;
	types: { itemId: string; name: string; shortName: string; loggedRolls: number | null }[];
}

/**
 * A family's types with the weight the engine normalises: its logged rolls, or
 * 1 each where the sample is marked uniform. No weights → no types (`no-data`).
 */
export function familyTypes(weights: WeightsData | null): FamilyType[] {
	if (!weights) return [];
	const uniform = weights.sample?.uniform === true;
	return weights.types.map((t) => ({
		itemId: t.itemId,
		name: t.name,
		shortName: t.shortName,
		weight: uniform ? 1 : (t.loggedRolls ?? Number.NaN)
	}));
}

interface Solved {
	keep: boolean[];
	R: number[];
}

/** Value iteration; `forced[i]` is the decision a pick or the unpriced rule imposed, `null` when prices decide. */
function solveKeepSet(
	P: number[][],
	prices: number[],
	forced: (boolean | null)[],
	cost: number
): Solved {
	const n = prices.length;
	const V = [...prices];
	const R = new Array<number>(n).fill(0);
	const keep = new Array<boolean>(n).fill(true);
	for (let pass = 0; pass < VALUE_ITERATION_MAX_PASSES; pass++) {
		let change = 0;
		for (let i = 0; i < n; i++) {
			let sum = 0;
			for (let j = 0; j < n; j++) sum += P[i][j] * V[j];
			R[i] = sum - cost;
		}
		for (let i = 0; i < n; i++) {
			keep[i] = forced[i] ?? prices[i] >= R[i];
			const next = keep[i] ? prices[i] : R[i];
			change = Math.max(change, Math.abs(next - V[i]));
			V[i] = next;
		}
		if (change < VALUE_ITERATION_TOLERANCE) break;
	}
	return { keep, R };
}

/** Solves `A·x = b` for each right-hand side by Gaussian elimination with partial pivoting; `null` when singular. */
function solveLinear(A: number[][], rhs: number[][]): number[][] | null {
	const m = A.length;
	const M = A.map((row, i) => [...row, ...rhs.map((b) => b[i])]);
	const k = rhs.length;
	for (let col = 0; col < m; col++) {
		let pivot = col;
		for (let r = col + 1; r < m; r++) if (Math.abs(M[r][col]) > Math.abs(M[pivot][col])) pivot = r;
		if (!(Math.abs(M[pivot][col]) > SINGULAR_PIVOT)) return null;
		[M[col], M[pivot]] = [M[pivot], M[col]];
		for (let r = 0; r < m; r++) {
			if (r === col) continue;
			const f = M[r][col] / M[col][col];
			if (f === 0) continue;
			for (let c = col; c < m + k; c++) M[r][c] -= f * M[col][c];
		}
	}
	return rhs.map((_, q) => M.map((row, i) => row[m + q] / row[i]));
}

/**
 * Exact value of a fixed keep set: `V_F = (I − P_FF)⁻¹(P_FK·p_K − c)` and
 * `N_F = (I − P_FF)⁻¹·1` (README § Engine's fixed points), then `R` and `N` for
 * every type from those. `null` when a fed type cannot reach a keeper.
 */
function evaluateKeepSet(
	P: number[][],
	prices: number[],
	keep: boolean[],
	cost: number
): { R: number[]; N: number[] } | null {
	const n = prices.length;
	const fed = keep.map((k, i) => (k ? -1 : i)).filter((i) => i >= 0);
	const A = fed.map((i) => fed.map((j) => (i === j ? 1 : 0) - P[i][j]));
	const bV = fed.map((i) => P[i].reduce((s, pij, j) => s + (keep[j] ? pij * prices[j] : 0), 0) - cost);
	const bN = fed.map(() => 1);
	const solved = fed.length > 0 ? solveLinear(A, [bV, bN]) : [[], []];
	if (!solved) return null;
	const V = [...prices];
	const NF = new Array<number>(n).fill(0);
	fed.forEach((i, q) => {
		V[i] = solved[0][q];
		NF[i] = solved[1][q];
	});
	const R = P.map((row) => row.reduce((s, pij, j) => s + pij * V[j], 0) - cost);
	const N = P.map((row) => row.reduce((s, pij, j) => s + (keep[j] ? 0 : pij * NF[j]), 1));
	if (![...R, ...N].every(Number.isFinite)) return null;
	return { R, N };
}

type Policy = { keep: boolean[]; R: number[]; N: number[] };

/**
 * The bounded value iteration decides a keep set; that set is then evaluated
 * exactly and re-decided (`forced ?? p ≥ R`) until consistent. `no-keepers`
 * when the set has nothing reachable to stop on; `null` if it never settles.
 */
function solvePolicy(
	P: number[][],
	w: number[],
	prices: number[],
	forced: (boolean | null)[],
	cost: number
): Policy | 'no-keepers' | null {
	let { keep } = solveKeepSet(P, prices, forced, cost);
	for (let round = 0; round < POLICY_MAX_ROUNDS; round++) {
		const keptWeight = w.reduce((s, wi, i) => s + (keep[i] ? wi : 0), 0);
		if (!(keptWeight > 0)) return 'no-keepers';
		const exact = evaluateKeepSet(P, prices, keep, cost);
		if (!exact) return 'no-keepers';
		const next = prices.map((p, i) => forced[i] ?? p >= exact.R[i]);
		if (next.every((k, i) => k === keep[i])) return { keep, ...exact };
		keep = next;
	}
	return null;
}

/** Every number in a value is finite (D3: an `ok` result never carries NaN or ±Infinity). */
function allFinite(value: unknown): boolean {
	if (typeof value === 'number') return Number.isFinite(value);
	if (Array.isArray(value)) return value.every(allFinite);
	if (value && typeof value === 'object') return Object.values(value).every(allFinite);
	return true;
}

function byPriceThenName(a: { price: number; name: string }, b: { price: number; name: string }): number {
	if (a.price !== b.price) return a.price < b.price ? -1 : 1;
	return a.name < b.name ? -1 : a.name > b.name ? 1 : 0;
}

export function solveFamily(input: FamilyInput): FamilyResult {
	const { types } = input;
	const totalWeight = types.reduce((s, t) => s + t.weight, 0);
	if (
		types.length < 2 ||
		!(totalWeight > 0) ||
		!Number.isFinite(totalWeight) ||
		types.some((t) => !Number.isFinite(t.weight) || t.weight < 0) ||
		!Number.isFinite(input.rerollCostChaos) ||
		!Number.isFinite(input.lifeforcePerReroll) ||
		!(input.lifeforcePerReroll > 0) ||
		!Number.isFinite(input.lifeforcePerDivine) ||
		input.lifeforcePerDivine < 0 ||
		!Number.isFinite(input.divineChaosRate) ||
		input.divineChaosRate < 0
	) {
		return { kind: 'no-data' };
	}

	const n = types.length;
	const w = types.map((t) => t.weight / totalWeight);
	const P = w.map((wi, i) => w.map((wj, j) => (i === j || wi >= 1 ? 0 : wj / (1 - wi))));

	const rawPrices = types.map((t) => {
		const p = input.prices.get(t.itemId);
		return typeof p === 'number' && Number.isFinite(p) ? p : null;
	});
	const unpriced = rawPrices.map((p) => p === null);
	const prices = rawPrices.map((p) => p ?? 0);
	const picks = types.map((t, i) => (unpriced[i] ? null : (input.picks.get(t.itemId) ?? null)));
	const forced = types.map((_, i) => (unpriced[i] ? true : picks[i] === null ? null : picks[i] === 'keep'));

	/**
	 * Every type kept: `V = p`, so `R(i) = Σ P(i,j)·p(j) − c` exactly. The closest
	 * is the priced type with the highest loop EV (prototype: "Closest: <name>
	 * <EV>"); an unpriced one counts 0c and is never a feeder candidate.
	 */
	const noFeeders = (): FamilyResult => {
		const candidates = types
			.map((t, i) => ({
				itemId: t.itemId,
				name: t.name,
				loopEvChaos: P[i].reduce((s, pij, j) => s + pij * prices[j], 0) - input.rerollCostChaos - prices[i]
			}))
			.filter((_, i) => !unpriced[i]);
		const closest = candidates.reduce<Closest | null>(
			(best, c) => (best === null || c.loopEvChaos > best.loopEvChaos ? c : best),
			null
		);
		return closest === null || Number.isFinite(closest.loopEvChaos)
			? { kind: 'no-feeders', closest }
			: { kind: 'no-data' };
	};

	// Degenerate sets decided by picks alone: check before iterating.
	if (forced.every((f) => f === false)) return { kind: 'no-keepers' };
	if (forced.every((f) => f === true)) return noFeeders();

	const policy = solvePolicy(P, w, prices, forced, input.rerollCostChaos);
	if (policy === null) return { kind: 'no-data' };
	if (policy === 'no-keepers') return { kind: 'no-keepers' };
	const { keep, R, N } = policy;
	if (keep.every(Boolean)) return noFeeders();

	const feederOrder = types
		.map((t, i) => ({ i, price: prices[i], name: t.name }))
		.filter(({ i }) => !keep[i])
		.sort(byPriceThenName);
	const h = feederOrder[0].i;
	const loopEv = R[h] - prices[h];

	let loopEvAtLastSeen: number | null = null;
	if (unpriced.some(Boolean)) {
		const seen = types.map((t, i) => (unpriced[i] ? (input.lastSeen.get(t.itemId) ?? null) : prices[i]));
		if (seen.every((p) => typeof p === 'number' && Number.isFinite(p))) {
			const atSeen = solvePolicy(P, w, seen as number[], forced, input.rerollCostChaos);
			if (atSeen && atSeen !== 'no-keepers') loopEvAtLastSeen = atSeen.R[h] - prices[h];
		}
	}

	const keeperHit = w.reduce((s, _, k) => s + (keep[k] ? P[h][k] : 0), 0);
	const keepersPerDivine = input.lifeforcePerDivine / input.lifeforcePerReroll / N[h];
	const divineLine =
		loopEv > DIVINE_LINE_MIN_EV_CHAOS && input.divineChaosRate > 0
			? (() => {
					const feeders = Math.ceil(input.divineChaosRate / loopEv);
					return {
						feeders,
						rerolls: Math.round(feeders * N[h]),
						lifeforce: Math.round(feeders * N[h] * input.lifeforcePerReroll)
					};
				})()
			: null;

	const typeResults: TypeResult[] = types.map((t, i) => ({
		itemId: t.itemId,
		name: t.name,
		shortName: t.shortName,
		weightShare: w[i],
		priceChaos: rawPrices[i],
		kept: keep[i],
		picked: picks[i],
		rerollValueChaos: R[i],
		loopEvChaos: R[i] - prices[i]
	}));

	const result: FamilyResult = {
		kind: 'ok',
		types: typeResults,
		keepers: types.filter((_, i) => keep[i]).map((t) => t.itemId),
		feeders: types.filter((_, i) => !keep[i]).map((t) => t.itemId),
		unpriced: types.filter((_, i) => unpriced[i]).map((t) => t.itemId),
		headline: {
			itemId: types[h].itemId,
			name: types[h].name,
			shortName: types[h].shortName,
			priceChaos: prices[h],
			loopEvChaos: loopEv,
			loopEvAtLastSeenChaos: loopEvAtLastSeen,
			keeperHitPerRoll: keeperHit,
			rerollsPerKeeper: N[h],
			lifeforcePerKeeper: input.lifeforcePerReroll * N[h],
			lifeforceChaosPerKeeper: input.rerollCostChaos * N[h],
			keepersPerDivine,
			keepersPerDivineWorthChaos: keepersPerDivine * loopEv
		},
		divineLine,
		rerollCostChaos: input.rerollCostChaos,
		rerollsPerDivine: input.lifeforcePerDivine / input.lifeforcePerReroll
	};
	// Finite inputs can still overflow (a huge rate × rerolls per keeper).
	return allFinite(result) ? result : { kind: 'no-data' };
}
