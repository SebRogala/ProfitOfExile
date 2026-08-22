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
 * `'roi'` re-orders by the OPTIMISTIC chaos the ROI column prints, which is a
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
 * served order was called while the server ranked on the optimistic
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
 * and the optimistic per-hour figure overstates that by 4-8x, so scaling on it
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
 * `hours` is deliberately OPTIMISTIC, and the tooltip says so: `depth` is the
 * WHOLE market's hourly volume on the play's thinnest leg, so this is the time
 * the fill takes if the reader takes every unit of it and no one else trades.
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
 * positivity floor applies to the optimistic number, while a measured
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

// ------------------------------------------------------------- the money --

/** The three money columns of one row, all priced at one scale. */
export interface MoneyColumns {
	/** Chaos the row ties up — the Investment column. */
	investment: number;
	/** OPTIMISTIC chaos gained — the ROI column. */
	roi: number;
	/** Measured chaos expected — the Exp. ROI column. */
	expectedRoi: number;
}

/**
 * The row's three money figures, at the size the row is worth running
 * (POE-193).
 *
 * THE INVARIANT: every money figure on a row is on ONE scale — the same scale
 * the route slots print. The row used to mix three of them. Spend/Get/`keep ≈`
 * were the run (`routeSlots`), the step rates were run-sized orders, and these
 * three columns were the server's PER-EXCHANGE fields — so a two-flip play read
 * "Spend 450 / Get 579 / keep ≈ 129" beside "Investment 225c, ROI +173c,
 * Exp. ROI +64c", six numbers about two different trips with nothing on screen
 * saying which was which.
 *
 * So both branches here mirror `routeSlots`' own two branches exactly:
 *
 * - A play with a worthwhile scale reports the RUN. `investment` is
 *   `scale.investment` read directly — the same figure the filter bar's Run cost
 *   bounds compare against (`applyNumericFilters`) and the same one the Scale
 *   column's "N c in" sub-line prints, so a bound, a sub-line and a column
 *   cannot disagree about what the trip costs. `expectedRoi` is `scale.gain`,
 *   which is the number behind the Get slot's `keep ≈` line for the same reason:
 *   one source, not two agreeing calculations.
 * - A play with none reports the PER-EXCHANGE fields, exactly as the wire sends
 *   them — which is what `routeSlots` falls back to as well. The row stays on one
 *   scale in BOTH branches; that is the invariant, not "always the run".
 *
 * `roi` is scaled by MULTIPLYING the wire's per-exchange figure by the flip
 * count rather than being read off the scale, because `WorthwhileScale` carries
 * no optimistic total and should not: the scale exists to answer how far the
 * MEASURED expectation has to be repeated. Multiplying keeps the column what it
 * has always been — the raw best case, the RAW half of the NET/RAW pair the
 * ROI% column spells out — now told at the run's size instead of one exchange's.
 * The flip count is a positive integer, so the sign of the column is the wire's
 * sign and a losing raw return still reads as one.
 */
