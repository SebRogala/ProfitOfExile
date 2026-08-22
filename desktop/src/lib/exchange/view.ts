/**
 * Presentation derivations for the Currency Exchange page (POE-176).
 *
 * The sibling of `temple/view.ts` and `mercenaries/ladder-view.ts`, and for
 * the same reason: a `.svelte` file has no unit-test harness in this app, so
 * everything the page would otherwise compute inline lives here as pure
 * functions over the wire types in `$lib/api`.
 *
 * It decides nothing about the market. The ranking, the edge and the depth are
 * the server's (POE-175); this file only words them, and turns the page's four
 * loose variables (result / lastFetchedAt / lastError / now) into the single
 * state the header renders.
 *
 * Pure TypeScript on purpose — no Svelte runes, no Tauri imports — so the
 * whole file is reachable from vitest without a component harness.
 *
 * This file is the ONLY emitter of a Currency Exchange row's mechanical
 * numbers — the route's ends, its step totals and the three money columns. What
 * those numbers must mean and how they must agree is specified in
 * `docs/CURRENCY-EXCHANGE-ROW-INVARIANT.md`, which is normative because the
 * invariant also reaches `filters.ts`, the page, `ExchangeRoute.svelte`,
 * `tooltips.ts` and the wire's own field docs. Enforcement belongs in the
 * closure tests in `view.test.ts`, not in the document: a change to an equation
 * there and a change to those tests travel together, in one commit.
 */
import type {
	CurrencyExchangeHorizon,
	CurrencyExchangeLeg,
	CurrencyExchangeMode,
	CurrencyExchangePlay,
	CurrencyExchangeResponse
} from '$lib/api';

/**
 * Re-exported from `$lib/api` so the page can import its wire enums from the
 * same module as the helpers that consume them. One definition, two entry
 * points — the fetcher and the pickers cannot drift.
 */
export type { CurrencyExchangeMode, CurrencyExchangeHorizon };

// ------------------------------------------------------------ the filter --

/** The mode picker's entries, in display order, for `SegmentedButtons`. */
export const MODE_OPTIONS: { value: CurrencyExchangeMode; label: string }[] = [
	{ value: 'all', label: 'All' },
	{ value: 'direct', label: 'Direct' },
	{ value: '1-hop', label: '1-hop' }
];

/**
 * Narrow a persisted or user-supplied string to a mode.
 *
 * Anything unrecognised — an empty preference, a value written by an older
 * build, a mode the server has since dropped — becomes `'all'` rather than
 * reaching the API and coming back as a 400.
 */
export function parseMode(raw: string): CurrencyExchangeMode {
	return MODE_OPTIONS.some((option) => option.value === raw)
		? (raw as CurrencyExchangeMode)
		: 'all';
}

/**
 * The horizon picker's entries. The labels carry the window length because
 * "Recent" and "Day" alone do not say how much history ranked the list, and
 * that length is what the Hours column counts against.
 */
export const HORIZON_OPTIONS: { value: CurrencyExchangeHorizon; label: string }[] = [
	{ value: 'recent', label: 'Recent 6h' },
	{ value: 'day', label: 'Day 24h' }
];

/**
 * Narrow a persisted or user-supplied string to a horizon.
 *
 * `'recent'` is both the API default and the fallback, so a preference written
 * by a build that predates the horizon toggle resolves to the window the page
 * has always fetched rather than silently widening it.
 */
export function parseHorizon(raw: string): CurrencyExchangeHorizon {
	return HORIZON_OPTIONS.some((option) => option.value === raw)
		? (raw as CurrencyExchangeHorizon)
		: 'recent';
}

/**
 * Which number the table is ordered by. `'expected'` is the server's own
 * ranking, which since POE-193 is the fill-simulated `expectedRoi` (ADR-016);
 * `'roi'` re-orders by the BEST-CASE chaos the ROI column prints, which is a
 * different question — a 40% best case on 2c is not the play a stocked account
 * wants; and `'fastest'` by how long the market needs to absorb the play's
 * worthwhile scale (`worthwhileScale().hours`), which is the question a big
 * return on a thin market answers badly.
 */
export type ExchangeSort = 'expected' | 'roi' | 'fastest';

/** The sort picker's entries, in display order. */
export const SORT_OPTIONS: { value: ExchangeSort; label: string }[] = [
	{ value: 'expected', label: 'Exp. ROI' },
	{ value: 'roi', label: 'ROI' },
	{ value: 'fastest', label: 'Fastest' }
];

/**
 * Narrow a persisted or user-supplied string to a sort; default `'expected'`.
 *
 * Two older spellings map FORWARD rather than falling back, because in both
 * cases the order the reader picked still exists under a new name. `'fill'` is
 * what `'fastest'` was called while the table scaled by a typed Quantity
 * (POE-192 replaced that with the derived scale). `'roiPct'` is what the
 * served order was called while the server ranked on the best-case
 * percentage; POE-193 re-based that ranking on `expectedRoi`, and the pick has
 * always meant "the list as the server ranked it", so it resolves to
 * `'expected'` rather than to a percentage sort that no longer exists.
 *
 * Everything else — a mode from a build this one has never seen, an empty
 * preference — also answers `'expected'`, which is the one order that carries
 * the server's full set of tie-breaks.
 */
export function parseSort(raw: string): ExchangeSort {
	if (raw === 'fill') return 'fastest';
	if (raw === 'roiPct') return 'expected';
	return SORT_OPTIONS.some((option) => option.value === raw) ? (raw as ExchangeSort) : 'expected';
}

/**
 * How much of each row is drawn. `'dense'` drops every sub-line and shrinks the
 * icons; the content the sub-lines carried moves into the cell tooltips rather
 * than disappearing.
 */
export type ExchangeDensity = 'comfortable' | 'dense';

/** The density picker's entries, in display order. */
export const DENSITY_OPTIONS: { value: ExchangeDensity; label: string }[] = [
	{ value: 'comfortable', label: 'Comfortable' },
	{ value: 'dense', label: 'Dense' }
];

/**
 * Narrow a persisted or user-supplied string to a density; default
 * `'comfortable'`, which is the layout every sub-line's copy was written for.
 */
export function parseDensity(raw: string): ExchangeDensity {
	return DENSITY_OPTIONS.some((option) => option.value === raw)
		? (raw as ExchangeDensity)
		: 'comfortable';
}

/** Which currency the investment bounds are typed in. */
export type ExchangeUnit = 'chaos' | 'divine';

/** The unit picker's entries, in display order. */
export const UNIT_OPTIONS: { value: ExchangeUnit; label: string }[] = [
	{ value: 'chaos', label: 'Chaos' },
	{ value: 'divine', label: 'Divine' }
];

/**
 * Narrow a persisted or user-supplied string to a unit; default `'chaos'`,
 * which is the currency every wire number is already denominated in and the
 * only one that stays meaningful when `divineChaosRate` is 0.
 */
export function parseUnit(raw: string): ExchangeUnit {
	return UNIT_OPTIONS.some((option) => option.value === raw) ? (raw as ExchangeUnit) : 'chaos';
}

// ------------------------------------------------------------- the state --

/**
 * What the header says about the data below it.
 *
 * - `loading` — nothing fetched yet and nothing has failed.
 * - `warming` — the server answered, but no Currency Exchange hour has closed.
 * - `ready` — a warm result is on screen.
 * - `stale` — a result is on screen but the latest fetch failed; the table stays.
 * - `unreachable` — the fetch failed and there is nothing to fall back on.
 */
export type ViewStateKind = 'loading' | 'warming' | 'ready' | 'stale' | 'unreachable';

/** The four page variables `deriveState` reads. */
export interface ViewStateInput {
	result: CurrencyExchangeResponse | null;
	lastFetchedAt: Date | null;
	lastError: string | null;
	now: Date;
}

/** The derived state, plus whichever relative string its kind displays. */
export interface ViewState {
	kind: ViewStateKind;
	/** `stale` only: how long ago the last successful fetch landed. */
	staleSince?: string;
	/** `ready` only: how long ago the server's data was computed. */
	updatedAgo?: string;
}

/**
 * Collapse result / error / timestamps into the one state the header renders.
 *
 * An error outranks `warming`: a stale warm-up is still a connectivity
 * problem, and "waiting for the first hour" would hide the fact that the last
 * request failed. `stale` falls back to `now` when nothing recorded a fetch
 * time, which yields "just now" instead of an empty gap in the sentence.
 */
export function deriveState({ result, lastFetchedAt, lastError, now }: ViewStateInput): ViewState {
	if (!result) {
		return { kind: lastError ? 'unreachable' : 'loading' };
	}
	if (lastError) {
		return { kind: 'stale', staleSince: formatTimeAgo(lastFetchedAt ?? now, now) };
	}
	if (result.warm === false) {
		return { kind: 'warming' };
	}
	return {
		kind: 'ready',
		updatedAgo: result.lastUpdated === null ? undefined : formatTimeAgo(result.lastUpdated, now)
	};
}

// -------------------------------------------------------------- the time --

const MINUTE_MS = 60_000;
const HOUR_MS = 60 * MINUTE_MS;
const DAY_MS = 24 * HOUR_MS;

/**
 * A coarse "how long ago" for the header.
 *
 * Deliberately single-unit ("2 h ago", not "2 h 14 m ago"): the page re-derives
 * it from a ticking `now` every 30s, and the extra unit would only make that
 * flicker more visible. A `null` value, an unparseable date, and a timestamp in
 * the future are all rendered as their least alarming reading — `""` for the
 * first two, "just now" for a clock that runs ahead of the server's.
 */
export function formatTimeAgo(value: string | Date | null, now: Date): string {
	if (value === null) return '';
	const then = value instanceof Date ? value : new Date(value);
	const elapsed = now.getTime() - then.getTime();
	if (Number.isNaN(elapsed)) return '';
	if (elapsed < MINUTE_MS) return 'just now';
	if (elapsed < HOUR_MS) return `${Math.floor(elapsed / MINUTE_MS)} min ago`;
	if (elapsed < DAY_MS) return `${Math.floor(elapsed / HOUR_MS)} h ago`;
	return `${Math.floor(elapsed / DAY_MS)} d ago`;
}

