import type { DriverView, MarketView, OfferView, RoomValueView, TempleSlice } from './slice';
import { formatCount } from './values';
import { forcedKillNote, marketNote, marketStale, topRecommendation } from './view';

/**
 * One offer box — everything the decision about ONE architect block needs
 * (POE-249).
 *
 * The owner's ask, verbatim: *"two offer boxes on the LEFT margin of the temple
 * sheet, stacked to mirror the side panel's own block order, each with
 * everything the decision needs: the room the kill builds and its tier,
 * Vertolka's rating line, the advisor's first reason. The advisor's pick gets a
 * cyan frame — that frame IS the pointer; no arrows anywhere."*
 *
 * POE-277 replaced the rating and reason lines with the tier ladder and the
 * temple-mod block, and led the header with the room. POE-367 cut that block
 * to the architect's name when 3.30 removed the temple-mod items.
 *
 * This replaces the single kill callout, which said one thing about ONE block
 * and left the other block — the one the player is choosing against — unnamed
 * on the overlay. Two boxes state both offers and mark which one the advisor
 * took, which is the comparison the player is actually making.
 */
export interface OfferBox {
	/** The block this box is about, so a surface can point at it. */
	offer: OfferView;
	/** The three-tier area bonus ladder for this line, or null when absent. */
	ladder: OfferLadder | null;
	/** The panel read's architect name for this block, or null when the read
	 *  is blank — the box's neutral architect line (POE-367). */
	architect: string | null;
	/** The resolved room, or the printed target when resolution failed. */
	headline: string;
	/** The kind and tier the resolved block builds, or the honest refusal when
	 *  the printed target did not resolve. */
	builds: string;
	/** The line's content prose for a content box, or null for a chest box. */
	content: string | null;
	/** Whether this is the block the top recommendation chose — the cyan frame,
	 *  which is the whole of the pointer. */
	pick: boolean;
	/** `"only architect read"` on the pick when the kill was forced rather than
	 *  chosen, else null.
	 *
	 *  Riding on the pick loses nothing: a forced kill with no named index is
	 *  UNREACHABLE, because `slice.rs` sets `forced_kill` only when
	 *  `recommendations.first()` carries an architect (its own comment says why
	 *  — `kill either (only architect read)` would point at an architect the
	 *  advice is not recommending). So a board with the note always has a pick
	 *  to carry it; this is not an accepted narrowing. */
	forced: string | null;
	/** What the prices behind this box's numbers are — `marketNote()`'s one
	 *  line (POE-258).
	 *
	 *  Retained on every box as the page's and the tests' answer, but no longer
	 *  DRAWN: since POE-277 the box repeats this line only when it warns
	 *  (`ageLine`), because a fresh live read printing its own age was a row of
	 *  noise on every board. */
	market: string;
	/** The market note only when it warns; fresh prices omit this row. */
	ageLine: string | null;

	// ------------------------------------------- what the number is (POE-260)

