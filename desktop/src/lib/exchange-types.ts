// --- Currency Exchange (POE-175) ---

/**
 * Which plays `GET /currency-exchange/plays` returns: every ranked play, only
 * the single-swap ones, or only the two-swap ones. `'all'` is an explicit
 * filter value rather than the absence of one, so it is sent like any other.
 *
 * `$lib/exchange/view.ts` re-exports this type, which is what keeps the page's
 * mode picker and this fetcher from drifting apart.
 */
export type CurrencyExchangeMode = 'all' | 'direct' | '1-hop';

/**
 * Which window `GET /currency-exchange/plays` ranks in. `'recent'` is a short
 * window that reacts within hours and is what the server assumes when the
 * parameter is absent; `'day'` is the overnight-planning window, where a play
 * has to have held for most of a day to be listed at all.
 *
 * The server computes both windows in one recompute and answers a horizon it
 * does not know with a 400, so this is sent explicitly like `mode` is.
 */
export type CurrencyExchangeHorizon = 'recent' | 'day';

/**
 * One swap inside a play, as ONE feed hour observed it — the hour the play's
 * `lastHour` names on a leg priced in its own scored hour, and one of the hours
 * inside the trailing clock window on a `windowPriced` one (below). Either way
 * each PRICE is one hour's realized print and never a blend. `item`/`quote` are
 * the raw Currency Exchange ids and `itemName`/`quoteName` their display names
 * — real item names (POE-177), with the humanized id as the fallback for an id
 * the server's asset does not know. `price` is quote per item.
 *
 * `price` is that hour's realized extreme on the leg's side (cheapest for a buy,
 * dearest for a sell) and it is RAW: the undercut a real order has to pay lives
 * in the play's `roiPct`/`roi`, never in this number. `fair` is the same hour's
 * volume-weighted price, where the traded mass actually cleared, and is the
 * anchor `price` should be read against; `fairOk` is false when the quote side
 * reported no volume — the scored hour's, or on a `windowPriced` leg (below) the
 * WINDOW's POOLED quote volume, whichever `fair` itself came from — and `fair`
 * is then 0, a missing reading rather than a free item. `suspect` is that comparison already made: an extreme too far
 * from `fair` to be repeatable (a buy under `fair × 0.67`, a sell over
 * `fair × 1.5`), and it stays false when there is no `fair` to judge against.
 * `tick` is the hour's price resolution on this market as a fraction of the
 * price (0.5 = the next representable price is 50% away), which is what
 * separates a real spread from one integer step (POE-184) — and what lets a
 * client rebuild the undercut price: `price × (1 + tick)` to buy,
 * `price × (1 − tick)` to sell.
 *
 * Both of those readings move on a `windowPriced` leg, and they move
 * DIFFERENTLY. `fair` becomes the window's POOLED volume-weighted price instead
 * of the scored hour's, so `suspect` still compares the extreme against the mass
 * that traded beside it — like with like, which is why such a leg is often
 * suspect for the ordinary reason. `tick` does not follow it: on a leg whose
 * scored hour was live the step stays THAT hour's, because the step is what the
 * market can express now even where the price is older; only on a RESCUED leg —
 * no row in the scored hour at all, or one under the server's per-hour volume
 * floor — does it come from the newest window row that contributed, as old as
 * the price beside it and no older.
 *
 * So `tick === 1 / Math.max(priceItemQty, priceQuoteQty)` is NOT a wire
 * invariant, on this leg or any other: the server reads the step from the
 * COARSER of its source row's two pairs, and on a window-priced leg the pair
 * beside the price can come from a different hour than the step. Rebuild the
 * undercut price FROM `tick`; never re-derive `tick` from the pair.
 *
 * `depletedSide` is a reading of the OPPOSITE side of this leg's book: true when
 * nothing stood there in the hour — no asks facing a sell, no bids facing a buy
 * — so the order this leg describes would have been alone on its side of the
 * market. It is the SHAPE of the book and not a verdict on the price: an empty
 * facing side is part of why the extreme was reachable at all, and it is equally
 * why an order posted into it may stand unmatched. It is a per-leg fact like
 * `suspect`, and the two are independent — a leg can be one-sided at a perfectly
 * ordinary price, and a leg far outside its fair band can have had a full book
 * on both sides.
 *
 * OPTIONAL on the wire — a server older than the field omits it, and an absent
 * field means false, so no client may distinguish "not sent" from "both sides
 * stood".
 *
 * `noOffers` is the EXECUTING side of this leg's book read the same way: true
 * when nothing stood where this leg trades — no asks for a buy, no bids for a
 * sell — so the order cannot be filled from the book and the reader posts it
 * and waits (POE-281, "no offers - make your own"). The price is still the
 * hour's realized trade, so the row ranks by value like any other. It is
 * independent of `depletedSide` (the opposite side) and is a mark, never a sort
 * key (ADR-018). Optional on the wire the same way: absent means false.
 *
 * `priceItemQty`/`priceQuoteQty` are that same price as the in-game Currency
 * Exchange posts it: the reduced integer quantity pair behind this hour's
 * extreme, with `priceQuoteQty / priceItemQty === price` EXACTLY — the server
 * builds the pair beside the price and never re-derives either from the other.
 * The game trades whole quantities on both sides, so a leg with `priceItemQty`
 * 4 and `priceQuoteQty` 1 quoted in divine is an order reading "sell 4 for 1
 * div"; the 0.25 divine it divides out to is not a price the exchange can
 * express, which is why the pair travels rather than being reconstructed here.
 * They are oriented to the LEG — `priceItemQty` counts units of `item` and
 * `priceQuoteQty` units of `quote`, whichever side of the raw feed row those
 * turn out to be — so a direct flip's two legs quote one market with two
 * different pairs, the buy leg carrying the hour's low and the sell leg its
 * high.
 *
 * It is the pair the extreme PRINTED at, and not the order to post: the order
 * that actually fills sits one quantity step tighter, is not served, and is
 * already paid for in `roiPct`/`roi`. Client copy must not promise that posting
 * this exact pair is the play.
 *
 * `itemIcon`/`quoteIcon` are API-RELATIVE paths into this server's icon route
 * (`/icon/currency-exchange/<escaped id>`), not upstream poewiki URLs —
 * production cannot reach poewiki (ADR-012). Join them onto `getApiBase()` with
 * `$lib/exchange/view`'s `iconSrc`. `null` means the item has no artwork, which
 * is a different thing from a path that later 404s: the first renders no icon
 * at all, the second the "?" fallback.
 *
 * `itemCategory`/`quoteCategory` are the leg's two sides placed in the in-game
 * Currency Exchange sidebar — one of the sixteen names the response's
 * `categories` lists, or `""` for an id the server's item asset does not cover.
 * `""` is a usable answer rather than an ambiguous one: a client filtering by
 * category treats an uncategorised side as UNFILTERED, so it never has to tell
 * an absent category from an empty one, and the fields are plain strings for
 * that reason. Both sides carry one because a filter applies to whichever side
 * the reader is shopping for.
 */