/**
 * The absolute clock time behind a relative string, for the tooltip.
 *
 * Built from `getHours`/`getMinutes` rather than `toLocaleTimeString` so it is
 * 24-hour "HH:MM" in every locale — the header pairs it with a relative string
 * that is already English-only, and a stray "01:30 PM" would read as a
 * different quantity. Local time, since that is the clock the user farms by.
 */
export function formatTime(iso: string | null): string {
	if (iso === null) return '';
	const date = new Date(iso);
	if (Number.isNaN(date.getTime())) return '';
	return `${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`;
}

/** The clock reading and the relative age the status line leads with. */
export interface DataAge {
	/** `"as of 14:35"` — the local clock time the prices below were read at. */
	label: string;
	/** How long ago that was, in `formatTimeAgo`'s wording. */
	ago: string;
}

/**
 * How old the table is, for the status line.
 *
 * Dated from `to` — the end of the settled hour the prices come from — and not
 * from `lastUpdated`, which is when the server computed the ranking: the feed
 * publishes 40-60 minutes after an hour closes, so the two differ by most of an
 * hour and only the first answers "how stale are these prices". `lastUpdated`
 * is the fallback for a body served without a window, and `null` (no timestamp
 * at all, or one that will not parse) means the page shows no badge rather than
 * an "as of :" with a hole in it.
 */
export function dataAgeParts(
	response: CurrencyExchangeResponse | null,
	now: Date
): DataAge | null {
	const stamp = response?.to ?? response?.lastUpdated ?? null;
	if (stamp === null) return null;
	const clock = formatTime(stamp);
	if (clock === '') return null;
	return { label: `as of ${clock}`, ago: formatTimeAgo(stamp, now) };
}

// ------------------------------------------------------------ the numbers --

/**
 * An ROI fraction as a signed percentage: `0.1234` → `"+12.3%"`,
 * `-0.05` → `"-5.0%"`.
 *
 * Exchange ROI arrives as a FRACTION (0.05 = +5%), unlike the gem side's
 * percentage points — this formatter multiplies, so pointing it at a gem number
 * inflates it a hundredfold.
 *
 * The sign is always explicit — a bare "12.3%" next to a "-5.0%" reads as an
 * absolute price rather than a delta. The sign is taken from the *rounded*
 * magnitude, so a return that rounds to nothing prints "+0.0%" instead of the
 * nonsense "-0.0%".
 */
export function formatRoiPct(roiPct: number): string {
	const percent = roiPct * 100;
	const magnitude = Math.abs(percent).toFixed(1);
	const sign = Number(magnitude) !== 0 && percent < 0 ? '-' : '+';
	return `${sign}${magnitude}%`;
}

/**
 * A per-hour volume, abbreviated: `0`, `42`, `1.2k`, `13.0M`.
 *
 * Depth spans several orders of magnitude across currencies, and the column is
 * scanned for rank rather than read for an exact count — one decimal keeps the
 * cells the same width without collapsing 1.2k and 9.9k onto the same string.
 */
export function formatVolume(v: number): string {
	if (v >= 1_000_000) return `${(v / 1_000_000).toFixed(1)}M`;
	if (v >= 1000) return `${(v / 1000).toFixed(1)}k`;
	return String(Math.round(v));
}

/** Precision floor for sub-1 leg prices, in decimal places. */
const MIN_PRICE_DECIMALS = 2;
/** Significant digits kept for sub-1 leg prices. */
const PRICE_SIGNIFICANT_DIGITS = 4;

/**
 * A leg price at a precision that stays readable across the whole currency
 * range: `196`, `0.50`, `0.1429`, `0.004975`.
 *
 * Prices here are quote-per-item in arbitrary currencies, so there is no single
 * decimal count that works — a mirror-priced item needs none, and a fragment
 * priced in divine needs four leading zeros before the first real digit. Above
 * 1 the scale carries the meaning, so fixed decimals are enough; below 1 the
 * significant digits do, so the count floats with the exponent and trailing
 * zeros are trimmed back to a two-decimal floor ("0.5" would read as less
 * precise than it is).
 */
export function formatLegPrice(price: number): string {
	if (!Number.isFinite(price)) return '0';
	const magnitude = Math.abs(price);
	if (magnitude >= 100) return price.toFixed(0);
	if (magnitude >= 1) return price.toFixed(MIN_PRICE_DECIMALS);
	if (magnitude === 0) return price.toFixed(MIN_PRICE_DECIMALS);

	// One decimal place past the first significant digit, e.g. 0.004975 (first
	// digit at 1e-3) keeps six places, 0.1429 (first digit at 1e-1) keeps four.
	const firstDigitExponent = Math.floor(Math.log10(magnitude));
	const decimals = Math.min(PRICE_SIGNIFICANT_DIGITS - 1 - firstDigitExponent, 20);
	const trimmed = price.toFixed(decimals).replace(/0+$/, '');
	const fractionDigits = trimmed.length - trimmed.indexOf('.') - 1;
	return fractionDigits < MIN_PRICE_DECIMALS ? price.toFixed(MIN_PRICE_DECIMALS) : trimmed;
}

/**
 * A chaos AMOUNT as the route slots and the money columns print it: `50`,
 * `5,050`, `0`.
 *
 * Whole orbs, because that is the unit the game trades in — a chaos amount is a
 * count of items in a stash tab, and there is no such holding as 0.07 of one.
 * The decimals this used to print belonged to the leg RATES, which are still
 * fractional and still go through `formatLegPrice` (POE-189).
 *
 * A sub-1c amount therefore rounds to "0", and that is the reading the owner
 * asked for: a play whose whole per-exchange investment or gain is under one
 * chaos is not flippable once the exchange's gold fee is paid, so printing four
 * significant digits of it dressed junk as precision. Since POE-196 the
 * INVESTMENT side of that reading is also a filter: the shipped `minItemPrice`
 * floor of 0.5 drops exactly the plays whose entry cost would have printed 0
 * here, unless the reader types 0 into the knob. (The gain side is the Min
 * profit gate, which has shipped OFF since POE-193.)
 *
 * Rounded on the MAGNITUDE and signed afterwards, so `-1234.5` and `1234.5`
 * round to the same number of orbs — `Math.round` alone breaks halves towards
 * +Infinity, which would print a loss one orb smaller than the matching gain.
 *
 * Grouped by hand rather than through `toLocaleString`, for the reason
 * `formatTime` gives: a locale that groups with "." would print 5.050 beside an
 * English sentence and read as five point zero five.
 */
export function formatChaos(amount: number): string {
	if (!Number.isFinite(amount)) return '0';
	const rounded = Math.round(Math.abs(amount));
	const grouped = String(rounded).replace(/\B(?=(\d{3})+(?!\d))/g, ',');
	return rounded === 0 || amount > 0 ? grouped : `-${grouped}`;
}

/**
 * A NON-CHAOS orb amount, always to the hundredth: `0.50`, `3.16`, `1,204.75`.
 *
 * The sibling of `formatChaos` for the currency at the other end of the scale.
 * Chaos rounds to whole orbs because a fraction of one is not a holding; divine
 * is worth two hundred of them, so the same rounding would erase a 100c slice of
 * a run — and the precision has to hold at EVERY magnitude, which is what
 * separates this from `formatLegPrice`. That one drops to `toFixed(0)` above
 * 100, which is right for a rate (the scale carries the meaning) and wrong here:
 * a 101.5-divine run printed "102" beside a profit line reading "keep ≈ 100c
 * (≈ 0.50 div)" contradicts itself twice over — the total moved by more than the
 * profit that produced it, and the two numbers disagree about how precise the
 * page is being.
 *
 * Grouping and the signed-zero rule are `formatChaos`'s, for `formatChaos`'s
 * reasons: hand-grouped so a locale that separates with "." cannot turn 1,204
 * into one point two, and the sign taken from the rounded magnitude so an amount
 * too small to print never comes out as "-0.00".
 */
function formatFractionalOrbs(amount: number): string {
	if (!Number.isFinite(amount)) return '0.00';
	const fixed = Math.abs(amount).toFixed(2);
	const [whole, fraction] = fixed.split('.');
	const grouped = whole.replace(/\B(?=(\d{3})+(?!\d))/g, ',');
	const sign = Number(fixed) !== 0 && amount < 0 ? '-' : '';
	return `${sign}${grouped}.${fraction}`;
}

/**
 * A chaos gain as the ROI column prints it: `+700`, `+5,050`, `0`.
 *
 * The sign is explicit on anything that moved, because the column sits beside
 * Investment and a bare "700" reads as a second cost rather than a return.
 * It is taken from the ROUNDED magnitude, the same rule `formatRoiPct` follows:
 * a gain too small to print must not be dressed as a gain, and a loss that
 * rounds away must not print "-0". Exactly zero keeps no sign at all — a play
 * that returns what it cost is not a positive one.
 */
export function formatGain(amount: number): string {
	const magnitude = formatChaos(Math.abs(amount));
	if (magnitude === '0') return magnitude;
	return `${amount < 0 ? '-' : '+'}${magnitude}`;
}

/**
 * How much of the window a play held, as a 0–1 fraction for the Hours bar.
 *
 * Clamped at both ends rather than trusted: the bar is a CSS width, so a
 * `hours` of 0 (a body served before any hour closed) would otherwise divide to
 * Infinity and a `hoursSeen` above the window would draw past the track. Both
 * are read as their honest extreme — nothing seen, and the whole window.
 */
export function hoursProgress(hoursSeen: number, hours: number): number {
	if (!Number.isFinite(hoursSeen) || !Number.isFinite(hours) || hours <= 0) return 0;
	return Math.min(1, Math.max(0, hoursSeen / hours));
}

// --------------------------------------------------------------- the scale --

/**
 * The chaos a play is scaled up to before it is worth the reader's attention
 * (POE-192).
 *
 * An OWNER CONSTANT, not a preference: the page's job is a quick market
 * overview, and the moment the reader has to type their own case the overview
 * stops being quick — that is the whole reason the Quantity stepper this
 * replaced was removed. 100c is the size at which a flip pays for the clicking
 * on a Currency Exchange whose fee is gold: below it the row is real but not
 * worth the trip, above it the table starts recommending inventories nobody
 * holds. Revisit on field feedback, not per reader.
 */