	/** The room's worth in chaos per run, or null when the offer resolved to
	 *  no room. On the fallback ladder it is the rung's chaos, marked `est.`
	 *  (POE-368, T10/T11). */
	value: number | null;
	/** What the value slot prints: `"96"`, in every state.
	 *  Null when the offer resolved to no room — there is nothing to price, so
	 *  the box draws no value row at all rather than a zero or a dash. */
	valueText: string | null;
	/** How complete the sum behind the number is, straight off
	 *  `RoomValueView.priced`. FIVE states and not a boolean: `instrumental` is
	 *  not a missing price (the room is worth what it DOES) and `override` is
	 *  the player's own number, so wording either one as "no market" would be a
	 *  false statement about a number that is not in doubt. Null on an offer
	 *  that resolved to no room.
	 *
	 *  The SOURCE of every state-dependent field on this object — `valueText`,
	 *  `chip`, `note`, `marks` and the rows' em dash are all worded off this
	 *  one answer ([`offerState`]) rather than off the wire string. The markup
	 *  does not branch on it: what a state changes is words, and the words are
	 *  here. It is in [`offerBoxSignature`] because a state change IS a shape
	 *  change — a fallback box keeps only its drop rows, never a sale. */
	state: OfferValueState | null;
	/** `"floor · 2 unpriced"` on a partial sum, else null. It replaces
	 *  `per run` in the value row, so the row's height does not move. */
	chip: string | null;
	/** The single box-level provenance marks: `F` for the fallback ladder and
	 *  `G` when any value term is guessed or the value is a fallback rung.
	 *
	 *  Kept as DATA and no longer drawn as letters. POE-277's v5 retired both
	 *  boxed glyphs from the overlay: `F` is said by the ` · est.` its `G`
	 *  brings, and `G` became the muted ` · est.` on the header's second
	 *  line — so the box reads this array for that one word and prints nothing
	 *  else from it. The page and the tests still read both letters. */
	marks: BoxMark[];
	/** The dropped items and the sale, in display order, at most
	 *  [`FULL_DRIVER_ROWS`]. */
	drivers: OfferDriver[];
	/** The item cells: every unique row among the drawn `drivers`, in wire
	 *  order. Empty where the line drops no unique. A chest room's pool has
	 *  several (POE-370: Glittering Halls' four), and the fold carries any
	 *  past [`FULL_DRIVER_ROWS`]. */
	uniqueDrops: OfferDriver[];
	/** How many rows there were before the cap — what the collapse rule reads
	 *  (`offersCompact` in `overlay-geometry.ts`). */
	driverCount: number;
	/** `"+1 more item · 34c"` for the rows past the cap, or null. */
	fold: string | null;
	/** The rarity bonus, or null on a line poedb prints no
	 *  percentage for (Toxic Grove, Storm of Corruption). */
	bonus: OfferBonus | null;
	/** The one line that says something the rows cannot — an instrumental
	 *  line's worth, the player's own number, or a line that drops nothing at
	 *  all. Null when the rows say it themselves. */
	note: string | null;
	/** The compact form's price strip — `"68 · 41 · 30c"` — or null with no
	 *  priced driver to put in it. */
	stripPrices: string | null;
	/** Whether the market read behind the number is past its staleness
	 *  threshold. All this FLAG draws is the value's dotted underline and the
	 *  age line's colour; the box's shape on a stale board is decided by
	 *  [`state`](Self.state), which a stale read puts in `fallback` — Rust
	 *  prices nothing off a stale snapshot, so the rows go and the box gets
	 *  shorter. See [`offerBoxSignature`]. */
	stale: boolean;
}

/** The three-tier bonus ladder and the text pieces the box prints. */
export interface OfferLadder {
	rarity: [number, number, number] | null;
	rarityText: [string, string, string] | null;
	tier: number;
}

/** How complete the sum behind a box's number is — `RoomValueView.priced`. */
export type OfferValueState = 'market' | 'partial' | 'fallback' | 'instrumental' | 'override';

/** A mark on the value itself. `F` — the grade ladder priced this room.
 *  `G` — the number rests on somebody's estimate. */
export type BoxMark = 'F' | 'G';

/** A mark on one driver row. `W` — priced from a trailing window rather than
 *  the newest print (POE-252). `L` — a thin market (POE-131). `G` — the count
 *  or the price behind it is somebody's estimate. */
export type DriverMark = 'W' | 'L' | 'G';

/** Which term of the value a driver row is — it decides the icon's frame. */
export type OfferDriverKind = 'sale' | 'unique';

/** One row under the value: what the room drops, and what it goes for. */
export interface OfferDriver {
	kind: OfferDriverKind;
	/** The row's label — the poe.ninja item name or the sale row's own
	 *  wording. */
	name: string;
	/** The poe.ninja name to fetch an icon for, or null where there is no item
	 *  (the sale row is a coin glyph). NOT a URL: `view.ts` is pure and
	 *  TempleOfferBoxes resolves it with the typed temple icon helper. */
	iconName: string | null;
	/** What the price cell prints: `"68c"`, `"+186c"`, `"no price"` or `"—"`.
	 *  Already worded, so the difference between "looked up and came back
	 *  empty" and "there is no market at all" is decided in one place. */
	price: string;
	/** False when [`price`](Self.price) is a refusal rather than a number. */
	priced: boolean;
	/** `"×0.25"`, or null where the term has no count (the sale). */
	perRun: string | null;
	/** `"4.8% / run"` or `"1 in ~850 runs"` for a rare unique drop
	 *  ([`formatDropRate`]), or null otherwise. */
	caption: string | null;
	/** `"(+107c)"`: what the upgraded unique would ADD with a Cradle of
	 *  Creation on the board (POE-370; g744679169.csv line 13, Operator ruling
	 *  2026-11-13 00:20, T28c), or null. Shown, never added to the value.
	 *  Worded in [`offerDriverTerms`]. */
	bracket: string | null;
	marks: DriverMark[];
}