export interface CurrencyExchangeLeg {
	action: 'buy' | 'sell';
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
	suspect: boolean;
	/** Optional: absent on a server older than the field, and absent means false. */
	depletedSide?: boolean;
	/** Optional: absent on a server older than the field, and absent means false. */
	noOffers?: boolean;
	/** Optional: absent on a server older than the field, and absent means false. */
	windowPriced?: boolean;
	/** Optional: absent on a server older than the field, and 0 when not window-priced. */
	windowHours?: number;
	/** Optional: absent on a server older than the field, and 0 when not window-priced. */
	windowVolume?: number;
	itemName: string;
	itemIcon: string | null;
	itemCategory: string;
	quoteName: string;
	quoteIcon: string | null;
	quoteCategory: string;
}

/**
 * A ranked arbitrage play: ONE hour's prices, plus the cross-hour readings of
 * how the recipe has actually behaved. `direct` swaps back through the same
 * market; `1-hop` routes through an intermediate currency.
 *
 * `expectedRoi` is the number the server RANKS on (POE-193, ADR-016): the mean
 * chaos one exchanged unit actually returned across the simulated entries of
 * the last day — post this play's undercut orders in each of those hours, chase
 * the buy, wait for the sell, fire-sale whatever never sold. It can be
 * NEGATIVE, and such a play is still served. The SERVED order is clean before
 * `suspect`, then covered before `lowCoverage`, then `expectedRoi` desc — a
 * chaos payout, so a bigger stake outranks a better rate.
 *
 * That order is the wire's, and the desktop does not display it: the table
 * re-sorts every response by the column the sort picker names and by nothing
 * else, flags included (`exchange/view.ts` `sortPlays`, POE-220). What survives
 * of the served order on screen is its tie-breaks, which a stable sort carries
 * through rows tied on the picked column.
 *
 * OPTIMISTIC is the word here on purpose: this block words the WIRE, and its
 * wording mirrors the Go docs on the fields it describes. The desktop-surface
 * BEST CASE / UNCONTESTED sweep (`docs/CURRENCY-EXCHANGE-ROW-INVARIANT.md` §6.4,
 * which scopes itself to `tooltips.ts`, `view.ts` and the page caption)
 * deliberately stops here — align this with `internal/exchange/plays.go` if that
 * side is ever renamed, rather than with the row's reader-facing vocabulary.
 *
 * `roiPct` is the OPTIMISTIC reading: that one hour's round trip at the
 * UNDERCUT prices (0.123 = 12.3%), each leg paying one of its own ticks to be
 * the order that fills. Measured over 960 top-20 play-hours it overstates what
 * the play realized by 4-8x, which is why it no longer ranks anything — but it
 * is still what the client's quality gates judge, because those levels were
 * calibrated against it (ADR-015). `roiPctRaw` is the same round trip at the
 * raw extremes the legs show — never below `roiPct`, and the gap between the
 * two is what the ticks cost. `roi` is chaos gained per exchange of one unit
 * and `investment` the chaos that one unit costs to enter, both priced at the
 * undercut entry, so `roi === roiPct * investment` still holds by construction
 * for that optimistic pair. `expectedRoiPct` carries no such identity: it is a
 * mean of per-entry fractions, each with its own chased outlay, and NOT
 * `expectedRoi` divided by `investment`.
 *
 * `turnover` is chaos per hour through the play's thinnest leg, which is the
 * liquidity reading — `depth` (units per hour) is not one. `tick` is the
 * coarsest leg tick, the worst price step the recipe has to live with.
 * `suspect` is true when any leg is; such a play is still served — the reader
 * judges it — and the wire ranks it after every clean one, an ordering the
 * desktop table drops (see above). `hoursSeen` counts the window
 * hours in which the recipe cleared every gate on that hour's own prices, and
 * `lastHour` is the hour the recipe was SCORED at, and the hour every price
 * above was read from unless the row is `windowPriced` (below). It is always the
 * window's newest, because a recipe that did not clear in the last snapshot is
 * not served at all.
 *
 * `lowLiquidity` is the play-level thin-hour flag, and it is about `lastHour`
 * alone: that hour printed no spread worth taking, the round trip at its
 * undercut prices returning less than the server's floor. The PREDICATE is that
 * spread and the name is the cause the measured case showed — an hour so quiet
 * that the little which traded all cleared at one ratio — so `roiPct` on such a
 * play is a real measurement and may be NEGATIVE, down to the -1 a market whose
 * whole price step is 100% arithmetically returns.
 *
 * It is not a verdict on the market and it is not a ranking key. `expectedRoi`
 * reads the hours this flag lacks, so a recipe whose quiet hour is an exception
 * keeps its place and one that never had a spread sinks on its own — which is
 * why the flag marks the ROW, replaces no number on it, and is never a reason to
 * hide the play (it was a DROP until 2026-08-22, and what the drop removed was a
 * card flipping at 70-92% in five of the window's other six hours).
 *
 * OPTIONAL on the wire — a server older than the field omits it, and an absent
 * field means false, so no client may distinguish "not sent" from "clean".
 *
 * `windowPriced` is the other thin-hour mark, and it says something the flag
 * above cannot: the leg prices on this row are NOT `lastHour`'s. That hour
 * traded so little that one trade collapsed its low and its high onto the same
 * number, so the server priced the legs from the extremes the market actually
 * REALIZED over the trailing clock window instead, and `windowHours` says how
 * many hours of that window printed while `windowVolume` says how many units
 * they traded between them. The measured case is 2026-09-04: the Apocalypse
 * card's newest hour traded one card at 552 — the reader's own purchase — and
 * the row read −0%, while the six-hour window had realized 486/1148 and the
 * game's book stood near 550/1150 all day.
 *
 * The staleness is real, bounded and DISCLOSED: a price here can be hours old,
 * and `windowHours` beside the mark is how the reader judges how old. What does
 * NOT move with it is `lastHour` and the response's `divineChaosRate`, still the
 * scored hour's, and `expectedRoi`, whose simulation reads the legs' own-hour
 * prices and never the window's. `tick` is off that list because it has no one
 * answer: it is the scored hour's on every leg that HAD a row there, and the
 * newest contributing window row's on a RESCUED leg — the leg block above says
 * which and why. Such a row also commonly carries `suspect`, for the ordinary
 * reason and not as a second symptom: a window that stood at 486/1148 is 2.4×
 * wide against its own fair, which is what the band is for.
 *
 * Like `lowLiquidity` it is a MARK. It is in neither comparator — not the
 * wire's ranking and not `sortPlays` (ADR-018) — and no default gate reads it
 * (ADR-017), so such a row's `roiPct` and `expectedRoi` order at face value.
 *
 * OPTIONAL on the wire, the same three ways: a server older than the fields
 * omits them, an absent `windowPriced` means false, and an absent
 * `windowHours`/`windowVolume` means 0 — which is also what a served
 * hour-priced row carries.
 *
 * `simEntries` is how many entry hours the two expectations are averaged over
 * and `lowCoverage` says that count fell under the server's guard — "we could
 * not measure this", which is a different claim from "we measured this and it
 * loses" and ranks ahead of neither. Those two and the two expectations are
 * fields a client CANNOT recheck from the row it is shown in: their inputs are
 * hours the row does not carry. Neither can `turnover` or `hoursSeen`, for
 * their own reasons — the first is the hour's quote-side volume, which the legs
 * do not ship, and the second counts hours. What IS recheckable is the price
 * arithmetic: `roiPct`, `roi`, `investment`, `tick` and `depth` are a ratio, a
 * product, a min or a max over the legs on this row.
 */