export const SCALE_TARGET_CHAOS = 100;

/** What one play looks like scaled to `SCALE_TARGET_CHAOS`. */
export interface WorthwhileScale {
	/** Whole exchanges needed to clear the target — the `×N` the column leads with. */
	flips: number;
	/** Chaos expected across those flips: `expectedRoi × flips`, so at least the target. */
	gain: number;
	/** Chaos tied up across those flips: `investment × flips`. */
	investment: number;
	/**
	 * Hours the market needs to absorb them, rounded UP to whole hours; `null`
	 * when the play's thinnest leg traded nothing and the wait cannot be read.
	 */
	hours: number | null;
}

/**
 * How far a play has to be repeated to be worth doing, and what that costs.
 *
 * Scaled on `expectedRoi` and NOT on `roi` (POE-193): the question the column
 * answers is how many exchanges it takes to make the trip worth the clicking,
 * and the best-case per-hour figure overstates that by 4-8x, so scaling on it
 * answered with a flip count the play would never have paid off at. The
 * exchange count grows accordingly — a play whose best case gains 30c but whose
 * simulated mean is 6c now reads ×17 rather than ×4, which is the size the
 * reader would actually have had to run.
 *
 * The app derives the size so the reader does not type one. `flips` is rounded
 * UP — a play that reaches 99c in three exchanges has not cleared the target, so
 * the honest answer is the fourth exchange and the gain it actually pays, not
 * the target itself. `gain` and `investment` are therefore the scaled figures,
 * which is why `gain` is reported rather than assumed to be 100: a play worth
 * 33c an exchange clears the bar at 102c, and the column says so.
 *
 * `hours` is deliberately UNCONTESTED, and the tooltip says so: `depth` is the
 * WHOLE market's hourly volume on the play's thinnest leg, so this is the time
 * the fill takes if the reader takes every unit of it and no one else trades.
 * UNCONTESTED is the word on purpose. The row's other favourable assumption —
 * the hour's extreme prices — is BEST CASE, and the two used to share one word
 * across this surface; a single word carrying a price basis in one comment and
 * a volume assumption in the next is how two numbers come to disagree in a
 * reader's head, so that word is retired here
 * (`docs/CURRENCY-EXCHANGE-ROW-INVARIANT.md` §6.4).
 * The real wait is longer by however much of the book the competition holds, and
 * longer again on a direct play, which buys and sells on the one market.
 * Rounded up to whole hours because that is how the column reads it, and there
 * is no cap branch: a scale the market cannot absorb inside the hour simply
 * answers 2, 5, 40 — the wait IS the warning, and a capped `flips` would quietly
 * report a scale that does not clear the target.
 *
 * `null` for a play whose expectation is nothing or less than nothing: there is
 * no repeat count that reaches a positive target from a non-positive step, and
 * dividing would answer `Infinity` or a negative count. Unlike the `roi` this
 * used to read, that is a case the page IS expected to hit — the server's
 * positivity floor applies to the best-case number, while a measured
 * expectation is free to come out negative and the play is served anyway
 * (ADR-016). The row keeps its rank, its ROI and its depth; the Scale column
 * shows a dash, because "repeat this until it pays 100c" is not advice a losing
 * expectation has an answer to.
 */
export function worthwhileScale(play: CurrencyExchangePlay): WorthwhileScale | null {
	if (!Number.isFinite(play.expectedRoi) || play.expectedRoi <= 0) return null;

	const flips = Math.ceil(SCALE_TARGET_CHAOS / play.expectedRoi);
	const readableDepth = Number.isFinite(play.depth) && play.depth > 0;
	return {
		flips,
		gain: play.expectedRoi * flips,
		investment: play.investment * flips,
		hours: readableDepth ? Math.ceil(flips / play.depth) : null
	};
}

/**
 * What the run ties up, whatever scale the row DISPLAYS.
 *
 * The one money figure that is still the RUN's on every row. Its only caller is
 * the filter bar's Run cost bound (`filters.ts`); the Scale column prints the
 * same quantity, reading `worthwhileScale().investment` off the object it
 * already holds for the ×N and the hours.
 *
 * It exists as a named export so that bound reads the run DELIBERATELY rather
 * than by happening to share a function with the Investment column. Since the
 * display scale of a chaos-entry row is one posting, `moneyColumns` no longer
 * answers the run's cost, and a bound left pointing at it would have become a
 * per-posting ceiling silently — the code compiles, every divine-row test stays
 * green, and a reader typing 500 to mean 500c of bankroll ends up filtering on
 * what one trash-market order costs (spec §6.1).
 *
 * A play with no worthwhile run falls back to the wire's per-exchange
 * `investment`, which is what the bound compared such a play against before this
 * existed — the fallback moved, the answer did not.
 */
export function runInvestment(play: CurrencyExchangePlay): number {
	return worthwhileScale(play)?.investment ?? play.investment;
}

// ------------------------------------------------------- the display scale --

/**
 * How the row's displayed quantity was chosen.
 *
 * - `posting` — one minimal order of the BUY market: the quantity pair that
 *   market actually posts.
 * - `run` — the worthwhile run, `worthwhileScale().flips`.
 * - `single` — one item, because neither of the above is readable.
 */
export type DisplayBasis = 'posting' | 'run' | 'single';

/** The size every money figure and both step quantities on one row count. */
export interface DisplayScale {
	/** Items the row counts. */
	units: number;
	/** Which rule chose that count. */
	basis: DisplayBasis;
}

/**
 * The size the ROW READS AT — one decision, taken once, by ENTRY CURRENCY
 * (owner ruling, 2026-08-22).
 *
 * The run used to size every row, and on a chaos entry it deceived. A 30c card
 * whose worthwhile run is sixteen exchanges rendered "buy 16 for ≈ 384c", which
 * reads at a glance as a 384c blockbuster and ranks in the reader's eye as one —
 * while buying sixteen of anything on a chaos market is slow and, at the depths
 * these markets carry, not realistic. Chaos markets post either X-items-for-1c
 * or Xc-per-item, so the honest size for such a row is the MINIMAL POSTING of
 * the market it enters on: the order the reader can actually place, once.
 *
 * DIVINE-entry rows keep the run for now, and NOT because their legs carry
 * mixed pairs. A buy lot that disagrees with its sell lot is not a divine-side
 * shape at all — the twelve-for-3c card sold on a five-at-a-time market is a
 * chaos row and carries the same mismatch — and on both sides that mismatch is
 * an accepted cost, disclosed on the sell step's hover rather than resolved
 * (spec §4.6). What actually separates the two is WHERE the deception was. The
 * buy market's posting is the ENTRY order, the one quantity the reader can
 * place once, and the inflated headline lived on the CHAOS entries: "buy 16 for
 * ≈ 384c" on a 30c card reads as a 384c blockbuster. A divine entry's Spend is
 * a fraction of an orb whatever count it carries, so the run there does not
 * fake a size at a glance. The divine side is therefore UNTREATED rather than
 * exempt on a principle, and it gets its own treatment later.
 *
 * N > F IS A LEGITIMATE READING, not a case to guard against. A market that
 * posts a thousand at a time on a play whose worthwhile run is 167 exchanges
 * gives a row counting 1,000 beside a Scale column reading ×167. The posting is
 * the minimal executable trade, so there is no smaller order to print and no
 * honest way to shrink the row to the run; the row prints the posting whole and
 * the buy step's hover discloses the overshoot (`marketPair`, spec §1).
 *
 * What this does NOT touch: `worthwhileScale` itself. The run is still derived,
 * still what the Scale column prints (×N, its cost, its hours), still what the
 * Fastest sort orders by, and on a chaos row the Scale column becomes the ONE
 * place the run appears. The display scale decides what the money figures and
 * the step quantities count, and nothing else.
 *
 * Three branches, in the order they are read:
 *
 * 1. A CHAOS entry with a usable pair on its buy leg counts that pair's item
 *    quantity — `buy 1 for ≈ 24c` on a market that posts one at a time,
 *    `buy 4 for ≈ 1c` on a trash market that posts four.
 * 2. A chaos entry whose buy leg carries no usable pair (version skew) counts
 *    ONE item. There is no posting to read, and one item is the smallest claim
 *    the row can make that is still true.
 * 3. Anything else — a divine entry — counts the worthwhile run, or ONE when
 *    there is none, which is the branch every row took before this rule.
 *
 * The entry currency is read off the buy leg's `quote`, the same field
 * `chaosPerQuote` reads, so a row's display scale and its entry-currency
 * rendering can never disagree about which currency the reader pays with. A play
 * with no legs at all answers the run branch; `runLedger` and `routeSlots` both
 * refuse such a body before any of this is rendered.
 */
export function displayScale(play: CurrencyExchangePlay): DisplayScale {
	const buyLeg = play.legs[0];
	if (buyLeg !== undefined && buyLeg.quote === CHAOS_ID) {
		return hasQuantityPair(buyLeg)
			? { units: buyLeg.priceItemQty, basis: 'posting' }
			: { units: 1, basis: 'single' };
	}

	const scale = worthwhileScale(play);
	return scale === null ? { units: 1, basis: 'single' } : { units: scale.flips, basis: 'run' };
}

// ------------------------------------------------------------- the money --

/** The three money columns of one row, all priced at one scale. */
export interface MoneyColumns {
	/** Chaos the row ties up — the Investment column. */
	investment: number;
	/** BEST-CASE chaos gained — the ROI column. */
	roi: number;
	/** Measured chaos expected — the Exp. ROI column. */
	expectedRoi: number;
}