/** The rarity line. */
export interface OfferBonus {
	/** `"+12% rarity"`. */
	label: string;
	/** `"+15c"`, or null where neither rate priced anything. */
	amount: string | null;
}

/**
 * How many driver rows the full form draws (POE-260 design §7).
 *
 * Three. The fourth and beyond fold into one muted line rather than growing
 * the box: the height is designed against the 316 px the panel's own diagonal
 * admits at 1920×1080. Any other row grows the box past plate C0.
 */
export const FULL_DRIVER_ROWS = 3;

// ------------------------------------------- the value, in the box (POE-260) --

/**
 * A chaos amount at overlay size.
 *
 * Not `values.ts`'s `formatChaos`, and the two are deliberately different: that
 * one prints a settings TABLE, where two decimals under 100 are what makes two
 * cells comparable, and this one prints one number the size of a headline over
 * a game. `96.00` at 24 px is three characters of noise.
 *
 * Three bands, and the lowest keeps its decimals rather than rounding: a room
 * worth 0.4 c must not print as `0`, which reads as "worth nothing" when the
 * whole point of the box is that it is worth something small.
 */
export function offerChaos(amount: number): string {
	if (!Number.isFinite(amount)) return '—';
	if (amount >= 10) return Math.round(amount).toString();
	if (amount >= 1) return (Math.round(amount * 10) / 10).toString();
	return amount.toFixed(2);
}

/**
 * The wire's `priced` string as this file's union, and the SINGLE source every
 * other wording below reads.
 *
 * A guard and not a cast: `RoomValueView.priced` is a plain `string` on the
 * wire, and an unrecognised one — a state Rust grew that this file has not been
 * taught — would otherwise reach the box's `{#if}` chain and fall through every
 * branch silently. `partial` is the fallback because it is the only one of the
 * five that claims nothing: the number may be incomplete, which is true of a
 * state nobody here understands.
 *
 * That guard only holds if nothing else in this file compares `value.priced`
 * itself. It used to: the chip, the note, the marks, the value slot and the
 * ladder's em dash each re-read the raw string, so an unrecognised state came
 * out as `partial` in [`OfferBox.state`] while every one of those branches took
 * the "none of the five" path — a box calling itself a partial sum with no
 * `floor · N unpriced` chip on it. So this function runs ONCE per box in
 * [`offerBoxes`] and its answer is passed down; the markup then reads the
 * words, and branches on `state` nowhere.
 */
function offerState(priced: string): OfferValueState {
	return priced === 'market' ||
		priced === 'partial' ||
		priced === 'fallback' ||
		priced === 'instrumental' ||
		priced === 'override'
		? priced
		: 'partial';
}

/** The wire driver kinds that become a ROW, in the order they are drawn. */
const ROW_KINDS: Record<string, OfferDriverKind> = {
	sale: 'sale',
	unique_drop: 'unique'
};

/** What the sale row calls itself — the room's own name is already the box's
 *  `builds` line, and repeating it there would read as a second item. */
const SALE_ROW_NAME = 'sale price above floor';

/** And what it calls itself on a SCALED box, where the cell holds the tier-3
 *  room's price rather than the delta ([`rowsAreTier3`]). The tier is named
 *  rather than left to the scale note: this is the row a player is most likely
 *  to read as the box's own number, because on an unscaled box it IS one. */
const SCALED_SALE_ROW_NAME = 'tier 3 sale price';

/**
 * The price cell of one row, and the difference the two absences carry.
 *
 * `—` means the market has no answer at all — the read is cold or stale and
 * every term on the board is in the same state. `no price` means this ITEM was
 * looked up and came back empty while other terms priced. Never `0c` for
 * either: zero is a real price and neither of these is one.
 */
function driverPrice(amount: number | null, ladder: boolean, gain: boolean): string {
	if (amount === null) return ladder ? '—' : 'no price';
	return `${gain ? '+' : ''}${offerChaos(amount)}c`;
}

/** Below this per-run rate a caption reads as odds rather than a percentage. */
const ODDS_BELOW_RATE = 0.01;

/**
 * A rare drop's caption: `"4.8% / run"`, or `"1 in ~850 runs"` under 1%.
 *
 * Under one percent a percentage needs a decimal place that rounds the two
 * smallest rates in the table (0.1 x 20/1689, about 0.12%) to `0.1%`, and a
 * player reads odds faster than a fraction of a percent. N is rounded to two
 * significant figures, hence the `~`: the rate itself is an estimate. Zero is
 * not odds — `1 in ~Infinity` is not a sentence — so it keeps `0.0% / run`.
 */
