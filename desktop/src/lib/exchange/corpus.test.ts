/**
 * THE CROSS-LAYER CORPUS TIER: the desktop's row derivations run over the
 * ENGINE'S OWN golden wire bytes.
 *
 * `view.test.ts` and `filters.test.ts` pin this surface against HAND-BUILT
 * fixtures. That is the right shape for an equation — a hand-worked case states
 * its `u0`, `u1` and back-solved `roiPct` in a comment and pins literals against
 * them — and it has one blind spot that has cost this project real markets: a
 * hand-built fixture is a row the DESKTOP AUTHOR believes the server sends. When
 * the Go side changes what it serves, or the shape it serves it in, every
 * hand-built case stays green.
 *
 * This file closes that seam. It reads `internal/exchange/testdata/wire/*.json`
 * — the committed output of `TestCorpus_wireGolden`, which marshals the engine's
 * own `exchange.Result` over a deterministic feed carrying every currency-exchange
 * regression this surface has actually shipped — and re-runs the desktop's row
 * closure and filter derivations over those exact bytes. A Go-side change to what
 * is served now breaks a DESKTOP test named after the incident it re-opened.
 *
 * WHAT THE FIXTURES CARRY (fifteen served plays, both horizons):
 *
 * - the Apocalypse card's spreadless newest hour (2026-08-22, ADR-017's first
 *   amendment) — a best-case LOSS with a positive measured expectation;
 * - the Journey Tattoo sold into a one-sided book (2026-08-23, ADR-017's second
 *   amendment) — served as a 1-hop whose SELL leg carries `depletedSide`;
 * - the 2026-08-23 screenshot's spreadless-170 print — a measured loser;
 * - Mawr Blaidd's junk low in the newest hour (POE-188) — served `suspect`;
 * - the Divine Vessel's 100% tick (POE-184) — a −100% round trip with no run;
 * - a recipe too young to have an expectation (`lowCoverage`);
 * - and the clean direct / divine-quoted / 1-hop shapes they are ranked against.
 *
 * WHAT IT ASSERTS, and against what: the equations of
 * `docs/CURRENCY-EXCHANGE-ROW-INVARIANT.md` §3 (E1–E8) and the rendering rules of
 * §4, on EMITTED values, for every served play in both fixtures — the same battery
 * `view.test.ts`'s `describe('row closure')` runs on its eleven hand-built cases,
 * now over rows nobody on this side wrote.
 *
 * WHAT IT DOES NOT ASSERT. The spreadless-170 row's step lines read `buy 1 for
 * ≈ 171c` / `sell 1 for ≈ 169c` on a market whose newest hour printed 170 on both
 * sides. Whether those undercut fill prices are the DESIRED display on a
 * spreadless market is an open owner decision, and nothing here endorses them:
 * the generic §3 equations apply to that row like any other, and no assertion in
 * this file pins those two strings as correct. See the incident pin below.
 *
 * ONE COPY OF THE FIXTURE, AT ITS CANONICAL PATH. The two imports below reach up
 * out of `desktop/` into `internal/exchange/testdata/wire/`, and they do that
 * rather than reading a copy vendored under `desktop/` because a second copy is a
 * thing that drifts — and the drift would be invisible in exactly the case this
 * tier exists to catch, a Go-side change that regenerates the golden bytes. Vitest
 * resolves the out-of-root import in the node environment with no config change;
 * `make desktop-test-js` needed the fixture directory mounted read-only into the
 * desktop container (`docker-compose.yml`), which is the whole cost of keeping one
 * copy.
 *
 * WRITTEN 2026-08-23, against the fixtures produced by `internal/exchange/corpus_test.go`.
 */
import { describe, it, expect } from 'vitest';
import dayWire from '../../../../internal/exchange/testdata/wire/day.json';
import recentWire from '../../../../internal/exchange/testdata/wire/recent.json';
import {
	CHAOS_ID,
	DIVINE_ID,
	SCALE_TARGET_CHAOS,
	displayScale,
	formatChaos,
	formatGain,
	moneyColumns,
	parseUnit,
	routeSlots,
	runLedger,
	sortPlays,
	worthwhileScale
} from './view';
import type { ExchangeSort } from './view';
import {
	applyGates,
	applyNumericFilters,
	applyRules,
	gateDefaults,
	matchesSearch,
	parseCategoryRules,
	parseGates,
	parseItemRules
} from './filters';
import type { GateInputs } from './filters';
import type {
	CurrencyExchangeLeg,
	CurrencyExchangePlay,
	CurrencyExchangeResponse
} from '$lib/api';

// ------------------------------------------------------ the wire, as it lands --

/**
 * The engine's `exchange.Leg`, as `json.MarshalIndent` writes it.
 *
 * Declared here rather than reused from `$lib/api` because the two are
 * deliberately different shapes: `CurrencyExchangeLeg` is what the DESKTOP
 * receives, which is this plus the six display fields the transport layer adds.
 * Keeping the engine's shape stated separately is what lets `wrapAsResponse`
 * below be a decoration rather than a cast, and what makes an engine field
 * disappearing a type error here instead of an `undefined` three assertions
 * downstream.
 */
interface WireLeg {
	action: string;
	item: string;
	quote: string;
	price: number;
	priceItemQty: number;
	priceQuoteQty: number;
	fair: number;
	fairOk: boolean;
	tick: number;
	volume: number;
	stock: number;
	depletedSide: boolean;
	suspect: boolean;
}

/** The engine's `exchange.Play`. */
interface WirePlay {
	key: string;
	mode: string;
	legs: WireLeg[];
	roiPct: number;
	edge: number;
	roiPctRaw: number;
	roi: number;
	investment: number;
	turnover: number;
	tick: number;
	depth: number;
	suspect: boolean;
	lowLiquidity: boolean;
	hoursSeen: number;
	expectedRoi: number;
	expectedRoiPct: number;
	simEntries: number;
	lowCoverage: boolean;
	lastHour: string;
}

/** The engine's `exchange.Result` — the whole committed fixture. */
interface WireResult {
	league: string;
	horizon: string;
	from: string;
	to: string;
	hours: number;
	divineChaosRate: number;
	plays: WirePlay[];
}

/**
 * The sixteen sidebar categories `exchange.Categories()` returns, which the
 * HANDLER puts on every body independent of the plays in it.
 *
 * Copied rather than derived, because the engine fixture does not carry them:
 * they are the transport layer's, and this list exists so the wrapped response
 * is the shape the page actually receives rather than one missing a field.
 */
const CATEGORIES = [
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
];

/**
 * The prefix every display string this file invents carries.
 *
 * The engine deliberately ships raw feed ids and no names — `internal/server/handlers`
 * is what resolves them out of the item asset — so a wrapped fixture has to put
 * SOMETHING in `itemName`/`quoteName`. Making the invention loud is the point: a
 * failure message reading `synthetic:MapMawrBlaidd` can never be mistaken for a
 * claim about what the server actually names that market, and no assertion in
 * this file reads a name for anything but identification.
 */