/**
 * The row's three money figures, at the size the row DISPLAYS (POE-193, rescaled
 * by the owner ruling of 2026-08-22).
 *
 * THE INVARIANT: every money figure on a row is on ONE scale — the same scale
 * the route slots print. The row used to mix three of them. Spend/Get/`keep ≈`
 * were the run (`routeSlots`), the step rates were run-sized orders, and these
 * three columns were the server's PER-EXCHANGE fields — so a two-flip play read
 * "Spend 450 / Get 579 / keep ≈ 129" beside "Investment 225c, ROI +173c,
 * Exp. ROI +64c", six numbers about two different trips with nothing on screen
 * saying which was which.
 *
 * The size comes from `displayScale`, which decides it ONCE per row: one minimal
 * posting of the buy market on a chaos entry, the worthwhile run on a divine
 * one. `runLedger` reads the same function, so the route ends, the step totals
 * and these three columns take the same branch at the same moment rather than
 * each deciding for itself what this row counts
 * (`docs/CURRENCY-EXCHANGE-ROW-INVARIANT.md` §1, SCALE).
 *
 * All three are the wire's PER-EXCHANGE field multiplied by that count, and
 * `WorthwhileScale`'s own `investment`/`gain` are deliberately not read here any
 * more: they are the RUN's totals, which on a chaos row is no longer the size
 * this answers. The multiplication is the same expression `worthwhileScale`
 * itself uses, so a divine row's figures are bit-identical to the ones it
 * carries — the Scale column's "N c in" and the Investment column still print
 * one number on those rows, and on a chaos row the Scale column is the run's
 * lone home and says `×N` beside it.
 *
 * The count is a positive integer, so the sign of each column is the wire's sign
 * and a losing raw return still reads as one.
 *
 * The one figure that stayed the run's is the filter bar's Run cost bound, which
 * reads `runInvestment` and not this — a bankroll ceiling is a run-sized
 * question whatever the row prints (spec §6.1).
 */
export function moneyColumns(play: CurrencyExchangePlay): MoneyColumns {
	const { units } = displayScale(play);
	return {
		investment: play.investment * units,
		roi: play.roi * units,
		expectedRoi: play.expectedRoi * units
	};
}

// ------------------------------------------------------------- the ledger --

/**
 * One row's mechanical numbers, derived once (POE-193).
 *
 * The row's ends, its step totals, its three money columns and the filter bar's
 * Run-cost bound are all renderings of the fields below, which is what
 * `docs/CURRENCY-EXCHANGE-ROW-INVARIANT.md` §1 means by CLOSURE BY
 * CONSTRUCTION: two figures that must agree are one variable, not two
 * calculations that happen to land on the same number.
 */
export interface RunLedger {
	/**
	 * Items the row counts — `displayScale().units`: one posting of the buy
	 * market on a chaos entry, the worthwhile run on a divine one.
	 */
	units: number;
	/** Which rule chose that count — the branch every emitter takes together. */
	basis: DisplayBasis;
	/** Chaos per unit of the entry quote: 1 for chaos, the response's rate for divine. */
	entryRate: number;
	/** The entry quote's id — the currency both ends are rendered in. */
	entryQuote: string;
	/** CHAOS ROOTS. `moneyColumns`' three figures, unmodified. */
	investmentChaos: number;
	roiChaos: number;
	expectedRoiChaos: number;
	/** The mechanical end of the row, in chaos: I + R. One addition, no second path. */
	chainEndChaos: number;
	/** What the row ends with, in chaos: I + X. */
	getChaos: number;
	/** ENTRY-CURRENCY RENDERINGS. One division each from the chaos roots above. */
	spend: number;
	chainEnd: number;
	get: number;
	/**
	 * What step 2 sells, in the SELL leg's own quote: `chainEnd` on a direct play,
	 * `chainEnd / u2` on a 1-hop. Falls FORWARD to `units * u1` — and only there —
	 * when the convert leg carries no usable price; `sellForward` says so.
	 * `null` only when that forward derivation is unusable too.
	 */
	sellTotal: number | null;
	/**
	 * `sellTotal` was derived forwards from the sell leg rather than backwards
	 * from `chainEnd`, because the convert leg had no usable price. The convert
	 * step must then print the market ratio instead of a total — the two amounts
	 * of a convert line derived by different routes would not be in this market's
	 * ratio, which is exactly the fact that made the fallback necessary.
	 */
	sellForward: boolean;
}

/**
 * The row's mechanical numbers, read once from the wire and the money columns.
 *
 * `null` on a body with fewer than two legs — the same guard `routeSlots` keeps,
 * because a row with no round trip has no chain to total — and on an entry quote
 * this response cannot value in chaos. That second case is the row's ONE exempt
 * branch (spec §3, "The one exempt branch"). `chaosPerQuote` answers `null` in
 * two shapes: a quote that is neither chaos nor divine, and a divine entry in an
 * hour whose `divineChaosRate` is 0. Neither reaches a served body — a play
 * quoted in anything but those two is dropped outright
 * (`internal/exchange/plays.go:678-681`), and no divine-quoted play is served in
 * an hour that carried no divine/chaos trade. It is guarded anyway because the
 * alternative is a division by zero printed as an amount, and a caller that gets
 * `null` here renders both ends in chaos instead.
 *
 * `entryRate` is therefore never 0: `chaosPerQuote` answers `null` rather than 0
 * for an unreadable divine rate, and that `null` has already left this function.
 *
 * THE SCALE IS TAKEN ONCE. `units` and the three chaos roots come from
 * `displayScale` and `moneyColumns` and are not recomputed here — that single
 * decision is what makes the ends, the step totals and the columns take the same
 * branch at the same moment, rather than each deciding for itself whether this
 * row counts a posting, a run or one exchange. The filter bar's Run-cost bound
 * is the deliberate exception and reads `runInvestment` instead: it is the one
 * question on the surface that is about bankroll rather than about the row
 * (spec §6.1).
 *
 * `chainEndChaos` and `getChaos` are single additions of those roots, and
 * deliberately not `spend + roiChaos / entryRate`: reassociating the division
 * across the addition costs exactness for nothing, and the whole point of the
 * field is that there is no second expression for it to drift against.
 */
export function runLedger(
	play: CurrencyExchangePlay,
	divineChaosRate: number
): RunLedger | null {
	const [buyLeg, sellLeg, convertLeg] = play.legs;
	if (buyLeg === undefined || sellLeg === undefined) return null;

	const entryRate = chaosPerQuote(buyLeg.quote, divineChaosRate);
	if (entryRate === null) return null;

	const { units, basis } = displayScale(play);
	const money = moneyColumns(play);
	const chainEndChaos = money.investment + money.roi;
	const getChaos = money.investment + money.expectedRoi;
	const chainEnd = chainEndChaos / entryRate;

	return {
		units,
		basis,
		entryRate,
		entryQuote: buyLeg.quote,
		investmentChaos: money.investment,
		roiChaos: money.roi,
		expectedRoiChaos: money.expectedRoi,
		chainEndChaos,
		getChaos,
		spend: money.investment / entryRate,
		chainEnd,
		get: getChaos / entryRate,
		...saleTotal(chainEnd, units, sellLeg, convertLeg)
	};
}

/**
 * What step 2 sells, in its own quote, and by which route it was derived.
 *
 * A DIRECT play has no third leg, and its sell total is `chainEnd` itself. That
 * is legitimate rather than a coincidence: a direct play's key is
 * `direct:<marketID>` (`internal/exchange/plays.go:149-151`), one market, so both
 * legs carry the same `quote` and the total is already in the entry currency.
 * Nothing here re-checks that — the wire's own key shape is the guarantee.
 *
 * On a 1-HOP the total is recovered BACKWARDS, `chainEnd / u2`, where `u2` is
 * leg 3's UNDERCUT price. Leg 3 is read by POSITION and is a `sell` on the wire
 * — this file consults `action` nowhere — which is why the undercut takes the
 * minus form. Backwards and not forwards because `chainEnd` is built from the
 * wire's own `roiPct`, which already contains `u2`; a forward figure would print
 * a second answer to one question beside it (spec §3).
 *
 * THE ONE PERMITTED FORWARD DERIVATION. When leg 3 arrives with no usable price
 * (version skew), `chainEnd / u2` is not a reading, and the total falls forward
 * to `units * u1`. It is the only forward-derived mechanical total on the row,
 * and it exists because the alternative prints the market's LOT quantity on a
 * step whose two ends count the row's own size (spec §3, "The one permitted
 * forward derivation"). With `u1` unusable too there is nothing left to print a
 * total from, so the answer is `null` and the caller shows the market's ratio
 * instead.
 */
function saleTotal(
	chainEnd: number,
	units: number,
	sellLeg: CurrencyExchangeLeg,
	convertLeg: CurrencyExchangeLeg | undefined
): { sellTotal: number | null; sellForward: boolean } {
	if (convertLeg === undefined) return { sellTotal: chainEnd, sellForward: false };

	const u2 = convertLeg.price * (1 - convertLeg.tick);
	if (Number.isFinite(u2) && u2 > 0) return { sellTotal: chainEnd / u2, sellForward: false };

	const u1 = sellLeg.price * (1 - sellLeg.tick);
	if (Number.isFinite(u1) && u1 > 0) return { sellTotal: units * u1, sellForward: true };

	return { sellTotal: null, sellForward: false };
}

// -------------------------------------------------------------- the order --