export interface CurrencyExchangePlay {
	key: string;
	mode: 'direct' | '1-hop';
	legs: CurrencyExchangeLeg[];
	roiPct: number;
	/** @deprecated use `roiPct` — the server sends both with the same value. */
	edge: number;
	roiPctRaw: number;
	roi: number;
	investment: number;
	expectedRoi: number;
	expectedRoiPct: number;
	simEntries: number;
	lowCoverage: boolean;
	turnover: number;
	tick: number;
	depth: number;
	suspect: boolean;
	/** Optional: absent on a server older than the field, and absent means false. */
	lowLiquidity?: boolean;
	/** Optional: true when any leg is `noOffers` (POE-281); absent means false. A mark, never a sort key. */
	noOffers?: boolean;
	/** Optional: absent on a server older than the field, and absent means false. */
	windowPriced?: boolean;
	/** Optional: absent on a server older than the field, and 0 when not window-priced. */
	windowHours?: number;
	/** Optional: absent on a server older than the field, and 0 when not window-priced. */
	windowVolume?: number;
	hoursSeen: number;
	lastHour: string;
	/** The server's row figures (ADR-030), read by the `view.ts` display helpers. */
	display: PlayDisplay;
}

/**
 * One play's Go-computed display figures (`internal/exchange/plays_display.go`).
 * JSON has no NaN or ±Inf, so a non-finite figure arrives as null.
 */