const SYNTHETIC = 'synthetic:';

/**
 * A feed id as an obviously-invented display name: the id's last path segment
 * behind the `synthetic:` marker.
 */
function syntheticName(id: string): string {
	return `${SYNTHETIC}${id.slice(id.lastIndexOf('/') + 1)}`;
}

/**
 * The engine fixture decorated into the body the desktop actually receives.
 *
 * THE ADDITIVE-ENVELOPE CLAIM, verified against
 * `internal/server/handlers/currency_exchange.go` on 2026-08-23 and restated here
 * because this whole file rests on it. `playResponse` EMBEDS `exchange.Play` and
 * `legResponse` EMBEDS `exchange.Leg`, so every engine key and every engine value
 * reaches the client byte-identical; `legResponse` adds six display fields
 * (`itemName`/`itemIcon`/`itemCategory` and their quote twins) and shadows `legs`
 * with the decorated slice; `playsResponse` adds `lastUpdated`, `warm`, `mode`,
 * `count` and `categories` around the envelope and makes `from`/`to`/`lastUpdated`
 * nullable. Nothing is renamed, nothing is dropped, no number is recomputed. That
 * is what makes the engine's golden bytes a legitimate desktop fixture at all.
 *
 * THE DISPLAY FIELDS ARE INERT ON PURPOSE. Icons are `null` — the fixture vouches
 * for no artwork — and BOTH categories are `''`, which is the wire's own
 * "uncategorised" marker and which every filter treats as UNFILTERED
 * (`effectiveRule`). Inventing categories would put a fabricated taxonomy under
 * the visibility test and let it pass or fail for a reason the engine never sent.
 * The cost is stated rather than hidden: a future DEFAULT category rule would not
 * be caught here, because no play in this corpus is filed under a category at all.
 */
function wrapAsResponse(wire: WireResult): CurrencyExchangeResponse {
	const plays = wire.plays.map(
		(play): CurrencyExchangePlay => ({
			...play,
			mode: play.mode === '1-hop' ? '1-hop' : 'direct',
			legs: play.legs.map(
				(leg): CurrencyExchangeLeg => ({
					...leg,
					action: leg.action === 'sell' ? 'sell' : 'buy',
					itemName: syntheticName(leg.item),
					itemIcon: null,
					itemCategory: '',
					quoteName: syntheticName(leg.quote),
					quoteIcon: null,
					quoteCategory: ''
				})
			)
		})
	);

	return {
		league: wire.league,
		lastUpdated: wire.to,
		from: wire.from,
		to: wire.to,
		hours: wire.hours,
		warm: true,
		mode: 'all',
		horizon: wire.horizon === 'day' ? 'day' : 'recent',
		divineChaosRate: wire.divineChaosRate,
		count: plays.length,
		plays,
		categories: CATEGORIES
	};
}

const RECENT = wrapAsResponse(recentWire as WireResult);
const DAY = wrapAsResponse(dayWire as WireResult);

/** Both committed horizons, so a rule that only fires on one window is caught. */
const FIXTURES: { horizon: string; response: CurrencyExchangeResponse }[] = [
	{ horizon: 'recent', response: RECENT },
	{ horizon: 'day', response: DAY }
];

// ------------------------------------------------------------ the incidents --

/**
 * The recipe keys the corpus is about, mirroring the `var` block in
 * `internal/exchange/corpus_test.go` so a failure on this side names the same
 * market the Go side does. Spelled out rather than built from id constants,
 * because the KEY FORMAT is itself part of the wire contract
 * (`direct:<quote>|<item>`, `1-hop:<item>|<buyQuote>|<sellQuote>`) and a change to
 * it should fail here rather than be papered over by a shared builder.
 */
const CHAOS = CHAOS_ID;
const DIVINE = DIVINE_ID;
const APOCALYPSE_KEY = `direct:${CHAOS}|Metadata/Items/DivinationCards/DivinationCardApocalypse`;
const TATTOO_SELL_KEY = `1-hop:Metadata/Items/Tattoos/JourneyTattoo|${DIVINE}|${CHAOS}`;
const TATTOO_TWIN_KEY = `direct:${CHAOS}|Metadata/Items/Tattoos/JourneyTattooTwoSided`;
const TATTOO_DIVINE_DIRECT_KEY = `direct:${DIVINE}|Metadata/Items/Tattoos/JourneyTattoo`;
const SPREADLESS_KEY = `direct:${CHAOS}|Metadata/Items/Currency/CurrencySpreadless170`;
const MAWR_KEY = `direct:${CHAOS}|Metadata/Items/Maps/MapMawrBlaidd`;
const MAWR_JUNK_KEY = `direct:${CHAOS}|Metadata/Items/Maps/MapMawrBlaiddJunkNewest`;
const VESSEL_KEY = `direct:${CHAOS}|Metadata/Items/Currency/CurrencyDivineVessel`;
const CLEAN_FLIP_KEY = `direct:${CHAOS}|Metadata/Items/Currency/CurrencyCleanFlip`;
const YOUNG_KEY = `direct:${CHAOS}|Metadata/Items/Currency/CurrencyYoungRecipe`;
const SCARAB_HOP_KEY = `1-hop:Metadata/Items/Scarabs/ScarabDomination3|${CHAOS}|${DIVINE}`;
const SCARAB_DIVINE_KEY = `direct:${DIVINE}|Metadata/Items/Scarabs/ScarabDomination3`;
const SCARAB_CHAOS_KEY = `direct:${CHAOS}|Metadata/Items/Scarabs/ScarabDomination3`;
const SCARAB_HOP_DIVINE_KEY = `1-hop:Metadata/Items/Scarabs/ScarabDomination3|${DIVINE}|${CHAOS}`;
const DIVINE_ANCHOR_KEY = `direct:${CHAOS}|${DIVINE}`;

/**
 * Every served key, named after the incident or the shape it carries.
 *
 * The names are what the visibility test asserts on, so a default that starts
 * hiding a row fails with a sentence naming the market and its date rather than
 * with a diff of `Metadata/Items/...` paths. The eleven names the Go corpus
 * already uses are copied verbatim from
 * `TestCorpus_everyIncidentMarket_isServedInBothHorizons`; the four the Go list
 * does not name are described by the spec that produces them.
 */
