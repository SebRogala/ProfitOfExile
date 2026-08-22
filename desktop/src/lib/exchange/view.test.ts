import { describe, it, expect } from 'vitest';
import {
	CHAOS_ID,
	DIVINE_ID,
	HORIZON_OPTIONS,
	MODE_OPTIONS,
	REFETCH_DEBOUNCE_MS,
	REFETCH_JITTER_MS,
	SORT_OPTIONS,
	anyConvertStep,
	chaosIconPath,
	currencyIconPath,
	dataAgeParts,
	deriveState,
	formatChaos,
	formatGain,
	formatLegPrice,
	formatRoiPct,
	formatTime,
	formatTimeAgo,
	formatVolume,
	hoursProgress,
	iconSrc,
	moneyColumns,
	parseDensity,
	parseHorizon,
	parseMode,
	parseSort,
	parseUnit,
	quoteUnit,
	refetchDelay,
	routeSlots,
	runLedger,
	sortPlays,
	worthwhileScale
} from './view';
import type {
	CurrencyExchangeLeg,
	CurrencyExchangePlay,
	CurrencyExchangeResponse
} from '$lib/api';

/**
 * A warm response with no plays. Every `deriveState` case overrides only the
 * field it is about, so a test that reads as "warm + error" cannot be quietly
 * carrying a second difference.
 */
function response(overrides: Partial<CurrencyExchangeResponse> = {}): CurrencyExchangeResponse {
	return {
		league: 'Mirage',
		lastUpdated: '2026-08-19T12:00:00.000Z',
		from: '2026-08-18T12:00:00.000Z',
		to: '2026-08-19T12:00:00.000Z',
		hours: 24,
		warm: true,
		mode: 'all',
		horizon: 'recent',
		divineChaosRate: 198.97,
		count: 0,
		plays: [],
		// The full sixteen, as the wire guarantees on every served body — a
		// shorter stand-in would let a category-filter test pass against a
		// universe the server never sends.
		categories: [
			'Currency',
			'Essences',
			'Delve',
			'Scarabs',
			'Divination Cards',
			'Delirium',
			'Legion',
			'Fragments',
			'Oils',
			'Catalysts',
			'Omens',
			'Tattoos',
			'Expedition',
			'Harvest',
			'Runegrafts',
			'Allflame'
		],
		...overrides
	};
}

/**
 * A clean, ranked play whose BEST-CASE `roi` (10c) and measured `expectedRoi`
 * (4c) deliberately differ, and differ from every value a case overrides.
 *
 * The two re-sorting branches read different fields — `'roi'` the best-case
 * one, `'fastest'` the expectation through `worthwhileScale` — so a case about
 * one of them overrides that field only, and leaves the other at the factory
 * value across the whole fixture. A production swap between the two then reads
 * a column that says nothing about the row, and the answer stops being the one
 * the test asked for.
 *
 * The `'expected'` cases INVERT that idiom, and deliberately: that branch sorts
 * by nothing at all, so a fixture tied on the untested field would let a
 * mutation that lost the branch fall through to a comparator, tie, and hand
 * back the served order anyway. Those cases vary both fields instead.
 *
 * `expectedRoiPct` is NOT `expectedRoi / investment` (4/200 would be 0.02): the
 * wire's expectation pair carries no such identity — each is a mean over the
 * simulated entries, each with its own chased outlay — unlike
 * `roi === roiPct * investment`, which the best-case pair does hold to
 * (POE-193).
 */
function play(overrides: Partial<CurrencyExchangePlay> = {}): CurrencyExchangePlay {
	return {
		key: 'direct:divine:chaos',
		mode: 'direct',
		legs: [],
		roiPct: 0.05,
		edge: 0.05,
		roiPctRaw: 0.08,
		roi: 10,
		investment: 200,
		expectedRoi: 4,
		expectedRoiPct: 0.015,
		simEntries: 22,
		lowCoverage: false,
		turnover: 5000,
		tick: 0.005,
		depth: 40,
		suspect: false,
		hoursSeen: 6,
		lastHour: '2026-08-19T11:00:00.000Z',
		...overrides
	};
}

// -------------------------------------------------------- the wire fixtures --

/**
 * ONE fixture set for every suite that reads a row's mechanical numbers.
 *
 * `runLedger`, `routeSlots` and the closure suite all print out of the same
 * three roots — `investment`, `roi` and the legs — so a fixture that is right
 * for one of them is right for all three, and two copies of "the scarab play"
 * with different `roi` values would let one suite pin a row the other says is
 * impossible. Per §7 of `docs/CURRENCY-EXCHANGE-ROW-INVARIANT.md` there is one
 * set, wire-consistent, and each matrix is built from it by overriding
 * `expectedRoi`.
 */

/** The rate every divine-entry fixture below is read back into chaos with. */
const DIVINE_RATE = 200;
/** What the server puts on a chaos-quoted leg's `quoteIcon`. */
const CHAOS_ICON = '/currency-exchange/icon/Chaos';
/** What the server puts on a divine-quoted leg's `quoteIcon`. */
const DIVINE_ICON = '/currency-exchange/icon/Divine';
/** The quote side of any leg traded against divine. */
const DIVINE_SIDE = { quote: DIVINE_ID, quoteName: 'Divine Orb', quoteIcon: DIVINE_ICON };

/**
 * A wire leg. The pair and the price are ONE fact on the wire —
 * `priceQuoteQty / priceItemQty === price` exactly — so every fixture below
 * overrides all three together, and the factory's own default 1-for-1 is 1c. A
 * fixture whose pair contradicted its price would let a rate read right for the
 * wrong reason.
 */
function leg(overrides: Partial<CurrencyExchangeLeg> = {}): CurrencyExchangeLeg {
	return {
		action: 'buy',
		item: 'scarab',
		quote: CHAOS_ID,
		price: 1,
		priceItemQty: 1,
		priceQuoteQty: 1,
		fair: 1.1,
		fairOk: true,
		tick: 0.01,
		volume: 2000,
		stock: 40,
		suspect: false,
		itemName: 'Ambush Scarab',
		itemIcon: '/icon/Scarab',
		itemCategory: 'Scarabs',
		quoteName: 'Chaos Orb',
		quoteIcon: CHAOS_ICON,
		quoteCategory: 'Currency',
		...overrides
	};
}

// The plays below are WIRE-CONSISTENT, per §7 of the invariant spec:
// `investment` is `u0 · r`, `roiPct` is the server's own formula over the legs'
// UNDERCUT prices (`internal/exchange/plays.go:1083-1092`), and `roi` is
// `investment · roiPct`. A fixture that contradicted those identities could make
// an assertion pass or fail for a reason that has nothing to do with the code —
// and since every string on the row is now read out of those three together, an
// `investment` that contradicts its own buy leg pins a row no server could send.
//
// The PAIR is pinned from the readable end: each fixture states `roi` as the
// round decimal it wants the row to carry, and `roiPct` is `roi / investment` —
// the back-solve, so that `investment · roiPct` returns exactly that `roi` in
// float. Stating `roiPct` as the float `u1/u0 − 1` instead would land an ulp
// away from the back-solved literal, and the product would then miss the round
// `roi` by an ulp too. Each fixture's comment shows the formula the wire
// computes and the value it lands on, to the digits where the two agree.

/**
 * Chaos-entry direct: a scarab bought at 19c and sold back at 21c on the one
 * market.
 *
 * Worked by hand: `u0 = 19 × 1.01 = 19.19`, `u1 = 21 × 0.99 = 20.79`, and the
 * entry is chaos so `r = 1` and `investment = u0 = 19.19`.
 * `roiPct = u1/u0 − 1 = 0.0833767…`, and the gain the fixture pins is
 * `roi = 1.6`, so the literal `roiPct` carries is `1.6 / 19.19` — the same
 * value to the digits shown, an ulp off the float division.
 * `expectedRoi` 0.5 needs `ceil(100 / 0.5)` = 200 exchanges to clear the 100c
 * target, gaining `0.5 × 200` = 100c.
 */
function chaosScarab(overrides: Partial<CurrencyExchangePlay> = {}): CurrencyExchangePlay {
	return play({
		key: 'direct:scarab:chaos',
		legs: [
			leg({ price: 19, priceItemQty: 1, priceQuoteQty: 19 }),
			leg({ action: 'sell', price: 21, priceItemQty: 1, priceQuoteQty: 21 })
		],
		roiPct: 0.08337675872850443,
		edge: 0.08337675872850443,
		roiPctRaw: 0.10526315789473684,
		investment: 19.19,
		roi: 1.6,
		expectedRoi: 0.5,
		...overrides
	});
}

/**
 * Divine-entry direct: the same scarab, quoted in divine on both sides — the
 * mirror route that used to read chaos → chaos → chaos with the divine
 * appearing nowhere on the row.
 *
 * `u0 = 0.0625 × 1.01 = 0.063125` div, `u1 = 0.1 × 0.99 = 0.099` div, and at
 * `r = 200` chaos a divine `investment = u0 · r = 12.625`c.
 * `roiPct = u1/u0 − 1 = 0.5683168…`, and the gain the fixture pins is
 * `roi = 7.175`c — the same figure `r · (u1 − u0)` states — so the literal
 * `roiPct` carries is `7.175 / 12.625`. Both float routes to the gain miss
 * 7.175 by an ulp in opposite directions, which is why the round gain is the
 * pinned end of the pair. `expectedRoi` 2 needs 50 exchanges, gaining 100c.
 */
function divineScarab(overrides: Partial<CurrencyExchangePlay> = {}): CurrencyExchangePlay {
	return play({
		key: 'direct:scarab:divine',
		legs: [
			leg({ ...DIVINE_SIDE, price: 0.0625, priceItemQty: 16, priceQuoteQty: 1 }),
			leg({ ...DIVINE_SIDE, action: 'sell', price: 0.1, priceItemQty: 10, priceQuoteQty: 1 })
		],
		roiPct: 0.5683168316831683,
		edge: 0.5683168316831683,
		roiPctRaw: 0.6,
		investment: 12.625,
		roi: 7.175,
		expectedRoi: 2,
		...overrides
	});
}

/** The omen the screenshot case trades, on all three of its markets. */
const OMEN = { item: 'omen', itemName: 'Omen of Amelioration', itemIcon: '/icon/Omen' };

/** Bought with chaos on a market that posts one omen at a time for 35c. */
function omenBuy(overrides: Partial<CurrencyExchangeLeg> = {}): CurrencyExchangeLeg {
	return leg({ ...OMEN, price: 35, priceItemQty: 1, priceQuoteQty: 35, ...overrides });
}

/** Sold against divine on a market that posts four omens for one divine. */
function omenSell(overrides: Partial<CurrencyExchangeLeg> = {}): CurrencyExchangeLeg {
	return leg({
		...OMEN,
		...DIVINE_SIDE,
		action: 'sell',
		price: 0.25,
		priceItemQty: 4,
		priceQuoteQty: 1,
		...overrides
	});
}

/** The divine proceeds turned back into chaos: one divine for 209c. */
function omenConvert(overrides: Partial<CurrencyExchangeLeg> = {}): CurrencyExchangeLeg {
	return leg({
		action: 'sell',
		item: DIVINE_ID,
		itemName: 'Divine Orb',
		itemIcon: DIVINE_ICON,
		itemCategory: 'Currency',
		price: 209,
		priceItemQty: 1,
		priceQuoteQty: 209,
		...overrides
	});
}

/**
 * Chaos-entry 1-hop — the screenshot case: buy the omen for chaos, sell it
 * against divine, convert the divine back into chaos. The triangle whose third
 * leg is the only place `u2` is read, and whose three steps carry three
 * different pairs.
 *
 * `u0 = 35 × 1.01 = 35.35`c, `u1 = 0.25 × 0.99 = 0.2475` div,
 * `u2 = 209 × 0.99 = 206.91`c a divine — leg 3 is a `sell` on the wire, which
 * is why its undercut takes the minus form. `r = 1`, so
 * `investment = 35.35`, `roiPct = u1·u2/u0 − 1 = 0.4486626…` and
 * `roi = investment · roiPct = 15.860225`. `roiPctRaw` is that same formula
 * over the RAW prices, `0.25 × 209 / 35 − 1 = 0.4928571…`. `expectedRoi` 8.5
 * needs `ceil(100 / 8.5)` = 12 exchanges, gaining `8.5 × 12` = 102c.
 */
function omen(overrides: Partial<CurrencyExchangePlay> = {}): CurrencyExchangePlay {
	return play({
		key: '1-hop:omen:divine',
		mode: '1-hop',
		legs: [omenBuy(), omenSell(), omenConvert()],
		roiPct: 0.44866265912305514,
		edge: 0.44866265912305514,
		roiPctRaw: 0.4928571428571429,
		investment: 35.35,
		roi: 15.860225,
		expectedRoi: 8.5,
		...overrides
	});
}

/**
 * The omen triangle whose SELL market posts five omens at a time for two divine.
 *
 * The one fixture here that is deliberately NOT wire-consistent, and the reason
 * is what the case is about. Its money fields are `omen`'s, which the server
 * computed at `u1 = 0.25 × 0.99`, while its sell leg prints 0.4 — the leg is
 * internally consistent (`2 / 5 = 0.4`), the play is not. Reconciling them would
 * move `roiPct`, `roi` and `expectedRoi` together and with them the flip count,
 * so the run would stop being 12 and become whatever the arithmetic landed on;
 * the case needs a run of 12 against a lot of 5, because 5 is the lot that
 * cannot divide it.
 *
 * The inconsistency cannot reach the strings under test. `stepRate` takes its
 * total from the ledger — which reads `investment`, `roi` and the CONVERT leg's
 * undercut, never this leg's price — and `marketPair` takes the lot from the
 * leg's pair. So the sell line still totals the run at 2.97 div, exactly as the
 * plain four-at-a-time market does, and the only thing the override changes is
 * the lot the hover has to report.
 */
function chunkyLot(overrides: Partial<CurrencyExchangePlay> = {}): CurrencyExchangePlay {
	return omen({
		legs: [omenBuy(), omenSell({ price: 0.4, priceItemQty: 5, priceQuoteQty: 2 }), omenConvert()],
		...overrides
	});
}

/**
 * Divine-entry 1-hop: buy the scarab against DIVINE, sell it against chaos,
 * then convert the chaos back into divine. The triangle whose every step is
 * quoted in a different currency from the step before it, which is what makes
 * it the one shape that can catch a convert slot reading its leg's quote
 * instead of its item.
 *
 * `u0 = 0.0625 × 1.01 = 0.063125` div, `u1 = 14 × 0.99 = 13.86`c,
 * `u2 = 0.005 × 0.99 = 0.00495` div an orb — 200 chaos for one divine, which is
 * the same rate `DIVINE_RATE` states. `r = 200`, so `investment = u0 · r =
 * 12.625`c and `roi = r · (u1·u2 − u0) = 1.0964`c; the literal `roiPct` is the
 * back-solve `1.0964 / 12.625`, matching `u1·u2/u0 − 1 = 0.0868435…`.
 * `expectedRoi` 2 needs 50 exchanges, gaining 100c.
 *
 * Its ROI (55c across the run) lands BELOW its Exp. ROI (100c), so its chain
 * end prints below its Get — the deviation of spec §5 running the other way.
 * Deliberate: field measurement has the best case overstating the measurement
 * four to eight times, but nothing in the arithmetic guarantees the sign, and
 * this is the fixture that catches any emitter, formatter or assertion that
 * quietly assumes `chainEnd > get`.
 */
function divineOneHop(overrides: Partial<CurrencyExchangePlay> = {}): CurrencyExchangePlay {
	return play({
		key: '1-hop:scarab:chaos',
		mode: '1-hop',
		legs: [
			leg({ ...DIVINE_SIDE, price: 0.0625, priceItemQty: 16, priceQuoteQty: 1 }),
			leg({ action: 'sell', price: 14, priceItemQty: 1, priceQuoteQty: 14 }),
			leg({
				...DIVINE_SIDE,
				action: 'sell',
				item: CHAOS_ID,
				itemName: 'Chaos Orb',
				itemIcon: CHAOS_ICON,
				itemCategory: 'Currency',
				price: 0.005,
				priceItemQty: 200,
				priceQuoteQty: 1
			})
		],
		roiPct: 0.08684356435643564,
		edge: 0.08684356435643564,
		roiPctRaw: 0.12,
		investment: 12.625,
		roi: 1.0964,
		expectedRoi: 2,
		...overrides
	});
}