export function moneyColumns(play: CurrencyExchangePlay): MoneyColumns {
	const scale = worthwhileScale(play);
	if (scale === null) {
		return { investment: play.investment, roi: play.roi, expectedRoi: play.expectedRoi };
	}
	return {
		investment: scale.investment,
		roi: play.roi * scale.flips,
		expectedRoi: scale.gain
	};
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
 * That option is deliberately NOT re-pointed at the Exp. ROI column's run-scaled
 * figure, and the reason is what that figure is: `scale.gain` is the expectation
 * repeated until it clears about 100c, so every play whose expectation is under
 * 100c lands in [100, 200) and ordering by it would rank the table on the
 * remainder of a division. The served order stays the ranking; the column tells
 * the reader what the run pays. The seam is that the Exp. ROI column is not
 * monotonic in the order it sits in — a 101c expectation runs once for 101c and
 * ranks above a 99c one that runs twice for 198c. It is a seam by choice: the
 * alternative throws away the clean/suspect partition and the coverage band for
 * an order nobody asked for.
 *
 * `'roi'` re-sorts by the OPTIMISTIC chaos the ROI COLUMN prints — which since
 * the money columns moved to run pricing is `moneyColumns().roi`, the per-run
 * total, and no longer the wire's per-exchange `roi`. The two orders genuinely
 * differ (the flip count is `ceil(100 / expectedRoi)`, so a small optimistic
 * return repeated forty times out-totals a large one that runs once), and a
 * table ordered by a number it does not show reads as broken. The Investment
 * column has no sort of its own; if it ever gets one it reads the same helper.
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
 * rounds; the convert step's two figures are a run total and the proceeds that
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

/** A step's rate line, plus the hover it needs when the two disagree. */
interface LegRate {
	/** `buy 12 for 420c` — the order, or the decimal fallback. */
	text: string;
	/** Why the printed quantity is not the one asked for; `null` when it is. */
	title: string | null;
	/**
	 * Units of the leg's item the printed order moves — what step 2 is allowed to
	 * sell, since it can only sell what step 1 bought. `null` on the decimal
	 * fallback, which posts no quantity at all.
	 */
	posted: number | null;
}

/** What a step is being asked to move, and the run that asked for it. */
interface RunOrder {
	/**
	 * Units this step should cover: the run for step 1, and for step 2 the
	 * quantity step 1 actually bought — which is not the same number whenever the
	 * two markets post in different lots.
	 */
	target: number;
	/** The whole worthwhile run, which is what both END amounts are priced at. */
	run: number;
	/**
	 * `target` is stock the row has already BOUGHT — true for step 2 behind a buy
	 * step that posted a quantity, false for step 1 and for a step 2 whose buy leg
	 * fell back to the decimal and so bought nothing on screen.
	 *
	 * It is what licenses the "stay unsold" clause: units a sell lot cannot cover
	 * are stock left in the stash, while units a BUY lot cannot cover were simply
	 * never bought and are already accounted for by the run the ends price. Keyed
	 * on this rather than on `target !== run`, which would have disclosed the
	 * residue on a row whose buy market happens to post one at a time and hidden
	 * it on one whose lots merely coincide — the same fact, told or not told by an
	 * accident of the buy side.
	 */
	bought: boolean;
}

/**
 * One leg's rate as the slot prints it: a POSTABLE ORDER, quantities on both
 * sides — `buy 12 for 420c`, `sell 12 for 3 div`, `convert 1 div for 209c`.
 *
 * The in-game Currency Exchange trades whole quantities against whole
 * quantities. It has no field for a per-unit price, so "sell @ 0.25 div" — what
 * this printed before POE-193 — is an instruction the reader cannot carry out;
 * the same trade posts as "sell 4 for 1 div". The wire now carries that pair
 * (`priceItemQty`/`priceQuoteQty`, exactly the leg's `price`), so the rate is
 * rendered FROM it rather than from the decimal.
 *
 * The pair is the extreme the hour PRINTED at, which is not quite the order to
 * post: the one that fills sits a step tighter, and the end amounts already pay
 * for that step. So the step is worded as the market's shape and never as
 * "post this" — the `Route` tooltip carries the caveat in words.
 *
 * Steps 1 and 2 both carry an `order`, because an order with no quantity is not
 * an order. The pair is scaled by the largest whole multiple that fits
 * `order.target`, so the printed order is always exactly postable —
 * `integer × pair`, never a rounded quantity the exchange would refuse — and
 * when the target is not a multiple of `priceItemQty` the displayed quantity
 * SNAPS to the largest one that is (a target of 12 on a 5-for-2 market shows 10
 * for 4) with the hover saying so. The run's own arithmetic is untouched: the
 * route ends are computed from `price` and the true flip count, never from this
 * string.
 *
 * The two steps DO NOT snap independently. `routeSlots` derives the traded
 * quantity once — step 1 snaps against the buy market's lot, and step 2's target
 * is then what step 1 actually bought — because the reader can only sell what
 * they hold, and two independent snaps against different lots printed rows like
 * "buy 48 for 3 div → sell 50 for 5 div" on a single fixture. Whatever step 2's
 * own lot cannot cover stays in the stash; that residue is the market's, not an
 * arithmetic slip, and it never touches the ends — which is exactly why the sell
 * step's hover has to name it (`order.bought`), since the ends price the whole
 * run and nothing else on the row would show the reader the unsold remainder.
 *
 * The one exception to "step 2 sells no more than step 1 bought" is a target
 * BELOW a single lot, where the floor would be zero. There the step posts one
 * whole lot and the hover says it overshoots — "sell 0 for 0 div" is not a
 * reading, and the honest alternative is telling the reader the smallest order
 * this market accepts.
 *
 * Step 3 passes no `order` and answers the BARE pair. What the convert step
 * moves is the proceeds of step 2 — a quantity of the intermediate currency, not
 * of items — which is not an integer and not predictable from the pair, so there
 * is no order to scale here. `routeSlots` totals that quantity from the run's own
 * ends instead and prints THAT on the slot; this string becomes the ratio behind
 * its hover, and stays the slot's own line on a play with no run to total.
 *
 * The unit words come from `quoteUnit` on BOTH sides. The quote side always has
 * one (chaos or divine); the item side has one only when the item is itself one
 * of those two, which is exactly the convert step, whose item is the
 * intermediate currency. A scarab or a card gets a bare count, as it should.
 *
 * A leg whose pair is missing or not a positive integer pair falls back to the
 * decimal rate this printed before, unit and all — "buy 12 for 0" would be worse
 * than a price the reader has to divide themselves. The reachable cause is
 * VERSION SKEW and not a stale cache (this app holds no response cache): the
 * desktop points at a configurable server, so a build shipped with these fields
 * can be aimed at a server from before POE-193 that never sends them. That
 * fallback is the old FORM and not the old output exactly: step 2 now carries a
 * quantity in either branch, so a skewed sell leg reads `sell 12 @ 0.25 div`
 * where it once read `sell @ 0.25 div`.
 */
function legRate(verb: string, leg: CurrencyExchangeLeg, order?: RunOrder): LegRate {
	const quote = quoteUnit(leg.quote);
	const itemQty = leg.priceItemQty;
	const quoteQty = leg.priceQuoteQty;

	if (!(Number.isInteger(itemQty) && Number.isInteger(quoteQty) && itemQty > 0 && quoteQty > 0)) {
		const head = order === undefined ? verb : `${verb} ${formatChaos(order.target)}`;
		const price = formatLegPrice(leg.price);
		return {
			text: quote === '' ? `${head} @ ${price}` : `${head} @ ${price} ${quote}`,
			title: null,
			posted: null
		};
	}

	// At least one whole lot: a target under the market's own lot has no postable
	// order beneath it, and printing zero of the item would be worse than printing
	// the smallest order the market will take.
	const multiple = order === undefined ? 1 : Math.max(1, Math.floor(order.target / itemQty));
	const posted = multiple * itemQty;
	const text = `${verb} ${withUnit(posted, quoteUnit(leg.item))} for ${withUnit(multiple * quoteQty, quote)}`;

	if (order === undefined || posted === order.target) return { text, title: null, posted };

	// Both titles are told against `target` — the quantity THIS step was asked to
	// move — because that is the number the printed order diverges from. The run
	// is named too, but only where it is a second number: on step 1 the two are
	// the same, and citing both would invent a distinction the row does not have.
	const ofTheRun =
		order.target === order.run ? '' : ` of the run of ${formatChaos(order.run)}`;

	if (posted > order.target) {
		return {
			text,
			// Not "bigger than the amounts at either end": the ends are chaos and
			// divine totals and the order is a count of items, so the comparison has
			// to be made against the run those ends are priced for.
			title: `This market posts ${formatChaos(itemQty)} at a time, which is more than the ${formatChaos(order.target)}${ofTheRun} this step moves — the order shown is one whole lot, bigger than the run the two ends are priced for.`,
			posted
		};
	}

	// Units a SELL lot cannot cover are bought stock nobody takes off the reader's
	// hands, and the row says so nowhere else — the ends price the whole run, so
	// without this the residue is invisible. Units a BUY lot cannot cover need no
	// such clause: they were never bought.
	const unsold =
		order.bought && order.target > posted
			? ` ${formatChaos(order.target - posted)} of the ${formatChaos(order.target)} bought stay unsold.`
			: '';
	return {
		text,
		title: `This market posts in multiples of ${formatChaos(itemQty)}, so ${formatChaos(order.target)} is not one order — the step shows the largest order that fits, and the amounts at both ends still count the whole run of ${formatChaos(order.run)}.${unsold}`,
		posted
	};
}

/** One traded step of a route: what it moves, at what price. */
export interface RouteStep {
	/**
	 * The item the step acts on — bought, sold, or converted.
	 *
	 * `null` on a convert step that prints a RUN TOTAL, and only there. That line
	 * already names both currencies ("≈ 2.52 div → 526c") and the tile already
	 * carries the artwork of the one being converted, so a "Divine Orb" heading
	 * above it says nothing a reader cannot see — while the height it costs is the
	 * one thing a three-step row has none of. The name is not lost: it leads the
	 * step's `rateTitle`.
	 */
	name: string | null;
	/** Its API-relative icon path, for `iconSrc`; `null` when it has none. */
	icon: string | null;
	/** `buy 12 for 420c`, `sell 12 for 3 div`, `≈ 2.52 div → 526c`. */
	rate: string;
	/**
	 * The hover behind a rate whose printed quantity is not the run's, because
	 * the market only posts in lots the run does not divide by; `null` whenever
	 * the two agree, which is the ordinary case.
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
	 * `keep ≈ 102c` profit line under Get. `null` when there is nothing to add.
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
 *    Both are `legRate`'s, which words the leg's quantity pair instead.
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
 * SIZE. The amounts are the whole worthwhile RUN, not one exchange:
 * `worthwhileScale(play).flips` is the same derived size the Scale column prints
 * and the Run cost bounds are compared against, so the three cannot disagree
 * about how big this play is. Step 3 is sized the same way and reads as a total
 * rather than as an order (`convertStep` below): it is the only step whose
 * quantity the reader does not choose, so a per-lot ratio there told them to
 * convert one orb of a run that converts hundreds. Spend is
 * `flips × price × (1 + tick)` — the
 * UNDERCUT entry, the price an order that actually fills is posted at, which is
 * what `investment` and `expectedRoi` are already priced at (POE-188). Rebuilding
 * it from `price` alone would print the raw best case under a net expectation.
 * Get is Spend plus the run's expected profit, and the profit line under it says
 * that profit in chaos, because chaos is the currency `expectedRoi` is measured
 * in and the one a reader compares plays across.
 *
 * NO SIZE. `worthwhileScale` answers `null` for a play whose measured
 * expectation is not positive — a live case, since ADR-016 serves the measured
 * losers — and there is no run size to render. The ends then fall back to the
 * PER-EXCHANGE chaos figures this drew before, which are the two numbers the wire
 * guarantees for such a play; the steps keep their units and their item icons,
 * so the two fixes that are not about size still apply. `positive` follows the
 * `roi` those ends are built from, not the expectation: the flag styles the
 * numbers on screen, and this branch's Get is visibly larger than its Spend
 * (the server's positivity floor sees to that), so flagging it as no gain would
 * read as broken arithmetic rather than as a warning. The warning has its own
 * two homes — the red Exp. ROI cell and the Scale column's dash.
 *
 * `null` for a play with fewer than two legs — a shape the server does not
 * send. Nothing drops such a play: `ExchangeRoute` guards on this answer and
 * renders an empty route cell, so the row is still there with its rank, ROI and
 * depth, and only the route is missing.
 */
export function routeSlots(play: CurrencyExchangePlay, divineChaosRate: number): RouteView | null {
	const [buyLeg, sellLeg, convertLeg] = play.legs;
	if (buyLeg === undefined || sellLeg === undefined) return null;

	const scale = worthwhileScale(play);

	const step = (leg: CurrencyExchangeLeg, rate: LegRate, unscaled?: string): RouteStep => ({
		name: leg.itemName,
		icon: leg.itemIcon,
		rate: rate.text,
		// `unscaled` is the no-worthwhile-size note, and it applies only to a step
		// that actually printed a lot: a decimal fallback shows no lot to explain,
		// and a step with its own snap note has the more specific thing to say.
		rateTitle: rate.title ?? (rate.posted === null ? null : (unscaled ?? null)),
		suspect: leg.suspect
	});

	// ONE traded quantity for the whole row, derived buy-first. Step 1 snaps the
	// run against the buy market's lot; step 2 then targets what step 1 actually
	// bought, because the reader cannot sell what they did not buy — two steps
	// snapping independently against different lots printed "buy 48 → sell 50" on
	// a single fixture. A buy leg that fell back to the decimal posted no
	// quantity, so the sell step targets the run itself rather than nothing.
	// Step 3 gets no quantity at all: what it converts is the sale's proceeds.
	const run = scale?.flips;
	const buyRate = legRate(
		'buy',
		buyLeg,
		run === undefined ? undefined : { target: run, run, bought: false }
	);
	const sellOrder =
		run === undefined
			? undefined
			: { target: buyRate.posted ?? run, run, bought: buyRate.posted !== null };

	// NO SCALE, and the steps say so. `worthwhileScale` answered `null`, so there
	// is no run to scale toward and each step falls back to one lot of its OWN
	// market — which on a row whose two markets post in different lots reads as a
	// divergence ("buy 1 for 35c → sell 4 for 1 div") with nothing on screen to
	// explain it. The note is the explanation; it is not a snap, so `legRate`
	// leaves the title empty and this branch fills it.
	const unscaled =
		run === undefined
			? // Told without asserting that the two lots DIFFER: they line up on a
				// row whose markets happen to post the same lot, and the note is on
				// every unscaled step rather than only the mismatched ones.
				'This play has no worthwhile size to repeat toward, so each step shows one lot of its own market rather than a run quantity — two counts that agree only where the two markets happen to post the same lot. The ends fall back to what ONE exchange costs and returns.'
			: undefined;

	const steps = {
		buy: step(buyLeg, buyRate, unscaled),
		sell: step(sellLeg, legRate('sell', sellLeg, sellOrder), unscaled)
	};

	// The convert step's market ratio, derived once: it is what step 3 prints when
	// there is no run to total, and the hover behind the total when there is.
	const convertRatio = convertLeg === undefined ? null : legRate('convert', convertLeg);

	/**
	 * Step 3 as the RUN TOTAL it moves — `≈ 2.52 div → 526c` — or the market's own
	 * ratio when `entry` is `null`.
	 *
	 * What step 3 converts is the proceeds of step 2 for the whole worthwhile run,
	 * and what it yields is the run's Get. The per-lot ratio this replaced read as
	 * an instruction to convert ONE orb ("convert 1 div for 209c") on a row whose
	 * other four slots are all counting the run, and how much of the run that one
	 * orb is depends entirely on how large the proceeds happen to be — which is
	 * the number the reader came to the slot for and the one it did not show.
	 *
	 * Both figures come off the SAME numbers the ends are built from — `entry.get`
	 * is the Get amount itself, and the proceeds are that amount divided by this
	 * leg's own price — rather than being re-derived from the quantities the buy
	 * and sell steps printed. Those quantities are snapped to their markets' lots
	 * and are deliberately not the run (see `legRate`), so multiplying them back
	 * up would put a third, disagreeing total on a row that already has one.
	 *
	 * The bare `price` and not `price × (1 + tick)`: the undercut every leg of the
	 * run is filled at is already inside `expectedRoi`, which is what `entry.get`
	 * is built from, so charging it a second time here would shrink the proceeds
	 * against the very total they are divided out of.
	 *
	 * `entry` is `null` in the two branches whose ends are not the entry-currency
	 * run pair — no worthwhile size, and an entry this response cannot value in
	 * chaos — and a non-positive `price` (version skew, a leg served without one)
	 * falls back the same way, since a total divided by nothing is not a reading.
	 */
	const convertStep = (entry: { get: number; unit: string } | null): RouteStep | null => {
		if (convertLeg === undefined || convertRatio === null) return null;
		if (entry === null || !Number.isFinite(convertLeg.price) || convertLeg.price <= 0) {
			return step(convertLeg, convertRatio);
		}
		const proceeds = entry.get / convertLeg.price;
		return {
			// The line names both currencies and the tile carries the artwork of the
			// one being converted, so the name would only repeat the tile. It leads
			// the hover instead, where the ratio it replaced also lives.
			name: null,
			icon: convertLeg.itemIcon,
			rate: `≈ ${withOrbUnit(proceeds, quoteUnit(convertLeg.item))} → ${withOrbUnit(entry.get, entry.unit)}`,
			rateTitle: `${convertLeg.itemName} — this market posts "${convertRatio.text}". The line totals the whole run instead: the step-2 proceeds it converts, and what they come back as, which is the Get amount at the end of the row.`,
			suspect: convertLeg.suspect
		};
	};

	// No derivable run size: the per-exchange chaos ends, exactly as before.
	if (scale === null) {
		const chaos = chaosIconPath();
		return {
			spend: {
				amount: formatChaos(play.investment),
				icon: chaos,
				unit: 'chaos',
				sub: null,
				title: endTitle('chaos', null)
			},
			...steps,
			convert: convertStep(null),
			get: {
				amount: formatChaos(play.investment + play.roi),
				icon: chaos,
				unit: 'chaos',
				sub: null,
				title: endTitle('chaos', null)
			},
			// The flag describes the two numbers ON SCREEN, and this branch renders
			// the optimistic pair. Saying "no gain" over a Get that is visibly
			// larger than its Spend would read as a bug in the arithmetic; the
			// measured verdict is already carried where it belongs, by the red
			// Exp. ROI cell and the Scale column's dash.
			positive: play.roi > 0
		};
	}

	// `scale.gain` and nothing derived from it: this is the same figure the Exp.
	// ROI column reports through `moneyColumns`, so the profit line under Get and
	// the column two cells to its right are one number by construction.
	const keep = `keep ≈ ${formatChaos(scale.gain)}c`;
	const chaosPerEntry = chaosPerQuote(buyLeg.quote, divineChaosRate);

	// An entry currency this response cannot value in chaos. Unreachable on a
	// served body — see `chaosPerQuote` — so the ends answer in the currency the
	// run's own figures are already in rather than dividing by a rate that is not
	// there. The run size, its profit line and the units on every rate all stand.
	if (chaosPerEntry === null) {
		const chaos = chaosIconPath();
		return {
			spend: {
				amount: formatChaos(scale.investment),
				icon: chaos,
				unit: 'chaos',
				sub: null,
				title: endTitle('chaos', null)
			},
			...steps,
			// These ends are chaos and the entry currency is not, so the Get amount is
			// not what this leg's price converts INTO — the one figure a run total
			// would have to end on. The market ratio is the honest line here.
			convert: convertStep(null),
			get: {
				amount: formatChaos(scale.investment + scale.gain),
				icon: chaos,
				unit: 'chaos',
				sub: keep,
				title: endTitle('chaos', keep)
			},
			positive: true
		};
	}

	// Only chaos and divine get past `chaosPerQuote`, so the entry is one of the
	// two here and the unit words are a pair rather than a lookup.
	const entryIsChaos = buyLeg.quote === CHAOS_ID;
	const entryUnit = entryIsChaos ? 'chaos' : 'divine';
	// Whole orbs are the honest precision for chaos (POE-189) and nonsense for
	// divine, where a whole run can cost a fraction of one.
	const amount = (value: number) =>
		entryIsChaos ? formatChaos(value) : formatFractionalOrbs(value);

	const spend = scale.flips * buyLeg.price * (1 + buyLeg.tick);
	const profit = scale.gain / chaosPerEntry;
	const spendSub = entryIsChaos ? null : `≈ ${formatChaos(scale.investment)}c`;
	const getSub = entryIsChaos ? keep : `${keep} (≈ ${formatFractionalOrbs(profit)} div)`;

	return {
		spend: {
			amount: amount(spend),
			icon: currencyIconPath(buyLeg.quote),
			unit: entryUnit,
			// `scale.investment` rather than `spend × chaosPerEntry`: the two are
			// the same run at the same undercut, and this is the exact figure the
			// filter bar's Run cost bounds compare against, so reading it directly
			// leaves no seam for the sub-line and the bound to disagree across.
			sub: spendSub,
			title: endTitle(entryUnit, spendSub)
		},
		...steps,
		// The one branch with a run to total: `spend + profit` is the Get amount
		// itself, so the convert line's tail and the slot at the end of the row are
		// the same string by construction and not by two agreeing calculations.
		convert: convertStep({ get: spend + profit, unit: entryIsChaos ? 'c' : 'div' }),
		get: {
			amount: amount(spend + profit),
			icon: currencyIconPath(buyLeg.quote),
			unit: entryUnit,
			// One rule for every non-chaos orb amount on the row, tail included: at
			// a realistic rate the ~100c profit lands near half a divine, where
			// this and `formatLegPrice` agree — so the choice buys no visible
			// difference today and costs nothing, and it means the tail cannot
			// drift into a different precision from the total right above it.
			sub: getSub,
			title: endTitle(entryUnit, getSub)
		},
		// A derivable scale IS a positive expectation, and `gain` is that
		// expectation multiplied by a positive flip count.
		positive: true
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