const INCIDENT_NAMES: Record<string, string> = {
	[CLEAN_FLIP_KEY]: 'a clean profitable flip',
	[APOCALYPSE_KEY]: 'Apocalypse card, spreadless newest hour (2026-08-22)',
	[MAWR_KEY]: 'Mawr Blaidd, junk lows behind the newest hour (POE-188)',
	[TATTOO_TWIN_KEY]: 'Journey Tattoo twin, both sides alive',
	[TATTOO_DIVINE_DIRECT_KEY]: 'Journey Tattoo against divine, a direct flip',
	[TATTOO_SELL_KEY]: 'Journey Tattoo, sold into a one-sided book (2026-08-23)',
	[SCARAB_HOP_KEY]: 'a one-hop triangle',
	[SCARAB_DIVINE_KEY]: 'a divine-quoted flip',
	[DIVINE_ANCHOR_KEY]: 'the divine/chaos anchor market, a two-tick loser',
	[SCARAB_CHAOS_KEY]: 'the scarab against chaos, a two-tick loser',
	[SCARAB_HOP_DIVINE_KEY]: 'the scarab triangle entered in divine, a two-tick loser',
	[SPREADLESS_KEY]: 'the 2026-08-23 screenshot\'s spreadless 170 print',
	[YOUNG_KEY]: 'a recipe too young to have an expectation',
	[MAWR_JUNK_KEY]: 'Mawr Blaidd, junk low IN the newest hour (POE-188)',
	[VESSEL_KEY]: 'Divine Vessel, a 100% tick (POE-184)'
};

/** One play's incident name, or the raw key when the corpus grew a row this file has never seen. */
function incidentName(play: CurrencyExchangePlay): string {
	return INCIDENT_NAMES[play.key] ?? `UNNAMED CORPUS ROW ${play.key}`;
}

/** A list of plays as the names a failure message should read. */
function names(plays: CurrencyExchangePlay[]): string[] {
	return plays.map(incidentName);
}

/**
 * `displayScale` for every served key, as LITERALS.
 *
 * §7 of the invariant spec: a case may not assert its own arithmetic at whatever
 * size the code happened to choose, so the count and the branch that chose it are
 * pinned before anything else is read, and the T3 cross-checks multiply THIS
 * number rather than the ledger's own. Every value here is one minimal posting of
 * the row's BUY market, read off the engine's `priceItemQty` by hand.
 */
const POSTINGS: Record<string, { units: number; basis: 'posting' | 'single' }> = {
	[CLEAN_FLIP_KEY]: { units: 1, basis: 'posting' },
	[APOCALYPSE_KEY]: { units: 1, basis: 'posting' },
	[MAWR_KEY]: { units: 1, basis: 'posting' },
	[TATTOO_TWIN_KEY]: { units: 1, basis: 'posting' },
	[TATTOO_DIVINE_DIRECT_KEY]: { units: 25, basis: 'posting' },
	[TATTOO_SELL_KEY]: { units: 25, basis: 'posting' },
	[SCARAB_HOP_KEY]: { units: 1, basis: 'posting' },
	[SCARAB_DIVINE_KEY]: { units: 20, basis: 'posting' },
	[DIVINE_ANCHOR_KEY]: { units: 200, basis: 'posting' },
	[SCARAB_CHAOS_KEY]: { units: 1, basis: 'posting' },
	[SCARAB_HOP_DIVINE_KEY]: { units: 20, basis: 'posting' },
	[SPREADLESS_KEY]: { units: 1, basis: 'posting' },
	[YOUNG_KEY]: { units: 1, basis: 'posting' },
	[MAWR_JUNK_KEY]: { units: 1, basis: 'posting' },
	[VESSEL_KEY]: { units: 35, basis: 'posting' }
};

// ---------------------------------------------------------------- the tools --

/**
 * Two figures that must be the same number by different routes.
 *
 * DUPLICATED from `view.test.ts`'s closure suite, which declares it inside that
 * suite's `describe` and exports nothing. Copying three helpers is cheaper than
 * editing a file whose own cases this change must not touch — the seam is stated
 * here rather than closed, and if a third consumer ever appears the three belong
 * in a shared module.
 *
 * The tolerance is §7's T3 rule and nothing looser: the wire's identities
 * reassociate in float (`19*1.01*200` is not `19.19*200`), and every cross-check
 * below deliberately recomputes a figure the ledger derived once.
 */
function expectClose(actual: number, expected: number, rel = 1e-9): void {
	expect(Math.abs(actual - expected)).toBeLessThanOrEqual(rel * Math.max(1, Math.abs(expected)));
}

/** A printed amount read back the way the reader reads it: sign, digits, point. */
function parseAmount(printed: string): number {
	return Number(printed.replace(/[^0-9.-]/g, ''));
}

/**
 * The last mechanical total the row emits, as a string: the sell step's total on
 * a direct play, the convert line's right-hand amount on a 1-hop.
 */
function printedChainEnd(sellRate: string, convertRate: string | undefined): string {
	const line = convertRate ?? sellRate;
	const marker = convertRate === undefined ? 'for ≈ ' : '→ ';
	return line.slice(line.lastIndexOf(marker) + marker.length);
}

/** The undercut BUY price of leg 1, in entry-quote units per item: `u0` of §2. */
function undercutBuy(play: CurrencyExchangePlay): number {
	return play.legs[0].price * (1 + play.legs[0].tick);
}

/** The undercut SELL price of a leg, in that leg's own quote per item: `u1`/`u2` of §2. */
function undercutSell(leg: CurrencyExchangeLeg): number {
	return leg.price * (1 - leg.tick);
}

/** One play out of a response, by key — a `Fatal` rather than an `undefined` two lines down. */
function playByKey(response: CurrencyExchangeResponse, key: string): CurrencyExchangePlay {
	const found = response.plays.find((p) => p.key === key);
	if (found === undefined) {
		throw new Error(`${INCIDENT_NAMES[key] ?? key} is not in the fixture: served ${names(response.plays).join(', ')}`);
	}
	return found;
}

/**
 * A FRESH INSTALL's filter pass, in the order the page composes it.
 *
 * Every layer is fed the state a never-touched install actually stores — the
 * empty `persisted()` strings of ADR-013 — and parsed through this file's own
 * parsers rather than hand-built, so a build that changes what an unset knob
 * means reaches this test the same way it reaches the reader. `parseGates` over
 * seven blank strings resolves to `gateDefaults`, which is the two sanctioned
 * trash-price floors armed and the other five off; `parseCategoryRules('')` and
 * `parseItemRules('')` answer no rules; both investment bounds are unset; the
 * search box is empty.
 */
function freshInstallInputs(): GateInputs {
	return {
		minItemPrice: '',
		minItemPriceDiv: '',
		minRoiChaos: '',
		minTurnover: '',
		maxTickPct: '',
		minEdgeTickRatio: '',
		minRoiPct: ''
	};
}

function freshInstallPass(response: CurrencyExchangeResponse): CurrencyExchangePlay[] {
	const ruled = applyRules(response.plays, parseCategoryRules(''), parseItemRules(''));
	const gated = applyGates(ruled, parseGates(freshInstallInputs()), response.divineChaosRate);
	const bounded = applyNumericFilters(gated, {
		investMin: '',
		investMax: '',
		unit: parseUnit(''),
		divineChaosRate: response.divineChaosRate
	});
	return bounded.filter((play) => matchesSearch(play, ''));
}

// ------------------------------------------------------------ the shape pin --

