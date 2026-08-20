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
 * Which number the table is ordered by. `'roiPct'` is the server's own
 * ranking; `'roi'` re-orders by chaos gained per exchange, which is a different
 * question — a 40% return on 2c is not the play a stocked account wants; and
 * `'fill'` by how long the reader's quantity would take to trade, which is the
 * question a big return on a thin market answers badly.
 */
export type ExchangeSort = 'roiPct' | 'roi' | 'fill';

/** The sort picker's entries, in display order. */
export const SORT_OPTIONS: { value: ExchangeSort; label: string }[] = [
	{ value: 'roiPct', label: 'ROI%' },
	{ value: 'roi', label: 'ROI' },
	{ value: 'fill', label: 'Fill' }
];

/** Narrow a persisted or user-supplied string to a sort; default `'roiPct'`. */
export function parseSort(raw: string): ExchangeSort {
	return SORT_OPTIONS.some((option) => option.value === raw) ? (raw as ExchangeSort) : 'roiPct';
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

/** How many exchanges the row's figures are multiplied by when nothing is set. */
export const DEFAULT_QUANTITY = 1;

/**
 * Narrow a persisted or user-supplied string to a repeat count.
 *
 * Whole exchanges only, and never below one: the quantity multiplies
 * investment, ROI and the depth comparison, so a `0` would flatten the whole
 * table to zero and a fraction would claim a partial exchange the book cannot
 * fill. A typed-in decimal truncates rather than rejecting, because the stepper
 * writes while the user is still typing.
 */
export function parseQuantity(raw: string): number {
	const value = Number(raw);
	if (!Number.isFinite(value) || value < DEFAULT_QUANTITY) return DEFAULT_QUANTITY;
	return Math.floor(value);
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
 * significant digits of it dressed junk as precision. The min-gain filter hides
 * such rows outright when one is set.
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

/**
 * How long the reader's quantity would take to trade, in hours, at the depth of
 * the play's thinnest leg (POE-189).
 *
 * Deliberately OPTIMISTIC, and the tooltip says so: `depth` is the whole
 * market's hourly volume on that leg, so this is the time the fill would take if
 * the reader took every unit of it and no one else traded. The real number is
 * larger by however much of the book the competition holds, and larger again on
 * a direct play, which buys and sells the same item on the one market.
 *
 * Unrounded on purpose — the page rounds UP for display, and rounding here would
 * flatten every play under an hour onto the same sort key.
 *
 * `null` for a leg that traded nothing (`depth` 0, the shape a just-listed
 * market has) and for a non-finite one: both mean the hours cannot be computed,
 * and dividing would answer `Infinity`/`NaN`, which the column would print as a
 * duration.
 */
export function fillHours(play: CurrencyExchangePlay, quantity: number): number | null {
	if (!Number.isFinite(play.depth) || play.depth <= 0) return null;
	if (!Number.isFinite(quantity)) return null;
	return quantity / play.depth;
}

// -------------------------------------------------------------- the order --

/**
 * The table's rows in the order the sort picker asks for.
 *
 * `'roiPct'` is the list exactly as served: the server already ranks clean
 * before suspect, then `roiPct` desc, then turnover, then direct-first, then
 * key (POE-188). Re-sorting it here on `roiPct` alone would throw away those
 * tie-breaks, so the ROI% sort keeps the served order — copied, so a caller
 * holding the result can never mutate the response's own array through it.
 *
 * `'roi'` re-sorts by chaos per exchange, and `'fill'` by how long the reader's
 * quantity would take to trade — ascending, because the fastest fill is the best
 * one, and a play whose depth cannot be read (`fillHours` `null`) sits at the
 * end of its partition rather than being dropped or treated as instant.
 *
 * Both re-sorts keep the one property the server ordering exists to carry: every
 * suspect play stays after every clean one, however large its ROI or however
 * fast its fill. A suspect number is the reason it ranks last, so letting it
 * out-sort a clean play would hand the reader the very row the flag warns about.
 * Within a partition the sort is stable, so tied plays keep the server's
 * remaining tie-breaks.
 *
 * `quantity` names the size the fill is measured at, so the order and the Fill
 * column always read the same figure. For the finite quantity ≥ 1 that
 * `parseQuantity` guarantees every caller, it does not change the ORDER on its
 * own — one positive multiplier over every play cannot reorder `quantity /
 * depth` — so a build that dropped it would still sort correctly and would then
 * drift the moment the column's rule stops being a plain division.
 */
export function sortPlays(
	plays: CurrencyExchangePlay[],
	sort: ExchangeSort,
	quantity: number
): CurrencyExchangePlay[] {
	if (sort === 'roiPct') return [...plays];
	if (sort === 'fill') {
		return [...plays].sort((a, b) => {
			if (a.suspect !== b.suspect) return a.suspect ? 1 : -1;
			const left = fillHours(a, quantity);
			const right = fillHours(b, quantity);
			if (left === null || right === null) {
				if (left === right) return 0;
				return left === null ? 1 : -1;
			}
			return left - right;
		});
	}
	return [...plays].sort((a, b) => {
		if (a.suspect !== b.suspect) return a.suspect ? 1 : -1;
		return b.roi - a.roi;
	});
}

// --------------------------------------------------------------- the legs --

/**
 * One leg as a sentence: `buy Mod Values with Reroll Rare @ 0.004975`.
 *
 * Wording follows the direction of the swap — you buy an item *with* a quote
 * currency and sell it *for* one — so a two-leg play reads as the sequence the
 * player performs. Display names only; the raw ids stay in the row's `title`
 * attribute, where they are available without cluttering the chip.
 */
export function legLabel(leg: CurrencyExchangeLeg): string {
	const preposition = leg.action === 'buy' ? 'with' : 'for';
	return `${leg.action} ${leg.itemName} ${preposition} ${leg.quoteName} @ ${formatLegPrice(leg.price)}`;
}

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
 * The API-relative artwork path for Chaos Orbs (POE-189).
 *
 * Built rather than scavenged off a leg. The server's icon route serves ANY
 * asset id (`IconPath` in `internal/exchange/items.go`), so the path exists
 * whether or not this particular play happens to quote a leg in chaos — and the
 * plays that do not are exactly the ones the old leg-scavenging version left
 * with two empty tiles, on a row whose ends are still denominated in chaos.
 *
 * `encodeURIComponent` is the mirror of the server's `url.PathEscape`: the id
 * carries slashes, Go escapes them as `%2F` in a path SEGMENT, and so does this.
 * Verified against a running server — the escaped URL answers 200, not the 404 a
 * mismatched escaping would give.
 */
export function chaosIconPath(): string {
	return `/currency-exchange/icon/${encodeURIComponent(CHAOS_ID)}`;
}

/** One traded step of a route: what it moves, at what price. */
export interface RouteStep {
	/** The item the step is about — bought, received, or converted. */
	name: string;
	/** Its API-relative icon path, for `iconSrc`; `null` when it has none. */
	icon: string | null;
	/** `buy @ 1.00`, `sell @ 0.50`, `convert @ 204`. */
	rate: string;
	/** The leg's price sits outside its fair band — the tile is marked. */
	suspect: boolean;
}

/** The chaos in and the chaos out, per exchange. */
export interface RouteEnd {
	amount: string;
	/** Always the chaos artwork — `chaosIconPath()`, never `null` (POE-189). */
	icon: string;
}

/**
 * A play as the five fixed slots the row draws: what you spend, the two or
 * three steps, what you get back.
 */
export interface RouteView {
	spend: RouteEnd;
	/** Step 1 — the item bought with the entry currency. */
	buy: RouteStep;
	/** Step 2 — what selling it pays out in. */
	sell: RouteStep;
	/** Step 3 — the intermediate currency converted back; `null` on a direct play. */
	convert: RouteStep | null;
	get: RouteEnd;
	/** The play gains chaos, so the Get amount is drawn as a gain. */
	positive: boolean;
}

/**
 * The five route slots of one play.
 *
 * The legs arrive in execution order — two for direct (buy X, sell X), three
 * for 1-hop (buy X in A, sell X in B, sell B in A) — so the slots are read off
 * by position, not by `action`: the third leg is a `sell` on the wire and a
 * *convert* on screen, because what it does for the reader is turn the
 * intermediate currency back into the one they started in.
 *
 * Which half of each leg the slot names follows what the reader receives at
 * that step: step 1 is the leg's item (the thing bought), step 2 is the leg's
 * QUOTE (what the sale pays in — chaos on a direct play, the intermediate on a
 * 1-hop), and step 3 the leg's item again (the intermediate being converted).
 *
 * The ends are the play's own chaos figures rather than a product of the leg
 * prices: `investment` is what one exchange costs at the undercut entry and
 * `investment + roi` what it returns, both already net of the ticks the legs
 * do not show (POE-188). Rebuilding them from `price` here would print the raw
 * best case beside an ROI that is not. Both wear the chaos artwork
 * unconditionally, because both are chaos figures whatever the legs are quoted
 * in — see `chaosIconPath`.
 *
 * `null` for a play with fewer than two legs — a shape the server does not
 * send. Nothing drops such a play: `ExchangeRoute` guards on this answer and
 * renders an empty route cell, so the row is still there with its rank, ROI and
 * depth, and only the route is missing.
 */
export function routeSlots(play: CurrencyExchangePlay): RouteView | null {
	const [buyLeg, sellLeg, convertLeg] = play.legs;
	if (buyLeg === undefined || sellLeg === undefined) return null;

	const chaos = chaosIconPath();
	return {
		spend: { amount: formatChaos(play.investment), icon: chaos },
		buy: {
			name: buyLeg.itemName,
			icon: buyLeg.itemIcon,
			rate: `buy @ ${formatLegPrice(buyLeg.price)}`,
			suspect: buyLeg.suspect
		},
		sell: {
			name: sellLeg.quoteName,
			icon: sellLeg.quoteIcon,
			rate: `sell @ ${formatLegPrice(sellLeg.price)}`,
			suspect: sellLeg.suspect
		},
		convert:
			convertLeg === undefined
				? null
				: {
						name: convertLeg.itemName,
						icon: convertLeg.itemIcon,
						rate: `convert @ ${formatLegPrice(convertLeg.price)}`,
						suspect: convertLeg.suspect
					},
		get: { amount: formatChaos(play.investment + play.roi), icon: chaos },
		positive: play.roi > 0
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