/**
 * Chaos-entry 1-hop: buy the astrolabe in chaos, sell it in divine, convert the
 * divine back. The second chaos-entry triangle, at a size the omen never
 * reaches — its run costs over a thousand chaos, which is where the thousands
 * grouping meets a step total.
 *
 * `u0 = 50 × 1.01 = 50.5`c, `u1 = 0.28 × 0.99 = 0.2772` div,
 * `u2 = 204 × 0.99 = 201.96`c a divine; `r = 1`, so `investment = 50.5` and
 * `roi = u1·u2 − u0 = 5.483312`. `expectedRoi` is the factory's 4c, which needs
 * `ceil(100 / 4)` = 25 exchanges and gains 100c.
 */
function oneHop(overrides: Partial<CurrencyExchangePlay> = {}): CurrencyExchangePlay {
	const astrolabe = { item: 'astrolabe', itemName: 'Nameless Astrolabe' };
	return play({
		key: '1-hop:astrolabe:divine',
		mode: '1-hop',
		legs: [
			leg({ ...astrolabe, price: 50, priceItemQty: 1, priceQuoteQty: 50 }),
			leg({
				...astrolabe,
				...DIVINE_SIDE,
				action: 'sell',
				price: 0.28,
				priceItemQty: 25,
				priceQuoteQty: 7
			}),
			leg({
				action: 'sell',
				item: DIVINE_ID,
				itemName: 'Divine Orb',
				itemIcon: DIVINE_ICON,
				itemCategory: 'Currency',
				price: 204,
				priceItemQty: 1,
				priceQuoteQty: 204
			})
		],
		roiPct: 0.10858043564356436,
		edge: 0.10858043564356436,
		roiPctRaw: 0.1424,
		investment: 50.5,
		roi: 5.483312,
		...overrides
	});
}

/**
 * The omen triangle with a convert leg that came through with no usable price —
 * the version skew the forward fallback exists for.
 *
 * The money fields stay the consistent ones: `roiPct`, `roi` and `investment`
 * are what the server computed in the hour it DID price that leg, and the skew
 * is on the leg alone. `chainEnd` is `I + R` and never touched `u2`, so it is
 * unaffected; only the sale's own total loses its backward reading.
 */
function skewedConvert(price: number): CurrencyExchangePlay {
	return omen({ legs: [omenBuy(), omenSell(), unpriced(omenConvert(), price)] });
}

/**
 * A leg whose price came through unusable, with the quantity pair dropped
 * alongside it: the pair and the price are ONE fact (see `leg`), so a leg that
 * lost its price lost the market ratio the pair states too. Leaving the priced
 * fixture's 1-for-209 behind would put a readable ratio on the very leg the
 * fixture is calling unpriced.
 */
function unpriced(base: CurrencyExchangeLeg, price: number): CurrencyExchangeLeg {
	return leg({ ...base, price, priceItemQty: 0, priceQuoteQty: 0 });
}

describe('parseMode', () => {
	it('passes "all" through', () => {
		expect(parseMode('all')).toBe('all');
	});

	it('passes "direct" through', () => {
		expect(parseMode('direct')).toBe('direct');
	});

	it('passes "1-hop" through', () => {
		expect(parseMode('1-hop')).toBe('1-hop');
	});

	it('falls back to "all" for a mode the server would reject with a 400', () => {
		expect(parseMode('bogus')).toBe('all');
	});

	it('falls back to "all" for an unset preference', () => {
		// `persisted()` hands back the raw stored string, so a preference written
		// by an older build (or never written at all) arrives here as "".
		expect(parseMode('')).toBe('all');
	});
});

describe('MODE_OPTIONS', () => {
	it('offers the three server modes, labelled, in picker order', () => {
		// The values are wire values: SegmentedButtons hands the selected one
		// straight to fetchCurrencyExchangePlays, so a prettified value ("1 hop")
		// would be a 400 rather than a cosmetic slip.
		expect(MODE_OPTIONS).toEqual([
			{ value: 'all', label: 'All' },
			{ value: 'direct', label: 'Direct' },
			{ value: '1-hop', label: '1-hop' }
		]);
	});
});

describe('parseHorizon', () => {
	it('passes "recent" through', () => {
		expect(parseHorizon('recent')).toBe('recent');
	});

	it('passes "day" through', () => {
		expect(parseHorizon('day')).toBe('day');
	});

	it('falls back to "recent" for a horizon the server would reject with a 400', () => {
		expect(parseHorizon('week')).toBe('recent');
	});

	it('falls back to "recent" for an unset preference', () => {
		// The window the page fetched before the toggle existed, so a preference
		// written by an older build resolves to what that build already showed.
		expect(parseHorizon('')).toBe('recent');
	});
});

describe('HORIZON_OPTIONS', () => {
	it('offers the two server horizons, labelled with their window length', () => {
		// Wire values: the selected one goes to fetchCurrencyExchangePlays as a
		// query param, so a prettified value would be a 400 rather than a slip.
		expect(HORIZON_OPTIONS).toEqual([
			{ value: 'recent', label: 'Recent 6h' },
			{ value: 'day', label: 'Day 24h' }
		]);
	});
});

describe('parseSort', () => {
	it('passes "expected" through', () => {
		expect(parseSort('expected')).toBe('expected');
	});

	it('passes "roi" through', () => {
		expect(parseSort('roi')).toBe('roi');
	});

	it('passes "fastest" through', () => {
		expect(parseSort('fastest')).toBe('fastest');
	});

	it('reads a stored "fill" as the fastest order rather than dropping to the default', () => {
		// The Fill order was renamed, not removed (POE-192): a reader who left the
		// picker on it asked for the shortest wait, and falling back to the served
		// order would silently re-rank their table on the next launch.
		expect(parseSort('fill')).toBe('fastest');
	});

	it('reads a stored "roiPct" as the served order rather than dropping to the default', () => {
		// The same forward mapping for the other renamed pick (POE-193): that name
		// meant "the list as the server ranked it", and the server now ranks on
		// `expectedRoi`. Falling back would land on the same order by accident
		// today and stop doing so the moment the default moves.
		expect(parseSort('roiPct')).toBe('expected');
	});

	it('falls back to "expected" for an unknown sort', () => {
		expect(parseSort('turnover')).toBe('expected');
	});

	it('falls back to "expected" for an unset preference', () => {
		// The server's own ranking is the default order, so an unset preference
		// leaves the list exactly as served — tie-breaks included.
		expect(parseSort('')).toBe('expected');
	});
});

describe('SORT_OPTIONS', () => {
	it('offers the three orders the table can be read in, labelled, in picker order', () => {
		expect(SORT_OPTIONS).toEqual([
			{ value: 'expected', label: 'Exp. ROI' },
			{ value: 'roi', label: 'ROI' },
			{ value: 'fastest', label: 'Fastest' }
		]);
	});
});

describe('parseDensity', () => {
	it('passes "comfortable" through', () => {
		expect(parseDensity('comfortable')).toBe('comfortable');
	});

	it('passes "dense" through', () => {
		expect(parseDensity('dense')).toBe('dense');
	});

	it('falls back to "comfortable" for an unknown density', () => {
		expect(parseDensity('compact')).toBe('comfortable');
	});

	it('falls back to "comfortable" for an unset preference', () => {
		expect(parseDensity('')).toBe('comfortable');
	});
});

describe('parseUnit', () => {
	it('passes "chaos" through', () => {
		expect(parseUnit('chaos')).toBe('chaos');
	});

	it('passes "divine" through', () => {
		expect(parseUnit('divine')).toBe('divine');
	});

	it('falls back to "chaos" for a currency the bounds cannot be converted to', () => {
		expect(parseUnit('exalted')).toBe('chaos');
	});

	it('falls back to "chaos" for an unset preference', () => {
		// Chaos is what every wire number is denominated in, so it is the one unit
		// that stays readable when divineChaosRate is 0.
		expect(parseUnit('')).toBe('chaos');
	});
});

describe('deriveState', () => {
	const now = new Date('2026-08-19T12:00:00.000Z');

	it('reports loading while the first fetch is still in flight', () => {
		expect(deriveState({ result: null, lastFetchedAt: null, lastError: null, now })).toEqual({
			kind: 'loading'
		});
	});

	it('reports unreachable when the first fetch failed and there is nothing to show', () => {
		expect(
			deriveState({ result: null, lastFetchedAt: null, lastError: 'connection refused', now })
		).toEqual({ kind: 'unreachable' });
	});

	it('reports stale with the age of the last good fetch when a later fetch failed', () => {
		const state = deriveState({
			result: response(),
			lastFetchedAt: new Date('2026-08-19T11:58:00.000Z'),
			lastError: 'connection refused',
			now
		});

		expect(state).toEqual({ kind: 'stale', staleSince: '2 min ago' });
	});

	it('dates a stale result from now when no fetch time was recorded', () => {
		// Not a real path today, but the fallback is what keeps the sentence from
		// reading "stale since  — server unreachable" if one appears.
		const state = deriveState({
			result: response(),
			lastFetchedAt: null,
			lastError: 'connection refused',
			now
		});

		expect(state).toEqual({ kind: 'stale', staleSince: 'just now' });
	});

	it('reports warming while the server has not closed a Currency Exchange hour', () => {
		expect(
			deriveState({ result: response({ warm: false }), lastFetchedAt: now, lastError: null, now })
		).toEqual({ kind: 'warming' });
	});

	it('reports stale rather than warming when a cold result is also out of date', () => {
		// An error outranks warming: "waiting for the first hour" would hide the
		// fact that the last request never landed.
		const state = deriveState({
			result: response({ warm: false }),
			lastFetchedAt: new Date('2026-08-19T11:00:00.000Z'),
			lastError: 'connection refused',
			now
		});

		expect(state).toEqual({ kind: 'stale', staleSince: '1 h ago' });
	});

	it('reports ready with the age of the server-side computation', () => {
		// `updatedAgo` comes from result.lastUpdated (when the server computed the
		// hour), not from lastFetchedAt (when this client asked) — the two differ
		// by up to a full hour and only the first is what the header claims.
		const state = deriveState({
			result: response({ lastUpdated: '2026-08-19T09:00:00.000Z' }),
			lastFetchedAt: new Date('2026-08-19T11:59:59.000Z'),
			lastError: null,
			now
		});

		expect(state).toEqual({ kind: 'ready', updatedAgo: '3 h ago' });
	});

	it('omits the age on a ready result the server could not timestamp', () => {
		const state = deriveState({
			result: response({ lastUpdated: null }),
			lastFetchedAt: now,
			lastError: null,
			now
		});

		expect(state).toEqual({ kind: 'ready', updatedAgo: undefined });
	});
});

describe('formatTimeAgo', () => {
	const now = new Date('2026-08-19T12:00:00.000Z');

	function ago(ms: number): string {
		return formatTimeAgo(new Date(now.getTime() - ms), now);
	}

	it('says "just now" for the current instant', () => {
		expect(ago(0)).toBe('just now');
	});

	it('says "just now" one second before the first minute closes', () => {
		expect(ago(59_000)).toBe('just now');
	});

	it('switches to minutes exactly on the first minute', () => {
		expect(ago(60_000)).toBe('1 min ago');
	});

	it('switches to hours exactly on the first hour', () => {
		expect(ago(60 * 60_000)).toBe('1 h ago');
	});

	it('reports whole hours below a day', () => {
		expect(ago(2 * 60 * 60_000)).toBe('2 h ago');
	});

	it('switches to days exactly on the first day', () => {
		expect(ago(24 * 60 * 60_000)).toBe('1 d ago');
	});

	it('reads an ISO string as well as a Date', () => {
		// deriveState passes result.lastUpdated (a wire string) for `ready` and a
		// Date for `stale`; both go through this one function.
		expect(formatTimeAgo('2026-08-19T09:30:00.000Z', now)).toBe('2 h ago');
	});

	it('renders nothing for a missing timestamp', () => {
		expect(formatTimeAgo(null, now)).toBe('');
	});

	it('renders nothing rather than "NaN min ago" for an unparseable timestamp', () => {
		expect(formatTimeAgo('not-a-date', now)).toBe('');
	});

	it('reads a timestamp ahead of the local clock as "just now", not a negative age', () => {
		expect(formatTimeAgo(new Date(now.getTime() + 5 * 60_000), now)).toBe('just now');
	});
});

describe('formatTime', () => {
	it('renders a local 24-hour clock reading', () => {
		expect(formatTime(new Date(2026, 7, 19, 14, 35).toISOString())).toBe('14:35');
	});

	it('zero-pads both the hour and the minute', () => {
		expect(formatTime(new Date(2026, 7, 19, 9, 5).toISOString())).toBe('09:05');
	});

	it('renders midnight as 00:00 rather than 24:00 or 12:00', () => {
		expect(formatTime(new Date(2026, 7, 19, 0, 0).toISOString())).toBe('00:00');
	});

	it('renders nothing for a missing timestamp', () => {
		expect(formatTime(null)).toBe('');
	});

	it('renders nothing rather than "NaN:NaN" for an unparseable timestamp', () => {
		expect(formatTime('not-a-date')).toBe('');
	});
});

describe('formatRoiPct', () => {
	it('renders a positive return as a signed percentage with one decimal', () => {
		// The wire value is a FRACTION — 0.1234 is 12.34%, not 0.12% — so the
		// formatter multiplies. Pointing it at a gem roiPct (already percentage
		// points) would inflate the number a hundredfold.
		expect(formatRoiPct(0.1234)).toBe('+12.3%');
	});

	it('renders a negative return with a minus sign', () => {
		expect(formatRoiPct(-0.05)).toBe('-5.0%');
	});

	it('renders a zero return as +0.0%', () => {
		expect(formatRoiPct(0)).toBe('+0.0%');
	});

	it('renders a return that rounds away to nothing as +0.0%, never -0.0%', () => {
		expect(formatRoiPct(-0.00001)).toBe('+0.0%');
	});
});

describe('dataAgeParts', () => {
	// Built from local components, like the formatTime cases: formatTime reads a
	// local clock, so an ISO literal would assert a different hour per timezone.
	const hourEnd = new Date(2026, 7, 19, 11, 0);
	const computedAt = new Date(2026, 7, 19, 11, 45);
	const now = new Date(2026, 7, 19, 12, 0);

	it('dates the badge from the end of the settled hour, not from the ranking', () => {
		// The feed publishes 40-60 min after an hour closes, so `to` and
		// `lastUpdated` differ by most of an hour — and only `to` answers "how old
		// are these prices", which is what the badge claims.
		expect(
			dataAgeParts(
				response({ to: hourEnd.toISOString(), lastUpdated: computedAt.toISOString() }),
				now
			)
		).toEqual({ label: 'as of 11:00', ago: '1 h ago' });
	});

	it('falls back to the ranking time for a body served without a window', () => {
		expect(
			dataAgeParts(response({ to: null, lastUpdated: computedAt.toISOString() }), now)
		).toEqual({ label: 'as of 11:45', ago: '15 min ago' });
	});

	it('renders no badge for a body carrying neither timestamp', () => {
		expect(dataAgeParts(response({ to: null, lastUpdated: null }), now)).toBeNull();
	});

	it('renders no badge rather than "as of " for a timestamp that will not parse', () => {
		expect(dataAgeParts(response({ to: 'not-a-date' }), now)).toBeNull();
	});

	it('renders no badge before the first fetch has landed', () => {
		expect(dataAgeParts(null, now)).toBeNull();
	});
});