describe('the engine fixtures', () => {
	for (const { horizon, response } of FIXTURES) {
		describe(horizon, () => {
			it('carries every wire field the desktop’s derivations read', () => {
				// The one test that fails FIRST and clearly when the Go side renames or
				// drops a field. Everything else in this file would fail too, as an
				// `undefined` propagating into an arithmetic assertion three layers
				// down; this says which field went missing on which market.
				for (const play of response.plays) {
					const where = incidentName(play);

					for (const field of [
						'roiPct',
						'roi',
						'investment',
						'expectedRoi',
						'expectedRoiPct',
						'turnover',
						'tick',
						'depth'
					] as const) {
						expect(`${where}.${field}=${typeof play[field]}`).toBe(`${where}.${field}=number`);
						expect(`${where}.${field} finite=${Number.isFinite(play[field])}`).toBe(
							`${where}.${field} finite=true`
						);
					}
					expect(`${where}.suspect=${typeof play.suspect}`).toBe(`${where}.suspect=boolean`);
					expect(`${where}.lowCoverage=${typeof play.lowCoverage}`).toBe(
						`${where}.lowCoverage=boolean`
					);
					expect(`${where}.legs=${play.legs.length}`).toBe(
						`${where}.legs=${play.mode === '1-hop' ? 3 : 2}`
					);

					for (const [index, leg] of play.legs.entries()) {
						for (const field of ['price', 'priceItemQty', 'priceQuoteQty', 'tick'] as const) {
							expect(`${where} leg ${index}.${field}=${typeof leg[field]}`).toBe(
								`${where} leg ${index}.${field}=number`
							);
						}
						expect(`${where} leg ${index}.depletedSide=${typeof leg.depletedSide}`).toBe(
							`${where} leg ${index}.depletedSide=boolean`
						);
					}
				}
			});

			it('quotes every entry in a currency this response can value in chaos', () => {
				// The precondition under E1–E4 and E7: `chaosPerQuote` answers `null` for
				// anything but chaos and divine, and for divine at a zero rate. A served
				// body cannot carry either shape (`plays.go:678-681`, `plays.go:776`), and
				// this is what says the fixture is still a served body — the whole closure
				// battery below would otherwise SKIP silently through a null ledger.
				expect(response.divineChaosRate).toBeGreaterThan(0);
				for (const play of response.plays) {
					expect(`${incidentName(play)} enters in ${play.legs[0].quote}`).toBe(
						`${incidentName(play)} enters in ${play.legs[0].quote === DIVINE ? DIVINE : CHAOS}`
					);
				}
			});
		});
	}

	it('serves the same fifteen recipes in both horizons, in the same order', () => {
		// The horizons differ in window length and therefore in `hoursSeen`, and in
		// nothing else the desktop reads. That is a fact about the corpus feed rather
		// than a rule — but a horizon that started dropping a market, or reordering
		// the ranking against the other, is exactly the kind of change this tier
		// exists to surface, so it is pinned rather than assumed.
		expect(names(DAY.plays)).toEqual(names(RECENT.plays));
	});

	it('names every served recipe, so no row can slip through this file unasserted', () => {
		// The guard on the guard. Every loop below is driven by the fixture's own
		// plays, so a row the Go corpus adds is picked up automatically — but its
		// posting literal and its incident name would be missing, and a `??` fallback
		// would let it run the battery anonymously. It fails here instead.
		for (const play of RECENT.plays) {
			expect(`${play.key} named=${play.key in INCIDENT_NAMES}`).toBe(`${play.key} named=true`);
			expect(`${play.key} sized=${play.key in POSTINGS}`).toBe(`${play.key} sized=true`);
		}
		expect(RECENT.plays.length).toBe(Object.keys(INCIDENT_NAMES).length);
	});
});

// ----------------------------------------------------- closure over the wire --

/**
 * §3's equations and §4's rendering rules, on every served play of both
 * fixtures.
 *
 * This is `view.test.ts`'s closure battery with the hand-built case table
 * replaced by the engine's own rows. What it loses is the hand-worked expected
 * STRINGS — an engine row's `spend` is whatever the feed produced, and pinning
 * thirty of those as literals would pin the fixture rather than the equations.
 * What it gains is that the rows are the server's. The two tiers are
 * complementary and neither replaces the other: `view.test.ts` says the row reads
 * `buy 16 for ≈ 1.01 div`, this says every row the server actually sends closes.
 *
 * Every assertion below is therefore RELATIONAL — one emitted figure against
 * another emitted figure, or against the wire fields and the case's own literal
 * `units`. The only literals are `POSTINGS`, which §7 requires.
 */
