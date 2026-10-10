/**
 * Harvest Flipping data seam — the read models and ports the page reads, one
 * per data owner (handoff `fixtures.json` → `_owners`).
 *
 * Going live swaps the adapter behind a port and nothing else: the Currency
 * Exchange owns prices, lifeforce and the divine rate; the Harvest server data
 * owns weights, reroll cost and colour, and the tiers it caches per horizon.
 * Picks are desktop prefs, not a port here.
 *
 * Both ports are async even under the fixture, so the page runs the same
 * loading/generation control flow it will run against the server.
 */
import type { CurrencyExchangeHorizon } from '$lib/exchange/view';

export type LifeforceColour = 'Primal' | 'Vivid' | 'Wild';

/**
 * One item's Currency Exchange price. `chaos` is `null` when the window has no
 * price (an unpriced type); `lastSeenChaos` is the latest earlier price, read
 * only for an unpriced type (C1). The fixture is fully priced, so it is `null`.
 */
export interface ExchangeItemPrice {
	chaos: number | null;
	divine: number | null;
	lastSeenChaos: number | null;
}

export interface LifeforcePrice {
	itemId: string;
	/** The in-game colour name shown on the page ("purple", "blue", "yellow"). */
	colour: string;
	chaos: number;
	/** Lifeforce of this colour bought per 1 divine on the lifeforce/divine market. */
	perDivine: number;
}

/** Owner: Currency Exchange. */
export interface ExchangePriceRead {
	league: string;
	horizon: CurrencyExchangeHorizon;
	/** ISO time of the newest price; `null` before the first exchange hour. */
	lastUpdated: string | null;
	/** ISO hour the prices (and the tiers built on them) were read from. */
	priceHour: string;
	/** `false` while the server has not read an exchange hour (cold). */
	warm: boolean;
	divineChaosRate: number;
	/** Keyed by itemId; read with `Object.hasOwn`. */
	prices: Record<string, ExchangeItemPrice>;
	lifeforce: Record<LifeforceColour, LifeforcePrice>;
}

export interface HarvestWeightSample {
	/** `null` when no sample was logged (weights assumed uniform). */
	rolls: number | null;
	lifeforceSpent: number | null;
	date: string | null;
	source: string;
	/** Every type weighs the same; `loggedRolls` is then ignored. */
	uniform?: boolean;
}

export interface HarvestWeightType {
	itemId: string;
	name: string;
	shortName: string;
	loggedRolls: number | null;
}

export interface HarvestWeights {
	sample: HarvestWeightSample;
	types: HarvestWeightType[];
}

/**
 * The unified tier system run on the family's prices. TOP is not in `names`;
 * it is present when `byItem` maps an item to `"TOP"`.
 */
export interface HarvestTiers {
	boundariesChaos: number[];
	names: string[];
	/** `null` for some families (Delirium Orbs and Corrupted Essences in the fixture). */
	topBoundaryChaos: number | null;
	byItem: Record<string, string>;
	/** The exchange price hour these tiers were computed on. */
	priceHour: string;
}

export interface HarvestFamily {
	id: string;
	label: string;
	/** `null` for a family without logged weights. */
	lifeforce: LifeforceColour | null;
	/** Lifeforce per reroll; `null` for a family without logged weights. */
	rerollCost: number | null;
	/** `null`: no HarvestForge log yet — the tab shows "no data". */
	weights: HarvestWeights | null;
	tiers: HarvestTiers | null;
	note: string | null;
}

/** Owner: Harvest server data. */
export interface HarvestFamilyData {
	warm: boolean;
	families: HarvestFamily[];
}

export type LoadExchangePrices = (horizon: CurrencyExchangeHorizon) => Promise<ExchangePriceRead>;
export type LoadHarvestFamilies = (horizon: CurrencyExchangeHorizon) => Promise<HarvestFamilyData>;