export function formatDropRate(rate: number): string {
	if (rate > 0 && rate < ODDS_BELOW_RATE) {
		return `1 in ~${Number((1 / rate).toPrecision(2))} runs`;
	}
	return `${(rate * 100).toFixed(1)}% / run`;
}

/** The marks on one row: how the price was measured first, then whether the
 *  number rests on somebody's estimate. Muted before yellow, so a row's colour
 *  reads left to right from "measured but thin" to "nobody measured it". */
function driverMarks(driver: DriverView): DriverMark[] {
	const marks: DriverMark[] = [];
	if (driver.windowPriced) marks.push('W');
	if (driver.lowConfidence) marks.push('L');
	if (driver.guessed) marks.push('G');
	return marks;
}

/** One driver row, with the two numbers behind it kept apart.
 *
 *  `shown` is what the price cell prints and `contributed` is what the term
 *  added to the total, and they are different on every drop row: a unique goes
 *  for 68 c and adds a quarter of that. Confusing them is how a box ends up
 *  printing a price nobody trades at. */
interface DriverTerm {
	row: OfferDriver;
	shown: number | null;
	contributed: number | null;
}

/**
 * The rows under the value: the sale delta, then what the room drops.
 *
 * Wire order is kept — `valuation.rs` pushes sale, then unique, in that
 * order — so the sale leads and the drops follow it, which is the order the
 * design draws.
 *
 * The one term that is DROPPED is a sale worth exactly zero. Eighty-four of the
 * capture's eighty-six room-tiers sit at the floor, so `+0c` would be on nearly
 * every box, and a row saying the room is not worth selling would push a row
 * saying what it drops into the fold. An UNPRICED sale is kept: that the room's
 * own line could not be priced is a fact about this read.
 *
 * A grade-fallback box drops a QUOTED sale for the same reason. Rust takes the
 * ladder only where nothing summed, so a quoted sale there was worth zero, but
 * the fallback nulls its `chaos` and keeps its `unitPrice` — and a scaled box
 * prints that unit price, which put the tier-3 room's at-floor poe.ninja price
 * on a box the valuation never counted it on.
 */
function offerDriverTerms(
	value: RoomValueView,
	state: OfferValueState,
	cradle: boolean
): DriverTerm[] {
	const ladder = state === 'fallback';
	const scaled = rowsAreTier3(value);
	return value.drivers
		.filter((driver) => driver.kind in ROW_KINDS)
		.filter((driver) => !(driver.kind === 'sale' && saleAddedNothing(driver, ladder)))
		.map((driver) => {
			const kind = ROW_KINDS[driver.kind];
			const sale = kind === 'sale';
			// The sale row prints what it ADDED (the delta above the floor); a
			// drop row prints what ONE of the item goes for, with its expected
			// count beside it. Two different numbers, because a drop's own
			// contribution — 0.25 of a unique — is not a price anybody trades
			// at, and a player checking the box against poe.ninja is looking up
			// the item. The count goes through `formatCount` because a derived
			// rate's raw float renders as `×0.16666666666666666`.
			//
			// On a SCALED box the delta is the one thing the sale row may not
			// print — see [`rowsAreTier3`] — so it falls back to the same
			// number every other row shows: the tier-3 room's own price.
			const saleDelta = sale && !scaled;
			const shown = saleDelta ? driver.chaos : driver.unitPrice;
			return {
				row: {
					kind,
					name: sale ? (scaled ? SCALED_SALE_ROW_NAME : SALE_ROW_NAME) : driver.name,
					iconName: sale ? null : driver.name,
					price: driverPrice(shown, ladder, saleDelta),
					priced: shown !== null,
					perRun: driver.count === null || sale ? null : `×${formatCount(driver.count)}`,
					caption:
						kind === 'unique' &&
						driver.count !== null &&
						driver.count < 0.05
							? formatDropRate(driver.count)
							: null,
					bracket: kind === 'unique' ? cradleBracket(driver, cradle, scaled, ladder) : null,
					marks: driverMarks(driver)
				},
				shown,
				contributed: driver.chaos
			};
		});
}