describe('sortPlays', () => {
	function keys(plays: CurrencyExchangePlay[]): string[] {
		return plays.map((p) => p.key);
	}

	it('leaves the server ranking untouched under the Exp. ROI sort', () => {
		// The served order already carries the low-coverage band and expectedRoi
		// desc plus turnover, direct-first and key tie-breaks (POE-193);
		// re-sorting on expectedRoi alone would discard them.
		//
		// BOTH money fields vary, and out of order in the same direction, because
		// this case has two ways to go wrong and a tie would hide one of them:
		// re-sorting on expectedRoi desc answers b,c,a, and losing the branch
		// entirely — falling through to the roi comparator — answers b,c,a as
		// well. Either way the served order is gone.
		const served = [
			play({ key: 'a', roi: 5, expectedRoi: 2 }),
			play({ key: 'b', roi: 500, expectedRoi: 50 }),
			play({ key: 'c', roi: 50, expectedRoi: 20 })
		];

		expect(keys(sortPlays(served, 'expected'))).toEqual(['a', 'b', 'c']);
	});

	it('orders by chaos gained per exchange under the ROI sort', () => {
		const served = [
			play({ key: 'a', roi: 5 }),
			play({ key: 'b', roi: 500 }),
			play({ key: 'c', roi: 50 })
		];

		expect(keys(sortPlays(served, 'roi'))).toEqual(['b', 'c', 'a']);
	});

	it('orders by the run-priced ROI the column shows, not by the per-exchange one', () => {
		// The two orders genuinely disagree here, which is the whole point: `a`
		// gains 5c an exchange but its 1c expectation takes 100 flips to clear the
		// target, so its ROI column reads +500c; `b` gains ten times as much per
		// exchange and clears in 2, so its column reads +100c. A table ordered by
		// the wire's `roi` would put `b` on top of a number half `a`'s.
		const served = [
			play({ key: 'a', roi: 5, expectedRoi: 1 }),
			play({ key: 'b', roi: 50, expectedRoi: 50 })
		];

		expect(keys(sortPlays(served, 'roi'))).toEqual(['a', 'b']);
	});

	it('keeps a suspect play behind every clean one even when its ROI is the largest', () => {
		// The flag is the reason the server ranks it last: its price sits outside
		// the fair band, so the big number is the part not to trust.
		const served = [
			play({ key: 'clean-small', roi: 5 }),
			play({ key: 'clean-big', roi: 50 }),
			play({ key: 'suspect-huge', roi: 5000, suspect: true })
		];

		expect(keys(sortPlays(served, 'roi'))).toEqual([
			'clean-big',
			'clean-small',
			'suspect-huge'
		]);
	});

	it('keeps the server order between two plays tied on ROI', () => {
		const served = [play({ key: 'first', roi: 40 }), play({ key: 'second', roi: 40 })];

		expect(keys(sortPlays(served, 'roi'))).toEqual(['first', 'second']);
	});

	it('sorts into a new array rather than reordering the fetched list', () => {
		// The page holds the fetched list in reactive state and re-derives the sort
		// from it; an in-place sort would make the Exp. ROI option unable to
		// restore the server ranking without a refetch.
		const served = [play({ key: 'a', roi: 5 }), play({ key: 'b', roi: 500 })];

		sortPlays(served, 'roi');

		expect(keys(served)).toEqual(['a', 'b']);
	});

	it("hands out a copy under the Exp. ROI sort too, never the caller's own array", () => {
		// The Exp. ROI branch keeps the served order but must not alias the
		// response's array: the page mutating the sorted list (or Svelte state
		// wrapping it) would otherwise write through to the fetched result.
		//
		// Both fields vary for the reason the order case above gives — the copy
		// has to be a copy of the SERVED list, and a tied fixture would let a
		// lost branch return a re-sorted array that happens to read the same.
		const served = [
			play({ key: 'a', roi: 5, expectedRoi: 2 }),
			play({ key: 'b', roi: 500, expectedRoi: 50 })
		];

		const sorted = sortPlays(served, 'expected');

		expect(sorted).not.toBe(served);
		expect(keys(sorted)).toEqual(['a', 'b']);
	});

	it('puts the play the market absorbs soonest first under the Fastest sort', () => {
		// Ascending, not descending: the question the Scale column answers is how
		// long the worthwhile size waits, and the shortest wait is the best row.
		// 100 flips at 10/h is 10 hours; 10 flips at 5/h is 2; 10 at 4000/h is 1.
		//
		// The scale is counted off the EXPECTATION (POE-193), so that is the field
		// each row varies; every one of them keeps the fixture's single `roi`, at
		// which the target divides into ten flips for all three alike. A scale
		// counted off that number would leave only `depth` to tell the rows apart
		// and answer slow, fast, middling — the thin book first, because ten flips
		// at 10/h and ten at 4000/h both round up to the same single hour.
		const served = [
			play({ key: 'slow', expectedRoi: 1, depth: 10 }),
			play({ key: 'fast', expectedRoi: 10, depth: 4000 }),
			play({ key: 'middling', expectedRoi: 10, depth: 5 })
		];

		expect(keys(sortPlays(served, 'fastest'))).toEqual(['fast', 'middling', 'slow']);
	});

	it('puts a play whose depth cannot be read last under the Fastest sort', () => {
		// An unreadable depth is an unknown wait, not a zero one — sorting it to
		// the front would put the least-known row at the top of the list.
		const served = [
			play({ key: 'unreadable', depth: 0 }),
			play({ key: 'slow', expectedRoi: 1, depth: 10 }),
			play({ key: 'fast', expectedRoi: 10, depth: 4000 })
		];

		expect(keys(sortPlays(served, 'fastest'))).toEqual(['fast', 'slow', 'unreadable']);
	});

	it('puts a play with no worthwhile scale last under the Fastest sort', () => {
		// A play the simulation expects nothing from never reaches the target, so
		// it has no wait to compare — the same "unknown, not instant" reading a
		// dead depth gets, and the branch a scale-less play would otherwise sort
		// by NaN. Since POE-193 this is a row the server really serves.
		const served = [
			play({ key: 'no-scale', expectedRoi: 0, depth: 4000 }),
			play({ key: 'slow', expectedRoi: 1, depth: 10 }),
			play({ key: 'fast', expectedRoi: 10, depth: 4000 })
		];

		expect(keys(sortPlays(served, 'fastest'))).toEqual(['fast', 'slow', 'no-scale']);
	});

	it('keeps the served order between two plays with no readable wait at all', () => {
		// Two unknowns compare as equal — a comparator that claims either one
		// precedes the other is non-reflexive, and the sort is free to act on
		// the lie in any order it likes.
		const served = [
			play({ key: 'first-unknown', depth: 0 }),
			play({ key: 'second-unknown', expectedRoi: 0, depth: 4000 })
		];

		expect(keys(sortPlays(served, 'fastest'))).toEqual(['first-unknown', 'second-unknown']);
	});

	it('keeps a clean play with an unreadable wait ahead of every suspect one', () => {
		// The suspect partition outranks the null-last rule: a clean unknown is
		// still a clean row, and the flag is the reason a suspect play sits last.
		const served = [
			play({ key: 'suspect-fast', expectedRoi: 100, depth: 100_000, suspect: true }),
			play({ key: 'clean-unreadable', depth: 0 })
		];

		expect(keys(sortPlays(served, 'fastest'))).toEqual(['clean-unreadable', 'suspect-fast']);
	});

	it('keeps a suspect play behind every clean one under the Fastest sort, however fast it absorbs', () => {
		const served = [
			play({ key: 'clean-slow', expectedRoi: 1, depth: 10 }),
			play({ key: 'suspect-instant', expectedRoi: 100, depth: 100_000, suspect: true }),
			play({ key: 'clean-quick', expectedRoi: 10, depth: 900 })
		];

		expect(keys(sortPlays(served, 'fastest'))).toEqual([
			'clean-quick',
			'clean-slow',
			'suspect-instant'
		]);
	});

	it('keeps the server order between two plays the market absorbs inside the same hour', () => {
		// The hours are whole ones, so everything under an hour ties at 1 — the
		// deeper book does NOT out-sort the thinner one, it keeps the server's
		// remaining tie-breaks.
		const served = [
			play({ key: 'first', expectedRoi: 10, depth: 900 }),
			play({ key: 'second', expectedRoi: 10, depth: 4000 })
		];

		expect(keys(sortPlays(served, 'fastest'))).toEqual(['first', 'second']);
	});

	it('sorts into a new array under the Fastest sort as well', () => {
		const served = [
			play({ key: 'slow', expectedRoi: 1, depth: 10 }),
			play({ key: 'fast', expectedRoi: 10, depth: 4000 })
		];

		sortPlays(served, 'fastest');

		expect(keys(served)).toEqual(['slow', 'fast']);
	});
});

describe('formatVolume', () => {
	it('renders zero volume as "0"', () => {
		expect(formatVolume(0)).toBe('0');
	});

	it('renders a two-digit volume verbatim', () => {
		expect(formatVolume(42)).toBe('42');
	});

	it('renders the largest un-abbreviated volume verbatim', () => {
		expect(formatVolume(999)).toBe('999');
	});

	it('abbreviates a thousand with one decimal', () => {
		expect(formatVolume(1234)).toBe('1.2k');
	});

	it('abbreviates a million with one decimal', () => {
		expect(formatVolume(13_001_051)).toBe('13.0M');
	});

	it('rounds a fractional volume rather than printing its decimals', () => {
		expect(formatVolume(41.6)).toBe('42');
	});
});

describe('formatLegPrice', () => {
	it('drops the decimals on a price of 100 or more', () => {
		expect(formatLegPrice(196)).toBe('196');
	});

	it('keeps two decimals just below the hundreds threshold', () => {
		// The threshold is on the value, not on its rounded form: 99.999 is still
		// a two-decimal price, so it prints "100.00" rather than "100".
		expect(formatLegPrice(99.999)).toBe('100.00');
	});

	it('keeps two decimals on a sub-unit price with one significant digit', () => {
		expect(formatLegPrice(0.5)).toBe('0.50');
	});

	it('keeps four significant digits on a tenths-scale price', () => {
		expect(formatLegPrice(0.142857)).toBe('0.1429');
	});

	it('keeps four significant digits past the leading zeros of a tiny price', () => {
		// A fragment priced in divine: two decimals would print "0.00" and lose
		// the whole quantity.
		expect(formatLegPrice(0.004975)).toBe('0.004975');
	});

	it('renders a zero price with the two-decimal floor', () => {
		expect(formatLegPrice(0)).toBe('0.00');
	});

	it('renders a non-finite price as "0" rather than "NaN"', () => {
		expect(formatLegPrice(Number.NaN)).toBe('0');
	});
});

describe('formatChaos', () => {
	it('renders a whole-chaos amount as a bare count of orbs', () => {
		// Chaos AMOUNTS are integers — a count of items in a stash tab. The
		// decimals belong to the leg RATES, which go through formatLegPrice.
		expect(formatChaos(50)).toBe('50');
	});

	it('rounds a fractional amount to the nearest whole orb', () => {
		expect(formatChaos(49.6)).toBe('50');
	});

	it('separates the thousands of a four-figure payout', () => {
		expect(formatChaos(5050)).toBe('5,050');
	});

	it('separates every group of a seven-figure payout', () => {
		expect(formatChaos(1234567.891)).toBe('1,234,568');
	});

	it('rounds a sub-chaos amount away to zero', () => {
		// Accepted (POE-189): a play whose whole per-exchange figure is under one
		// chaos is not flippable once the exchange's gold fee is paid, so the four
		// significant digits this used to print dressed junk as precision.
		expect(formatChaos(0.0125)).toBe('0');
	});

	it('renders zero as a bare "0"', () => {
		expect(formatChaos(0)).toBe('0');
	});

	it('puts the minus sign outside the grouped digits', () => {
		expect(formatChaos(-1234)).toBe('-1,234');
	});

	it('rounds a half orb up on a gain', () => {
		expect(formatChaos(2.5)).toBe('3');
	});

	it('rounds a half orb up in magnitude on a loss, matching the gain', () => {
		// Rounding the signed value would break halves towards +Infinity and print
		// a loss one orb smaller than the identical gain.
		expect(formatChaos(-2.5)).toBe('-3');
	});

	it('renders a loss too small to print as an unsigned "0", never "-0"', () => {
		expect(formatChaos(-0.4)).toBe('0');
	});

	it('renders a non-finite amount as "0" rather than "NaN"', () => {
		expect(formatChaos(Number.NaN)).toBe('0');
	});
});

describe('formatGain', () => {
	it('signs a gain so the column does not read as a second cost', () => {
		expect(formatGain(700)).toBe('+700');
	});

	it('keeps the thousands grouping under the sign', () => {
		expect(formatGain(5050)).toBe('+5,050');
	});

	it('signs a gain that rounds up to a single orb', () => {
		expect(formatGain(0.6)).toBe('+1');
	});

	it('leaves a sub-chaos gain unsigned, because it rounds away to nothing', () => {
		// The rule formatRoiPct follows: the sign comes from the ROUNDED
		// magnitude, so a gain too small to print is not dressed as a gain.
		expect(formatGain(0.0125)).toBe('0');
	});

	it('signs a loss', () => {
		expect(formatGain(-5)).toBe('-5');
	});

	it('leaves a play that returns what it cost unsigned', () => {
		// A round trip at 0c is not a positive play, and "+0" would rank it as one
		// to a reader scanning the column for plus signs.
		expect(formatGain(0)).toBe('0');
	});

	it('renders negative zero unsigned rather than as "-0"', () => {
		expect(formatGain(-0)).toBe('0');
	});
});

describe('hoursProgress', () => {
	it('reports the fraction of the window a play held', () => {
		expect(hoursProgress(3, 6)).toBe(0.5);
	});

	it('reports a full window as 1', () => {
		expect(hoursProgress(6, 6)).toBe(1);
	});

	it('reports 0 for a window of no hours rather than dividing by zero', () => {
		// A body served before any hour closed: the bar is a CSS width, so an
		// Infinity here would be a broken track rather than an empty one.
		expect(hoursProgress(2, 0)).toBe(0);
	});

	it('clamps a count above the window to a full bar', () => {
		expect(hoursProgress(8, 6)).toBe(1);
	});

	it('reports 0 for a non-finite count', () => {
		expect(hoursProgress(Number.NaN, 6)).toBe(0);
	});
});