/**
 * The table's rows in the order the sort picker asks for.
 *
 * `'expected'` is the list exactly as served: the server already ranks clean
 * before suspect, then covered before low-coverage, then `expectedRoi` desc,
 * then turnover, then direct-first, then key (POE-193). Re-sorting it here on
 * `expectedRoi` alone would throw away those tie-breaks — and the low-coverage
 * band in particular, which is the one the reader cannot reconstruct from a
 * single number — so the Exp. ROI sort keeps the served order, copied, so a
 * caller holding the result can never mutate the response's own array through
 * it.
 *
 * That option is deliberately NOT re-pointed at the Exp. ROI COLUMN, and the
 * reason is what that column is. On a divine-entry row it is `scale.gain`, the
 * expectation repeated until it clears about 100c, so every such play lands in
 * [100, 200) and ordering by it would rank the table on the remainder of a
 * division. On a chaos-entry row it is one posting's expectation, which is a
 * different question again — so the column mixes two sizes across rows and is
 * not an order at all. The served order stays the ranking; the column tells the
 * reader what the row in front of them pays. The seam is that the Exp. ROI
 * column is not monotonic in the order it sits in — a 101c expectation runs once
 * for 101c and ranks above a 99c one that runs twice for 198c, and a chaos row's
 * single posting ranks wherever its per-exchange mean put it. It is a seam by
 * choice: the alternative throws away the clean/suspect partition and the
 * coverage band for an order nobody asked for.
 *
 * `'roi'` re-sorts by the BEST-CASE chaos the ROI COLUMN prints, `moneyColumns().roi`,
 * and not by the wire's per-exchange `roi`. The rule is that the table is
 * ordered by the number it SHOWS: a table ordered by a figure printed nowhere on
 * it reads as broken, and the two orders genuinely differ.
 *
 * Since the display scale went per-entry-currency (owner ruling, 2026-08-22)
 * that column is a per-posting figure on a chaos row and a per-run one on a
 * divine row, so this order compares two sizes across rows and will bunch the
 * divine entries at the top. Two adjacent rows under it are not comparable: one
 * says what a single postable order gains, the next what a hundred-chaos run
 * does. That is the honest consequence of the column being what it is, and it
 * is INTERIM — the divine entries keep the run only until they get their own
 * treatment, after which both sides read at one posting and the order is
 * comparable again (spec §8). Re-pointing the sort at
 * `play.roi` to dodge it would order the table by a number no column prints,
 * which is the failure this option exists to avoid. The Investment column has no
 * sort of its own; if it ever gets one it reads the same helper.
 *
 * `'fastest'` sorts by how
 * long the market needs to absorb the play's worthwhile scale — ascending,
 * because the shortest wait is the best row, and a play whose hours cannot be
 * read (`worthwhileScale` `null`, which since POE-193 includes every play whose
 * expectation is not positive, or its `hours` `null`) sits at the end of its
 * partition rather than being dropped or treated as instant.
 *
 * Both re-sorts keep the one property the server ordering exists to carry: every
 * suspect play stays after every clean one, however large its ROI or however
 * fast it absorbs. A suspect number is the reason it ranks last, so letting it
 * out-sort a clean play would hand the reader the very row the flag warns about.
 * Within a partition the sort is stable, so tied plays keep the server's
 * remaining tie-breaks — and the Fastest sort ties often, because its hours are
 * whole ones: everything the market swallows inside the hour reads 1 and stays
 * in the server's order behind that.
 *
 * The order reads the same `worthwhileScale` the Scale column prints in
 * comfortable density (dense trades the wait away with every other sub-line),
 * so the two can never disagree about a play's hours.
 */
export function sortPlays(
	plays: CurrencyExchangePlay[],
	sort: ExchangeSort
): CurrencyExchangePlay[] {
	if (sort === 'expected') return [...plays];
	if (sort === 'fastest') {
		return [...plays].sort((a, b) => {
			if (a.suspect !== b.suspect) return a.suspect ? 1 : -1;
			const left = worthwhileScale(a)?.hours ?? null;
			const right = worthwhileScale(b)?.hours ?? null;
			if (left === null || right === null) {
				if (left === right) return 0;
				return left === null ? 1 : -1;
			}
			return left - right;
		});
	}
	return [...plays].sort((a, b) => {
		if (a.suspect !== b.suspect) return a.suspect ? 1 : -1;
		return moneyColumns(b).roi - moneyColumns(a).roi;
	});
}

// --------------------------------------------------------------- the legs --

/**
 * A leg's `itemIcon`/`quoteIcon` as a URL the browser can fetch.
 *
 * The server sends API-relative paths (`/currency-exchange/icon/<escaped id>`)
 * rather than poewiki URLs, because production cannot reach poewiki (ADR-012)
 * and serves the artwork from its own cache instead. The join is here, not in
 * `ItemIcon.svelte`, so it is unit-testable and so the page reads the API base
 * once for every chip on screen.
 *
 * `null` and `""` both mean "no artwork" and both answer `null` — an empty path
 * joined onto the base would request the base itself, which answers with
 * something that is not an image. The trailing slash is trimmed off the base
 * because a `server_url` typed with one would otherwise produce `//` in the
 * path, which some proxies normalise and others 404.
 */
export function iconSrc(apiBase: string, path: string | null): string | null {
	if (!path) return null;
	const base = apiBase.replace(/\/+$/, '');
	return path.startsWith('/') ? `${base}${path}` : `${base}/${path}`;
}

// ------------------------------------------------------------- the route --

/**
 * The Currency Exchange's id for Chaos Orbs, mirroring `ChaosID` in
 * `internal/exchange/pricing.go`.
 *
 * Held client-side for one reason: the Spend and Get amounts are chaos —
 * `investment` and `roi` are valued in it whatever currency the legs are quoted
 * in — but no wire field says "this side is the chaos side", so the artwork for
 * those two tiles can only be named by the client. Nothing else here reads it:
 * it decides no price and no verdict, so a rename upstream costs the two tiles
 * their icon and never a wrong number.
 */
export const CHAOS_ID = 'Metadata/Items/Currency/CurrencyRerollRare';

/**
 * The Currency Exchange's id for Divine Orbs, mirroring `DivineID` in
 * `internal/exchange/pricing.go`.
 *
 * Held for the same reason as `CHAOS_ID` and used for one more: chaos and divine
 * are the only two currencies a leg is ever QUOTED in, so a leg's `quote` is
 * enough to name the unit its price is in. A rename upstream costs a rate its
 * "div" suffix and an end tile its artwork; it can move no number.
 */
export const DIVINE_ID = 'Metadata/Items/Currency/CurrencyModValues';

/**
 * The API-relative artwork path for any exchange asset id (POE-189).
 *
 * Built rather than scavenged off a leg. The server's icon route serves ANY
 * asset id (`IconPath` in `internal/exchange/items.go`), so the path exists
 * whether or not this particular play happens to carry the artwork on a leg —
 * and a `quoteIcon` the server sent as `null` is exactly what left the row's two
 * end tiles empty before this was built.
 *
 * `encodeURIComponent` is the mirror of the server's `url.PathEscape`: the id
 * carries slashes, Go escapes them as `%2F` in a path SEGMENT, and so does this.
 * Verified against a running server — the escaped URL answers 200, not the 404 a
 * mismatched escaping would give.
 */
export function currencyIconPath(id: string): string {
	return `/currency-exchange/icon/${encodeURIComponent(id)}`;
}

/** The API-relative artwork path for Chaos Orbs. */
export function chaosIconPath(): string {
	return currencyIconPath(CHAOS_ID);
}

/**
 * The short suffix a rate quoted in `quoteId` carries: `c`, `div`, or `''`.
 *
 * Chaos and divine are the only two quote currencies the exchange ranks against
 * (`QuotePriority` in `internal/exchange`), so the map is closed. An id that is
 * neither answers `''` and the rate prints its bare number rather than being
 * labelled with a currency nobody checked — a wrong unit beside a real price is
 * the bug this whole rework exists to remove, so an unknown one says nothing.
 *
 * Since POE-193 it is also asked about a leg's ITEM side, where the closed map
 * does the same job for a different question. An item is a currency only on a
 * convert step, whose item IS the intermediate chaos or divine; a scarab or a
 * card is not in the map and correctly takes no unit word, so `''` reads as
 * "count these, do not name them" on that side rather than as a gap.
 */
export function quoteUnit(quoteId: string): string {
	if (quoteId === CHAOS_ID) return 'c';
	if (quoteId === DIVINE_ID) return 'div';
	return '';
}

/**
 * What one unit of `quoteId` is worth in chaos, or `null` when this response
 * cannot say.
 *
 * `null` is not a served shape: `divineChaosRate` is 0 only in an hour that
 * carried no divine/chaos trade, and no divine-quoted play is served in such an
 * hour at all. It is guarded anyway because the alternative is a division by
 * zero printed as an amount.
 */
function chaosPerQuote(quoteId: string, divineChaosRate: number): number | null {
	if (quoteId === CHAOS_ID) return 1;
	if (quoteId === DIVINE_ID && divineChaosRate > 0) return divineChaosRate;
	return null;
}

/**
 * A whole quantity beside its currency, in the shorthand the game's own trade
 * talk uses: `420c`, `3 div`, and a bare `12` for a unit nothing here names.
 *
 * The space is a rule, not an inconsistency: "420c" is one token wherever chaos
 * is written down and "3div" is not, so the chaos suffix binds to its number
 * and the divine one stands off it. `formatChaos` is borrowed for its hand-rolled
 * thousands grouping alone — every quantity that reaches here is an integer
 * multiple of the feed's own pair, so nothing it does rounds.
 */
function withUnit(quantity: number, unit: string): string {
	const amount = formatChaos(quantity);
	if (unit === '') return amount;
	return unit === 'c' ? `${amount}${unit}` : `${amount} ${unit}`;
}

/**
 * The same shorthand for an amount that is NOT a whole count of posted items:
 * `526c`, `2.52 div`.
 *
 * `withUnit`'s sibling, at the route ends' precision. Everything `withUnit`
 * prints is an integer multiple of a market's own pair, so nothing it does
 * rounds; the convert step's two figures are a total and the proceeds that
 * produced it, and neither lands on an orb boundary. So the precision
 * follows the ENDS rather than the orders: whole orbs for chaos, which is the
 * only holding chaos comes in (POE-189), and the hundredth for anything else,
 * where a whole run can cost a fraction of one orb. That is exactly the rule
 * `routeSlots` prints Spend and Get with, which is what lets the convert line's
 * tail be the same string as the Get amount rather than a second rendering of
 * the same number.
 */
function withOrbUnit(quantity: number, unit: string): string {
	const amount = unit === 'c' ? formatChaos(quantity) : formatFractionalOrbs(quantity);
	if (unit === '') return amount;
	return unit === 'c' ? `${amount}${unit}` : `${amount} ${unit}`;
}

/**
 * An end slot's hover: its unit word, and its sub-line when it has one.
 *
 * Both, in one string, because dense hides both — the unit word beside the
 * number and the sub-line under it — and a hover that carried only the second
 * would leave a dense reader with a bare 3.66 and no way to learn it is divine.
 */
function endTitle(unit: string, sub: string | null): string {
	return sub === null ? unit : `${unit} — ${sub}`;
}