/**
 * A unique row's Cradle bracket, `(+107c)`, or null.
 *
 * Upgraded uniques drop from the same chest only when a Cradle of Creation is
 * present (g744679169.csv line 13), and the bracket is 0.25 x the upgraded
 * price, pooled like the base (Operator ruling 2026-11-13 00:20, T28c) — the
 * driver's `upgraded.chaos`, worded with the `+` of what it would add. It is
 * withheld on a scaled box (a derived tier-3 amount, [`rowsAreTier3`]) and on
 * the grade fallback, and it never moves the value.
 */
function cradleBracket(
	driver: DriverView,
	cradle: boolean,
	scaled: boolean,
	ladder: boolean
): string | null {
	const chaos = driver.upgraded?.chaos ?? null;
	if (!cradle || scaled || ladder || chaos === null || !(chaos > 0)) return null;
	return `(+${offerChaos(chaos)}c)`;
}

/** Whether a sale term is a measured zero above the floor: `0` on a summed
 *  box, or any quote at all on the ladder ([`offerDriverTerms`]). */
function saleAddedNothing(driver: DriverView, ladder: boolean): boolean {
	return driver.chaos === 0 || (ladder && driver.unitPrice !== null);
}

/** The full form's item cells: every unique row, in wire order. */
export function offerUniqueDrops(drivers: OfferDriver[]): OfferDriver[] {
	return drivers.filter((driver) => driver.kind === 'unique');
}

/**
 * Whether this box's rows are the TIER-3 room's terms, copied unscaled — and
 * the one rule that follows from it (epic lock L2).
 *
 * **On a scaled box no tier-3 CHAOS AMOUNT is printed, with exactly one
 * exception: a per-item unit price.** The headline is a fraction of the line
 * and the rows under it are the whole line's terms, so every amount DERIVED
 * from those terms — the fold's sum, the bonus line's chaos — is a tier-3
 * number that would sit under a tier-1 total and read as part of it. A unit
 * price is not derived: 68 c is what one Story of the Vaal goes for at every
 * tier, it is the number a player checks against poe.ninja, and withholding it
 * would leave a row with an icon and nothing to look up.
 *
 * **The sale row is the one that changes shape under this rule**, because it is
 * the one whose ordinary cell IS a derived amount. Unscaled it prints the delta
 * above the floor — what the room ADDED — and on a tier-1 or tier-2 box that
 * delta is the tier-3 room's, which is how a box ended up printing `+13c` under
 * a total of 10: the committed sample's Crucible row (`sale.chaos` 12.5 against
 * a scaled total of 10) read as the larger part of a smaller number. So a
 * scaled box's sale row shows the tier-3 room's own PRICE instead
 * (`sale.unitPrice`), with no `+` and named [`SCALED_SALE_ROW_NAME`] — the same
 * treatment every drop row already gets, for the same reason. `offerStripPrices`
 * needs no rule of its own: the compact strip reads `DriverTerm.shown`, which
 * is where this is decided.
 *
 * What still shows is everything that is true at every tier: how many items
 * folded, and what the two bonus percentages are. The box's ladder facts carry
 * the explanation of the tiered percentages; no scale-note sentence is needed.
 */
function rowsAreTier3(value: RoomValueView): boolean {
	return value.scaledFromTier3 !== null;
}

/**
 * The fold line for the rows past [`FULL_DRIVER_ROWS`], or null.
 *
 * The chaos it names is what those rows CONTRIBUTED, and it is withheld on a
 * scaled row per [`rowsAreTier3`]. The count still shows — how many items are
 * off screen is true at every tier.
 */
function offerFold(value: RoomValueView, terms: DriverTerm[]): string | null {
	const hidden = terms.slice(FULL_DRIVER_ROWS);
	if (hidden.length === 0) return null;
	const word = hidden.length === 1 ? 'item' : 'items';
	const head = `+${hidden.length} more ${word}`;
	if (rowsAreTier3(value)) return head;
	const chaos = hidden.reduce((sum, term) => sum + (term.contributed ?? 0), 0);
	return chaos > 0 ? `${head} · ${offerChaos(chaos)}c` : head;
}

/** The game's own wording for the two area bonuses, shortened to fit one line. */
const BONUS_WORDS: Record<string, string> = {
	rarity_bonus: 'rarity'
};

/**
 * The rarity line, or null on a line poedb prints no percentage for.
 *
 * The percentages are the room's at every tier and always show. The chaos is
 * withheld on a scaled box for the same reason the fold's is
 * ([`rowsAreTier3`]): the bonus driver carries the TIER-3 room's rates against
 * the tier-3 room's value, so `+6c` under a tier-1 headline is an amount from
 * the wrong tier — and one the box shows the fold refusing two lines above it.
 */