describe('worthwhileScale', () => {
	// Every case is stated against the 100c target the constant carries, so a
	// change to that constant fails these tests rather than passing silently.
	//
	// The step the target is divided by is the EXPECTATION (POE-193), never the
	// best-case `roi` this used to read — so every case below, bar the last,
	// leaves `roi` at the fixture's 10c, which divides the target into 10 flips
	// and is therefore an answer none of the expectations here produce. The last
	// case is the one that needs a `roi` of its own, and says why.

	it('rounds the flip count up to the exchange that actually clears the target', () => {
		// 100 ÷ 3 is 33.3 exchanges, and 33 of them pay 99c — a chaos short.
		expect(worthwhileScale(play({ expectedRoi: 3 }))?.flips).toBe(34);
	});

	it('adds no extra flip to an expectation that divides the target exactly', () => {
		// Four exchanges expected to pay 25c clear exactly 100c, so the fifth is
		// not needed.
		expect(worthwhileScale(play({ expectedRoi: 25 }))?.flips).toBe(4);
	});

	it('reports a single flip for a play expected to clear the target on its own', () => {
		expect(worthwhileScale(play({ expectedRoi: 150 }))?.flips).toBe(1);
	});

	it('reports the chaos those flips are expected to pay, not the target', () => {
		// 34 exchanges at 3c overshoot to 102c; reporting the flat 100c would put a
		// number on screen the play does not pay.
		expect(worthwhileScale(play({ expectedRoi: 3 }))?.gain).toBe(102);
	});

	it('reports the chaos the flips tie up, not the cost of one exchange', () => {
		expect(worthwhileScale(play({ expectedRoi: 3, investment: 40 }))?.investment).toBe(1360);
	});

	it('rounds the hours the market needs to absorb the flips up to a whole hour', () => {
		// 200 flips against 30 units an hour is 6.7 hours of trading, which is a
		// seventh hour the reader spends, not a sixth.
		expect(worthwhileScale(play({ expectedRoi: 0.5, depth: 30 }))?.hours).toBe(7);
	});

	it('reports exactly one hour for a scale the hourly volume covers whole', () => {
		expect(worthwhileScale(play({ expectedRoi: 25, depth: 4 }))?.hours).toBe(1);
	});

	it('reports a second hour for a scale one flip past what the hour covers', () => {
		expect(worthwhileScale(play({ expectedRoi: 25, depth: 3 }))?.hours).toBe(2);
	});

	it('reports no hours for a play whose thinnest leg traded nothing', () => {
		// An hourly volume of 0 is an unreadable wait, not an instant one, and
		// dividing by it would answer Infinity — which the column would print.
		expect(worthwhileScale(play({ depth: 0 }))?.hours).toBeNull();
	});

	it('reports no hours for a negative depth', () => {
		expect(worthwhileScale(play({ depth: -5 }))?.hours).toBeNull();
	});

	it('reports no hours for a non-finite depth', () => {
		expect(worthwhileScale(play({ depth: Number.NaN }))?.hours).toBeNull();
	});

	it('still reports the scale for a play whose depth cannot be read', () => {
		// The flip count and what it ties up are known whatever the book did last
		// hour; only the wait is missing, so the row keeps its "×N → +Gc".
		expect(worthwhileScale(play({ expectedRoi: 25, depth: 0 }))?.flips).toBe(4);
	});

	it('reports no scale for a play the simulation expects to gain nothing', () => {
		// No repeat count reaches a positive target from a zero step, and dividing
		// would answer Infinity flips.
		expect(worthwhileScale(play({ expectedRoi: 0 }))).toBeNull();
	});

	it('reports no scale for a play the simulation expects to lose chaos', () => {
		expect(worthwhileScale(play({ expectedRoi: -5 }))).toBeNull();
	});

	it('reports no scale for a non-finite expectation', () => {
		expect(worthwhileScale(play({ expectedRoi: Number.NaN }))).toBeNull();
	});

	it('reports no scale for a measured loser however large its best-case return', () => {
		// The row POE-193 put on the table and the old `roi` reading could not
		// express: the server's positivity floor (ADR-015) still keeps `roi` above
		// zero, and the simulation is free to measure a loss anyway — so a play can
		// carry the table's biggest ROI and no scale at all. Counting the
		// best-case 500c would answer ×1 rather than the dash the column owes a
		// play with nothing to repeat toward.
		expect(worthwhileScale(play({ roi: 500, roiPct: 2.5, expectedRoi: -6 }))).toBeNull();
	});
});

describe('moneyColumns', () => {
	// The invariant under test is ONE SCALE PER ROW: the three money columns are
	// the worthwhile RUN's figures whenever the play has a run, and one
	// exchange's when it does not — the same two branches `routeSlots` takes.
	//
	// Every scaled case runs on an `expectedRoi` of 3, which is 34 flips to clear
	// the 100c target. That count is deliberately not the one the fixture's other
	// fields produce: the best-case `roi` of 10 would divide the target into 10,
	// so a production swap onto the wrong field answers a number no case here
	// expects. It is also not a divisor of the target, so the scaled gain
	// overshoots to 102c and cannot be confused with `SCALE_TARGET_CHAOS` itself.

	it('reports the chaos the whole run ties up, not the cost of one exchange', () => {
		// 34 flips at 40c each.
		expect(moneyColumns(play({ expectedRoi: 3, investment: 40 })).investment).toBe(1360);
	});

	it('reports the best-case return across the whole run', () => {
		// The column stays the RAW best case (the NET/RAW convention the ROI%
		// column spells out) and is simply told at the run's size: 34 × 10c.
		expect(moneyColumns(play({ expectedRoi: 3, roi: 10 })).roi).toBe(340);
	});

	it('reports the chaos the run is expected to pay, not the per-exchange mean', () => {
		expect(moneyColumns(play({ expectedRoi: 3 })).expectedRoi).toBe(102);
	});

	it('falls back to the cost of one exchange for a play with no worthwhile run', () => {
		// A measured loser is served and flagged (ADR-016), and has no run to
		// repeat toward — so the fallback is a live branch, not a guard.
		expect(moneyColumns(play({ expectedRoi: -6, investment: 40 })).investment).toBe(40);
	});

	it('falls back to the best-case return on one exchange for a play with no run', () => {
		expect(moneyColumns(play({ expectedRoi: -6, roi: 500 })).roi).toBe(500);
	});

	it('falls back to the per-exchange expectation for a play with no run', () => {
		// The one money figure on the row that can print a minus, and this is the
		// branch it prints in: scaling only ever happens above zero.
		expect(moneyColumns(play({ expectedRoi: -6 })).expectedRoi).toBe(-6);
	});

	it('falls back on an expectation of exactly zero', () => {
		// The boundary `worthwhileScale` refuses: no repeat count reaches a
		// positive target from a zero step.
		expect(moneyColumns(play({ expectedRoi: 0, investment: 40 })).investment).toBe(40);
	});
});

describe('runLedger', () => {
	it('counts the exchanges of the worthwhile run', () => {
		// `ceil(100 / 0.5)`.
		expect(runLedger(chaosScarab(), DIVINE_RATE)?.flips).toBe(200);
	});

	it('says the row is scaled when a worthwhile run exists', () => {
		expect(runLedger(chaosScarab(), DIVINE_RATE)?.scaled).toBe(true);
	});

	it('counts one exchange when the expectation is not worth repeating', () => {
		// A measured loser is served and ranked (ADR-016); it simply has no repeat
		// count that reaches a positive target, so the row counts a single trip.
		expect(runLedger(chaosScarab({ expectedRoi: -3 }), DIVINE_RATE)?.flips).toBe(1);
	});

	it('counts one exchange at an expectation of exactly zero', () => {
		// The boundary `worthwhileScale` refuses.
		expect(runLedger(chaosScarab({ expectedRoi: 0 }), DIVINE_RATE)?.flips).toBe(1);
	});

	it('says the row is unscaled when there is no worthwhile run', () => {
		expect(runLedger(chaosScarab({ expectedRoi: -3 }), DIVINE_RATE)?.scaled).toBe(false);
	});

	it('ties up the chaos the Investment column reports for the run', () => {
		// `19.19 × 200`, which in float is 3838.0000000000005 — NOT the 3838 that
		// `200 × 19 × 1.01` answers. The two differ in the last ulp precisely
		// because they associate the same product differently, so this literal is
		// what tells a ledger reading the Investment column apart from one
		// re-pricing the run off the buy leg.
		expect(runLedger(chaosScarab(), DIVINE_RATE)?.investmentChaos).toBe(3838.0000000000005);
	});

	it('carries the chaos the ROI column reports for the run', () => {
		// `1.6 × 200`, the best case at the run's size.
		expect(runLedger(chaosScarab(), DIVINE_RATE)?.roiChaos).toBe(320);
	});

	it('carries the chaos the Exp. ROI column reports for the run', () => {
		// `0.5 × 200` — the scale's own gain, and the only root that can go
		// negative.
		expect(runLedger(chaosScarab(), DIVINE_RATE)?.expectedRoiChaos).toBe(100);
	});

	it('roots an unscaled row in the wire’s per-exchange figures', () => {
		// One branch, so one wrong change loses all three at once.
		const ledger = runLedger(chaosScarab({ expectedRoi: -3 }), DIVINE_RATE);

		expect(ledger?.investmentChaos).toBe(19.19);
		expect(ledger?.roiChaos).toBe(1.6);
		expect(ledger?.expectedRoiChaos).toBe(-3);
	});

	it('ends the chain on the investment plus the best case', () => {
		// 3838.0000000000005 + 320, which lands back on a round 4158.
		expect(runLedger(chaosScarab(), DIVINE_RATE)?.chainEndChaos).toBe(4158);
	});

	it('ends the row on the investment plus the measured expectation', () => {
		// 3838.0000000000005 + 100. The Get end is the Spend end plus the Exp. ROI
		// column and nothing else — E5.
		expect(runLedger(chaosScarab(), DIVINE_RATE)?.getChaos).toBe(3938.0000000000005);
	});

	it('ends an unscaled row below its spend when the expectation is a loss', () => {
		// 19.19 − 3. The measurement, not broken arithmetic: the best case is still
		// on the row, as the chain end of 20.79.
		expect(runLedger(chaosScarab({ expectedRoi: -3 }), DIVINE_RATE)?.getChaos).toBe(16.19);
	});

	it('values a chaos entry at one chaos per unit', () => {
		expect(runLedger(chaosScarab(), DIVINE_RATE)?.entryRate).toBe(1);
	});

	it('values a divine entry at the response’s divine rate', () => {
		expect(runLedger(divineScarab(), DIVINE_RATE)?.entryRate).toBe(200);
	});

	it('names a divine entry as the currency both ends are rendered in', () => {
		// Leg 1's quote, and not chaos by default: this is what the reader pays
		// with.
		expect(runLedger(divineScarab(), DIVINE_RATE)?.entryQuote).toBe(DIVINE_ID);
	});

	it('names the entry quote of a row whose sale is quoted elsewhere', () => {
		// The omen enters in chaos and sells against divine, so a ledger reading
		// the SELL leg's quote — the one currency on this row that is neither the
		// entry nor the end — answers divine here.
		expect(runLedger(omen(), DIVINE_RATE)?.entryQuote).toBe(CHAOS_ID);
	});

	it('spends the investment column itself on a chaos entry', () => {
		// Same ulp as `investmentChaos`, and the same discriminator: a Spend
		// recomputed as `flips × price × (1 + tick)` answers a flat 3838.
		expect(runLedger(chaosScarab(), DIVINE_RATE)?.spend).toBe(3838.0000000000005);
	});

	it('spends the chaos investment divided by the entry rate', () => {
		// 631.25c of investment at 200c a divine. A Spend left in chaos would read
		// 631.25 on a row whose unit word says divine.
		expect(runLedger(divineScarab(), DIVINE_RATE)?.spend).toBe(3.15625);
	});

	it('renders the chain end in the entry currency', () => {
		// (631.25 + 358.75) / 200.
		expect(runLedger(divineScarab(), DIVINE_RATE)?.chainEnd).toBe(4.95);
	});

	it('renders what the row gets in the entry currency', () => {
		// (631.25 + 100) / 200.
		expect(runLedger(divineScarab(), DIVINE_RATE)?.get).toBe(3.65625);
	});

	it('sells a direct play’s run for the chain end itself', () => {
		// A direct play's key is `direct:<marketID>` — one market, so both legs
		// carry the same quote and the sale is already in the entry currency.
		expect(runLedger(chaosScarab(), DIVINE_RATE)?.sellTotal).toBe(4158);
	});

	it('sells a divine-entry direct play’s run in the entry currency', () => {
		// 4.95 div — the chain end the row renders, NOT the 990c it was built
		// from. Only a divine entry tells those two apart: at `r = 1` the chaos
		// root and the entry-currency rendering are the same number, so the chaos
		// fixture above passes either way.
		expect(runLedger(divineScarab(), DIVINE_RATE)?.sellTotal).toBe(4.95);
	});

	it('derives no forward sale on a direct play', () => {
		expect(runLedger(chaosScarab(), DIVINE_RATE)?.sellForward).toBe(false);
	});

	it('divides a 1-hop’s chain end by the undercut convert price', () => {
		// 614.5227 / 206.91 = 2.97 div exactly. The two wrong divisors are both
		// visible from here: the RAW 209 answers 2.9403, and the forward figure
		// `12 × 0.2475` answers 2.9699999999999998 — a different float from 2.97.
		expect(runLedger(omen(), DIVINE_RATE)?.sellTotal).toBe(2.97);
	});

	it('derives no forward sale while the convert leg carries a price', () => {
		expect(runLedger(omen(), DIVINE_RATE)?.sellForward).toBe(false);
	});

	it('falls forward to the sell leg’s own run total when the convert leg has no price', () => {
		// `12 × (0.25 × 0.99)` = 2.9699999999999998, which is NOT the 2.97 the
		// backward reading answers — so this pins the route, not just the value.
		expect(runLedger(skewedConvert(0), DIVINE_RATE)?.sellTotal).toBe(2.9699999999999998);
	});

	it('says the sale was derived forwards when the convert leg had no price', () => {
		// The flag is what tells the convert step to print the market's ratio: two
		// amounts derived by different routes would not be in this market's ratio.
		expect(runLedger(skewedConvert(0), DIVINE_RATE)?.sellForward).toBe(true);
	});

	it('falls forward on a convert price that is not a finite number', () => {
		// `Infinity > 0` is true, so only the finiteness half of the guard catches
		// this one — without it the sale would total a clean 0.
		expect(runLedger(skewedConvert(Number.POSITIVE_INFINITY), DIVINE_RATE)?.sellTotal).toBe(
			2.9699999999999998
		);
	});

	it('falls forward on a convert price below zero', () => {
		// −206.91 is a finite number, so only the sign half of the guard catches
		// it. A guard reading `!== 0` would divide the chain end by it and print a
		// negative 2.97 as the sale.
		expect(runLedger(skewedConvert(-209), DIVINE_RATE)?.sellTotal).toBe(2.9699999999999998);
	});

	it('reports no sale total when neither the convert nor the sell leg can price it', () => {
		const legs = omen().legs;
		const blind = omen({
			legs: [legs[0]!, unpriced(legs[1]!, 0), unpriced(legs[2]!, 0)]
		});

		expect(runLedger(blind, DIVINE_RATE)?.sellTotal).toBeNull();
	});

	it('refuses a forward sale on a sell price that is not a finite number', () => {
		// One guard, so one wrong change loses both facts: `Infinity > 0` is true,
		// so without the finiteness half the run would total Infinity AND claim
		// the total was derived forwards.
		const legs = omen().legs;
		const blind = omen({
			legs: [legs[0]!, unpriced(legs[1]!, Number.POSITIVE_INFINITY), unpriced(legs[2]!, 0)]
		});
		const ledger = runLedger(blind, DIVINE_RATE);

		expect(ledger?.sellTotal).toBeNull();
		expect(ledger?.sellForward).toBe(false);
	});

	it('reports no sale total on a sell price below zero', () => {
		// The sign half of the forward guard, the twin of the convert case above:
		// −0.2475 is finite, and a guard reading `!== 0` would total the run at a
		// negative 2.97 and print it as the sale.
		const legs = omen().legs;
		const blind = omen({
			legs: [legs[0]!, unpriced(legs[1]!, -0.25), unpriced(legs[2]!, 0)]
		});

		expect(runLedger(blind, DIVINE_RATE)?.sellTotal).toBeNull();
	});

	it('claims no forward derivation when there was no sale left to derive', () => {
		const legs = omen().legs;
		const blind = omen({
			legs: [legs[0]!, unpriced(legs[1]!, 0), unpriced(legs[2]!, 0)]
		});

		expect(runLedger(blind, DIVINE_RATE)?.sellForward).toBe(false);
	});

	it('draws no ledger for a body with fewer than two legs', () => {
		expect(runLedger(chaosScarab({ legs: [leg()] }), DIVINE_RATE)).toBeNull();
	});

	it('draws no ledger for a divine entry the hour cannot value in chaos', () => {
		// The one exempt branch of the invariant: no `r`, so no entry-currency
		// rendering exists to state the chain in. Unreachable on a served body and
		// guarded anyway, because the alternative is a division by zero printed as
		// an amount.
		expect(runLedger(divineScarab(), 0)).toBeNull();
	});
});