for (const { horizon, response } of FIXTURES) {
	for (const served of response.plays) {
		const key = served.key;
		const label = `${horizon} — ${INCIDENT_NAMES[key] ?? key}`;
		const posting = POSTINGS[key] ?? { units: NaN, basis: 'posting' as const };
		const rate = response.divineChaosRate;
		const entryIsChaos = served.legs[0].quote === CHAOS;
		const suffix = entryIsChaos ? 'c' : ' div';
		const isOneHop = served.legs.length > 2;
		// A fresh read per test, so no case can be observed after another touched it.
		const play = () => playByKey(response, key);

		describe(`row closure — ${label}`, () => {
			it('sizes the row by one decision, and says which rule took it', () => {
				// §1, SCALE, pinned before anything else is read: the count and the
				// branch that chose it. A rule that started answering the run — or one
				// item — on this row fails here first and everything after.
				expect(displayScale(play())).toEqual(posting);
				expect(runLedger(play(), rate)?.units).toBe(posting.units);
			});

			it('re-derives the row’s investment from the wire’s per-exchange cost', () => {
				// T2. A RE-DERIVATION and deliberately not a comparison against
				// `moneyColumns(play).investment`, which is the expression the field is
				// assigned from. The multiplier is the case's own literal, so a ledger
				// that took the wrong scale cannot satisfy this by being consistently
				// wrong.
				const p = play();

				expect(runLedger(p, rate)?.investmentChaos).toBe(p.investment * posting.units);
			});

			it('ends the chain on its own investment plus its own best case', () => {
				// E3, as a single float addition of the ledger's own roots — closure by
				// CONSTRUCTION, with no second expression to drift against.
				const ledger = runLedger(play(), rate);

				expect(ledger?.chainEndChaos).toBe(ledger!.investmentChaos + ledger!.roiChaos);
			});

			it('ends the row on its own investment plus its own measurement', () => {
				// E5, the equation that survives every branch of the row including the
				// exempt one, and the one the D5 regression broke: a `getChaos` built
				// from `roi` instead of `expectedRoi` fails here on every corpus row
				// whose two ROI figures differ, which is all fifteen.
				const ledger = runLedger(play(), rate);

				expect(ledger?.getChaos).toBe(ledger!.investmentChaos + ledger!.expectedRoiChaos);
			});

			it('costs the row its own undercut entry price', () => {
				// E1 crossed with E2, in the entry currency: `spend = N · u0`. A T3
				// cross-check, so it carries the reassociation tolerance and nothing
				// larger.
				const p = play();

				expectClose(runLedger(p, rate)!.spend, posting.units * undercutBuy(p));
			});

			it('prints the buy step’s total as the Spend end’s own string', () => {
				// E2 and §4.2. Two assertions rather than one, because `RouteEnd.amount`
				// is BARE — its unit word is a separate span in `ExchangeRoute.svelte` —
				// while a step total carries its unit. Either half alone can pass while
				// the two numbers differ.
				const route = routeSlots(play(), rate);

				expect(route?.buy.rate.startsWith('buy ')).toBe(true);
				expect(route?.buy.rate.endsWith(`for ≈ ${route?.spend.amount}${suffix}`)).toBe(true);
			});

			it('prices every step at the undercut fill price, and says so with ≈', () => {
				// §4.3. The `≈ ` — space included — is the claim a step line makes and
				// the claim it does not: a total at the undercut fill prices, not the
				// exact order anyone posts. The count before it is exact and carries no
				// `≈`.
				const route = routeSlots(play(), rate);

				expect(route?.buy.rate).toContain('for ≈ ');
				expect(route?.sell.rate).toContain('≈ ');
				expect(route?.spend.amount).not.toContain('≈');
				expect(route?.get.amount).not.toContain('≈');
			});

			it('words the profit line with the Exp. ROI column’s own chaos', () => {
				// E6 and §4.5: one variable in two homes, the VERB carrying the sign and
				// the amount carrying the magnitude. Sign-aware, so it runs on the
				// corpus's measured losers as well as its winners.
				const p = play();
				const column = moneyColumns(p).expectedRoi;
				const verb = column < 0 ? 'lose' : 'keep';

				expect(routeSlots(p, rate)?.get.sub?.startsWith(`${verb} ≈ ${formatChaos(Math.abs(column))}c`)).toBe(
					true
				);
			});

			it('draws the row by the sign of its measurement', () => {
				// THE D5 PIN, generalised. `positive` styles both numbers on screen and
				// under E5 the Get IS the measurement, so the flag follows the Exp. ROI
				// column and never the best case. The corpus carries rows where the two
				// disagree in BOTH directions — the Apocalypse card's best case loses
				// while its measurement wins — so a `positive` re-pointed at `roi` fails
				// here rather than only on a fixture somebody thought to write.
				const p = play();

				expect(routeSlots(p, rate)?.positive).toBe(moneyColumns(p).expectedRoi > 0);
			});

			it('reads the entry’s chaos value back on the Spend sub-line', () => {
				// §4.1: the ends are in the currency the reader pays with, with a chaos
				// sub-line when that is not chaos — and none when it is.
				const p = play();
				const ledger = runLedger(p, rate)!;

				expect(routeSlots(p, rate)?.spend.sub).toBe(
					entryIsChaos ? null : `≈ ${formatChaos(ledger.investmentChaos)}c`
				);
			});

			if (isOneHop) {
				it('sells the row’s count at the undercut sell price', () => {
					// E7's 1-hop left half, forward-derived as a T3 cross-check against
					// the backward `chainEnd / u2` the ledger actually computes.
					const p = play();

					expectClose(runLedger(p, rate)!.sellTotal!, posting.units * undercutSell(p.legs[1]));
				});

				it('ends the chain on the row’s count through both undercut prices', () => {
					const p = play();

					expectClose(
						runLedger(p, rate)!.chainEnd,
						posting.units * undercutSell(p.legs[1]) * undercutSell(p.legs[2])
					);
				});

				it('prints the sell step’s total as the convert line’s own left amount', () => {
					// E7's rendering half, pinned the same two ways the buy/Spend pair is.
					const route = routeSlots(play(), rate);
					const sold = route!.sell.rate.slice(
						route!.sell.rate.lastIndexOf('for ≈ ') + 'for ≈ '.length
					);

					expect(route?.convert?.rate.startsWith(`≈ ${sold} →`)).toBe(true);
				});
			} else {
				it('ends the chain on the row’s count at the undercut sell price', () => {
					// E7's direct form: the sell step's total IS the chain end, so the
					// forward reading and the backward one have to agree.
					const p = play();

					expectClose(runLedger(p, rate)!.chainEnd, posting.units * undercutSell(p.legs[1]));
				});
			}

			it('reads the printed ROI column back out of the two printed ends', () => {
				// E4/T4, as the READER checks it — on printed strings, not on
				// intermediate state. The ends print in the entry currency and the column
				// prints in chaos, so the assertion CONVERTS; comparing an entry-currency
				// difference against a chaos column is a different quantity, not a weaker
				// test. Three independently rounded values take part, hence §7's two
				// printed units of slack, which collapses to 1 by integrality on a chaos
				// entry.
				const p = play();
				const route = routeSlots(p, rate)!;
				const ledger = runLedger(p, rate)!;
				const slack = entryIsChaos ? 1 : 0.02 * ledger.entryRate;
				const chainEnd = parseAmount(printedChainEnd(route.sell.rate, route.convert?.rate));
				const spend = parseAmount(route.spend.amount);

				expect(
					Math.abs(
						(chainEnd - spend) * ledger.entryRate - parseAmount(formatGain(moneyColumns(p).roi))
					)
				).toBeLessThanOrEqual(slack);
			});

			it('reads the printed Exp. ROI column back out of the two printed ends', () => {
				// E5 as the reader checks it, under the same conversion and the same
				// slack as the ROI half above.
				const p = play();
				const route = routeSlots(p, rate)!;
				const ledger = runLedger(p, rate)!;
				const slack = entryIsChaos ? 1 : 0.02 * ledger.entryRate;
				const get = parseAmount(route.get.amount);
				const spend = parseAmount(route.spend.amount);

				expect(
					Math.abs(
						(get - spend) * ledger.entryRate -
							parseAmount(formatGain(moneyColumns(p).expectedRoi))
					)
				).toBeLessThanOrEqual(slack);
			});

			it('separates its two ends by exactly the gap between its two ROI columns', () => {
				// §5's permitted deviation, present and bounded on every row: the chain
				// end and the Get differ by `R − X` and by nothing else. A cross-check
				// rather than an exact pin, because `(I + R) − (I + X)` is not `R − X` in
				// float once `I` carries an ulp of its own.
				const ledger = runLedger(play(), rate);

				expectClose(
					ledger!.chainEndChaos - ledger!.getChaos,
					ledger!.roiChaos - ledger!.expectedRoiChaos
				);
			});

			it('answers the run and the posting as two separate questions', () => {
				// E8. The Scale column prints the RUN's gain (`expectedRoi · F`) and the
				// Exp. ROI column prints one POSTING's (`expectedRoi · N`), and the doc is
				// explicit that no rule makes them equal — they coincide only where a
				// market's lot happens to land on the flip count.
				//
				// The corpus carries BOTH arms, which is why this is stated as an
				// equivalence rather than as a divergence: the clean flip posts one at a
				// time and needs one exchange to clear the target, so its two figures ARE
				// equal, while eight other rows diverge. A `moneyColumns` re-multiplied by
				// the flip count would make the two equal everywhere and fail on those
				// eight; a `worthwhileScale.gain` re-based on the posting would fail on
				// them too.
				const p = play();
				const run = worthwhileScale(p);
				if (run === null) {
					expect(p.expectedRoi).toBeLessThanOrEqual(0);
					return;
				}

				expect(run.flips).toBe(Math.ceil(SCALE_TARGET_CHAOS / p.expectedRoi));
				expect(moneyColumns(p).expectedRoi).toBe(p.expectedRoi * posting.units);
				expect(run.gain === moneyColumns(p).expectedRoi).toBe(run.flips === posting.units);
			});

			it('discloses a posting that counts past its own run, and only then', () => {
				// §1's `N > F` reading and §4.6's clause. The posting is the minimal
				// executable trade, so a row whose market posts more than the run counts
				// past it — legal, unclamped, and disclosed on the BUY step's hover alone,
				// because the sentence names the ENTRY order's lot against the run and the
				// sell market's lot has no claim on that comparison.
				const p = play();
				const run = worthwhileScale(p);
				const overshoots = run !== null && posting.units > run.flips;
				const route = routeSlots(p, rate)!;

				expect(route.buy.rateTitle?.includes('more than the ×') ?? false).toBe(overshoots);
				expect(route.sell.rateTitle?.includes('more than the ×') ?? false).toBe(false);
			});
		});
	}
}