function offerBonusLine(value: RoomValueView): OfferBonus | null {
	const terms = value.drivers.filter((driver) => driver.kind in BONUS_WORDS);
	if (terms.length === 0) return null;
	const label = terms
		.map((term) => `+${term.count ?? 0}% ${BONUS_WORDS[term.kind]}`)
		.join(' · ');
	const chaos = terms.reduce((sum, term) => sum + (term.chaos ?? 0), 0);
	// Null rather than `+0c` where neither rate priced anything: the fallback
	// path nulls every term's chaos, and a bonus reading zero there would claim
	// the percentages are worth nothing rather than that nothing priced them.
	const priced = terms.some((term) => term.chaos !== null) && chaos > 0;
	const amount = priced && !rowsAreTier3(value) ? `+${offerChaos(chaos)}c` : null;
	return { label, amount };
}

/**
 * The one line the rows cannot say, or null.
 *
 * On a content line the content cell says what the room drops nothing for, so
 * the empty-drop wording yields to it; the instrumental and own-number wordings
 * are about the VALUE — where the number came from — and still print under the
 * content row (POE-277 WI-6). Three different facts share the slot because no
 * box can carry two of them: an estimated room has no priced sale, a
 * room the player priced has one row that is their own number, and a room with
 * no drops has an empty list that would otherwise read as a missing read.
 */
function offerNote(state: OfferValueState, rows: OfferDriver[], content: string | null): string | null {
	// Override wording stays authoritative even when a content line carries the
	// player's own value.
	if (state === 'override') return 'your own number for this room';
	// One wording for BOTH instrumental lines. Temple Nexus is worth the tiers
	// it lifts and Shrine of Unmaking the rooms it clears, and telling them
	// apart here would mean hard-coding two room names in a file that words
	// what the slice says rather than knowing which line is which. Both are
	// content lines, and the wording stays: the content cell says what the
	// room does, this line says why the number is a rung and not a sum.
	if (state === 'instrumental') {
		return 'estimated — its worth is what it does, not what it drops';
	}
	if (content !== null) return null;
	if (rows.length === 0 && state !== 'fallback') {
		return 'this line drops no unique';
	}
	return null;
}

/** What the value slot prints, and whether there is a chaos number behind it. */
function offerValueText(value: RoomValueView): string {
	// Interim (POE-368, T10/T11): a fallback box prints the rung's chaos, marked `est.`.
	return offerChaos(value.total);
}

/**
 * `"floor · 2 unpriced"`, or null where the sum is whole.
 *
 * `rows` is EVERY term, not the three the full form draws. What the chip says
 * is how complete the NUMBER above it is, and the number is the whole sum: a
 * partial box whose only unpriced item happens to be the fourth would
 * otherwise count zero and print a bare `floor`, which reads as "one term has
 * no count" on a box that is actually missing a price.
 */
function offerChip(state: OfferValueState, rows: OfferDriver[]): string | null {
	if (state !== 'partial') return null;
	const unpriced = rows.filter((row) => !row.priced).length;
	// `floor` alone still says the thing that matters — the number is a lower
	// bound — on the rare partial whose missing term is a COUNT rather than a
	// price, where every row on screen shows one.
	return unpriced === 0 ? 'floor' : `floor · ${unpriced} unpriced`;
}

/** The marks beside the value itself. `rows` is every term, for the same
 *  reason [`offerChip`]'s is: a folded term still makes the whole number
 *  guessed. Per-row marks remain in the view data but are not rendered. */
function offerBoxMarks(
	value: RoomValueView,
	state: OfferValueState,
	rows: OfferDriver[]
): BoxMark[] {
	const marks: BoxMark[] = [];
	if (state === 'fallback') marks.push('F');
	// A fallback slot prints a rung number, not a priced sum, so it wears
	// ` · est.` like a guessed one (POE-368, T10/T11).
	if (state === 'fallback' || value.guessed || rows.some((row) => row.marks.includes('G'))) {
		marks.push('G');
	}
	return marks;
}

/**
 * The compact strip's prices — `"68 · 41 · 30c"`, one unit at the end.
 *
 * Only the terms that HAVE a price: the compact form has no room to say why a
 * number is missing, and a bare `no price` in a run of numerals reads as one of
 * them. Null when nothing priced, which is the whole strip on a fallback board.
 */