describe('iconSrc', () => {
	// What `getApiBase()` hands the page: an origin plus the `/api` mount.
	const BASE = 'https://server.test/api';
	// What the server puts on a leg — `url.PathEscape`d id under the icon route.
	const PATH = '/currency-exchange/icon/Metadata%2FItems%2FCurrency%2FCurrencyRerollRare';

	it('joins the API base onto the icon path the server sent', () => {
		// The `%2F`s are the server's escaping of the metadata id's slashes; the
		// join must carry them through byte for byte, because a decoded
		// "Metadata/Items/..." would address a different route entirely and a
		// re-encoded "%252F" would reach the handler as a literal percent. The
		// `/api` mount has to survive too: the icon route lives under it, so a
		// join that treated the path as origin-relative (`new URL(path, base)`)
		// would drop the mount and request an endpoint the server does not serve.
		expect(iconSrc(BASE, PATH)).toBe(
			'https://server.test/api/currency-exchange/icon/Metadata%2FItems%2FCurrency%2FCurrencyRerollRare'
		);
	});

	it('renders no icon for a leg the server sent without artwork', () => {
		// `itemIcon: null` is the asset saying the item has no image at all, which
		// ItemIcon renders as nothing rather than as the "?" 404 fallback.
		expect(iconSrc(BASE, null)).toBeNull();
	});

	it('renders no icon for an empty path rather than requesting the API base itself', () => {
		// An empty path joined onto the base would fetch `/api`, which answers with
		// something that is not an image and would show the broken-image glyph.
		expect(iconSrc(BASE, '')).toBeNull();
	});

	it('trims a trailing slash off the base rather than emitting a double slash', () => {
		// Defensive: `iconSrc` takes the base as an argument, so its contract is
		// "join any base", not "join the one `getApiBase()` happens to build
		// today". `//currency-exchange/...` is normalised by some proxies and
		// 404d by others, so the trim is pinned for whatever base reaches it.
		expect(iconSrc('https://server.test/api/', PATH)).toBe(
			'https://server.test/api/currency-exchange/icon/Metadata%2FItems%2FCurrency%2FCurrencyRerollRare'
		);
	});

	it('inserts the separator when the path arrives without a leading slash', () => {
		expect(iconSrc(BASE, 'currency-exchange/icon/Chaos')).toBe(
			'https://server.test/api/currency-exchange/icon/Chaos'
		);
	});
});

describe('chaosIconPath', () => {
	it('escapes the id’s slashes the way the server’s PathEscape does', () => {
		// Pinned, not rebuilt from CHAOS_ID: the whole point is that this string
		// matches the route `IconPath` registers. Verified 200 against a running
		// server — a differently escaped path would 404 and empty the tile again.
		expect(chaosIconPath()).toBe(
			'/currency-exchange/icon/Metadata%2FItems%2FCurrency%2FCurrencyRerollRare'
		);
	});
});

describe('currencyIconPath', () => {
	it('escapes an id’s slashes the way the server’s path escaping does', () => {
		expect(currencyIconPath(DIVINE_ID)).toBe(
			'/currency-exchange/icon/Metadata%2FItems%2FCurrency%2FCurrencyModValues'
		);
	});
});

describe('quoteUnit', () => {
	it('abbreviates a chaos-quoted price to c', () => {
		expect(quoteUnit(CHAOS_ID)).toBe('c');
	});

	it('abbreviates a divine-quoted price to div', () => {
		expect(quoteUnit(DIVINE_ID)).toBe('div');
	});

	it('gives no unit to a currency it cannot name', () => {
		// A wrong unit beside a real price is the bug the units were added to
		// remove, so an id nobody checked prints nothing rather than "c".
		expect(quoteUnit('Metadata/Items/Currency/CurrencyUpgradeToRare')).toBe('');
	});
});