/**
 * A step's line as the TOTAL it moves at the row's own size: `buy 50 for ≈ 3.16
 * div`, `buy 4 for ≈ 1c`.
 *
 * The quantity is `displayScale().units` — the same count every other figure on
 * the row is priced at — and the total is the ledger figure the matching END
 * renders, printed through the SAME formatter that end uses. That is what makes
 * the two strings identical rather than merely equal (spec §4.2): `withOrbUnit`
 * is what `routeSlots`' own `amount()` calls, so the buy line's tail and the
 * Spend slot cannot drift into two precisions of one number.
 *
 * The `≈ ` — space included — is the claim the line makes and the claim it does
 * NOT make (§4.3). It is a total at the undercut fill prices, not the exact
 * order anyone posts; the item count before it is exact and carries no `≈`. What
 * the market actually posts moves to the hover, in `marketPair`.
 *
 * The `≈` survives even on a chaos row, whose count IS one posting of the buy
 * market: the posting is exact, the PRICE beside it is the undercut fill price
 * and not the extreme the market printed, and that is what the `≈` has always
 * been about (spec §1, BASIS).
 *
 * This replaced a POSTABLE-ORDER rendering that snapped the printed quantity
 * down to a whole multiple of the market's lot. The snap bought exact
 * postability and paid for it in the one currency this row cannot spend: a step
 * quantity that was not the size the ends are priced at. Under the invariant the
 * line is a total at that size, so the snap has nothing left to buy.
 *
 * The unit words come from `quoteUnit` on BOTH sides. The quote side always has
 * one (chaos or divine); the item side has one only when the item is itself one
 * of those two, which is exactly the convert step, whose item is the
 * intermediate currency. A scarab or a card gets a bare count, as it should.
 */
function stepRate(
	verb: string,
	leg: CurrencyExchangeLeg,
	units: number,
	total: number,
	totalUnit: string
): string {
	return `${verb} ${withUnit(units, quoteUnit(leg.item))} for ≈ ${withOrbUnit(total, totalUnit)}`;
}

/**
 * Does this leg carry the market's own quantity pair, whole and positive?
 *
 * ONE predicate for the three emitters that ask — `marketPair`, `pairLine` and
 * the caveat that words whichever branch `pairLine` took — because a guard
 * copied per emitter is a guard that can disagree with itself: a leg the line
 * treats as paired and the hover treats as pairless would describe a quantity
 * the reader is not being shown.
 *
 * All four conjuncts are load-bearing, and each rejects an arrival the others
 * would let through. A pre-POE-193 server sends neither key, which reaches here
 * as `undefined` and not as 0 — so it is the INTEGRALITY checks that reject
 * that shape, and a guard written as `=== 0` would pass it and word a hover
 * around `undefined`. A body that priced one side and sent the other as 0 gets
 * past both integrality checks and is caught by that side's POSITIVITY check
 * alone, which is why the two sides are checked apart. And a fractional
 * quantity is not an order the exchange can post at all — the game trades whole
 * counts against whole counts — so a pair that is not a pair of integers is a
 * number this file must not word as one.
 */
function hasQuantityPair(leg: CurrencyExchangeLeg): boolean {
	const itemQty = leg.priceItemQty;
	const quoteQty = leg.priceQuoteQty;
	return Number.isInteger(itemQty) && Number.isInteger(quoteQty) && itemQty > 0 && quoteQty > 0;
}

/**
 * The hover behind a step that printed a total: what this market actually
 * posted, and what its LOT does to a quantity of this size.
 *
 * The game trades whole quantities against whole quantities and has no per-unit
 * price field, which is why the pair is still shown — but it is shown as the
 * market's PRINTED extreme, in a hover, and never as an instruction on the line.
 * The order that fills sits one price step tighter than the printed extreme, and
 * that step is exactly what the `≈` totals on the line already pay for.
 *
 * The LOT clause exists because deleting the snap deleted the row's only
 * disclosure of it (spec §4.6). The markets still post in lots, so a line
 * reading `sell 12 for ≈ 2.97 div` on a five-at-a-time market asserts a quantity
 * no single order can move. `stepRate`'s rationale — the line is a total, not an
 * order — answers POSTABILITY and does not answer INDIVISIBILITY, so both facts
 * move onto the hover with the pair rather than being dropped.
 *
 * The clauses name "the N this row counts" and never "the run", because since
 * the owner ruling of 2026-08-22 that count is a run only on a divine-entry row
 * — a chaos row counts one posting of its BUY market, and this same hover words
 * the SELL market beside it, whose lot has no reason to match. Saying "run"
 * there would name a quantity the row does not print.
 *
 * `bought` is the sell step's flag, and it is what licenses the "stay unsold"
 * clause: units a SELL lot cannot cover are stock nobody takes off the reader's
 * hands, while units a BUY lot cannot cover were simply never bought.
 *
 * `runFlips` is the worthwhile run, and it is the BUY step's alone — `null`
 * everywhere else. It words the N > F case: a buy market whose minimal posting
 * counts MORE exchanges than the run the Scale column sizes. That is a legal
 * row and not a bug (spec §1, the N > F assumption): the posting is the
 * smallest trade this market accepts, so a row that counts it counts past the
 * run, and the honest move is to print the posting whole and disclose the
 * overshoot rather than to shrink the row to a quantity nobody can order. The
 * SELL step is not told the run, because the sentence names the ENTRY order's
 * own lot and that market's lot has no reason to match it.
 *
 * `null` for a leg served without a usable pair (version skew) — there is no
 * pair to word, and the line above it does not depend on one any more, so the
 * step simply carries no hover.
 */
function marketPair(
	leg: CurrencyExchangeLeg,
	units: number,
	bought: boolean,
	runFlips: number | null
): string | null {
	if (!hasQuantityPair(leg)) return null;
	const itemQty = leg.priceItemQty;
	const quoteQty = leg.priceQuoteQty;

	const printed = `This market printed ${withUnit(itemQty, quoteUnit(leg.item))} for ${withUnit(quoteQty, quoteUnit(leg.quote))} — the hour's extreme, exactly as the exchange posts it. The order that fills sits one price step tighter, which is the step the ≈ totals on this row already pay for.`;

	// Appended to whichever lot clause the count lands on, rather than to the
	// exact-lot branch alone. Today only that branch can carry it — the buy step
	// of a posting-sized row counts exactly one of its own lots — but a scale
	// rule that ever counted something else would otherwise drop the sentence
	// silently, and this is the one fact on the hover the reader cannot see
	// anywhere else on the row.
	const overshoot =
		runFlips !== null && units > runFlips
			? ` This market posts ${formatChaos(units)} at a time, more than the ×${formatChaos(runFlips)} run the Scale column sizes — one order is the smallest trade it accepts, so the row counts it whole.`
			: '';

	// A count under a single lot has no order beneath it at all, which is a
	// different fact from a count the lot merely fails to divide — and the one the
	// reader has to act on, since the smallest order this market accepts is more
	// than the whole play.
	if (units < itemQty) {
		return `${printed} This market posts ${formatChaos(itemQty)} at a time, which is more than the ${formatChaos(units)} this row counts — the smallest order it accepts is bigger than the whole play.${overshoot}`;
	}

	const rest = units % itemQty;
	if (rest === 0) return `${printed}${overshoot}`;

	const residue = bought
		? ` and ${formatChaos(rest)} of the ${formatChaos(units)} bought stay unsold.`
		: ` with ${formatChaos(rest)} left over.`;
	return `${printed} This market posts in multiples of ${formatChaos(itemQty)}, so the ${formatChaos(units)} this row counts is ${formatChaos(Math.floor(units / itemQty))} whole orders${residue}${overshoot}`;
}

/**
 * One market's own posted pair, as a LINE: `buy 16 for 1 div`,
 * `convert 1 div for 209c`, or the decimal rate when even the pair is unusable.
 *
 * A `pairLine` is a LOT and not the row's own count. The row's ends beside it
 * count the exchanges the row is priced for, so the caller is REQUIRED to hover it with
 * that fact (`lotNotScale`) — a mixed-quantity reading with nothing on screen to
 * name it is the exact failure `docs/CURRENCY-EXCHANGE-ROW-INVARIANT.md` exists
 * to end.
 *
 * THREE places on the row have no total to print, and each emits its line
 * through here (spec §4.7): the exempt branch, whose response cannot value the
 * entry currency in chaos at all; a 1-hop convert step whose leg carries no
 * usable price; and the sell step of a 1-hop whose sale could be priced from
 * neither the convert leg nor its own. A FOURTH caller emits no line at all —
 * the convert step's total hover quotes the market's posted pair inside its
 * own sentence, and quotes it from here so the hover and the fallback line
 * cannot word one market's order two ways. It is also the ONLY surviving use of
 * the decimal form — "convert @ 0.00 c" is awkward, and "convert 0 for 0" would
 * be wrong.
 *
 * The reachable cause of the decimal fallback is VERSION SKEW and not a stale
 * cache (this app holds no response cache): the desktop points at a configurable
 * server, so a build shipped with these fields can be aimed at a server from
 * before POE-193 that never sent them.
 */
function pairLine(verb: string, leg: CurrencyExchangeLeg): string {
	const quote = quoteUnit(leg.quote);

	if (hasQuantityPair(leg)) {
		const item = withUnit(leg.priceItemQty, quoteUnit(leg.item));
		return `${verb} ${item} for ${withUnit(leg.priceQuoteQty, quote)}`;
	}

	const price = formatLegPrice(leg.price);
	return quote === '' ? `${verb} @ ${price}` : `${verb} @ ${price} ${quote}`;
}

/**
 * The caveat a `pairLine` carries when the line IS the market's posted pair
 * (spec §4.7).
 *
 * It is the note the deleted no-scale branch used to carry, told about the one
 * thing that is actually true of these lines: they print a market's LOT while
 * the amounts at both ends of the row count the exchanges the row is priced for
 * — a run on a divine entry, one posting of the buy market on a chaos one.
 */
const LOT_NOT_SCALE_PAIR =
	'This step has no total to print, so the line shows one order of this market — the quantity pair it posts — while the amounts at both ends of the row count the exchanges the row is priced for.';

/**
 * The same caveat for a line that fell all the way through to the DECIMAL rate.
 *
 * Such a line posts no pair, so the pair wording above would name a quantity
 * that is nowhere on screen — the caveat would be describing the reading the
 * reader was not given. What is true of it instead is that it is a per-unit
 * rate, which is not an order the exchange can hold at all; the count-versus-lot
 * half of the sentence is unchanged, because the ends beside it still count the
 * row's own size either way.
 */