function offerStripPrices(terms: DriverTerm[]): string | null {
	const priced = terms.filter((term) => term.shown !== null);
	if (priced.length === 0) return null;
	const amounts = priced.map((term) => offerChaos(term.shown as number));
	return `${amounts.join(' · ')}c`;
}

/**
 * What a box's measurement was taken FOR — everything that decides its HEIGHT,
 * and nothing else (POE-260).
 *
 * **Shape, never content.** Every part below is a state name, a count or a
 * presence flag; not one of them is a rendered string. That is the correction
 * of a shipped defect: POE-258 put the market-age LINE into the overlay's
 * measurement signature, because the box was content-sized against a
 * `max-width` and its widest line decided its width — so `prices 12 min old`
 * becoming `prices 13 min old` re-measured the pair and hid it for a frame,
 * once a minute, for as long as a board was up. POE-260's box has a fixed
 * width and fixed-height rows, so its geometry is a function of WHICH ROWS
 * EXIST and how many driver rows there are, and no line's text can move it.
 *
 * `stale` is deliberately absent, and NOT because the stale form is the priced
 * form's height. The design spec said that ("stale = priced, by design") and it
 * is wrong about the code: a stale read prices nothing (`MarketInput::is_live`
 * is false, so `prices_anything` is false), every line takes `cold_fallback`'s
 * single `GradeFallback` driver, and the box comes back in the FALLBACK form —
 * the rung's chaos marked `est.`, no rows. It gets SHORTER, not
 * equal. What makes `stale` redundant in this string is that `state` is already
 * in it: the transition a snapshot ageing causes is `market` → `fallback`, and
 * that re-measures on the field that names the form. The flag itself buys no
 * height — it is a dotted underline and a yellow age line on a box whose shape
 * `state` has already described — so putting it here would only add a second
 * spelling of a change already covered.
 *
 * `pick` LEFT this string with POE-277's v5 restyle: the frame is 2 px on both
 * boxes now — faint is a muted frame and a muted text colour, never an
 * `opacity` — so the pick buys no height, and a field that decides nothing
 * about the geometry only re-measures the pair for a change the eye already
 * has. `compact` is present because it is the whole form.
 *
 * `ladder.rarity` is present as a PRESENCE and not as a value, and what it
 * buys is the bonus cell's two-line shape: a cell is a value over a word. Its
 * numbers are not here and must not be — `+4/8/12%` becoming `+12/24/36%` is
 * text, and text moves nothing.
 *
 * `content` is present when the content cell draws, and the `uniqueDrops`
 * count and whether any carries a caption are present because the item row's
 * cell count and the rare-caption threshold are row-shape changes.
 *
 * Here rather than in the component because a `.svelte` file has no unit-test
 * harness in this app: the one thing that must be provable about this string is
 * that changing a rendered word does not change it.
 */
export function offerBoxSignature(box: OfferBox, compact: boolean): string {
	return [
		box.offer.index,
		compact ? 'compact' : 'full',
		box.state ?? 'none',
		box.drivers.length,
		box.fold ? 'fold' : '-',
		box.bonus ? 'bonus' : '-',
		box.note ? 'note' : '-',
		box.content !== null ? 'content' : '-',
		box.ladder ? 'ladder' : '-',
		box.ladder?.rarity ? 'r' : '-',
		`unique${box.uniqueDrops.length}`,
		// A bracket draws on the caption's line, so its presence is the same
		// shape change; its text is not.
		box.uniqueDrops.some((drop) => drop.caption || drop.bracket) ? 'caption' : '-',
		box.architect !== null ? 'architect' : '-',
		box.ageLine ? 'age' : '-'
	].join(' ');
}

function formatPercentage(value: number): string {
	return Number.isInteger(value) ? String(value) : value.toFixed(1);
}

function offerLadder(offer: OfferView): OfferBox['ladder'] {
	const line = offer.line ?? null;
	if (
		line === null ||
		offer.builtTier === null ||
		line.rarityPct === null
	) {
		return null;
	}
	const rarityText = [
		formatPercentage(line.rarityPct[0]),
		formatPercentage(line.rarityPct[1]),
		formatPercentage(line.rarityPct[2])
	] as [string, string, string];
	return {
		rarity: line.rarityPct,
		rarityText,
		tier: offer.builtTier
	};
}