describe('routeSlots', () => {
	it('spends the whole worthwhile run at the undercut buy price', () => {
		// 200 flips of the 19.19c the Investment column reports per exchange — read
		// through `moneyColumns().investment`, not re-priced here off the buy leg.
		expect(routeSlots(chaosScarab(), DIVINE_RATE)?.spend.amount).toBe('3,838');
	});

	it('counts the run size into the buy step’s total', () => {
		// The single most legible symptom of the bug this closed: the buy step used
		// to print 3,800 (200 × the RAW 19c) while Spend said 3,838 — one row, two
		// prices for one order. Both are now the run's own investment.
		expect(routeSlots(chaosScarab(), DIVINE_RATE)?.buy.rate).toBe('buy 200 for ≈ 3,838c');
	});

	it('gets the run’s spend back plus the profit it is expected to make', () => {
		expect(routeSlots(chaosScarab(), DIVINE_RATE)?.get.amount).toBe('3,938');
	});

	it('keeps the run’s expected profit in chaos alone when the entry is chaos', () => {
		expect(routeSlots(chaosScarab(), DIVINE_RATE)?.get.sub).toBe('keep ≈ 100c');
	});

	it('keeps exactly the chaos the Exp. ROI column reports for the same play', () => {
		// The two are one number by construction, not two agreeing calculations,
		// and this is the case that can tell the difference: a 3c expectation
		// takes 34 flips and overshoots the target to 102c, so neither the wire
		// field (3) nor the target constant (100) can produce the answer.
		const scarab = chaosScarab({ expectedRoi: 3 });
		const column = moneyColumns(scarab).expectedRoi;

		expect(column).toBe(102);
		expect(routeSlots(scarab, DIVINE_RATE)?.get.sub).toBe(`keep ≈ ${formatChaos(column)}c`);
	});

	it('adds no chaos reading under a spend that is already chaos', () => {
		expect(routeSlots(chaosScarab(), DIVINE_RATE)?.spend.sub).toBeNull();
	});

	it('names the item bought at step 1, at its posted buy price', () => {
		const buy = routeSlots(chaosScarab(), DIVINE_RATE)?.buy;

		expect(buy?.name).toBe('Ambush Scarab');
		expect(buy?.icon).toBe('/icon/Scarab');
	});

	it('names the item sold at step 2 rather than the currency it is sold for', () => {
		// The headline bug: the sell leg's QUOTE — "Divine Orb", with the divine
		// artwork — sat beside the SCARAB's 0.10 price.
		const sell = routeSlots(divineScarab(), DIVINE_RATE)?.sell;

		expect(sell?.name).toBe('Ambush Scarab');
		expect(sell?.icon).toBe('/icon/Scarab');
	});

	it('quotes the sell step in the currency its own leg is priced in', () => {
		// The 0.1 divine this used to print is not a price the exchange can hold,
		// and the 40 that replaced it was not the run: the line snapped down to the
		// four whole 10-for-1 lots that 48 bought scarabs covered. It is a TOTAL
		// now — all 50 flips, at the chain end the ROI column is built from — so the
		// quantity is the run's and the unit is still the leg's own.
		expect(routeSlots(divineScarab(), DIVINE_RATE)?.sell.rate).toBe('sell 50 for ≈ 4.95 div');
	});

	it('names the stock a sell lot leaves behind', () => {
		// The line prints the whole run of 12 while this market posts in fives, so
		// two of the twelve are stock no order takes off the reader's hands. The
		// ends price all 12 flips, so without this clause the residue is invisible.
		expect(routeSlots(chunkyLot(), DIVINE_RATE)?.sell.rateTitle).toContain(
			'2 of the 12 bought stay unsold'
		);
	});

	it('leaves the residue clause off a sell lot that divides the run exactly', () => {
		// The sibling of the case above: this market posts 10 at a time and the run
		// is 50, so there is no remainder to disclose and the hover is the printed
		// pair alone.
		const title = routeSlots(divineScarab(), DIVINE_RATE)?.sell.rateTitle;

		expect(title).toContain('This market printed');
		expect(title).not.toContain('unsold');
	});

	it('leaves the unsold clause off a buy step, which bought no stock to leave', () => {
		// The buy market posts 16 at a time and the run is 50, so two of them are
		// left over — but they were never bought, and "2 of the 50 bought stay
		// unsold" would invent a holding.
		const title = routeSlots(divineScarab(), DIVINE_RATE)?.buy.rateTitle;

		expect(title).toContain('multiples of 16');
		expect(title).toContain('2 left over');
		expect(title).not.toContain('unsold');
	});

	it('quotes the buy step of a divine-entry run in divine', () => {
		// The whole 50-flip run at the investment the row is priced for, in the
		// currency the reader pays with. It used to read "buy 48 for 3 div" — the
		// three whole 16-for-1 lots the market posts — which is neither the run nor
		// what the Spend slot beside it says.
		expect(routeSlots(divineScarab(), DIVINE_RATE)?.buy.rate).toBe('buy 50 for ≈ 3.16 div');
	});

	it('spends a divine-entry run in divine rather than in chaos', () => {
		const spend = routeSlots(divineScarab(), DIVINE_RATE)?.spend;

		expect(spend?.amount).toBe('3.16');
		expect(spend?.unit).toBe('divine');
	});

	it('reads a divine spend back into chaos on its sub-line', () => {
		expect(routeSlots(divineScarab(), DIVINE_RATE)?.spend.sub).toBe('≈ 631c');
	});

	it('returns a divine-entry run in divine', () => {
		expect(routeSlots(divineScarab(), DIVINE_RATE)?.get.amount).toBe('3.66');
	});

	it('gives a divine-entry profit line the divine equivalent of its chaos', () => {
		expect(routeSlots(divineScarab(), DIVINE_RATE)?.get.sub).toBe('keep ≈ 100c (≈ 0.50 div)');
	});

	it('hands the unit word and the profit line back on one hover', () => {
		// Dense hides both the unit beside the number and the sub-line under it, so
		// this title is the only route back to either — a bare 3.66 with no way to
		// learn it counts divine is not a row anyone can act on.
		expect(routeSlots(divineScarab(), DIVINE_RATE)?.get.title).toBe(
			'divine — keep ≈ 100c (≈ 0.50 div)'
		);
	});

	it('hovers an end with nothing under it with its unit word alone', () => {
		// A chaos spend carries no sub-line, and a title ending in a dash with
		// nothing after it reads as a string that lost its tail.
		expect(routeSlots(chaosScarab(), DIVINE_RATE)?.spend.title).toBe('chaos');
	});

	it('keeps a divine end precise to the hundredth above a hundred orbs', () => {
		// A leg-price formatter drops to whole numbers above 100, which here would
		// print 101 spent and 102 returned on a run that keeps half a divine — the
		// total moving by four times the profit that produced it.
		// The ends read `moneyColumns().investment`, so the figure is 50 flips ×
		// 405c = 20,250c at 200c a divine = 101.25, plus 100c / 200 = 0.5.
		const big = divineScarab({
			legs: [
				leg({ ...DIVINE_SIDE, price: 2.005, priceItemQty: 200, priceQuoteQty: 401 }),
				leg({ ...DIVINE_SIDE, action: 'sell', price: 3, priceItemQty: 1, priceQuoteQty: 3 })
			],
			investment: 405
		});

		const route = routeSlots(big, DIVINE_RATE);

		expect(route?.spend.amount).toBe('101.25');
		expect(route?.get.amount).toBe('101.75');
		expect(route?.get.sub).toBe('keep ≈ 100c (≈ 0.50 div)');
	});

	it('groups a divine end into thousands', () => {
		// 50 flips × 4,040c = 202,000c, which at 200c a divine is 1,010. Grouped by
		// hand, for the reason `formatChaos` groups by hand: a locale that
		// separates with "." would print 1.010 beside an English sentence.
		const huge = divineScarab({
			legs: [
				leg({ ...DIVINE_SIDE, price: 20, priceItemQty: 1, priceQuoteQty: 20 }),
				leg({ ...DIVINE_SIDE, action: 'sell', price: 25, priceItemQty: 1, priceQuoteQty: 25 })
			],
			investment: 4040
		});

		expect(routeSlots(huge, DIVINE_RATE)?.spend.amount).toBe('1,010.00');
	});

	it('gives the end tiles the built artwork of the currency the run is entered with', () => {
		// Built from the id, never scavenged off `quoteIcon` — a leg served with
		// no artwork left both ends as empty tiles (POE-189).
		const route = routeSlots(divineScarab(), DIVINE_RATE);

		expect(route?.spend.icon).toBe(currencyIconPath(DIVINE_ID));
		expect(route?.get.icon).toBe(currencyIconPath(DIVINE_ID));
		expect(route?.spend.icon).not.toBe(DIVINE_ICON);
	});

	it('gives a chaos-entry run the built chaos artwork', () => {
		expect(routeSlots(chaosScarab(), DIVINE_RATE)?.spend.icon).toBe(chaosIconPath());
	});

	it('falls back to chaos ends when the hour published no divine rate', () => {
		// Not a served shape — an hour with no divine trade serves no divine-quoted
		// play — but the alternative to the guard is a division by zero printed as
		// an amount. 50 flips × 12.625c invested, plus the same 100c gain, and the
		// Get is `investment + expectedRoi` here as it is everywhere else (E5).
		const route = routeSlots(divineScarab(), 0);

		expect(route?.spend.unit).toBe('chaos');
		expect(route?.spend.amount).toBe('631');
		expect(route?.get.amount).toBe('731');
	});

	it('leaves a direct play without a convert step', () => {
		expect(routeSlots(chaosScarab(), DIVINE_RATE)?.convert).toBeNull();
	});

	it('converts the intermediate currency back at step 3', () => {
		// The third leg is a `sell` on the wire; on screen it is the conversion that
		// returns the reader to the currency they started in. 25 flips cost 1,262.5c
		// and the hour's best case ends the chain at 1,399.58c; this market's
		// undercut price is 204 × 0.99 = 201.96c a divine, so what the run converts
		// is 1,399.58 / 201.96 = 6.93 divine — which is `25 × 0.2772`, the run at
		// the sell leg's own undercut, arrived at from the other direction.
		const convert = routeSlots(oneHop(), DIVINE_RATE)?.convert;

		expect(convert?.rate).toBe('≈ 6.93 div → 1,400c');
		expect(convert?.icon).toBe(DIVINE_ICON);
	});

	it('sells the bought item at step 2 of a 1-hop play, not the currency it lands in', () => {
		expect(routeSlots(oneHop(), DIVINE_RATE)?.sell.name).toBe('Nameless Astrolabe');
	});

	it('marks step 3 with the currency being converted, not the one it is converted into', () => {
		// The convert leg's ITEM is the intermediate chaos being spent; its QUOTE
		// is the divine coming back. The tile is what names the currency now that
		// the run total has taken the name line's place, so reading the quote here
		// would put the divine artwork over a quantity of chaos orbs.
		expect(routeSlots(divineOneHop(), DIVINE_RATE)?.convert?.icon).toBe(CHAOS_ICON);
	});

	it('ends a divine-entry triangle’s convert line below the Get slot beside it', () => {
		// The premise INVERTED with the chain: the line now ends on `I + R`, the
		// hour's best case, while Get is `I + X`, the measurement. On this fixture
		// the deviation of §5 runs the other way — its ROI (55c) is BELOW its
		// Exp. ROI (100c) — so the chain end (3.43 div) prints BELOW the Get (3.66),
		// and nothing in the arithmetic guarantees which way round that lands.
		const route = routeSlots(divineOneHop(), DIVINE_RATE);

		expect(route?.convert?.rate).toBe('≈ 693c → 3.43 div');
		expect(route?.get.amount).toBe('3.66');
	});

	it('quotes each step of a divine-entry triangle in that step’s own currency', () => {
		// Three steps, two currencies, and the row used to show neither: buying in
		// divine, selling in chaos and converting back read as one undifferentiated
		// sequence of bare numbers.
		const route = routeSlots(divineOneHop(), DIVINE_RATE);

		// The whole 50-flip run on both steps: 3.16 divine in, and the sale that
		// backs the chain end totals 693c — `50 × 13.86`, the run at the sell leg's
		// undercut price.
		expect(route?.buy.rate).toBe('buy 50 for ≈ 3.16 div');
		expect(route?.sell.rate).toBe('sell 50 for ≈ 693c');
	});

	it('enters a divine-entry triangle in divine', () => {
		const route = routeSlots(divineOneHop(), DIVINE_RATE);

		expect(route?.spend.unit).toBe('divine');
		expect(route?.spend.amount).toBe('3.16');
		expect(route?.spend.sub).toBe('≈ 631c');
	});

	it('returns a divine-entry triangle in divine', () => {
		const route = routeSlots(divineOneHop(), DIVINE_RATE);

		expect(route?.get.amount).toBe('3.66');
		expect(route?.get.sub).toBe('keep ≈ 100c (≈ 0.50 div)');
	});

	it('marks only the slot whose own leg is suspect', () => {
		const route = routeSlots(
			chaosScarab({
				legs: [
					leg({ price: 19, priceItemQty: 1, priceQuoteQty: 19 }),
					leg({ action: 'sell', price: 21, priceItemQty: 1, priceQuoteQty: 21, suspect: true })
				]
			}),
			DIVINE_RATE
		);

		expect(route?.buy.suspect).toBe(false);
		expect(route?.sell.suspect).toBe(true);
	});

	it('reports a gain when the run is expected to profit', () => {
		expect(routeSlots(chaosScarab(), DIVINE_RATE)?.positive).toBe(true);
	});

	it('reports no gain on a measured loser, whose ends are its measured pair', () => {
		// The headline case. This row used to end on 21 — `investment + roi`, the
		// hour's best case — and call it a gain while its Exp. ROI cell showed a
		// loss. Under E5 the ends are the MEASURED pair: `investment + expectedRoi`
		// = 19.19 − 3 = 16.19, which prints 19 in and 16 back, and the flag follows
		// them.
		const route = routeSlots(chaosScarab({ expectedRoi: -3 }), DIVINE_RATE);

		expect(route?.get.amount).toBe('16');
		expect(route?.get.sub).toBe('lose ≈ 3c');
		expect(route?.positive).toBe(false);
	});

	it('reports no gain when a play with no run size returns exactly what it cost', () => {
		// The premise moved with the flag: `positive` reads the expectation now, so
		// a `roi` of 0 proves nothing about it. The real boundary is an expectation
		// of exactly zero — not a gain, and a Get that equals its own Spend.
		const route = routeSlots(chaosScarab({ expectedRoi: 0 }), DIVINE_RATE);

		expect(route?.positive).toBe(false);
		expect(route?.get.amount).toBe('19');
		expect(route?.get.amount).toBe(route?.spend.amount);
	});

	it('drops the ends from the run’s cost to one exchange’s when the scale disappears', () => {
		// ONE fixture, both branches, which is what makes this a case about the
		// branch rather than about either number. `chaosScarab`'s 0.5c expectation
		// needs `ceil(100 / 0.5)` = 200 exchanges, so its ends price the whole run
		// at 3,838c; at an expectation of zero there is no repeat count that
		// reaches the target, ADR-016 serves the play anyway, and the ends fall
		// back to the wire's own per-exchange 19.19c — the same undercut entry,
		// once.
		expect(routeSlots(chaosScarab(), DIVINE_RATE)?.spend.amount).toBe('3,838');
		expect(routeSlots(chaosScarab({ expectedRoi: 0 }), DIVINE_RATE)?.spend.amount).toBe('19');
	});

	it('totals the run of one on the buy step when there is no run size', () => {
		// The quantity is the RUN OF ONE and not the market's lot — it coincides
		// here only because this market posts one at a time — and the total is that
		// one exchange's investment, `≈` and all.
		expect(routeSlots(chaosScarab({ expectedRoi: 0 }), DIVINE_RATE)?.buy.rate).toBe(
			'buy 1 for ≈ 19c'
		);
	});

	it('totals every step of a play with no run size at the same single exchange', () => {
		// What replaced the no-worthwhile-size note. That branch rendered "buy 1 for
		// 35c → sell 4 for 1 div" — a 4:1 divergence between two steps of one row —
		// and apologised for it in a hover. Both steps now print 1, so there is
		// nothing left to apologise for, and each hover carries the market's printed
		// pair instead.
		const unscaled = routeSlots(omen({ expectedRoi: 0 }), DIVINE_RATE);

		expect(unscaled?.buy.rate).toBe('buy 1 for ≈ 35c');
		expect(unscaled?.sell.rate).toBe('sell 1 for ≈ 0.25 div');
		expect(unscaled?.convert?.rate).toBe('≈ 0.25 div → 51c');
		expect(unscaled?.buy.rateTitle).toContain('This market printed');
		expect(unscaled?.sell.rateTitle).toContain('This market printed');
	});

	it('claims no lot mismatch on a row whose two markets post different lots', () => {
		// The stronger form of what the old note hedged about: this row's markets
		// post 1 and 4, and both steps still lead with the same count, because the
		// count is the run and not either lot. Neither hover has a mismatch to
		// report, because the run divides both.
		const route = routeSlots(omen(), DIVINE_RATE);

		expect(route?.buy.rate.startsWith('buy 12 ')).toBe(true);
		expect(route?.sell.rate.startsWith('sell 12 ')).toBe(true);
		expect(route?.buy.rateTitle).not.toContain('multiples of');
		expect(route?.sell.rateTitle).not.toContain('multiples of');
	});

	it('leaves the hover off a step whose market posted no pair to word', () => {
		// Version skew on top of a losing expectation. The LINE no longer depends on
		// the pair at all — it is a total — so it prints exactly as it would with
		// one, and only the hover is missing, because `marketPair` has nothing to
		// say.
		const skewed = omen({
			expectedRoi: 0,
			legs: [omenBuy({ priceItemQty: 0, priceQuoteQty: 0 }), omenSell(), omenConvert()]
		});

		expect(routeSlots(skewed, DIVINE_RATE)?.buy.rate).toBe('buy 1 for ≈ 35c');
		expect(routeSlots(skewed, DIVINE_RATE)?.buy.rateTitle).toBeNull();
	});

	it('keeps the unit on a rate of a play with no run size', () => {
		// The mirror-route fix does not depend on the scale: a divine total stays
		// labelled divine on a play the simulation could not measure. The chain end
		// is `(12.625 + 7.175) / 200` = 0.099 divine, which prints 0.10.
		expect(routeSlots(divineScarab({ expectedRoi: 0 }), DIVINE_RATE)?.sell.rate).toBe(
			'sell 1 for ≈ 0.10 div'
		);
	});

	it('promises the exact nothing it keeps under a play measured at zero', () => {
		// The profit line is never absent now: under E5 the Get is the Spend plus
		// the Exp. ROI column on every branch, so a zero expectation has a zero to
		// print rather than a hole.
		expect(routeSlots(chaosScarab({ expectedRoi: 0 }), DIVINE_RATE)?.get.sub).toBe('keep ≈ 0c');
	});

	it('names the loss under a play measured below what it cost', () => {
		// The sign lives in the WORD, which is the only place on the row where a
		// word carries one — "keep ≈ -3c" asks the reader to hold two negations at
		// once.
		expect(routeSlots(chaosScarab({ expectedRoi: -3 }), DIVINE_RATE)?.get.sub).toBe('lose ≈ 3c');
	});

	it('leaves a rate quoted in neither chaos nor divine without a unit', () => {
		const exotic = chaosScarab({
			legs: [
				leg({ price: 19, priceItemQty: 1, priceQuoteQty: 19 }),
				leg({
					action: 'sell',
					price: 21,
					priceItemQty: 1,
					priceQuoteQty: 21,
					quote: 'Metadata/Items/Currency/CurrencyUpgradeToRare'
				})
			]
		});

		// 200 flips of a market quoted in a currency nothing here names: the total
		// still scales and still prints, and only the unit word is withheld, because
		// a wrong currency beside a real number is worse than none. With no unit
		// word to pick a formatter, the total takes the fractional one.
		expect(routeSlots(exotic, DIVINE_RATE)?.sell.rate).toBe('sell 200 for ≈ 4,158.00');
	});

	it('totals the buy step over the run rather than pricing one unit', () => {
		// The rendering this replaced said "buy 12 @ 35.00 c", which is a number the
		// in-game exchange has no field for. The run of twelve costs 424c at the
		// undercut entry — `moneyColumns().investment`, the same figure the Spend
		// slot and the Investment column print.
		expect(routeSlots(omen(), DIVINE_RATE)?.buy.rate).toBe('buy 12 for ≈ 424c');
	});

	it('totals the sell step over the same run the buy step counted', () => {
		// Both steps count 12, whatever their markets' lots are, and the sale totals
		// the chain end read back through the convert market's undercut price:
		// 614.5227 / 206.91 = 2.97 div, never the 0.25 each the decimal implied.
		expect(routeSlots(omen(), DIVINE_RATE)?.sell.rate).toBe('sell 12 for ≈ 2.97 div');
	});

	it('adds no lot clause to a step whose run is a whole number of its lots', () => {
		// The hover is always there once the leg carries a pair — it is where the
		// printed extreme lives now — but 12 is exactly three of this market's
		// four-at-a-time lots, so there is no indivisibility to disclose.
		const title = routeSlots(omen(), DIVINE_RATE)?.sell.rateTitle;

		expect(title).toContain('This market printed');
		expect(title).not.toContain('multiples of');
	});

	it('totals the whole run at step 3 rather than one lot of its market', () => {
		// "convert 1 div for 209c" read as an order to convert ONE divine on a row
		// whose other four slots all count the 12-flip run. What step 3 actually
		// moves is that run's proceeds, 2.97 div, and what they come back as is the
		// chain end of 615c.
		expect(routeSlots(omen(), DIVINE_RATE)?.convert?.rate).toBe('≈ 2.97 div → 615c');
	});

	it('ends the convert line on the chain end and not on the Get beside it', () => {
		// The premise INVERTED. The line ends on `I + R` — what the hour's best case
		// would have paid — while Get is `I + X`, what the run is measured to
		// return, so the two are deliberately different numbers sitting one slot
		// apart. The gap between them is the gap between the ROI and Exp. ROI
		// columns: 190.3227 − 102.
		const money = moneyColumns(omen());
		const route = routeSlots(omen(), DIVINE_RATE);

		expect(route?.convert?.rate.endsWith('→ 615c')).toBe(true);
		expect(route?.get.amount).toBe('526');
		expect(money.roi - money.expectedRoi).toBe(88.3227);
	});

	it('moves step 3’s item name off the line and onto its hover', () => {
		// The total names both currencies and the tile carries the artwork, so a
		// "Divine Orb" heading over it repeats the tile and costs a line the row
		// has no height for. It is not dropped — it leads the hover.
		const convert = routeSlots(omen(), DIVINE_RATE)?.convert;

		expect(convert?.name).toBeNull();
		expect(convert?.rateTitle).toContain('Divine Orb');
	});

	it('keeps the market’s own ratio on the convert step’s hover', () => {
		// The line is a total now, so the order this market actually posts appears
		// nowhere else on the row: a reader who wants to know what one divine
		// fetches has only the hover to ask.
		expect(routeSlots(omen(), DIVINE_RATE)?.convert?.rateTitle).toContain(
			'convert 1 div for 209c'
		);
	});

	it('names which column each of the two amounts beside step 3 came from', () => {
		// The convert line ends on `I + R` and the Get slot one place along ends on
		// `I + X`, so the row shows two materially different amounts a slot apart
		// and this hover is the only thing on it that says which column produced
		// each. Naming the same column twice would read as one figure printed
		// twice and the gap between them as an arithmetic fault.
		const title = routeSlots(omen(), DIVINE_RATE)?.convert?.rateTitle;

		expect(title).toContain('the Spend plus the ROI column');
		expect(title).toContain(
			'The Get slot at the end of the row is the Spend plus the Exp. ROI column'
		);
		// The identities are chaos identities, and the columns are chaos while a
		// divine route's ends are not — the qualifying clause is what keeps the
		// hover true on that route, so it is pinned with the identities it guards.
		expect(title).toContain('at the divine rate');
	});

	it('shows the market’s ratio at step 3 of a row it cannot value in chaos', () => {
		// The exempt branch is one of the three places with no total to print
		// (§4.7), so step 3 falls back to the pair its market posts — and keeps its
		// name line, which a run total gives up.
		const exempt = routeSlots(divineOneHop(), 0);

		expect(exempt?.convert?.rate).toBe('convert 200c for 1 div');
		expect(exempt?.convert?.name).toBe('Chaos Orb');
	});

	it('shows no run total at step 3 when the convert leg came through without a price', () => {
		// Version skew. The chain end divided by a price of 0 is not a quantity, and
		// printing it as "≈ 0.00 div → 615c" would put a total on the row that no
		// market backs — so the step shows the ratio and says why, while the SALE it
		// converts still totals the run, derived forwards at `12 × 0.2475`.
		const priceless = omen({
			legs: [omenBuy(), omenSell(), omenConvert({ price: 0, priceItemQty: 0, priceQuoteQty: 0 })]
		});
		const route = routeSlots(priceless, DIVINE_RATE);

		expect(route?.convert?.rate).toBe('convert @ 0.00 c');
		expect(route?.sell.rate).toBe('sell 12 for ≈ 2.97 div');
		expect(route?.convert?.rateTitle).toContain('carried no usable price this hour');
	});

	it('words the caveat for a line that shows no quantity pair at all', () => {
		// The caveat follows the branch the LINE took. This convert market posted
		// no pair, so the line fell through to its per-unit rate — and a hover
		// promising "the quantity pair it posts" would name a reading that is
		// nowhere on the row, on the one line that has no pair to show.
		const priceless = omen({ legs: [omenBuy(), omenSell(), unpriced(omenConvert(), 0)] });

		const title = routeSlots(priceless, DIVINE_RATE)?.convert?.rateTitle;

		expect(title).toContain(
			'this market posted no quantity pair, so the line shows its per-unit rate'
		);
		expect(title).not.toContain('the quantity pair it posts');
	});

	it('shows the sell market’s own ratio when neither leg can price the sale', () => {
		// Both markets came through unpriced, so `chainEnd / u2` is not a reading
		// and the forward `flips × u1` is not one either: step 2 has no total left
		// to print at all. It falls back to the only thing still true of that
		// market — its own rate — and carries the caveat that says the amounts at
		// the ends of the row are still counting the run.
		const doubleDead = omen({
			legs: [omenBuy(), unpriced(omenSell(), 0), unpriced(omenConvert(), 0)]
		});

		const route = routeSlots(doubleDead, DIVINE_RATE);

		expect(route?.sell.rate).toBe('sell @ 0.00 div');
		expect(route?.sell.rateTitle).toContain(
			'the amounts at both ends of the row count the exchanges the row is priced for'
		);
	});

	it('totals the whole run on a step whose market posts a lot the run does not divide', () => {
		// The LINE no longer snaps. A market that posts five omens for two divine
		// used to force "sell 10 for 4 div" — short of the run, exactly postable —
		// while the ends counted all 12. It now reads the same string the plain
		// four-at-a-time market produces, because the sell leg's own lot never
		// touched the total.
		expect(routeSlots(chunkyLot(), DIVINE_RATE)?.sell.rate).toBe('sell 12 for ≈ 2.97 div');
	});

	it('says on a step whose lot cannot divide the run how many orders it takes', () => {
		// The fact the snap used to carry, moved onto the hover with the pair: the
		// market still posts in fives, so a line reading 12 asserts a quantity no
		// single order can move.
		const title = routeSlots(chunkyLot(), DIVINE_RATE)?.sell.rateTitle;

		expect(title).toContain('multiples of 5');
		expect(title).toContain('the run of 12 is 2 whole orders');
		expect(title).toContain('2 of the 12 bought stay unsold');
	});

	it('buys the whole run when the buy market posts one at a time', () => {
		// `expectedRoi` 40c needs `ceil(100 / 40)` = 3 flips, costing 106c at the
		// undercut entry. This market posts one at a time, so the run is a whole
		// number of its lots and the hover is the printed pair alone.
		const tiny = omen({ expectedRoi: 40 });

		expect(routeSlots(tiny, DIVINE_RATE)?.buy.rate).toBe('buy 3 for ≈ 106c');
		expect(routeSlots(tiny, DIVINE_RATE)?.buy.rateTitle).toContain('This market printed');
		expect(routeSlots(tiny, DIVINE_RATE)?.buy.rateTitle).not.toContain('multiples of');
	});

	it('totals the run on a step whose market posts a bigger lot than the whole play', () => {
		// 3 omens against a four-for-one-divine market. The line used to post one
		// whole lot — "sell 4 for 1 div", more than the play holds — because a
		// snapped order could not go below one. The total has no such floor: it is
		// the run, and the overshoot is the hover's to report.
		const tiny = omen({ expectedRoi: 40 });

		expect(routeSlots(tiny, DIVINE_RATE)?.sell.rate).toBe('sell 3 for ≈ 0.74 div');
	});

	it('claims no overshoot on a run that is exactly one lot', () => {
		// The boundary between the two clauses: `expectedRoi` 25c needs `ceil(100 /
		// 25)` = 4 flips, which is precisely what this market posts. One order
		// covers the whole run, so neither the smaller-than-a-lot sentence nor the
		// multiples sentence has anything to report — a guard reading `<=` would
		// tell the reader the market cannot take an order this small.
		const exact = omen({ expectedRoi: 25 });

		const title = routeSlots(exact, DIVINE_RATE)?.sell.rateTitle;

		expect(title).toContain('This market printed');
		expect(title).not.toContain('bigger than the whole play');
		expect(title).not.toContain('multiples of');
	});

	it('says on a step whose lot is bigger than the play that no order fits', () => {
		// The clause the reader has to act on: a market that will not take an order
		// this small is a different problem from one whose lot merely fails to
		// divide the run.
		const tiny = omen({ expectedRoi: 40 });

		const title = routeSlots(tiny, DIVINE_RATE)?.sell.rateTitle;

		expect(title).toContain('posts 4 at a time');
		expect(title).toContain('more than the run of 3');
		expect(title).toContain('the smallest order it accepts is bigger than the whole play');
	});

	it('totals a step whose market posted no pair at all', () => {
		// Version skew — this app caches no response, but it does point at a
		// configurable server, so a build carrying these fields can be aimed at one
		// from before POE-193. The line no longer depends on the pair, so it prints
		// exactly as it would with one and only the hover is missing.
		const skewed = omen({
			legs: [omenBuy({ priceItemQty: 0, priceQuoteQty: 0 }), omenSell(), omenConvert()]
		});

		expect(routeSlots(skewed, DIVINE_RATE)?.buy.rate).toBe('buy 12 for ≈ 424c');
		expect(routeSlots(skewed, DIVINE_RATE)?.buy.rateTitle).toBeNull();
	});

	it('totals a step whose pair is absent entirely', () => {
		// The shape an older server actually sends: no such keys in the JSON at
		// all, which reaches the renderer as `undefined` however the type reads.
		// The cast is the point of the case — a guard written as `=== 0` would let
		// this through and word a hover around `undefined`.
		const skewed = omen({
			legs: [
				omenBuy({
					priceItemQty: undefined as unknown as number,
					priceQuoteQty: undefined as unknown as number
				}),
				omenSell(),
				omenConvert()
			]
		});

		expect(routeSlots(skewed, DIVINE_RATE)?.buy.rate).toBe('buy 12 for ≈ 424c');
		expect(routeSlots(skewed, DIVINE_RATE)?.buy.rateTitle).toBeNull();
	});

	/**
	 * The four ways a leg fails the quantity-pair guard — one per conjunct, each
	 * with the other three satisfied, so no case can pass for a neighbour's
	 * reason.
	 *
	 * The both-zero and both-absent cases above cannot separate the two sides:
	 * with either positivity check gone the other still rejects them, and with
	 * either integrality check gone the other still rejects `undefined`. Only a
	 * pair broken on ONE side at a time can say which conjunct is load-bearing.
	 *
	 * These pairs deliberately contradict their leg's own price, which every
	 * other fixture here keeps consistent (see `leg`). That is the shape being
	 * tested: a pair that arrives half-formed IS a pair that no longer states its
	 * leg's price, and a fixture that "fixed" the price to match would have no
	 * broken pair left to reject.
	 */
	const BROKEN_PAIRS: { label: string; pair: Partial<CurrencyExchangeLeg> }[] = [
		{ label: 'a fractional item quantity', pair: { priceItemQty: 2.5 } },
		{ label: 'a fractional quote quantity', pair: { priceQuoteQty: 1.5 } },
		{ label: 'an item quantity of nothing', pair: { priceItemQty: 0 } },
		{ label: 'a quote quantity of nothing', pair: { priceQuoteQty: 0 } }
	];

	for (const { label, pair } of BROKEN_PAIRS) {
		it(`reads no market pair off a buy leg served with ${label}`, () => {
			// ONE fixture through BOTH emitters of the guard. `divineScarab` is the
			// play that changes branch on the response's divine rate alone, so the
			// same broken leg can be read once where a run total is printed and once
			// where there is none — and a guard that drifted between the two would
			// have the line claiming a pair the hover denies.
			const broken = divineScarab({
				legs: [
					leg({ ...DIVINE_SIDE, price: 0.0625, priceItemQty: 16, priceQuoteQty: 1, ...pair }),
					leg({ ...DIVINE_SIDE, action: 'sell', price: 0.1, priceItemQty: 10, priceQuoteQty: 1 })
				]
			});

			// `marketPair`, on the branch that totals the run: no pair to word, so
			// the step carries no hover while its line prints exactly as before.
			expect(routeSlots(broken, DIVINE_RATE)?.buy.rateTitle).toBeNull();
			// `pairLine`, on the exempt branch, which has no total to print: the line
			// falls all the way through to the leg's own decimal rate.
			expect(routeSlots(broken, 0)?.buy.rate).toBe('buy @ 0.0625 div');
		});
	}

	it('totals the sell step over the run whatever the buy step could post', () => {
		// A skewed buy leg words no hover, and that changes nothing about step 2:
		// the sell total is the run's, read off the chain end, and was never capped
		// by what the buy line managed to print.
		const skewed = omen({
			legs: [omenBuy({ priceItemQty: 0, priceQuoteQty: 0 }), omenSell(), omenConvert()]
		});

		expect(routeSlots(skewed, DIVINE_RATE)?.sell.rate).toBe('sell 12 for ≈ 2.97 div');
	});

	it('draws no route for a play that arrives with fewer than two legs', () => {
		expect(routeSlots(play({ legs: [leg()] }), DIVINE_RATE)).toBeNull();
	});
});