describe('row closure — the corpus’s own coverage', () => {
	it('carries rows on both sides of the run/posting divergence', () => {
		// The guard that keeps E8's equivalence from being vacuous. It is asserted as
		// `(gain === column) === (flips === units)`, which passes trivially if every
		// row lands on one arm — so the corpus is held to carrying both.
		const withRun = RECENT.plays.filter((p) => worthwhileScale(p) !== null);
		const coincide = withRun.filter((p) => worthwhileScale(p)!.flips === POSTINGS[p.key].units);

		expect(coincide.length).toBeGreaterThan(0);
		expect(withRun.length - coincide.length).toBeGreaterThan(0);
	});

	it('carries a row whose two ROI readings have OPPOSITE signs', () => {
		// What makes the `positive` and profit-line pins above real D5 regression
		// tests rather than coincidences. On a row whose best case and measurement
		// agree in sign, an emitter re-pointed at `roi` draws the same verb and the
		// same colour and nothing fails; only an opposite-signed row separates the
		// two, and the corpus carries exactly one — the Apocalypse card, whose hour
		// lost while its last day measured a gain.
		//
		// Stated as what the corpus HAS and not as what it might: measured against
		// these fixtures on 2026-08-23, the mirror shape (`roi > 0` beside
		// `expectedRoi < 0`) is not in them, so no assertion in this file may assume
		// one — the sign-agreeing losers below are what exercise the `lose` arm.
		const opposed = RECENT.plays.filter((p) => p.expectedRoi > 0 && p.roi < 0);
		const bothNegative = RECENT.plays.filter((p) => p.expectedRoi < 0 && p.roi < 0);
		const bothPositive = RECENT.plays.filter((p) => p.expectedRoi > 0 && p.roi > 0);

		expect(names(opposed)).toContain(INCIDENT_NAMES[APOCALYPSE_KEY]);
		expect(bothNegative.length).toBeGreaterThan(0);
		expect(bothPositive.length).toBeGreaterThan(0);
	});
});

// ------------------------------------------------------------ incident pins --

describe('incident — Journey Tattoo, sold into a one-sided book (2026-08-23)', () => {
	// THE INCIDENT (ADR-017's second amendment). The tattoo's newest hour stood at
	// 1121 chaos of bids against zero asks — the shape a SELLER wants most, and the
	// biggest edge in its hour. The both-sides stock gate dropped the sell leg for
	// the empty ask side it was never going to trade on, and the newest-hour rule
	// then deleted the recipe.
	//
	// What the desktop owes that fix is two things: the surviving 1-hop must reach
	// the reader through a fresh install's filters, and the mark that says the book
	// was one-sided must reach the STEP the component draws.
	for (const { horizon, response } of FIXTURES) {
		it(`survives a fresh-install filter pass in the ${horizon} horizon`, () => {
			expect(names(freshInstallPass(response))).toContain(INCIDENT_NAMES[TATTOO_SELL_KEY]);
		});
	}

	it('reaches the route with the depleted mark on the leg that carried it', () => {
		// `RouteStep.depletedSide` is what `ExchangeRoute.svelte` draws the mark
		// from, so this is the field the component reads and not an intermediate.
		// The mark belongs to the ONE leg it describes — the sale into those bids —
		// and a flag that spread to the other two would stop naming the book.
		const route = routeSlots(playByKey(RECENT, TATTOO_SELL_KEY), RECENT.divineChaosRate);

		expect(route?.sell.depletedSide).toBe(true);
		expect(route?.buy.depletedSide).toBe(false);
		expect(route?.convert?.depletedSide).toBe(false);
	});

	it('marks no step on the twin market whose book kept both sides', () => {
		// The control arm, and what makes the pin above a reading of the wire rather
		// than a constant: the twin is the same market shape with stock on both
		// sides in the newest hour, and it is served as a direct flip with nothing
		// marked.
		const route = routeSlots(playByKey(RECENT, TATTOO_TWIN_KEY), RECENT.divineChaosRate);

		expect(route?.buy.depletedSide).toBe(false);
		expect(route?.sell.depletedSide).toBe(false);
		expect(route?.convert).toBeNull();
	});
});