export interface PlayDisplay {
	units: number;
	basis: 'posting' | 'single';
	money: { investment: number | null; roi: number | null; expectedRoi: number | null };
	expectedPrinted: boolean;
	scale: {
		flips: number | null;
		gain: number | null;
		investment: number | null;
		hours: number | null;
	} | null;
	runInvestment: number | null;
	sortKeys: { expected: number | null; roi: number | null; fastestHours: number | null };
}

/** One item-picker entry, counted over the plays served after the mode filter. */
export interface ExchangeUniverseItem {
	id: string;
	name: string;
	icon: string | null;
	category: string;
	playCount: number;
}

/** A paged `/currency-exchange/plays` request's params; a list repeats its key. */
export type ExchangePageParams = Record<string, string | string[]>;

/**
 * The `/currency-exchange/plays` payload. A cold server answers 200 with
 * `warm: false` and an empty `plays`, so an empty list is not an error — the
 * page tells the two apart through `warm`. `plays` is one page, filtered and
 * ordered by the server; `total` is the size after the mode filter.
 *
 * `horizon` echoes the window these plays were ranked in, so a body arriving
 * from the cache cannot be mistaken for the other horizon's. `divineChaosRate`
 * is the newest hour's chaos value of one divine, and because a play is served
 * only if it cleared in that hour, it is the rate every divine-quoted `roi`,
 * `investment` and `turnover` here was valued with. It is 0 when that hour
 * carried no divine/chaos trade, in which case no divine-quoted play is in the
 * list at all.
 *
 * `categories` is the in-game Currency Exchange sidebar, in sidebar order: the
 * whole taxonomy, on every served body, INDEPENDENT of the plays in it — a cold
 * body with no plays still carries all sixteen. It is the source the category
 * filter renders from; never hold a copy of the list client-side, or a category
 * GGG adds becomes a silent gap in the filter while the legs already name it.
 */
export interface CurrencyExchangeResponse {
	league: string;
	lastUpdated: string | null;
	from: string | null;
	to: string | null;
	hours: number;
	warm: boolean;
	mode: CurrencyExchangeMode;
	horizon: CurrencyExchangeHorizon;
	divineChaosRate: number;
	count: number;
	plays: CurrencyExchangePlay[];
	categories: string[];
	/** Paged body (ADR-030): `count` is the page's size, these are the stage counts. */
	total: number;
	afterRules: number;
	afterGates: number;
	matched: number;
	showConvert: boolean;
	itemUniverse: ExchangeUniverseItem[];
}