/**
 * The CLOSURE SUITE of `docs/CURRENCY-EXCHANGE-ROW-INVARIANT.md` §7.
 *
 * The equations of §3 and the rendering rules of §4 asserted on EMITTED output,
 * over the matrix `{direct, 1-hop} × {chaos entry, divine entry}` on the scaled
 * branch, two of those four repeated on the no-run fallback, and the exempt
 * branch as a seventh case that asserts the SUSPENSION rather than the
 * equations. The document states the invariant; this suite is what enforces it,
 * which is why a change to an equation there and a change here travel together.
 */
describe('row closure', () => {
	/**
	 * Two figures that must be the same number by different routes.
	 *
	 * The wire's own identities reassociate in float — `19*1.01*200` is not
	 * `19.19*200` — so a cross-check tolerates the reassociation and nothing
	 * larger.
	 */
	function expectClose(actual: number, expected: number, rel = 1e-9): void {
		expect(Math.abs(actual - expected)).toBeLessThanOrEqual(rel * Math.max(1, Math.abs(expected)));
	}

	/** A printed amount read back the way the reader reads it: sign, digits, point. */
	function parseAmount(printed: string): number {
		return Number(printed.replace(/[^0-9.-]/g, ''));
	}

	/**
	 * The last mechanical total the row emits, as a string: the sell step's total
	 * on a direct play, the convert line's right-hand amount on a 1-hop.
	 */
	function printedChainEnd(sellRate: string, convertRate: string | undefined): string {
		const line = convertRate ?? sellRate;
		const marker = convertRate === undefined ? 'for ≈ ' : '→ ';
		return line.slice(line.lastIndexOf(marker) + marker.length);
	}

	interface ClosureCase {
		/** The row this case is, as the suite names it. */
		label: string;
		/** A fresh play, so no case can be read after another has touched it. */
		play: () => CurrencyExchangePlay;
		/** The response's divine rate. */
		rate: number;
		/** The entry quote is chaos, which decides both formatter and slack. */
		entryIsChaos: boolean;
		/** The unit the STEP totals carry beside the entry-currency amount. */
		suffix: string;
		/** A worthwhile run exists, so the Scale column has a gain to compare. */
		scaled: boolean;
		flips: number;
		/** Undercut buy price of leg 1, in entry-quote units per item. */
		u0: number;
		/** Undercut sell price of leg 2, in leg 2's own quote per item. */
		u1: number;
		/** Undercut price of leg 3 in entry-quote per intermediate; `null` if direct. */
		u2: number | null;
		spend: string;
		spendSub: string | null;
		buyRate: string;
		sellRate: string;
		convertRate: string | null;
		get: string;
		getSub: string;
		/** `formatGain` of the ROI column, as the cell prints it. */
		roiColumn: string;
		/** `formatGain` of the Exp. ROI column, as the cell prints it. */
		expColumn: string;
		positive: boolean;
	}

	/**
	 * Six wire-consistent plays, all of them the shared fixtures with
	 * `expectedRoi` overridden where the branch demands it. Every expected string
	 * below is a literal, hand-worked in the comment above its case.
	 */
	const CASES: ClosureCase[] = [
		{
			// F1. `u0 = 19.19`, `u1 = 20.79`; `investment 19.19`, `roi 1.6`,
			// `expectedRoi 0.5` → 200 flips. `I = 3838`, `R = 320`, `X = 100`, so
			// `chainEndChaos = 4158` and `getChaos = 3938`. `r = 1`, so the chaos
			// roots and the entry-currency renderings are the same numbers.
			label: 'F1 — a chaos-entry direct flip with a run',
			play: () => chaosScarab(),
			rate: DIVINE_RATE,
			entryIsChaos: true,
			suffix: 'c',
			scaled: true,
			flips: 200,
			u0: 19.19,
			u1: 20.79,
			u2: null,
			spend: '3,838',
			spendSub: null,
			buyRate: 'buy 200 for ≈ 3,838c',
			sellRate: 'sell 200 for ≈ 4,158c',
			convertRate: null,
			get: '3,938',
			getSub: 'keep ≈ 100c',
			roiColumn: '+320',
			expColumn: '+100',
			positive: true
		},
		{
			// F2. Both legs quoted in divine: `u0 = 0.063125`, `u1 = 0.099`,
			// `r = 200`; `investment 12.625`, `roi 7.175`, `expectedRoi 2` → 50
			// flips. `I = 631.25`, `R = 358.75`, `X = 100`; `chainEndChaos = 990`,
			// `getChaos = 731.25`, so `spend = 3.15625`, `chainEnd = 4.95` and
			// `get = 3.65625` divine.
			label: 'F2 — a divine-entry direct flip with a run',
			play: () => divineScarab(),
			rate: DIVINE_RATE,
			entryIsChaos: false,
			suffix: ' div',
			scaled: true,
			flips: 50,
			u0: 0.063125,
			u1: 0.099,
			u2: null,
			spend: '3.16',
			spendSub: '≈ 631c',
			buyRate: 'buy 50 for ≈ 3.16 div',
			sellRate: 'sell 50 for ≈ 4.95 div',
			convertRate: null,
			get: '3.66',
			getSub: 'keep ≈ 100c (≈ 0.50 div)',
			roiColumn: '+359',
			expColumn: '+100',
			positive: true
		},
		{
			// F3. The omen triangle: `u0 = 35.35`, `u1 = 0.2475`, `u2 = 206.91`,
			// `r = 1`; `investment 35.35`, `roi 15.860225`, `expectedRoi 8.5` → 12
			// flips. `I = 424.2`, `R = 190.3227`, `X = 102`; `chainEnd = 614.5227`
			// and `get = 526.2`, so the sale totals `614.5227 / 206.91 = 2.97` div —
			// the forward figure `12 × 0.2475` to within a float ulp.
			label: 'F3 — a chaos-entry triangle with a run',
			play: () => omen(),
			rate: DIVINE_RATE,
			entryIsChaos: true,
			suffix: 'c',
			scaled: true,
			flips: 12,
			u0: 35.35,
			u1: 0.2475,
			u2: 206.91,
			spend: '424',
			spendSub: null,
			buyRate: 'buy 12 for ≈ 424c',
			sellRate: 'sell 12 for ≈ 2.97 div',
			convertRate: '≈ 2.97 div → 615c',
			get: '526',
			getSub: 'keep ≈ 102c',
			roiColumn: '+190',
			expColumn: '+102',
			positive: true
		},
		{
			// F4. The divine-entry triangle: buy the scarab against divine, sell it
			// against chaos, convert the chaos back. `u0 = 0.063125` div,
			// `u1 = 13.86`c, `u2 = 0.00495` div a chaos, `r = 200`;
			// `investment 12.625`, `roi 1.0964`, `expectedRoi 2` → 50 flips.
			// `I = 631.25`, `R = 54.82`, `X = 100`; `chainEndChaos = 686.07` and
			// `getChaos = 731.25`, so the sale totals `3.43035 / 0.00495 = 693`c,
			// which is `50 × 13.86` exactly.
			//
			// Its ROI (55c) is BELOW its Exp. ROI (100c), so its chain end (3.43 div)
			// prints BELOW its Get (3.66 div) — the deviation of §5 running the other
			// way. Deliberate: field measurement has the best case overstating the
			// measurement four to eight times, but nothing in the arithmetic
			// guarantees the sign, and this is the case that catches any emitter,
			// formatter or assertion that quietly assumes `chainEnd > get`.
			label: 'F4 — a divine-entry triangle with a run',
			play: () => divineOneHop(),
			rate: DIVINE_RATE,
			entryIsChaos: false,
			suffix: ' div',
			scaled: true,
			flips: 50,
			u0: 0.063125,
			u1: 13.86,
			u2: 0.00495,
			spend: '3.16',
			spendSub: '≈ 631c',
			buyRate: 'buy 50 for ≈ 3.16 div',
			sellRate: 'sell 50 for ≈ 693c',
			convertRate: '≈ 693c → 3.43 div',
			get: '3.66',
			getSub: 'keep ≈ 100c (≈ 0.50 div)',
			roiColumn: '+55',
			expColumn: '+100',
			positive: true
		},
		{
			// F5. F1's legs with no run: `flips 1`, `I = 19.19`, `R = 1.6`,
			// `X = -3`; `chainEndChaos = 20.79` and `getChaos = 16.19`. The row
			// reads 19 in, the hour's best case would have returned 21, the last day
			// measured 16 — all three on screen, none contradicting another.
			label: 'F5 — a chaos-entry direct flip with no run',
			play: () => chaosScarab({ expectedRoi: -3 }),
			rate: DIVINE_RATE,
			entryIsChaos: true,
			suffix: 'c',
			scaled: false,
			flips: 1,
			u0: 19.19,
			u1: 20.79,
			u2: null,
			spend: '19',
			spendSub: null,
			buyRate: 'buy 1 for ≈ 19c',
			sellRate: 'sell 1 for ≈ 21c',
			convertRate: null,
			get: '16',
			getSub: 'lose ≈ 3c',
			roiColumn: '+2',
			expColumn: '-3',
			positive: false
		},
		{
			// F6. F4's legs with no run: `flips 1`, `I = 12.625`, `R = 1.0964`,
			// `X = -5`; `chainEndChaos = 13.7214` and `getChaos = 7.625`, so at
			// `r = 200` the spend is 0.063125 div, the chain end 0.068607 and the get
			// 0.038125. Both ends land under one divine, where the entry-currency
			// reading is nearly useless — which is what the chaos sub-line is for,
			// and is why the fallback branch of a divine-entry play is where the
			// hundredth-of-an-orb precision earns or loses its keep.
			label: 'F6 — a divine-entry triangle with no run',
			play: () => divineOneHop({ expectedRoi: -5 }),
			rate: DIVINE_RATE,
			entryIsChaos: false,
			suffix: ' div',
			scaled: false,
			flips: 1,
			u0: 0.063125,
			u1: 13.86,
			u2: 0.00495,
			spend: '0.06',
			spendSub: '≈ 13c',
			buyRate: 'buy 1 for ≈ 0.06 div',
			sellRate: 'sell 1 for ≈ 14c',
			convertRate: '≈ 14c → 0.07 div',
			get: '0.04',
			getSub: 'lose ≈ 5c (≈ 0.03 div)',
			roiColumn: '+1',
			expColumn: '-5',
			positive: false
		}
	];

	for (const c of CASES) {
		describe(c.label, () => {
			it('prints the buy step’s total as the Spend end’s own string', () => {
				// E2. The two are one variable through one formatter, so the pin is on
				// the whole string and carries no tolerance — and it is two assertions
				// rather than one, because `RouteEnd.amount` is BARE (its unit word is
				// a separate span) while a step total carries its unit. Either half
				// alone can pass while the two numbers differ.
				const route = routeSlots(c.play(), c.rate);

				expect(route?.spend.amount).toBe(c.spend);
				expect(route?.buy.rate).toBe(c.buyRate);
				expect(route?.buy.rate.endsWith(`for ≈ ${route?.spend.amount}${c.suffix}`)).toBe(true);
			});

			it('totals the sale in the sell leg’s own quote', () => {
				// E7. On a direct play that total IS the chain end; on a 1-hop it is
				// the chain end read back through the convert market's undercut price.
				expect(routeSlots(c.play(), c.rate)?.sell.rate).toBe(c.sellRate);
			});

			if (c.convertRate !== null) {
				it('prints the sell step’s total as the convert line’s own left amount', () => {
					// The 1-hop half of E7, pinned the same two ways the buy/Spend pair
					// is: the literal line, and the character-identity between the sell
					// line's tail and the amount the convert line opens on.
					const route = routeSlots(c.play(), c.rate);
					const sold = route?.sell.rate.slice(
						(route?.sell.rate.lastIndexOf('for ≈ ') ?? 0) + 'for ≈ '.length
					);

					expect(route?.convert?.rate).toBe(c.convertRate);
					expect(route?.convert?.rate.startsWith(`≈ ${sold} →`)).toBe(true);
				});
			}

			it('words the profit line with the Exp. ROI column’s own chaos', () => {
				// E6 and §4.5: one variable in three homes, the verb carrying the sign
				// and the amount carrying the magnitude. The generalised form of the
				// pin that used to run on one fixture — verb included, so it runs on
				// the losing rows too.
				const play = c.play();
				const column = moneyColumns(play).expectedRoi;
				const verb = column > 0 ? 'keep' : 'lose';
				const route = routeSlots(play, c.rate);

				expect(route?.get.sub).toBe(c.getSub);
				expect(route?.get.sub?.startsWith(`${verb} ≈ ${formatChaos(Math.abs(column))}c`)).toBe(
					true
				);
			});

			if (c.scaled) {
				it('shows the Scale column’s gain and the Exp. ROI column as one number', () => {
					// Scoped to the scaled rows on purpose: `worthwhileScale` answers
					// `null` on a play with no positive expectation, where this
					// expression would throw rather than assert.
					const play = c.play();

					expect(worthwhileScale(play)!.gain).toBe(moneyColumns(play).expectedRoi);
					expect(formatGain(worthwhileScale(play)!.gain)).toBe(c.expColumn);
				});
			}

			it('ends the chain on its own investment plus its own best case', () => {
				// E3, as a single float addition of the ledger's own roots. This is
				// what closure BY CONSTRUCTION means mechanically: there is no second
				// expression for the field to drift against.
				const ledger = runLedger(c.play(), c.rate);

				expect(ledger?.chainEndChaos).toBe(ledger!.investmentChaos + ledger!.roiChaos);
			});

			it('ends the row on its own investment plus its own measurement', () => {
				// E5, the same way, and the equation that holds in every branch of the
				// row including the exempt one.
				const ledger = runLedger(c.play(), c.rate);

				expect(ledger?.getChaos).toBe(ledger!.investmentChaos + ledger!.expectedRoiChaos);
			});

			it('re-derives the run’s investment from the wire’s per-exchange cost', () => {
				// A RE-DERIVATION, deliberately not a comparison against
				// `moneyColumns(play).investment` — that is the expression the field is
				// assigned from, and comparing a value to its own source asserts
				// nothing.
				const play = c.play();
				const ledger = runLedger(play, c.rate);

				expect(ledger?.investmentChaos).toBe(play.investment * ledger!.flips);
			});

			it('costs the run its own undercut entry price', () => {
				// E1 crossed with E2, in the entry currency. A cross-check, so it
				// carries the reassociation tolerance and nothing larger.
				const ledger = runLedger(c.play(), c.rate);

				expectClose(ledger!.spend, c.flips * c.u0);
			});

			if (c.u2 === null) {
				it('ends the chain on the run at the undercut sell price', () => {
					// The forward reading of a direct play's chain end, which the
					// backward one must agree with to within the reassociation.
					const ledger = runLedger(c.play(), c.rate);

					expectClose(ledger!.chainEnd, c.flips * c.u1);
				});
			} else {
				it('sells the run at the undercut sell price', () => {
					const ledger = runLedger(c.play(), c.rate);

					expectClose(ledger!.sellTotal!, c.flips * c.u1);
				});

				it('ends the chain on the run through both undercut prices', () => {
					const ledger = runLedger(c.play(), c.rate);

					expectClose(ledger!.chainEnd, c.flips * c.u1 * c.u2!);
				});
			}

			it('reads the printed ROI column back out of the two printed ends', () => {
				// E4, as the READER checks it — on the printed strings, not on
				// intermediate state. The ends print in the entry currency and the
				// column prints in chaos, so the assertion converts: comparing
				// `4.95 − 3.16 = 1.79` against `+359` is a different quantity, not a
				// weaker test.
				//
				// Three independently rounded values take part: each printed end
				// carries up to half a printed unit and the column carries half a
				// chaos, so the residual runs to 1 ulp + 0.5c/r — about one and a half
				// printed units on a divine entry (0.0125 div at r = 200). Hence TWO
				// printed units, not one. A chaos entry's three values are all
				// integers, so its residual is an integer and the bound collapses to 1
				// by integrality.
				const play = c.play();
				const route = routeSlots(play, c.rate);
				const ledger = runLedger(play, c.rate);
				const slack = c.entryIsChaos ? 1 : 0.02 * ledger!.entryRate;
				const chainEnd = parseAmount(printedChainEnd(route!.sell.rate, route!.convert?.rate));
				const spend = parseAmount(route!.spend.amount);

				expect(formatGain(moneyColumns(play).roi)).toBe(c.roiColumn);
				expect(
					Math.abs(
						(chainEnd - spend) * ledger!.entryRate - parseAmount(formatGain(moneyColumns(play).roi))
					)
				).toBeLessThanOrEqual(slack);
			});

			it('reads the printed Exp. ROI column back out of the two printed ends', () => {
				// E5 as the reader checks it, under the same conversion and the same
				// slack as the ROI half above.
				const play = c.play();
				const route = routeSlots(play, c.rate);
				const ledger = runLedger(play, c.rate);
				const slack = c.entryIsChaos ? 1 : 0.02 * ledger!.entryRate;
				const get = parseAmount(route!.get.amount);
				const spend = parseAmount(route!.spend.amount);

				expect(route?.get.amount).toBe(c.get);
				expect(formatGain(moneyColumns(play).expectedRoi)).toBe(c.expColumn);
				expect(
					Math.abs(
						(get - spend) * ledger!.entryRate -
							parseAmount(formatGain(moneyColumns(play).expectedRoi))
					)
				).toBeLessThanOrEqual(slack);
			});

			it('separates its two ends by exactly the gap between its two ROI columns', () => {
				// §5's permitted deviation, present and bounded on every row: the chain
				// end and the Get differ by `R − X` and by nothing else.
				//
				// A cross-check and not an exact pin, because both sides reassociate:
				// `(I + R) − (I + X)` is not `R − X` in float once `I` carries an ulp
				// of its own, which it does on five of these six fixtures.
				const ledger = runLedger(c.play(), c.rate);

				expectClose(
					ledger!.chainEndChaos - ledger!.getChaos,
					ledger!.roiChaos - ledger!.expectedRoiChaos
				);
			});

			it('draws the row by the sign of its measurement', () => {
				// `positive` styles the two numbers on screen, and under E5 the Get IS
				// the measurement — so the flag follows the Exp. ROI column and not the
				// best case.
				expect(routeSlots(c.play(), c.rate)?.positive).toBe(c.positive);
			});

			it('reads the entry’s chaos value back on the Spend sub-line', () => {
				// §4.1: the ends are in the currency the reader pays with, with a chaos
				// sub-line when that is not chaos — in BOTH scale branches, which is
				// the change the no-run rows carry.
				expect(routeSlots(c.play(), c.rate)?.spend.sub).toBe(c.spendSub);
			});

			if (!c.scaled) {
				it('counts a single exchange when there is no worthwhile run', () => {
					// The fallback is no longer a branch of its own: it is this same
					// code path at `flips === 1`.
					expect(runLedger(c.play(), c.rate)?.flips).toBe(1);
				});

				it('keeps the best case on the row, above the Spend it started from', () => {
					// The measured Get prints below the Spend here, and the row would be
					// unreadable if that were the only end on it. The hour's best case
					// is still there, as the last step's total.
					const route = routeSlots(c.play(), c.rate);
					const chainEnd = parseAmount(printedChainEnd(route!.sell.rate, route!.convert?.rate));

					expect(chainEnd).toBeGreaterThan(parseAmount(route!.spend.amount));
					expect(parseAmount(route!.get.amount)).toBeLessThan(parseAmount(route!.spend.amount));
				});
			}
		});
	}

	describe('F7 — a divine entry the hour cannot value in chaos', () => {
		// The ONE exempt branch (§3). `divineChaosRate` is 0, so there is no `r`,
		// and E2, E3, E4 and E7 have no entry-currency rendering to be stated in.
		// The row renders both ends in CHAOS from `moneyColumns` and prints the
		// markets' own ratios on the steps. Unreachable on a served body — no
		// divine-quoted play is served in an hour that carried no divine/chaos
		// trade — and guarded anyway, because the alternative is a division by zero
		// printed as an amount.
		//
		// `divineScarab()` at `flips 50`: `I = 631.25`, `X = 100`.
		it('draws no ledger, because there is no rate to state the chain in', () => {
			expect(runLedger(divineScarab(), 0)).toBeNull();
		});

		it('spends what the Investment column reports, in chaos', () => {
			const route = routeSlots(divineScarab(), 0);

			expect(route?.spend.amount).toBe(formatChaos(moneyColumns(divineScarab()).investment));
			expect(route?.spend.amount).toBe('631');
			expect(route?.spend.unit).toBe('chaos');
		});

		it('ends on the Spend plus the Exp. ROI column even here', () => {
			// E5 SURVIVES the suspension, and is the reason the branch is still a
			// closed row: the ends are `I` and `I + X` in chaos.
			const money = moneyColumns(divineScarab());
			const route = routeSlots(divineScarab(), 0);

			expect(route?.get.amount).toBe(formatChaos(money.investment + money.expectedRoi));
			expect(route?.get.amount).toBe('731');
		});

		it('gives both ends the chaos artwork the amounts are actually counted in', () => {
			// The ends fall back to chaos on this branch, so the tiles have to fall
			// back with them: the divine artwork this row's legs carry would put a
			// divine icon over an amount counted in chaos orbs.
			const route = routeSlots(divineScarab(), 0);

			expect(route?.spend.icon).toBe(chaosIconPath());
			expect(route?.get.icon).toBe(chaosIconPath());
			expect(route?.spend.icon).not.toBe(currencyIconPath(DIVINE_ID));
		});

		it('hands a dense reader the unit word and the profit line back on the hovers', () => {
			// Dense hides the unit beside the number and the sub-line under it, so on
			// this branch as on every other the titles are the only route back to
			// either — and the Spend, being chaos already, has no chaos reading to
			// carry underneath it.
			const route = routeSlots(divineScarab(), 0);

			expect(route?.spend.sub).toBeNull();
			expect(route?.spend.title).toBe('chaos');
			expect(route?.get.title).toBe('chaos — keep ≈ 100c');
		});

		it('prints each step’s own market ratio rather than a run total', () => {
			// §4.7. There is no chain to total, so the honest line is the pair the
			// market posts — the buy market 16 scarabs for a divine, the sell market
			// 10 for one.
			const route = routeSlots(divineScarab(), 0);

			expect(route?.buy.rate).toBe('buy 16 for 1 div');
			expect(route?.sell.rate).toBe('sell 10 for 1 div');
		});

		it('says on each step that its line counts a lot and the ends count the run', () => {
			// The caveat §4.7 requires of every `pairLine` caller: the line prints
			// this market's LOT quantity while the ends beside it count the run.
			const route = routeSlots(divineScarab(), 0);

			expect(route?.buy.rateTitle).toContain('one order of this market');
			expect(route?.sell.rateTitle).toContain('one order of this market');
		});

		it('draws the row by the sign of its measurement here too', () => {
			// Today's hardcoded `true` is wrong for a play that is both exempt and a
			// measured loser, and the flag has one rule on every branch.
			expect(routeSlots(divineScarab(), 0)?.positive).toBe(true);
			expect(routeSlots(divineScarab({ expectedRoi: -3 }), 0)?.positive).toBe(false);
		});
	});
});