const LOT_NOT_SCALE_RATE =
	'This step has no total to print and this market posted no quantity pair, so the line shows its per-unit rate alone while the amounts at both ends of the row count the exchanges the row is priced for.';

/**
 * Whichever of the two caveats matches the branch `pairLine` took for this leg.
 *
 * Both read the SAME predicate the line did, so the sentence cannot claim a
 * pair the line did not print.
 */
function lotNotScale(leg: CurrencyExchangeLeg): string {
	return hasQuantityPair(leg) ? LOT_NOT_SCALE_PAIR : LOT_NOT_SCALE_RATE;
}

/** Why a convert step fell back to its market's ratio, appended to `lotNotScale`. */
const CONVERT_UNPRICED =
	'This market carried no usable price this hour, so the proceeds of the sale are read off the sell step instead.';

/**
 * The Get slot's profit line: `keep ≈ 102c`, `lose ≈ 3c`, `keep ≈ 0c`.
 *
 * The VERB carries the sign and the amount carries the magnitude (spec §4.5).
 * "keep ≈ -100c" asks the reader to hold two negations at once, and on a divine
 * entry it doubles up into "keep ≈ -100c (≈ -0.50 div)". Exactly zero keeps
 * `keep`, matching `formatGain`'s rule that a play returning what it cost is not
 * a negative one.
 */
function profitLine(expectedRoiChaos: number): string {
	return `${expectedRoiChaos < 0 ? 'lose' : 'keep'} ≈ ${formatChaos(Math.abs(expectedRoiChaos))}c`;
}

/** One traded step of a route: what it moves, at what price. */
export interface RouteStep {
	/**
	 * The item the step acts on — bought, sold, or converted.
	 *
	 * `null` on a convert step that prints a RUN TOTAL, and only there. That line
	 * already names both currencies ("≈ 2.97 div → 615c") and the tile already
	 * carries the artwork of the one being converted, so a "Divine Orb" heading
	 * above it says nothing a reader cannot see — while the height it costs is the
	 * one thing a three-step row has none of. The name is not lost: it leads the
	 * step's `rateTitle`.
	 */
	name: string | null;
	/** Its API-relative icon path, for `iconSrc`; `null` when it has none. */
	icon: string | null;
	/** `buy 200 for ≈ 3,838c`, `sell 12 for ≈ 2.97 div`, `≈ 2.97 div → 615c`. */
	rate: string;
	/**
	 * The hover behind the rate: what this market PRINTED, and what its lot does
	 * to a quantity of this size (`marketPair`) — or, on a step that shows a
	 * market ratio instead of a total, the caveat that the line counts one order
	 * while the row's ends count the row's own size (`lotNotScale`).
	 *
	 * `null` only for a leg served without a usable quantity pair, which leaves
	 * nothing to word. The line above it does not depend on the pair, so a step
	 * can carry a total with no hover at all.
	 */
	rateTitle: string | null;
	/** The leg's price sits outside its fair band — the tile is marked. */
	suspect: boolean;
}

/** What goes in at the start of a run and what comes back out at the end. */
export interface RouteEnd {
	/** The total, in `unit`. Whole orbs for chaos, fractional otherwise. */
	amount: string;
	/** The unit's artwork — always a built path, never `null` (POE-189). */
	icon: string;
	/** The unit the amount is counted in: `chaos`, `divine`. */
	unit: string;
	/**
	 * The slot's second line: `≈ 5,050c` under a non-chaos Spend, and the
	 * `keep ≈ 102c` / `lose ≈ 3c` profit line under Get.
	 *
	 * `null` only under a Spend that is already chaos. The Get slot always has
	 * one: under E5 its amount is the Spend plus the Exp. ROI column, so there is
	 * always a figure to name — `keep ≈ 0c` at exactly zero included.
	 */
	sub: string | null;
	/**
	 * The whole slot's hover: the unit word, and the sub-line behind it when there
	 * is one — `"divine — keep ≈ 100c (≈ 0.50 div)"`.
	 *
	 * Dense hides BOTH of those, so this is the only way back to either from a
	 * dense row. The unit is on it in comfortable too, where the word is already
	 * on screen: a hover that changed its content with the density would be a
	 * second contract to keep, for no reading the reader does not already have.
	 */
	title: string;
}

/**
 * A play as the five fixed slots the row draws: what you spend, the two or
 * three steps, what you get back.
 */
export interface RouteView {
	spend: RouteEnd;
	/** Step 1 — the item bought with the entry currency. */
	buy: RouteStep;
	/** Step 2 — the same item being sold. */
	sell: RouteStep;
	/** Step 3 — the intermediate currency converted back; `null` on a direct play. */
	convert: RouteStep | null;
	get: RouteEnd;
	/** The run is expected to gain, so the Get amount is drawn as a gain. */
	positive: boolean;
}

/**
 * Does the RENDERED set of plays need the convert slot at all?
 *
 * The route's slots are fixed-width so a column holds the same kind of step on
 * every row, and a direct play holds its convert slot open with an empty dashed
 * tile rather than letting Get slide left under the row above's sell. That
 * reserved width earns its keep only while some row on screen actually converts:
 * a table filtered down to direct plays alone spends the slot and both of its
 * arrows on a column that is empty top to bottom, on the widest cell in a table
 * that already scrolls sideways.
 *
 * So the answer is taken over the whole rendered set and applied to every row —
 * never per row, which would reintroduce the drift the fixed geometry exists to
 * prevent. The page hands this to `ExchangeRoute` and to its own header spans
 * together, because those two carry the same widths and must collapse as one.
 *
 * Read off the LEGS and not off `mode`: `routeSlots` reads the slots by
 * position, so the third leg is what draws the convert step, and a play whose
 * `mode` says `1-hop` on a two-leg body would otherwise reserve a slot nothing
 * can fill.
 *
 * An empty list answers false. The table is not rendered at all when nothing
 * survives the filters, so the answer is unobservable there — and false is the
 * honest one of the two: no play on screen has a step to put in the slot.
 */
export function anyConvertStep(plays: CurrencyExchangePlay[]): boolean {
	return plays.some((play) => play.legs.length > 2);
}

/**
 * The five route slots of one play, at the size the play is worth running
 * (POE-193).
 *
 * The row reads as one sentence: buy N items for X divine → sell those items for
 * chaos → convert the chaos back → keep the difference. Three things it used to
 * get wrong are fixed here, and each is a rule rather than a tweak:
 *
 * 1. EVERY STEP NAMES THE THING IT ACTS ON. Step 2 used to name the sell leg's
 *    QUOTE — "Divine Orb" beside the SCARAB's 0.10 price — which put a currency's
 *    name and artwork on a number that was the item's. All three steps now take
 *    `itemName`/`itemIcon`; no slot shows a quote as if it were the traded thing.
 * 2. EVERY RATE IS A WHOLE-QUANTITY ORDER, unit and all. A leg is quoted
 *    in chaos or in divine and the slot never said which, so a route entered in
 *    divine read chaos → chaos → chaos and the divine appeared nowhere on the
 *    row; and the price was a per-unit decimal, which the in-game exchange
 *    cannot express at all — "sell @ 0.25 div" is not a trade anyone can make.
 *    Both are `stepRate`'s, which totals the row's own count in the step's own
 *    currency and moves the leg's quantity pair onto the hover.
 * 3. THE ENDS ARE IN THE CURRENCY THE READER ACTUALLY SPENDS. `investment` and
 *    `roi` are chaos figures whatever the legs are quoted in, so a divine-entry
 *    play printed "spend 14 chaos" beside a flow denominated in divine. Spend is
 *    now the buy leg's quote — its icon, its name, its number — with the chaos
 *    reading kept as a sub-line rather than as the headline.
 *
 * The legs still arrive in execution order — two for direct (buy X, sell X),
 * three for 1-hop (buy X in A, sell X in B, sell B in A) — and the slots are
 * still read off by POSITION, not by `action`: the third leg is a `sell` on the
 * wire and a *convert* on screen, because what it does for the reader is turn the
 * intermediate currency back into the one they started in.
 *
 * SIZE, AND EVERY FIGURE. There is ONE derivation, `runLedger`, and every slot
 * below is a rendering of one of its fields. Spend is `investmentChaos`, which
 * IS `moneyColumns(play).investment` — the exact figure the Investment column
 * prints — divided by the entry rate. It is NOT rebuilt as
 * `units × price × (1 + tick)`, which is the same quantity in real arithmetic
 * and a different float, and which put two prices for one order on a single row.
 * The chain end is `investment + roi` and Get is `investment + expectedRoi`; the
 * step totals are those same figures rendered per step (E2, E3, E7).
 *
 * The scale that sizes all of it is `displayScale(play).units` — one posting of
 * the buy market on a chaos entry, the worthwhile run on a divine one — and that
 * is not a branch of this function. A row that counts a single item takes
 * exactly this code path at `units === 1`, which is what "in every emitter
 * simultaneously" means structurally: a fallback with its own rendering rules is
 * a second place for the row to disagree with itself.
 *
 * On a chaos row the RUN is not gone, it has moved: the Scale column still
 * prints it (×N, its cost, its hours), and the filter bar's Run cost bound still
 * compares against it through `runInvestment`. What the route slots stopped
 * doing is claiming it — "buy 16 for ≈ 384c" read as a 384c blockbuster on a
 * card worth 30c (owner ruling, 2026-08-22).
 *
 * NO RUN. `worthwhileScale` answers `null` for a play whose measured expectation
 * is not positive — a live case, since ADR-016 serves the measured losers. Its
 * Get is then `investment + expectedRoi` with a non-positive expectation, so it
 * can print BELOW its Spend, and `positive` says so. That is the MEASUREMENT and
 * not broken arithmetic: the row reads 19 in, 16 back, and the hour's best case
 * is still on it as the last step's total of 21. Reading `positive` off `roi`
 * instead — which this did — drew a red Exp. ROI cell beside a Get the row
 * called a gain.
 *
 * `null` for a play with fewer than two legs — a shape the server does not
 * send. Nothing drops such a play: `ExchangeRoute` guards on this answer and
 * renders an empty route cell, so the row is still there with its rank, ROI and
 * depth, and only the route is missing.
 */