describe('incident — Apocalypse card, spreadless newest hour (2026-08-22)', () => {
	// THE INCIDENT (ADR-017's first amendment). The card's 07:00 hour traded 2 cards
	// at a single 223:1 print, so both extremes were the same price and the round
	// trip paid two ticks against no spread: the best-case return is NEGATIVE. The
	// MinEdge drop removed that hour's candidate and the newest-hour rule removed
	// the recipe, deleting a market that printed 70-92% in five of the window's
	// other six hours.
	//
	// It reaches the desktop as the mirror of the shape D5 was reported against:
	// the ROI column loses while the Exp. ROI column WINS. That direction is the
	// harder one to draw correctly, and it is the one a `positive` read off `roi`
	// gets wrong — the row would be drawn as a loss while its Get prints above its
	// Spend.
	const play = () => playByKey(RECENT, APOCALYPSE_KEY);
	const rate = RECENT.divineChaosRate;

	it('closes on Get = Spend + Exp. ROI while its best case loses', () => {
		// E5, on the row whose two ROI figures have opposite signs.
		const ledger = runLedger(play(), rate)!;

		expect(ledger.roiChaos).toBeLessThan(0);
		expect(ledger.expectedRoiChaos).toBeGreaterThan(0);
		expect(ledger.getChaos).toBe(ledger.investmentChaos + ledger.expectedRoiChaos);
	});

	it('draws the row as a gain, because the measurement is what the Get is', () => {
		// §4.5 and the D5 fix: the verb reads the EXPECTATION's sign, and `positive`
		// with it. Both would invert if either were re-pointed at the best case.
		const route = routeSlots(play(), rate)!;

		expect(route.positive).toBe(true);
		expect(route.get.sub?.startsWith('keep ≈ ')).toBe(true);
		expect(parseAmount(route.get.amount)).toBeGreaterThan(parseAmount(route.spend.amount));
	});

	it('keeps the hour’s best case on the row, below the Spend it started from', () => {
		// The other half of the same row: the negative best case has not been hidden
		// to make the Get readable. It is still there, as the last mechanical total,
		// and it prints BELOW the Spend — which is the arithmetic being honest about
		// a hour that paid two ticks for no spread.
		const route = routeSlots(play(), rate)!;
		const chainEnd = parseAmount(printedChainEnd(route.sell.rate, route.convert?.rate));

		expect(chainEnd).toBeLessThan(parseAmount(route.spend.amount));
		expect(formatGain(moneyColumns(play()).roi).startsWith('-')).toBe(true);
	});
});

describe('incident — the 2026-08-23 screenshot’s spreadless 170 print', () => {
	// The market whose newest hour printed low == high == 170, served flagged rather
	// than dropped since the MinEdge demotion. It is the measured LOSER of the pair:
	// both its ROI columns are negative, so its Get prints below its Spend and its
	// profit line carries the `lose` verb.
	//
	// NOTHING HERE ENDORSES THE STEP LINES. This row's steps read `buy 1 for ≈ 171c`
	// and `sell 1 for ≈ 169c` on a market that printed 170 on both sides — the
	// undercut fill prices of §1's BASIS rule, applied to a market with no spread to
	// undercut into. Whether that is the right DISPLAY on a spreadless market is an
	// open owner decision, and this file takes no position on it: the assertions
	// below read the ledger and the ends, the generic closure battery above applies
	// its equations to this row like any other, and neither pins those two strings
	// as desired.
	const play = () => playByKey(RECENT, SPREADLESS_KEY);
	const rate = RECENT.divineChaosRate;

	it('closes on Get = Spend + Exp. ROI while measuring a loss', () => {
		const ledger = runLedger(play(), rate)!;

		expect(ledger.expectedRoiChaos).toBeLessThan(0);
		expect(ledger.getChaos).toBe(ledger.investmentChaos + ledger.expectedRoiChaos);
	});

	it('carries the loss in the verb and the magnitude in the amount', () => {
		// §4.5. "lose ≈ 1c" and never "keep ≈ -1c": the sign lives in the word, which
		// is the only place on the row where a word carries one.
		const route = routeSlots(play(), rate)!;
		const column = moneyColumns(play()).expectedRoi;

		expect(route.positive).toBe(false);
		expect(route.get.sub).toBe(`lose ≈ ${formatChaos(Math.abs(column))}c`);
		expect(route.get.sub).not.toContain('-');
		expect(parseAmount(route.get.amount)).toBeLessThan(parseAmount(route.spend.amount));
	});

	it('survives a fresh-install filter pass, because a served loser is still a row', () => {
		// ADR-017's serve-and-flag principle reaching the client: the measured losers
		// are ranked last, not hidden, and no default may hide one.
		expect(names(freshInstallPass(RECENT))).toContain(INCIDENT_NAMES[SPREADLESS_KEY]);
	});
});

describe('incident — Mawr Blaidd, junk low IN the newest hour (POE-188)', () => {
	// The extreme too far from the hour's VWAP to trade on, served with the leg
	// marked `suspect`. The reader judges it — that is ADR-015's split — but it
	// ranks after every clean play in every order the client offers, because a
	// suspect number out-sorting a clean one hands the reader the very row the flag
	// warns about.
	const SORTS: ExchangeSort[] = ['expected', 'roi', 'fastest'];

	for (const { horizon, response } of FIXTURES) {
		it(`survives a fresh-install filter pass in the ${horizon} horizon`, () => {
			expect(names(freshInstallPass(response))).toContain(INCIDENT_NAMES[MAWR_JUNK_KEY]);
		});
	}

	for (const sort of SORTS) {
		it(`sorts after every clean row under the ${sort} sort`, () => {
			// The partition property, asserted as the set of clean rows that ended up
			// BEHIND this one — so a failure names the markets the suspect row jumped
			// rather than reporting two indices. A comparator that lost the `suspect`
			// branch would let this row climb on its 192c best case, which is the
			// largest ROI column in the fixture and three times the next one.
			const ordered = sortPlays(RECENT.plays, sort);
			const junkAt = ordered.findIndex((p) => p.key === MAWR_JUNK_KEY);

			expect(ordered[junkAt]?.suspect).toBe(true);
			expect(names(ordered.slice(junkAt + 1).filter((p) => !p.suspect))).toEqual([]);
		});
	}

	it('marks the leg whose extreme was junk, and not the row’s other step', () => {
		// The mark is a LEG reading and reaches the step the component draws.
		const route = routeSlots(playByKey(RECENT, MAWR_JUNK_KEY), RECENT.divineChaosRate)!;

		expect(route.buy.suspect).toBe(true);
		expect(route.sell.suspect).toBe(false);
	});

	it('leaves the market whose junk lows sit BEHIND the newest hour clean', () => {
		// The control arm: same market shape, junk lows in older hours only. It is
		// priced from the newest hour alone, so nothing about it is suspect — which
		// is what makes the pin above a reading of this row rather than of the pair.
		const route = routeSlots(playByKey(RECENT, MAWR_KEY), RECENT.divineChaosRate)!;

		expect(playByKey(RECENT, MAWR_KEY).suspect).toBe(false);
		expect(route.buy.suspect).toBe(false);
	});
});