describe('anyConvertStep', () => {
	/**
	 * One fully-shaped leg, reused for every position. `anyConvertStep` reads the
	 * LENGTH of the leg list and nothing inside a leg, so the cases below vary the
	 * count and hold the contents fixed — a fixture that also varied the contents
	 * would leave it unclear which of the two the answer came from.
	 */
	const someLeg: CurrencyExchangeLeg = {
		action: 'buy',
		item: 'scarab',
		quote: CHAOS_ID,
		price: 19,
		priceItemQty: 1,
		priceQuoteQty: 19,
		fair: 19.5,
		fairOk: true,
		tick: 0.01,
		volume: 2000,
		stock: 40,
		suspect: false,
		itemName: 'Ambush Scarab',
		itemIcon: '/icon/Scarab',
		itemCategory: 'Scarabs',
		quoteName: 'Chaos Orb',
		quoteIcon: '/currency-exchange/icon/Chaos',
		quoteCategory: 'Currency'
	};

	/** A play with `count` legs — two is a direct flip, three a 1-hop route. */
	function withLegs(count: number, overrides: Partial<CurrencyExchangePlay> = {}) {
		return play({ legs: Array.from({ length: count }, () => someLeg), ...overrides });
	}

	it('collapses the column when every rendered play is a direct flip', () => {
		// The case the collapse exists for: a table filtered to direct plays draws
		// an empty dashed tile and two arrows on every row, and the convert column
		// is dead width down the whole table.
		expect(anyConvertStep([withLegs(2), withLegs(2), withLegs(2)])).toBe(false);
	});

	it('shows the column when one rendered play converts', () => {
		// All-or-nothing over the set: the single 1-hop row needs the slot, so
		// every direct row around it keeps holding it open. Collapsing the rest
		// per-row would slide their Get under this row's sell.
		expect(anyConvertStep([withLegs(2), withLegs(3), withLegs(2)])).toBe(true);
	});

	it('reads the legs and not the mode label a play carries', () => {
		// `routeSlots` draws the convert slot off the THIRD LEG, by position. A
		// play labelled 1-hop that arrived with two legs has no third step to put
		// in the slot, so reserving one would spend the width on a tile nothing
		// can fill.
		expect(anyConvertStep([withLegs(2, { mode: '1-hop' })])).toBe(false);
	});

	it('collapses on an empty set rather than reserving a slot for no row', () => {
		// Unobservable in practice — the table is not rendered when nothing
		// survives the filters — but the answer is pinned so the collapse cannot
		// flip to "shown" on the way through an empty render.
		expect(anyConvertStep([])).toBe(false);
	});
});

describe('refetchDelay', () => {
	it('waits the bare debounce when the roll comes up lowest', () => {
		expect(refetchDelay(() => 0)).toBe(REFETCH_DEBOUNCE_MS);
		expect(refetchDelay(() => 0)).toBe(2000);
	});

	it('waits debounce plus the whole jitter window when the roll comes up highest', () => {
		expect(refetchDelay(() => 1)).toBe(REFETCH_DEBOUNCE_MS + REFETCH_JITTER_MS);
		expect(refetchDelay(() => 1)).toBe(6000);
	});

	it('spreads the roll linearly across the window', () => {
		// A mid roll must land mid-window, not at an endpoint — the whole point of
		// the jitter is that clients receiving the same publish do not all refetch
		// in the same slot.
		expect(refetchDelay(() => 0.25)).toBe(3000);
	});

	it('re-rolls on every call instead of caching one offset per session', () => {
		// A fixed per-client offset would spread the herd once and then put the
		// same clients in the same slot on every subsequent publish.
		const rolls = [0, 0.5, 1];
		let i = 0;
		const random = () => rolls[i++];

		expect([refetchDelay(random), refetchDelay(random), refetchDelay(random)]).toEqual([
			2000, 4000, 6000
		]);
	});
});