/**
 * One box per architect block on the panel, in the panel's own order.
 *
 * **Panel order, not "upgrade first".** `PanelView.offers` is reading order
 * top-to-bottom, and a real board can print two `change` offers
 * (`panel.rs`'s own fixture does), so box `i` mirrors `offers[i]` and each box
 * says its own kind in its builds line. The stack's geometry follows the same
 * rule: `offerStackPlacement` puts box `i` level with block `i`.
 *
 * Empty when there is no advice or no panel. The boxes carry the advisor's PICK
 * and forced-kill state, so a panel read with no ranking behind it yet — the gap
 * between a sighting and a completed read in a new cycle — has nothing to draw
 * and draws nothing, rather than showing two unmarked boxes the player could
 * read as "the advisor has no preference".
 */
export function offerBoxes(slice: TempleSlice, now: number = Date.now()): OfferBox[] {
	const advice = slice.advice;
	const panel = slice.panel;
	if (advice === null || panel === null) return [];
	const chosenIndex = topRecommendation(advice)?.architectIndex ?? null;
	// One line for the whole read, not one per box: both boxes are priced off
	// the same market by construction (Rust values a read once), so a per-box
	// answer would be the same string twice with room to drift.
	const market = marketNote(slice.market, now);
	const ageLine = offerAgeLine(slice.market, now, market);
	return panel.offers.map((offer) => {
		const pick = chosenIndex !== null && chosenIndex === offer.index;
		// POE-260. Every number below comes from THIS object — the same
		// `Valued` row the advisor ranked the board on (POE-257 D6) — and
		// nothing here recomputes one. A box with no value at all is an offer
		// whose printed target did not resolve: there is no room, so there is
		// nothing to price and nothing to explain.
		const value = offer.value ?? null;
		const content = offer.line?.kind === 'content' ? offer.line.content : null;
		// The five-state classification, made ONCE — see [`offerState`] for why
		// nothing below may re-read `value.priced` instead.
		const state = value === null ? null : offerState(value.priced);
		const terms = value === null || state === null ? [] : offerDriverTerms(value, state, slice.layout?.cradle === true);
		// EVERY term, and the three the full form draws. The chip and box marks
		// read the first: they describe the number, which is the whole sum, and a
		// fact hidden by the fold is still a fact about it.
		const allRows = terms.map((term) => term.row);
		const drivers = allRows.slice(0, FULL_DRIVER_ROWS);
		return {
			offer,
			ladder: offerLadder(offer),
			architect: offer.architectName.trim() === '' ? null : offer.architectName,
			headline: offerBoxHeadline(offer),
			builds: offerBoxBuilds(offer),
			content,
			pick,
			// Only on the pick: the note says the kill on the frame was the only
			// one there was, and a note on the block the advisor did NOT choose
			// would be saying that about the wrong box.
			forced: pick ? forcedKillNote(advice) : null,
			market,
			ageLine,
			// Only an unresolved offer carries no number; a fallback box
			// carries the rung's chaos.
			value: value === null ? null : value.total,
			valueText: value === null || state === null ? null : offerValueText(value),

			state,
			chip: state === null ? null : offerChip(state, allRows),
			marks: value === null || state === null ? [] : offerBoxMarks(value, state, allRows),
			drivers,
			uniqueDrops: offerUniqueDrops(drivers),
			driverCount: terms.length,
			fold: value === null ? null : offerFold(value, terms),
			bonus: value === null ? null : offerBonusLine(value),
			note: state === null ? null : offerNote(state, allRows, content),
			stripPrices: offerStripPrices(terms),
			// Off the MARKET view and not off the value's own `asOf`, which is
			// null on a stale read by construction: the whole point of the flag
			// is that there IS a read and it is too old, so the field that
			// withholds its age cannot be the one that reports it (POE-258).
			// Through `marketStale` and not off the raw flag, so the mark and
			// the line above it can never say different things about the same
			// clock.
			stale: marketStale(slice.market, now)
		};
	});
}

/** The offer box's header: the resolved room, or the printed target on failure. */
export function offerBoxHeadline(offer: OfferView): string {
	return offer.displayName ?? offer.printedTarget;
}

/** The offer box's second line: the kind and tier the resolved block builds. */
export function offerBoxBuilds(offer: OfferView): string {
	if (offer.displayName === null) return 'does not resolve to a known room';
	return offer.builtTier === null ? offer.kind : `${offer.kind} · tier ${offer.builtTier}`;
}

/** The full and compact forms repeat the market note only when it warns. */
function offerAgeLine(market: MarketView, now: number, note: string): string | null {
	const warning =
		marketStale(market, now) ||
		note.startsWith('prices unavailable');
	return warning ? note : null;
}