describe('incident — Divine Vessel, a 100% tick (POE-184)', () => {
	// POE-184's measured noise market: a 1/35-to-1/1 extreme pair whose tick is a
	// full 100%, so the undercut sell price is `price × (1 − 1) = 0` and the round
	// trip returns exactly −100%. Its expectation is negative too, so there is no
	// repeat count that reaches a positive target and `worthwhileScale` answers
	// `null`.
	//
	// The rule that answers `null` is the one this pins: the Fastest sort puts such
	// a row at the END of its partition rather than dropping it or treating it as
	// instant. A comparator that read `null` as 0 hours would put the deadest market
	// in the fixture at the top of the "fastest" list.
	it('has no worthwhile run to sort by', () => {
		const play = playByKey(RECENT, VESSEL_KEY);

		expect(play.expectedRoi).toBeLessThanOrEqual(0);
		expect(worthwhileScale(play)).toBeNull();
	});

	it('stays in the Fastest sort rather than being dropped from it', () => {
		// A sort is not a filter. The row keeps its rank, its ROI and its depth; what
		// it loses is a position argument.
		expect(names(sortPlays(RECENT.plays, 'fastest'))).toEqual(
			expect.arrayContaining([INCIDENT_NAMES[VESSEL_KEY]])
		);
		expect(sortPlays(RECENT.plays, 'fastest').length).toBe(RECENT.plays.length);
	});

	it('sits at the end of its own partition under the Fastest sort', () => {
		// Its partition is the SUSPECT one — both of its legs are marked — so "the
		// end" is the end of the suspect band and not the end of the table by
		// accident. Asserted as the last unreadable-hours row within that band, so a
		// second such row appearing later cannot make this pass vacuously.
		const ordered = sortPlays(RECENT.plays, 'fastest');
		const suspects = ordered.filter((p) => p.suspect);
		const readable = suspects.filter((p) => worthwhileScale(p)?.hours != null);

		expect(names(suspects).at(-1)).toBe(INCIDENT_NAMES[VESSEL_KEY]);
		expect(readable.length).toBeGreaterThan(0);
		expect(names(suspects).slice(0, readable.length)).toEqual(names(readable));
	});

	it('still closes on Get = Spend + Exp. ROI at a chain end of nothing', () => {
		// The −100% round trip renders as a chain end of zero, and the row is still a
		// closed row: E3 and E5 both hold, and the Get is the Spend plus a negative
		// measurement rather than the zero the mechanical chain ends on.
		const ledger = runLedger(playByKey(RECENT, VESSEL_KEY), RECENT.divineChaosRate)!;

		expect(ledger.chainEndChaos).toBe(ledger.investmentChaos + ledger.roiChaos);
		expect(ledger.getChaos).toBe(ledger.investmentChaos + ledger.expectedRoiChaos);
		expect(ledger.chainEndChaos).toBe(0);
	});
});

// ------------------------------------------------------ fresh-install visibility --

describe('fresh-install visibility (ADR-017)', () => {
	// THE RULE: no default may hide a served play, with exactly two sanctioned
	// exceptions — the trash-price floors `minItemPrice` (0.5 chaos, POE-196) and
	// `minItemPriceDiv` (0.4 divine, owner ruling 2026-08-23).
	//
	// This is the fourth time a default-on filter has quietly removed served rows
	// from this table, so the assertion is on the survivor SET and not on a count:
	// a future default that starts hiding a market fails with that market's name in
	// the message.

	/**
	 * The eleven markets a fresh install shows, in served order.
	 *
	 * The four that do NOT appear are each below one of the two sanctioned floors,
	 * and the test below proves that by disarming exactly those two rather than by
	 * asserting it in a comment.
	 */
	const FRESH_INSTALL_SURVIVORS = [
		'a clean profitable flip',
		'Apocalypse card, spreadless newest hour (2026-08-22)',
		'Mawr Blaidd, junk lows behind the newest hour (POE-188)',
		'Journey Tattoo twin, both sides alive',
		'Journey Tattoo against divine, a direct flip',
		'Journey Tattoo, sold into a one-sided book (2026-08-23)',
		'a one-hop triangle',
		'the scarab against chaos, a two-tick loser',
		'the 2026-08-23 screenshot\'s spreadless 170 print',
		'a recipe too young to have an expectation',
		'Mawr Blaidd, junk low IN the newest hour (POE-188)'
	];

	for (const { horizon, response } of FIXTURES) {
		it(`shows exactly the markets above the two sanctioned floors in the ${horizon} horizon`, () => {
			expect(names(freshInstallPass(response))).toEqual(FRESH_INSTALL_SURVIVORS);
		});
	}

	it('hides nothing at all once the two sanctioned floors are disarmed', () => {
		// ADR-017 stated as an assertion rather than as prose: turn off the two knobs
		// the ADR sanctions and EVERY served play must come back. A sixth default
		// arming itself — a return floor, a turnover floor, a category rule — fails
		// here with the names of the markets it removed, whatever else it also passes.
		const disarmed = applyGates(
			RECENT.plays,
			parseGates({ ...freshInstallInputs(), minItemPrice: '0', minItemPriceDiv: '0' }),
			RECENT.divineChaosRate
		);
		const ruled = applyRules(disarmed, parseCategoryRules(''), parseItemRules(''));
		const bounded = applyNumericFilters(ruled, {
			investMin: '',
			investMax: '',
			unit: parseUnit(''),
			divineChaosRate: RECENT.divineChaosRate
		});

		expect(names(bounded.filter((p) => matchesSearch(p, '')))).toEqual(names(RECENT.plays));
	});

	/**
	 * Is this play under one of the two floors ADR-017 sanctions?
	 *
	 * The predicates the shipped knobs run, re-derived from the wire's own figures
	 * rather than taken on trust: `minItemPrice` judges `investment`, which is
	 * always chaos; `minItemPriceDiv` judges the same figure read back into divine
	 * and only on a DIVINE-quoted entry.
	 */
	function belowASanctionedFloor(play: CurrencyExchangePlay): boolean {
		if (play.investment < gateDefaults.minItemPrice) return true;
		return (
			play.legs[0].quote === DIVINE &&
			play.investment / RECENT.divineChaosRate < gateDefaults.minItemPriceDiv
		);
	}

	it('hides nothing that is not under one of the two sanctioned floors', () => {
		// A drop matching NEITHER predicate is a default hiding a live market, which
		// is the thing ADR-017 forbids — and it fails here naming the market rather
		// than only shrinking the survivor list above.
		const shown = new Set(freshInstallPass(RECENT).map((p) => p.key));
		const hidden = RECENT.plays.filter((p) => !shown.has(p.key));

		expect(hidden.length).toBeGreaterThan(0);
		expect(names(hidden.filter((p) => !belowASanctionedFloor(p)))).toEqual([]);
	});

	it('keeps every measured loser and every flagged row on screen', () => {
		// The half of the rule that is easiest to break by accident: `lowLiquidity`,
		// `lowCoverage`, `suspect` and a negative `expectedRoi` are marks and ranking
		// keys, never reasons to hide. All four shapes are in the fixture, and each
		// row carrying one must reach a fresh install unless a sanctioned floor
		// removed it for its PRICE — which is a different reason and the only other
		// one allowed.
		const shown = new Set(freshInstallPass(RECENT).map((p) => p.key));
		const flagged = RECENT.plays.filter(
			(p) => p.suspect || p.lowCoverage || p.lowLiquidity === true || p.expectedRoi < 0
		);
		const owed = flagged.filter((p) => !belowASanctionedFloor(p));

		expect(flagged.length).toBeGreaterThan(3);
		expect(owed.length).toBeGreaterThan(0);
		expect(names(owed.filter((p) => !shown.has(p.key)))).toEqual([]);
	});
});