export function routeSlots(play: CurrencyExchangePlay, divineChaosRate: number): RouteView | null {
	const [buyLeg, sellLeg, convertLeg] = play.legs;
	if (buyLeg === undefined || sellLeg === undefined) return null;

	const ledger = runLedger(play, divineChaosRate);

	const step = (leg: CurrencyExchangeLeg, rate: string, rateTitle: string | null): RouteStep => ({
		name: leg.itemName,
		icon: leg.itemIcon,
		rate,
		rateTitle,
		suspect: leg.suspect
	});

	// THE ONE EXEMPT BRANCH (spec §3). The response cannot value this row's entry
	// currency in chaos, so there is no `r`, no entry-currency rendering, and no
	// mechanical chain to total: E2, E3, E4 and E7 are suspended and the
	// suspension is this branch's whole content. Both ends fall back to CHAOS from
	// `moneyColumns` and every step prints its own market's ratio, hovered with
	// the caveat that says so.
	//
	// E5 SURVIVES here, and is why the branch is still a closed row: the ends are
	// `I` and `I + X`, so Get is still Spend plus the Exp. ROI column. That is a
	// change from the `investment + roi` this used to print — the best case — on a
	// row whose Exp. ROI cell showed a loss.
	if (ledger === null) {
		// The one money source in scope on this branch, declared inside it: below
		// it every figure is the ledger's, and a `money` still in scope down there
		// would be a second, per-exchange answer to questions the ledger already
		// answers at the row's own size.
		const money = moneyColumns(play);
		const chaos = chaosIconPath();
		const keep = profitLine(money.expectedRoi);
		return {
			spend: {
				amount: formatChaos(money.investment),
				icon: chaos,
				unit: 'chaos',
				sub: null,
				title: endTitle('chaos', null)
			},
			buy: step(buyLeg, pairLine('buy', buyLeg), lotNotScale(buyLeg)),
			sell: step(sellLeg, pairLine('sell', sellLeg), lotNotScale(sellLeg)),
			convert:
				convertLeg === undefined
					? null
					: step(convertLeg, pairLine('convert', convertLeg), lotNotScale(convertLeg)),
			get: {
				amount: formatChaos(money.investment + money.expectedRoi),
				icon: chaos,
				unit: 'chaos',
				sub: keep,
				title: endTitle('chaos', keep)
			},
			// One rule on every branch: the flag follows the measurement, because
			// under E5 the Get IS the measurement. A row that is both exempt and a
			// measured loser is not a served shape, but a hardcoded `true` would draw
			// its loss as a gain.
			positive: money.expectedRoi > 0
		};
	}

	// Only chaos and divine get past `chaosPerQuote`, so the entry is one of the
	// two here and the unit words are a pair rather than a lookup.
	const entryIsChaos = ledger.entryQuote === CHAOS_ID;
	const entryUnit = entryIsChaos ? 'chaos' : 'divine';
	const entryUnitShort = entryIsChaos ? 'c' : 'div';
	// Whole orbs are the honest precision for chaos (POE-189) and nonsense for
	// divine, where a whole run can cost a fraction of one. `RouteEnd.amount` stays
	// BARE — `ExchangeRoute.svelte` renders it beside a separate span holding the
	// full unit word, so a `withOrbUnit` amount would render "3,838c chaos". The
	// step side uses `withOrbUnit`, and the two agree because it calls these same
	// two formatters.
	const amount = (value: number) =>
		entryIsChaos ? formatChaos(value) : formatFractionalOrbs(value);

	const spendSub = entryIsChaos ? null : `≈ ${formatChaos(ledger.investmentChaos)}c`;
	const keep = profitLine(ledger.expectedRoiChaos);
	// Both halves take the MAGNITUDE: the verb already carries the sign, and
	// "lose ≈ 5c (≈ -0.03 div)" would negate twice in one line.
	const getSub = entryIsChaos
		? keep
		: `${keep} (≈ ${formatFractionalOrbs(Math.abs(ledger.expectedRoiChaos) / ledger.entryRate)} div)`;

	/**
	 * Step 3 as the RUN TOTAL it moves — `≈ 2.97 div → 615c`.
	 *
	 * What step 3 converts is the sale's proceeds at the row's own size, and what
	 * it yields is that size's CHAIN END. Both come off the ledger rather than being
	 * re-derived from the quantities the other steps print, so the sell line's
	 * tail and this line's head are the same string by construction (E7).
	 *
	 * The line's right amount is `chainEnd` and NOT the Get: the two identities are
	 * `I + R` and `I + X`, both in CHAOS, each divided by the entry rate for the
	 * rendering — so this line sits immediately left of a Get slot showing a
	 * materially different number, and the hover is what labels the gap: the line
	 * ends on what the hour's BEST CASE would have paid, Get is what the row is
	 * MEASURED to return, and the difference between them is the difference
	 * between the ROI and Exp. ROI columns (spec §5) — that gap in chaos too, so a
	 * divine-entry row shows it at the divine rate like everything else on the row.
	 *
	 * The proceeds are `chainEnd / u2` and not `chainEnd / price` — the UNDERCUT
	 * divisor, derived in `saleTotal`, whose doc carries the reasoning. The old
	 * rationale here (bare `price`, because the undercut is already inside
	 * `expectedRoi`) was correct only while this line's numerator was the Get.
	 * Under E3 the numerator is `chainEnd`, which is built from `R`, which is
	 * built from `roiPct`, which already contains `u2` — so recovering the
	 * proceeds requires dividing by `u2`, and the raw price would print a figure
	 * the wire's own `roiPct` contradicts.
	 *
	 * The ratio line is the fallback for a leg with no usable price, and the
	 * ledger has already decided that: `sellForward` says the sale was derived
	 * forwards because `u2` was unreadable, and two amounts derived by different
	 * routes would not be in this market's ratio — which is exactly the fact that
	 * made the fallback necessary.
	 */
	const convertStep = (): RouteStep | null => {
		if (convertLeg === undefined) return null;
		if (ledger.sellTotal === null || ledger.sellForward) {
			return step(
				convertLeg,
				pairLine('convert', convertLeg),
				`${lotNotScale(convertLeg)} ${CONVERT_UNPRICED}`
			);
		}
		return {
			// The line names both currencies and the tile carries the artwork of the
			// one being converted, so the name would only repeat the tile. It leads
			// the hover instead, where the ratio it replaced also lives.
			name: null,
			icon: convertLeg.itemIcon,
			rate: `≈ ${withOrbUnit(ledger.sellTotal, quoteUnit(convertLeg.item))} → ${withOrbUnit(ledger.chainEnd, entryUnitShort)}`,
			rateTitle: `${convertLeg.itemName} — this market posts "${pairLine('convert', convertLeg)}". The line totals what this row counts instead: the step-2 proceeds it converts, and what the hour's best case would have paid for them — the Spend plus the ROI column. The Get slot at the end of the row is the Spend plus the Exp. ROI column, and the gap between the two amounts is the gap between those two columns. Both identities are in chaos, and a divine-entry route prints them at the divine rate.`,
			suspect: convertLeg.suspect
		};
	};

	return {
		spend: {
			amount: amount(ledger.spend),
			icon: currencyIconPath(ledger.entryQuote),
			unit: entryUnit,
			// The ledger's `investmentChaos` and not `spend × entryRate`: the two are
			// the same run at the same undercut, and this is the exact figure the
			// Investment column prints and the filter bar's Run cost bounds compare
			// against, so reading it directly leaves no seam to disagree across.
			sub: spendSub,
			title: endTitle(entryUnit, spendSub)
		},
		buy: step(
			buyLeg,
			stepRate('buy', buyLeg, ledger.units, ledger.spend, entryUnitShort),
			// The buy hover is the only one told the run, because the N > F
			// overshoot is a fact about the ENTRY order: this market's minimal
			// posting counts past the run the Scale column sizes.
			marketPair(buyLeg, ledger.units, false, worthwhileScale(play)?.flips ?? null)
		),
		sell:
			ledger.sellTotal === null
				? // Neither the convert leg nor the sell leg could price the sale, so
					// there is no total to print and the market's own ratio is what is
					// left that stays true.
					step(sellLeg, pairLine('sell', sellLeg), lotNotScale(sellLeg))
				: step(
						sellLeg,
						stepRate('sell', sellLeg, ledger.units, ledger.sellTotal, quoteUnit(sellLeg.quote)),
						marketPair(sellLeg, ledger.units, true, null)
					),
		convert: convertStep(),
		get: {
			amount: amount(ledger.get),
			icon: currencyIconPath(ledger.entryQuote),
			unit: entryUnit,
			// One rule for every non-chaos orb amount on the row, tail included: at
			// a realistic rate the ~100c profit lands near half a divine, where
			// this and `formatLegPrice` agree — so the choice buys no visible
			// difference today and costs nothing, and it means the tail cannot
			// drift into a different precision from the total right above it.
			sub: getSub,
			title: endTitle(entryUnit, getSub)
		},
		positive: ledger.expectedRoiChaos > 0
	};
}

// ------------------------------------------------------------ the refetch --

/**
 * Debounce window for a Mercure-triggered refetch. Collapses a burst of
 * publishes into a single request.
 */
export const REFETCH_DEBOUNCE_MS = 2000;

/**
 * Random spread added on top of the debounce. The debounce alone does not
 * disperse a herd, it aligns one: every client receives the same publish within
 * milliseconds, so a fixed delay puts all of them on `publish + 2000ms`
 * exactly. The offset scatters arrivals across 2–6s instead. Same two values as
 * `LabPage.svelte`'s `MERCURE_DEBOUNCE_MS`/`MERCURE_JITTER_MS`, against the
 * same server, and for the same reason.
 */
export const REFETCH_JITTER_MS = 4000;

/**
 * How long to wait before a Mercure-triggered refetch.
 *
 * Re-roll it on every fire rather than once per session: a fixed per-client
 * offset would land the same clients in the same slot on every publish, which
 * spreads the herd once instead of on each tick. `random` is injectable so the
 * spread is testable.
 */
export function refetchDelay(random: () => number = Math.random): number {
	return REFETCH_DEBOUNCE_MS + random() * REFETCH_JITTER_MS;
}
