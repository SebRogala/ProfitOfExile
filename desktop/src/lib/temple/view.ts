/**
 * Presentation derivations for the temple builder surfaces (POE-171).
 *
 * The sibling of `mercenaries/ladder-view.ts` and for the same reason: a
 * `.svelte` file has no unit-test harness in this app, so everything the page
 * and the overlay would otherwise compute inline lives here, as pure functions
 * over the wire types in `./slice`.
 *
 * It decides nothing. Every ranking, every reason, every warning and the
 * leave-the-map verdict are the Rust advisor's (POE-170); this file words them,
 * places the 13 plates and picks the class one corridor is drawn in.
 *
 * # What it words for the overlay, since POE-249
 *
 * `offerBoxes()` is the overlay's advice surface: ONE box per architect block
 * on the panel, in the panel's own order, each carrying what the kill builds,
 * Vertolka's rating for the line it builds into, and the advisor's first reason
 * for that block. `killCallout()`, `KillCallout` and `killTitle()` are GONE
 * with it — they said one thing about one block and left the block the player
 * is choosing against unnamed. What survives from them is used by the boxes and
 * by the room widget: `chosenOffer`, `leadReason`, `forcedKillNote`,
 * `offerHeadline` and `offerBuilds`, none of them reworded.
 *
 * # The geometry is a transcription, not a design
 *
 * The slot offsets below are `src-tauri/src/temple/lattice.rs`'s measured
 * table, in the same reference pixels: column pitch 212, row pitch 105, and the
 * Entrance sitting 19 px below its two row-E siblings. The corridor set is
 * DERIVED from those offsets by the same rule `Edge::kind` uses rather than
 * typed in, so a wrong offset here shows up as a wrong corridor set — exactly
 * as it does on the Rust side, whose own test pins the 26 names.
 */
import type {
	AdviceView,
	DriverView,
	EdgeId,
	ExitLabelView,
	ItemSlotId,
	LayoutView,
	MarketView,
	OfferView,
	RankedView,
	RecipeItemView,
	RecipeView,
	RoomValueView,
	SlotId,
	SlotView,
	TempleSlice,
	TempleStatus
} from './slice';
import { formatCount } from './values';

// ------------------------------------------------------------ the status --

/** The tone vocabulary the pages already use for badges. */
export type TempleTone = 'muted' | 'pass' | 'warn' | 'fail';

/**
 * The module's state in words, one entry per wire string.
 *
 * Total over `TempleStatus` on purpose: a `Record` of the union will not
 * compile with a variant missing, which is what keeps a status added in Rust
 * from silently rendering as an empty badge.
 */
export const TEMPLE_STATUS_LABEL: Record<TempleStatus, string> = {
	off: 'module off',
	idle: 'watching for the layout panel',
	waiting: 'on, waiting for Alva',
	panel_not_visible: 'no layout panel on screen',
	reading: 'reading the board',
	read: 'board read',
	no_current_room: 'between rooms — layout only',
	unavailable: 'capture unavailable here',
	error: 'last read failed'
};

/** The badge colour for each status. Same totality rule as the labels. */
export const TEMPLE_STATUS_TONE: Record<TempleStatus, TempleTone> = {
	off: 'muted',
	idle: 'muted',
	waiting: 'muted',
	panel_not_visible: 'muted',
	reading: 'warn',
	read: 'pass',
	no_current_room: 'warn',
	unavailable: 'fail',
	error: 'fail'
};

/**
 * The statuses with a board worth drawing.
 *
 * The overlay's whole visibility rule, in one place so the window and any
 * future surface cannot disagree: `off`, `idle`, `waiting`, `panel_not_visible`
 * and `unavailable` mean there is nothing on the temple's layout panel to
 * advise about, and `error` means the last attempt produced nothing to draw —
 * the message belongs on the page, not floating over the game.
 */
export const OVERLAY_VISIBLE_STATUSES: readonly TempleStatus[] = [
	'reading',
	'read',
	'no_current_room'
];

/** Whether the overlay has anything to show for this status. */
export function overlayShowsBoard(status: TempleStatus): boolean {
	return OVERLAY_VISIBLE_STATUSES.includes(status);
}

/**
 * Whether the "waiting for the temple panel" notice belongs on screen
 * (POE-249).
 *
 * Rust owns the first half: `waitingForPanel` goes up on one of Alva's three
 * measured START phrases and comes down on any other Alva line, a zone change,
 * a completed read, a sighting, a stand-down and the module being switched off.
 *
 * The second clause is this side's own, and it is not redundant. Alva's start
 * line fires when the PORTAL OPENS, and the module may already have the sheet
 * on screen and read when it lands — the flag and a board are not mutually
 * exclusive states. Without the clause the notice would blink over a board the
 * player is reading, which is the same failure shape POE-246 fixed one layer
 * down.
 */
export function overlayShowsWaiting(slice: TempleSlice): boolean {
	return slice.waitingForPanel && !overlayShowsBoard(slice.status);
}

/**
 * Whether the DOOR widget has something to draw (POE-244, rewritten in
 * POE-248).
 *
 * **Not a status rule.** It was one — `OVERLAY_VISIBLE_STATUSES` plus
 * `panel_not_visible` — and the first live session showed what that costs:
 * `12:32:10 capture armed by the panel on screen` … `12:39:05 capture stood
 * down`, and the diamond went off screen with the status while the player was
 * still standing in the room it described. Owner: the panel-side advice — the
 * kill callout then, the offer boxes since POE-249 — lives with the PANEL, and
 * the room widget lives with the INCURSION.
 *
 * A status is a statement about whether anything is LOOKING at the screen, and
 * that is the wrong question here: the layout panel is shut for the whole
 * incursion, which is exactly when this widget is the only surface left. So the
 * gate is the ADVICE itself, which the module clears when the incursion
 * genuinely ends — a zone change, the next Alva voice line after the read, a
 * read that replaces it, or the module being switched off
 * (`temple::trigger::advice_end`, `slice::clear_advice`, `slice::force_off`).
 *
 * The diamond is tested with it because there is nothing to draw a door on
 * without one, and a read between rooms (`no_current_room`) publishes advice-
 * less layout in the other direction.
 */
export function overlayShowsDoors(slice: TempleSlice): boolean {
	return slice.advice !== null && (slice.layout?.diamond ?? null) !== null;
}

// ---------------------------------------------------------- the geometry --

/** Horizontal distance between two slots in the same row, reference px. */
export const COL_PITCH = 212;
/** Vertical distance between two rows, reference px. */
export const ROW_PITCH = 105;
/** Plate width including its border, reference px. */
export const PLATE_W = 173;
/** Plate height including its border, reference px. */
export const PLATE_H = 84;
/** The Entrance plate sits this much lower than the other two row-E plates. */
export const ENTRANCE_DROP = 19;

/** The 13 slots, in `Slot::ALL` order — which is also alphabetical, which is
 *  also the order `Edge`'s endpoints are written in. */
export const SLOT_IDS: readonly SlotId[] = [
	'A0',
	'B0',
	'B1',
	'C0',
	'C1',
	'C2',
	'D0',
	'D1',
	'D2',
	'D3',
	'E0',
	'E1',
	'E2'
];

/** Rows A..E as 0..4 — the gradient POE-170's rule R1 ranks by. */
const SLOT_ROW: Record<SlotId, number> = {
	A0: 0,
	B0: 1,
	B1: 1,
	C0: 2,
	C1: 2,
	C2: 2,
	D0: 3,
	D1: 3,
	D2: 3,
	D3: 3,
	E0: 4,
	E1: 4,
	E2: 4
};

/** x offset from the Entrance centre, reference px, per `lattice.rs`. */
const SLOT_X: Record<SlotId, number> = {
	A0: 0,
	B0: -106,
	B1: 106,
	C0: -212,
	C1: 0,
	C2: 212,
	D0: -318,
	D1: -106,
	D2: 106,
	D3: 318,
	E0: -212,
	E1: 0,
	E2: 212
};

/**
 * y offset for a row, reference px, `+y` down.
 *
 * The row-E LINE is `ENTRANCE_DROP` above the Entrance centre, and each row up
 * is one more `ROW_PITCH` — which is exactly how the Entrance ends up 19 px
 * below E0 and E2 without either of them being a special case.
 */
function rowY(row: number): number {
	return -(ENTRANCE_DROP + (4 - row) * ROW_PITCH);
}

/** One plate's centre, in reference px with the Entrance at the origin. */
export interface SlotPoint {
	slot: SlotId;
	x: number;
	y: number;
	row: number;
}

/** The 13 plate centres, in `SLOT_IDS` order. */
export function latticePoints(): SlotPoint[] {
	return SLOT_IDS.map((slot) => ({
		slot,
		x: SLOT_X[slot],
		// The Entrance is the origin; everything else hangs off the row line.
		y: slot === 'E1' ? 0 : rowY(SLOT_ROW[slot]),
		row: SLOT_ROW[slot]
	}));
}

/** Which of the two corridor geometries an edge is drawn as. */
export type Corridor = 'horizontal' | 'diagonal';

/** One corridor, with the two endpoints it joins already placed. */
export interface LatticeEdge {
	/** `"C1-C2"` — the same label Rust's `Edge` prints, endpoints in slot order. */
	id: EdgeId;
	a: SlotId;
	b: SlotId;
	kind: Corridor;
	x1: number;
	y1: number;
	x2: number;
	y2: number;
}

/** The corridor label for an unordered pair — endpoints in `SLOT_IDS` order. */
export function edgeId(a: SlotId, b: SlotId): EdgeId {
	return SLOT_IDS.indexOf(a) < SLOT_IDS.indexOf(b) ? `${a}-${b}` : `${b}-${a}`;
}

/**
 * The 26 geometrically possible corridors, derived from the offsets.
 *
 * The rule is `Edge::kind`'s, transcribed: same row and one column pitch apart
 * is horizontal (the tolerance is what lets `E0-E1` and `E1-E2` survive the
 * Entrance drop), half a column pitch and one row pitch apart is diagonal,
 * anything else is not a corridor at all.
 */
export function latticeEdges(): LatticeEdge[] {
	const points = latticePoints();
	const out: LatticeEdge[] = [];
	for (let i = 0; i < points.length; i++) {
		for (let j = i + 1; j < points.length; j++) {
			const a = points[i];
			const b = points[j];
			const dx = Math.abs(a.x - b.x);
			const dy = Math.abs(a.y - b.y);
			let kind: Corridor | null = null;
			if (dx === COL_PITCH && dy <= ENTRANCE_DROP) kind = 'horizontal';
			else if (dx === COL_PITCH / 2 && Math.abs(dy - ROW_PITCH) <= ENTRANCE_DROP)
				kind = 'diagonal';
			if (kind === null) continue;
			out.push({ id: edgeId(a.slot, b.slot), a: a.slot, b: b.slot, kind, x1: a.x, y1: a.y, x2: b.x, y2: b.y });
		}
	}
	return out;
}

/** An SVG `viewBox`, as its four numbers. */
export interface ViewBox {
	minX: number;
	minY: number;
	width: number;
	height: number;
}

/**
 * The box that contains all 13 plates, with a plate half-size of margin.
 *
 * This IS the scaling: the drawing stays in reference pixels — the same numbers
 * `lattice.rs` measures in — and the `viewBox` maps them onto whatever the
 * element is sized to. Nothing in the markup multiplies a coordinate, so there
 * is no second scale factor to keep in step with the reader's.
 */
export function latticeViewBox(margin = 12): ViewBox {
	const points = latticePoints();
	const halfW = PLATE_W / 2 + margin;
	const halfH = PLATE_H / 2 + margin;
	const xs = points.map((p) => p.x);
	const ys = points.map((p) => p.y);
	const minX = Math.min(...xs) - halfW;
	const minY = Math.min(...ys) - halfH;
	return {
		minX,
		minY,
		width: Math.max(...xs) + halfW - minX,
		height: Math.max(...ys) + halfH - minY
	};
}

// -------------------------------------------------------------- the doors --

/**
 * How one corridor is drawn.
 *
 * - `open` — a settled door. Solid.
 * - `unresolved` — incident to the current room and settled by NOTHING, which
 *   only happens on the diamond-read fallback. Marked, because "we could not
 *   see it" must not render the same as "it is shut".
 * - `closed` — everything else.
 */
export type EdgeState = 'open' | 'unresolved' | 'closed';

/**
 * Which of the three states a corridor is in, for one published layout.
 *
 * `unresolved` wins outright: it is the honesty guard, and every corridor in it
 * is one `doors` has no verdict on anyway.
 *
 * # The fourth state, and why it is gone (POE-248)
 *
 * There used to be an `uncertain` between the two: `doors.includes(id) &&
 * uncertain.includes(id)`. That read `layout.uncertain` as a VERDICT, and it is
 * not one — `doors.rs` puts EVERY corridor incident to the current room in it
 * before any open/closed judgement, because the gold selection frame covers
 * them and the beam sampler cannot settle them. It is the beam's self-doubt,
 * published on every read.
 *
 * On the settled path the door MARKERS answer those corridors — that is what
 * the diamond read is for — and `layout.doors` is the settled set. So an edge
 * the seals read GREEN was still in `uncertain`, and the old branch coloured it
 * grey: owner, 2026-09-04, on a read whose log shows 6/6 markers and no error,
 * *"the widget drew C1-C2 grey where the game's seal was green"*. On the
 * fallback path `doors` is `doors − uncertain`, so the branch could not fire at
 * all. It was reachable only when it was wrong.
 *
 * `LayoutView.uncertain` stays on the wire as what it is — a diagnostic about
 * the read — and no surface may read it as a door state again.
 */
export function edgeState(id: EdgeId, layout: LayoutView | null): EdgeState {
	if (layout === null) return 'closed';
	if (layout.unresolvedIncident.includes(id)) return 'unresolved';
	return layout.doors.includes(id) ? 'open' : 'closed';
}

/**
 * The one character a plate carries when there is no room for its name.
 *
 * The compact board (the overlay) drops names and tier lines, and dropping
 * everything with them would make an UNREAD plate look exactly like a read one
 * — the single distinction POE-171 refuses to blur, because an unread plate is
 * junk to the advisor rather than an empty slot. So the glyph keeps the part
 * that changes the reading: `?` for a plate that did not resolve, the tier for
 * one that did, and `·` for the rooms that legitimately have no tier (the
 * Entrance, the Apex, a filler).
 *
 * `undefined` — no entry for the slot at all — draws nothing: the plate itself
 * is already outlined as empty, and a glyph would claim a read that never
 * happened.
 */
export function plateGlyph(read: SlotView | undefined): string {
	if (read === undefined) return '';
	if (!read.known) return '?';
	return read.tier > 0 ? `${read.tier}` : '·';
}

/** Wording for one corridor's state, for a `title`. */
export const EDGE_STATE_LABEL: Record<EdgeState, string> = {
	open: 'open corridor',
	unresolved: 'could not be read — the diamond read failed',
	closed: 'closed'
};

// ------------------------------------------------------------ the advice --

/** The wire string R5 uses for "leave this map". Not snake_case — see `AdviceView`. */
export const LEAVE_MAP_ACTION = 'leaveMap';

/**
 * The leave-the-map banner, or null when the advisor said to continue.
 *
 * Null rather than an empty string so a surface cannot render an empty banner
 * by forgetting to test it.
 */
export function leaveMapBanner(advice: AdviceView | null): string | null {
	if (advice === null || advice.mapAction !== LEAVE_MAP_ACTION) return null;
	return 'Leave this map — the temple has what it needs from it.';
}

/** The best move, or null when there is nothing to rank. */
export function topRecommendation(advice: AdviceView | null): RankedView | null {
	return advice?.recommendations[0] ?? null;
}

/** The best RV-excluded option, or null when none was excluded. */
export function topGamble(advice: AdviceView | null): RankedView | null {
	return advice?.gambles[0] ?? null;
}

/**
 * A risk fraction as a whole percent, or null when there is none.
 *
 * Null is the recommended side: `risk` is only measured for the options RV
 * excluded, so a `0%` there would claim a measurement that was never taken.
 */
export function formatRisk(risk: number | null): string | null {
	if (risk === null || !Number.isFinite(risk)) return null;
	return `${Math.round(risk * 100)}%`;
}

/** One gamble's label — the word, and the risk that made it one. */
export function gambleLabel(gamble: RankedView): string {
	const risk = formatRisk(gamble.risk);
	return risk === null ? 'gamble' : `gamble · ${risk} risk`;
}

/** The move in one line: what to kill, and which doors to open. */
export function moveLine(ranked: RankedView): string {
	return `${ranked.headline} · ${ranked.doorsLabel}`;
}

/**
 * The mark a forced kill carries, or null when the kill was chosen.
 *
 * The side panel always prints two architect blocks (POE-243). When the read
 * produced one, the kill on the headline is not the better of two — it is the
 * only one there was, and a surface that shows it with the same weight as a
 * ranked choice is telling the player a decision was made that was not. The
 * ranking itself is untouched: what changes is what the headline claims.
 *
 * Derived from `AdviceView.forcedKill`, the typed half of the warning, rather
 * than from the warning's own prose — which is printed alongside and would
 * break this the first time it was reworded.
 */
export function forcedKillNote(advice: AdviceView | null): string | null {
	return advice?.forcedKill === true ? 'only architect read' : null;
}

/** The first reason, for surfaces with one line to spare. The page shows all. */
export function leadReason(ranked: RankedView): string | null {
	return ranked.reasons[0] ?? null;
}

/**
 * The architect block the top recommendation is about, or null.
 *
 * `architectIndex` is a position in the panel's own `offers`, so the lookup can
 * miss two ways that both mean "nothing to point at": the ranking named no
 * architect (`kill either`), and it named one the panel view no longer carries.
 * Both answer null rather than throwing an index at a surface.
 */
export function chosenOffer(slice: TempleSlice): OfferView | null {
	const index = topRecommendation(slice.advice)?.architectIndex;
	if (index === null || index === undefined) return null;
	return slice.panel?.offers[index] ?? null;
}

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
 * This replaces the single kill callout, which said one thing about ONE block
 * and left the other block — the one the player is choosing against — unnamed
 * on the overlay. Two boxes state both offers and mark which one the advisor
 * took, which is the comparison the player is actually making.
 */
export interface OfferBox {
	/** The block this box is about, so a surface can point at it. */
	offer: OfferView;
	/** The three-tier area bonus ladder for this line, or null when absent. */
	ladder: {
		quant: [number, number, number] | null;
		rarity: [number, number, number] | null;
		tier: number;
	} | null;
	/** The architect mod line, when the box has one. */
	mod: OfferMod | null;
	/** `"Guatelitzi · upgrade"` — which architect, and which kill. */
	headline: string;
	/** `"Locus of Corruption (tier 3)"`, or the honest refusal when the printed
	 *  target did not resolve. */
	builds: string;
	/** `"Vertolka A++"`, with ` · T3 <room>` appended when the kill lands below
	 *  tier 3. Null when the offer carried no grade — there is no line, so there
	 *  is nothing graded, and a blank rating is better than an invented one. */
	rating: string | null;
	/** The advisor's first reason for the ranked entry that names THIS block, or
	 *  null when the ranking named no entry for it — EXCEPT on a `kill either`
	 *  board, where the top recommendation names no architect and its own lead
	 *  reason (the door instruction, which is still valid whichever block the
	 *  player kills) goes on every box. See [`offerReason`]. */
	reason: string | null;
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
	 *  On EVERY box, including a live one: the box's whole job is to make a
	 *  comparison readable at arm's length over a game, and "these two numbers
	 *  came from the grade ladder, not from the market" is the difference
	 *  between a ranking the player can trust and one they cannot. */
	market: string;

	// ------------------------------------------- what the number is (POE-260)

	/** The room's worth in chaos per run, or null when no market answered and
	 *  the letter ladder ranked it — there is no chaos number to print then,
	 *  and [`valueText`](Self.valueText) carries the grade instead. */
	value: number | null;
	/** What the value slot prints: `"96"`, or `"grade A++"` on the ladder.
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
	 *  change — the fallback state has no rows at all. */
	state: OfferValueState | null;
	/** `"floor · 2 unpriced"` on a partial sum, else null. It replaces
	 *  `per run` in the value row, so the row's height does not move. */
	chip: string | null;
	/** The marks beside the value: `F` where the ladder priced the room, `G`
	 *  where the number rests on an estimate and there is no driver row to
	 *  carry that mark instead. */
	marks: BoxMark[];
	/** `"tier 1 = 80% of tier 3 · the rows below are tier 3's"`, or null on a
	 *  tier-3 kill and on a row the player overrode.
	 *
	 *  The one line that keeps the driver rows honest on a scaled row: epic
	 *  lock L2 prices tier 1 as a fraction of the LINE, so the rows under it
	 *  are the tier-3 room's terms copied unscaled and they do NOT add up to
	 *  the number above them. Without this line the box would look like a sum
	 *  that does not sum. */
	scaleNote: string | null;
	/** The dropped items and the sale, in display order, at most
	 *  [`FULL_DRIVER_ROWS`]. */
	drivers: OfferDriver[];
	/** How many rows there were before the cap — what the collapse rule reads
	 *  (`offersCompact` in `overlay-geometry.ts`). */
	driverCount: number;
	/** `"+1 more item · 34c"` for the rows past the cap, or null. */
	fold: string | null;
	/** The quantity / rarity bonus, or null on a line poedb prints no
	 *  percentage for (Toxic Grove, Storm of Corruption). */
	bonus: OfferBonus | null;
	/** The one line that says something the rows cannot — an instrumental
	 *  line's worth, the player's own number, or a line that drops nothing at
	 *  all. Null when the rows say it themselves. */
	note: string | null;
	/** The vial upgrade for this line's unique, or null. Most lines have none;
	 *  Locus of Corruption is the case that proves it. */
	recipe: OfferRecipe | null;
	/** The compact form's price strip — `"68 · 41 · 30c"` — or null with no
	 *  priced driver to put in it. */
	stripPrices: string | null;
	/** The compact form's one foot line: the rating and the price age. */
	foot: string;
	/** Whether the market read behind the number is past its staleness
	 *  threshold. All this FLAG draws is the value's dotted underline and the
	 *  age line's colour; the box's shape on a stale board is decided by
	 *  [`state`](Self.state), which a stale read puts in `fallback` — Rust
	 *  prices nothing off a stale snapshot, so the rows go and the box gets
	 *  shorter. See [`offerBoxSignature`]. */
	stale: boolean;
}

/** An architect mod line projected for an offer box. */
export interface OfferMod {
	name: string;
	hint: string | null;
	slots: ItemSlotId[];
	price: string;
	perRun: string | null;
	marks: BoxMark[];
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
export type OfferDriverKind = 'sale' | 'unique' | 'vial' | 'mod';

/** One row under the value: what the room drops, and what it goes for. */
export interface OfferDriver {
	kind: OfferDriverKind;
	/** The row's label — the poe.ninja item name, the temple mod's item hint,
	 *  or the sale row's own wording. */
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
	marks: DriverMark[];
}

/** The quant / rarity line. */
export interface OfferBonus {
	/** `"+6% quant · +12% rarity"`. */
	label: string;
	/** `"+15c"`, or null where neither rate priced anything. */
	amount: string | null;
}

/** One member of the recipe row. */
export interface OfferRecipeItem {
	name: string;
	/** The poe.ninja name to fetch an icon for — never null here, a recipe
	 *  member always has one. */
	iconName: string;
	/** `"68c"` or `"—"`. */
	price: string;
}

/** Base unique + vial → upgraded unique, priced. */
export interface OfferRecipe {
	base: OfferRecipeItem;
	vial: OfferRecipeItem;
	upgraded: OfferRecipeItem;
}

/**
 * How many driver rows the full form draws (POE-260 design §7).
 *
 * Three. The fourth and beyond fold into one muted line rather than growing
 * the box: the height is designed against the 316 px the panel's own diagonal
 * admits at 1920×1080, and a box that grows a row grows past plate C0.
 */
export const FULL_DRIVER_ROWS = 3;

/**
 * The rating line, or null when this offer has no line to rate.
 *
 * The grade is the LINE's — Vertolka ranks the 25 families by what their
 * tier-3 room is worth — so a kill that lands below tier 3 prints the tier-3
 * room's name beside the letter. Without it the box would credit the room in
 * hand with a rating it was never given.
 *
 * At tier 3 the suffix is dropped: `builds` already names that exact room, and
 * repeating it is noise on a box read at arm's length over a game.
 */
function offerRating(offer: OfferView): string | null {
	if (offer.grade === null) return null;
	const rating = `Vertolka ${offer.grade}`;
	if (offer.builtTier !== 3 && offer.lineTop !== null) return `${rating} · T3 ${offer.lineTop}`;
	return rating;
}

/**
 * The reason the ranking gave for the entry that names THIS block, or null.
 *
 * Recommendations first, then gambles, because a block can appear on both
 * sides and the recommended reading is the one the box is drawn to carry. A
 * block the ranking named nowhere gets NO line rather than a borrowed one: the
 * advisor ranks moves, not architects, and attributing one block's argument to
 * the other is the failure this lookup exists to avoid.
 *
 * # `kill either` is the one board where every box gets the same line
 *
 * When the top recommendation carries no `architectIndex` the ranking is saying
 * the kill does not matter — and its lead reason is then the DOOR instruction
 * (`R3: … open D3-C2`), which is computed, still valid, and about neither
 * block. Looking it up by index would leave every box with nothing under its
 * rating while the only instruction on the board went unsaid; on a read where
 * neither offer resolved that is a pair of boxes carrying "does not resolve to
 * a known room" and no advice at all.
 *
 * The attribution is unambiguous precisely because no architect is named: the
 * advisor says either kill is fine, so the same line on both boxes is what it
 * said rather than a borrowed one. That is why this branch is keyed on the TOP
 * recommendation naming no architect and not on a per-box miss — a board that
 * named some OTHER index has an opinion about which block to kill, and a box it
 * did not name still gets silence.
 */
function offerReason(advice: AdviceView, index: number): string | null {
	const top = topRecommendation(advice);
	if (top !== null && (top.architectIndex ?? null) === null) return leadReason(top);
	const ranked = [...advice.recommendations, ...advice.gambles].find(
		(entry) => entry.architectIndex === index
	);
	return ranked ? leadReason(ranked) : null;
}

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
	unique_drop: 'unique',
	vial_drop: 'vial',
	mod_item: 'mod'
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
 * Wire order is kept — `valuation.rs` pushes sale, unique, vial, mod in that
 * order — so the sale leads and the drops follow it, which is the order the
 * design draws.
 *
 * The one term that is DROPPED is a sale worth exactly zero. Eighty-four of the
 * capture's eighty-six room-tiers sit at the floor, so `+0c` would be on nearly
 * every box, and a row saying the room is not worth selling would push a row
 * saying what it drops into the fold. An UNPRICED sale is kept: that the room's
 * own line could not be priced is a fact about this read.
 */
function offerDriverTerms(value: RoomValueView, state: OfferValueState): DriverTerm[] {
	const ladder = state === 'fallback';
	const scaled = rowsAreTier3(value);
	return value.drivers
		.filter((driver) => driver.kind in ROW_KINDS)
		.filter((driver) => !(driver.kind === 'sale' && driver.chaos === 0))
		.map((driver) => {
			const kind = ROW_KINDS[driver.kind];
			const sale = kind === 'sale';
			// The sale row prints what it ADDED (the delta above the floor); a
			// drop row prints what ONE of the item goes for, with its expected
			// count beside it. Two different numbers, because a drop's own
			// contribution — 0.25 of a unique — is not a price anybody trades
			// at, and a player checking the box against poe.ninja is looking up
			// the item. The count goes through `formatCount` because since
			// POE-262 a vial rate is DERIVED (0.1 x 2815/1689), and the raw
			// float renders as `×0.16666666666666666`.
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
					// No icon for the sale row — its glyph is drawn instead —
					// and none for a MOD row either. A mod's `name` is the
					// prose hint `drops.rs` prices the architect's signature
					// rare by (`temple gloves`), not a poe.ninja item, so
					// `/api/icon/temple/temple%20gloves` is a request that cannot
					// succeed: every one of those rows would fetch a 404 and
					// then draw the unresolved glyph anyway. Drawing it
					// straight away is the same picture without the request.
					iconName: sale || kind === 'mod' ? null : driver.name,
					price: driverPrice(shown, ladder, saleDelta),
					priced: shown !== null,
					perRun: driver.count === null || sale ? null : `×${formatCount(driver.count)}`,
					marks: driverMarks(driver)
				},
				shown,
				contributed: driver.chaos
			};
		});
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
 * folded, and what the two bonus percentages are. `offerScaleNote` is the line
 * that says out loud why the rows do not add up.
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
	quantity_bonus: 'quant',
	rarity_bonus: 'rarity'
};

/**
 * The quant / rarity line, or null on a line poedb prints no percentage for.
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
 * Three different facts share the slot because no box can carry two of them: a
 * room valued at its letter has no rows at all, a room the player priced has
 * one row that is their own number, and a room with no drops has an empty list
 * that would otherwise read as a missing read.
 */
function offerNote(state: OfferValueState, rows: OfferDriver[]): string | null {
	// One wording for BOTH instrumental lines. Temple Nexus is worth the tiers
	// it lifts and Shrine of Unmaking the rooms it clears, and telling them
	// apart here would mean hard-coding two room names in a file that words
	// what the slice says rather than knowing which line is which.
	if (state === 'instrumental') {
		return 'valued at its letter — its worth is what it does, not what it drops';
	}
	if (state === 'override') return 'your own number for this room';
	if (rows.length === 0 && state !== 'fallback') {
		return 'this line drops no unique and no vial';
	}
	return null;
}

/** What the value slot prints, and whether there is a chaos number behind it. */
function offerValueText(offer: OfferView, value: RoomValueView, state: OfferValueState): string {
	// The ladder has no chaos number to print — the letter IS the answer — and
	// an invented one would be the fabrication epic lock L4 exists to prevent.
	// The `?? ` branch is unreachable in practice (a valued room resolved to a
	// line, and a line has a grade) and is here because `grade` is nullable.
	if (state === 'fallback') return `grade ${offer.grade ?? '—'}`;
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
 *  reason [`offerChip`]'s is: the roll-up below is about whether ANY row
 *  carries the mark, and a folded row carries it just as well as a drawn one. */
function offerBoxMarks(
	value: RoomValueView,
	state: OfferValueState,
	rows: OfferDriver[]
): BoxMark[] {
	const marks: BoxMark[] = [];
	if (state === 'fallback') marks.push('F');
	// `G` rolls up to the box only when there is no row to carry it. With rows
	// on screen the estimate is attributable — this count, that price — and a
	// second mark saying the same thing at box level is noise.
	if (value.guessed && rows.length === 0) marks.push('G');
	return marks;
}

/**
 * `"tier 1 = 80% of tier 3 · the rows below are tier 3's"`, or null.
 *
 * The rows on a scaled box do NOT add up to the number above them, by design
 * (epic lock L2), and this is the line that says so. Without it the box is a
 * sum that does not sum, which is worse than no explanation at all.
 */
function offerScaleNote(offer: OfferView, value: RoomValueView): string | null {
	if (!rowsAreTier3(value)) return null;
	const fraction = value.drivers.find((driver) => driver.kind === 'tier_fraction');
	if (fraction?.count == null) return null;
	const pct = Math.round(fraction.count * 100);
	return `tier ${offer.builtTier ?? '?'} = ${pct}% of tier 3 · the rows below are tier 3's`;
}

/** One recipe member, with the em dash for a price this read does not have. */
function recipeItem(item: RecipeItemView): OfferRecipeItem {
	return {
		name: item.name,
		iconName: item.name,
		price: item.chaos === null ? '—' : `${offerChaos(item.chaos)}c`
	};
}

/** The vial upgrade, worded, or null. */
function offerRecipe(recipe: RecipeView | null): OfferRecipe | null {
	if (recipe === null) return null;
	return {
		base: recipeItem(recipe.base),
		vial: recipeItem(recipe.vial),
		upgraded: recipeItem(recipe.upgraded)
	};
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
	return `${priced.map((term) => offerChaos(term.shown as number)).join(' · ')}c`;
}

/** The compact form's foot: the rating and the price age, or the age alone. */
function offerFoot(rating: string | null, market: string): string {
	return rating === null ? market : `${rating} · ${market}`;
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
 * the grade and an `F`, no rows, em dashes in the recipe. It gets SHORTER, not
 * equal. What makes `stale` redundant in this string is that `state` is already
 * in it: the transition a snapshot ageing causes is `market` → `fallback`, and
 * that re-measures on the field that names the form. The flag itself buys no
 * height — it is a dotted underline and a yellow age line on a box whose shape
 * `state` has already described — so putting it here would only add a second
 * spelling of a change already covered.
 *
 * `pick` is present because the frame is 2 px on the pick and 1 px otherwise,
 * which is 2 px of box. `compact` is present because it is the whole form.
 *
 * Here rather than in the component because a `.svelte` file has no unit-test
 * harness in this app: the one thing that must be provable about this string is
 * that changing a rendered word does not change it.
 */
export function offerBoxSignature(box: OfferBox, compact: boolean): string {
	return [
		box.offer.index,
		box.pick ? 'pick' : '-',
		compact ? 'compact' : 'full',
		box.state ?? 'none',
		box.drivers.length,
		box.fold ? 'fold' : '-',
		box.bonus ? 'bonus' : '-',
		box.note ? 'note' : '-',
		box.recipe ? 'recipe' : '-',
		box.scaleNote ? 'scale' : '-',
		box.rating ? 'rating' : '-',
		box.reason ? 'reason' : '-'
	].join(' ');
}

function offerLadder(offer: OfferView): OfferBox['ladder'] {
	const line = offer.line ?? null;
	if (
		line === null ||
		offer.builtTier === null ||
		(line.quantityPct === null && line.rarityPct === null)
	) {
		return null;
	}
	return {
		quant: line.quantityPct,
		rarity: line.rarityPct,
		tier: offer.builtTier
	};
}

/**
 * One box per architect block on the panel, in the panel's own order.
 *
 * **Panel order, not "upgrade first".** `PanelView.offers` is reading order
 * top-to-bottom, and a real board can print two `change` offers
 * (`panel.rs`'s own fixture does), so box `i` mirrors `offers[i]` and each box
 * says its own kind in its headline. The stack's geometry follows the same
 * rule: `offerStackPlacement` puts box `i` level with block `i`.
 *
 * Empty when there is no advice or no panel. The boxes carry the advisor's PICK
 * and its reason, so a panel read with no ranking behind it yet — the gap
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
	return panel.offers.map((offer) => {
		const pick = chosenIndex !== null && chosenIndex === offer.index;
		const rating = offerRating(offer);
		// POE-260. Every number below comes from THIS object — the same
		// `Valued` row the advisor ranked the board on (POE-257 D6) — and
		// nothing here recomputes one. A box with no value at all is an offer
		// whose printed target did not resolve: there is no room, so there is
		// nothing to price and nothing to explain.
		const value = offer.value ?? null;
		// The five-state classification, made ONCE — see [`offerState`] for why
		// nothing below may re-read `value.priced` instead.
		const state = value === null ? null : offerState(value.priced);
		const terms = value === null || state === null ? [] : offerDriverTerms(value, state);
		// EVERY term, and the three the full form draws. The chip and the box
		// marks read the first: they describe the number, which is the whole
		// sum, and a fact hidden by the fold is still a fact about it.
		const rows = terms.map((term) => term.row);
		const drivers = rows.slice(0, FULL_DRIVER_ROWS);
		return {
			offer,
			ladder: offerLadder(offer),
			mod: null,
			headline: offerHeadline(offer),
			builds: offerBuilds(offer),
			rating,
			reason: offerReason(advice, offer.index),
			pick,
			// Only on the pick: the note says the kill on the frame was the only
			// one there was, and a note on the block the advisor did NOT choose
			// would be saying that about the wrong box.
			forced: pick ? forcedKillNote(advice) : null,
			market,
			// The ladder's boxes carry no chaos number, and neither does an
			// unresolved offer — `valueText` says which of the two it is.
			value: value === null || state === 'fallback' ? null : value.total,
			valueText: value === null || state === null ? null : offerValueText(offer, value, state),
			state,
			chip: state === null ? null : offerChip(state, rows),
			marks: value === null || state === null ? [] : offerBoxMarks(value, state, rows),
			scaleNote: value === null ? null : offerScaleNote(offer, value),
			drivers,
			driverCount: terms.length,
			fold: value === null ? null : offerFold(value, terms),
			bonus: value === null ? null : offerBonusLine(value),
			note: state === null ? null : offerNote(state, rows),
			recipe: offerRecipe(offer.recipe ?? null),
			stripPrices: offerStripPrices(terms),
			foot: offerFoot(rating, market),
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

/** The corridors the top recommendation wants opened. Empty is a real answer —
 *  R3 can recommend a kill with no door. */
export function suggestedDoors(advice: AdviceView | null): EdgeId[] {
	return topRecommendation(advice)?.doors ?? [];
}

/**
 * The corridor a SECOND Stone of Passage would buy, or null (POE-248).
 *
 * Rust's answer, read and not derived: `AdviceView.secondaryDoor` is the partner
 * of the door already recommended in the best two-key set that CONTAINS it, and
 * the whole of the reasoning behind it — including every reason it is absent —
 * lives in `advisor::conditional_second_door`. Deriving anything like it here
 * would be a second ranking with no rollouts behind it.
 *
 * Null is sometimes an ANSWER and not a gap: the primary door's own singleton is
 * in that ranking, so RU can win it, which is the chain saying do not spend a
 * second key on this board. A surface must not fill the silence.
 *
 * Kept apart from [`suggestedDoors`] rather than appended to it, because the
 * two are different instructions: that list is what to open with the key in
 * hand, this is what to open with a key that has not dropped. The widget draws
 * them at different sizes and opacities for exactly that reason.
 *
 * `?? null` because the field is optional on the wire — a payload from a build
 * before POE-248 has none, and `undefined` reaching an SVG attribute inside an
 * overlay window fails with no devtools to see it.
 */
export function secondDoor(advice: AdviceView | null): EdgeId | null {
	return advice?.secondaryDoor ?? null;
}

/**
 * The convenience door, or null: the corridor to open with the key when the
 * top recommendation opens NOTHING (owner, 2026-09-05).
 *
 * Rust's answer, read and not derived, the same discipline `secondDoor` keeps:
 * `advisor::convenience` ranks the walk — Entrance → Apex, Entrance → the
 * wanted rooms, the wanted rooms → Apex, then the longest open loop — and owns
 * every reason it is null, RU's veto included. A surface that derived one here
 * would be a second ranking with no board model behind it.
 *
 * Never merged into `suggestedDoors`: that list is the MOVE, and this is what
 * to do with a key the move has no use for.
 */
export function convenienceDoor(advice: AdviceView | null): EdgeId | null {
	return advice?.convenience?.door ?? null;
}

/** The convenience door's one line — the door and the walk it shortens — or
 *  null. What the page prints under the top recommendation, the way it prints
 *  the second stone's door. */
export function convenienceNote(advice: AdviceView | null): string | null {
	return advice?.convenience?.reason ?? null;
}

/**
 * The corridor the room widget draws FAINT: the second stone's door, or the
 * convenience door when the move opens nothing.
 *
 * One seal for two answers, because they are the same kind of statement — not
 * the move, but what to do with a key the move has no use for — and because
 * Rust makes them exclusive by construction: the second stone's door needs a
 * primary door to be second to, the convenience door needs there to be none.
 * The `??` is therefore never a priority between two present answers; it is
 * only which field carried the one there is.
 */
export function faintDoor(advice: AdviceView | null): EdgeId | null {
	return secondDoor(advice) ?? convenienceDoor(advice);
}

/**
 * The name on the solid purple exit, or null (POE-261).
 *
 * A READER, the way `secondDoor` and `convenienceDoor` are readers. Rust's
 * `slice.rs::recommended_exit` decided both halves — which corridor, and what
 * the plate behind it read as at its own tier — and this file is not allowed a
 * second opinion about either: a name assembled here out of `layout.slots`
 * would be a second answer to what the board says, and it would go on saying it
 * after a re-read moved the plate's tier.
 *
 * Null is an ANSWER and not a gap, so nothing may fill it: the move opens no
 * door, or the plate behind the door did not resolve. The widget draws the seal
 * unlabelled — an unread room named by guess is worse than an unnamed one.
 *
 * `?? null` because the field is optional on the wire, the same reason
 * `secondDoor` has one: a payload from a build before POE-261 carries no field
 * at all, and `undefined` reaching the widget inside an overlay window fails
 * with no devtools to see it.
 */
export function recommendedExit(advice: AdviceView | null): ExitLabelView | null {
	return advice?.recommendedExit ?? null;
}

// ------------------------------------------------------------- the panel --

/**
 * What an offer actually builds.
 *
 * The **resolved** name and the tier the kill guarantees — never the printed
 * name on its own. POE-169's whole point is that the two differ: Contested
 * Development prints one line and builds `currentTier + 1` of it, so a surface
 * showing only what the panel printed is showing the player a room they are not
 * getting. When nothing resolved there is no name to show, and saying so is the
 * honest answer.
 */
export function offerBuilds(offer: OfferView): string {
	if (offer.displayName === null) return 'does not resolve to a known room';
	return offer.builtTier === null
		? offer.displayName
		: `${offer.displayName} (tier ${offer.builtTier})`;
}

/** The offer's own header: which architect, and which of the two kills it is. */
export function offerHeadline(offer: OfferView): string {
	return `${offer.architectName} · ${offer.kind}`;
}

/** The incursion budget in words. Null means the line was not legible. */
export function incursionsText(remaining: number | null): string {
	return remaining === null
		? 'incursions remaining: not legible'
		: `incursions remaining: ${remaining}`;
}

// ------------------------------------------------------------- the badges --

/**
 * The unread-plate badge, or null when every plate resolved.
 *
 * Named, not counted: which plates the advisor is treating as junk is the fact
 * a player needs to judge the recommendation, and a bare count hides it.
 */
export function unknownRoomsBadge(slice: TempleSlice): string | null {
	const unread = slice.unknownRooms;
	if (unread.length === 0) return null;
	const plates = unread.length === 1 ? 'plate' : 'plates';
	return `${unread.length} unread ${plates}: ${unread.join(', ')}`;
}

/**
 * The marker-fallback notice, or null when the diamond read settled the doors.
 *
 * Carries the reader's own message: "the corridors are a fallback" without the
 * reason is a warning nobody can act on.
 */
export function markerFallbackNotice(layout: LayoutView | null): string | null {
	if (!layout?.markerError) return null;
	return `Door markers unread — corridors fall back to the beam read (${layout.markerError}).`;
}

/**
 * The one line the door widget has room for when the read is not trustworthy,
 * or null when it is (POE-244).
 *
 * The overlay's advice panel used to print four honesty surfaces under the
 * recommendation — a low-confidence read, the marker fallback, the unread
 * plates and the advisor's warnings — and POE-244 replaced that panel with a
 * pointer. Three of those four move to the Temple page, which is the surface
 * for reading; this one does not, because it is the only one that says **do not
 * act on what this widget is showing you**, and the widget it is about is the
 * one still on screen inside the room.
 *
 * A PRECEDENCE, not a list: there is one line. Low confidence outranks the
 * marker fallback because it is the stronger statement — `Confidence::Low`
 * means the beam read itself is a best effort over a panel it could not
 * separate, and the marker fallback is the narrower "the seals were unread, so
 * these came from the beam". Both are also visible on the diamond as grey
 * seals; the words are what say the grey is not just this one corridor.
 *
 * Short on purpose. The page keeps `markerFallbackNotice`, which carries the
 * reader's own reason and does not have to fit under a 190 px widget.
 */
export function doorWarning(layout: LayoutView | null): string | null {
	if (layout === null) return null;
	if (layout.confidence === 'low') return 'low-confidence read — do not act on these doors';
	if (layout.markerError !== null) return 'seals unread — doors are a beam-read fallback';
	return null;
}

/** `"chase"` / `"scarab"` as a label. Null mode means no advice was produced. */
export function modeLabel(mode: string | null): string | null {
	if (mode === null) return null;
	if (mode === 'chase') return 'Chase';
	if (mode === 'scarab') return 'Scarab';
	return mode;
}

/** The last completed read as a local clock time, or null before the first. */
export function lastReadText(lastReadAt: number | null): string | null {
	if (lastReadAt === null) return null;
	return new Date(lastReadAt).toLocaleTimeString();
}

/** An hour, in ms — where `marketAge` switches units. */
const HOUR_MS = 60 * 60 * 1000;

/**
 * How old an observation is, in the coarsest unit that still says something.
 *
 * Minutes under the hour, whole hours above it, FLOORED in both: "12 min old"
 * is a claim about a price the player may act on, and rounding a 59-minute read
 * up to "1 h" would make it sound older than it is at exactly the moment the
 * next server recompute is due. A clock that runs behind the server's clamps to
 * zero rather than printing a negative age.
 */
function marketAge(asOf: number, now: number): string {
	const ms = Math.max(0, now - asOf);
	if (ms < HOUR_MS) return `${Math.floor(ms / 60_000)} min`;
	return `${Math.floor(ms / HOUR_MS)} h`;
}

/**
 * Whether a market view is too old to price with, AT THE CLOCK GIVEN.
 *
 * The wire's `stale` is Rust's answer at publish time, and a `MarketView` on a
 * board does not get republished — `TempleSlice.market` belongs to the read that
 * produced the board and stands still until the next one. So the flag is read
 * only when there is no observation to measure; whenever `asOf` is set, the
 * clock and the published `staleAfterMs` are the answer, which is what makes a
 * board left on screen cross the two-hour line on its own.
 */
export function marketStale(market: MarketView, now: number = Date.now()): boolean {
	if (market.asOf === null) return market.stale;
	return now - market.asOf > market.staleAfterMs;
}

/**
 * What the page and the offer boxes say about the prices behind the board
 * (POE-258).
 *
 * Three states, always one of them — this is never null, because "nothing said
 * about the prices" is the state the player cannot tell from "priced": a room
 * valued at its grade rung and one valued off the feed print the same kind of
 * number, and only this line says which.
 *
 * - `prices 12 min old` — the board is priced.
 * - `prices stale (3 h) — base values` — the read was already too old to price
 *   with when it was VALUED, so the numbers beside this line came off the cold
 *   grade ladder. The age is stated because it is what tells a stalled feed
 *   from a server that was never reached.
 * - `prices stale (3 h)`, no suffix — the read priced these numbers off a live
 *   market and has since aged past the line with nothing re-read. See below.
 * - `prices unavailable — base values` — everything else: no poll has landed,
 *   the server's cache is cold, the payload priced another league, or the floor
 *   was unusable. The player's move is the same in all four, so they get one
 *   sentence rather than four.
 *
 * **The line ages by itself.** Both the minutes and the STALE verdict come from
 * `now`, not from the publish (`marketStale`), so a board sitting on screen
 * rolls from `prices 12 min old` through `prices 59 min old` to
 * `prices stale (2 h)` with nothing republished behind it.
 *
 * **And that is why the suffix is conditional.** `— base values` is a claim
 * about the NUMBERS printed beside this line, not about the age: it says they
 * came off the grade ladder. On a board the clock aged after the fact those
 * numbers are real prices that have merely gone old, so the suffix would be
 * false — the caution belongs on the age, and `base values` arrives at the next
 * read, which is the one that will actually take the cold branch. Both
 * conditions are tested rather than only `unavailable`: Rust makes a stale read
 * unavailable by construction (`prices_anything` is `is_live() && …`), so the
 * flag is redundant TODAY, and asserting it anyway is what keeps this wording
 * honest if that entailment ever stops holding.
 *
 * `now` is a parameter so the age is testable; callers pass nothing.
 */
export function marketNote(market: MarketView, now: number = Date.now()): string {
	if (market.asOf === null) return 'prices unavailable — base values';
	if (marketStale(market, now)) {
		const age = `prices stale (${marketAge(market.asOf, now)})`;
		const pricedWhenValued = !market.unavailable && !market.stale;
		return pricedWhenValued ? age : `${age} — base values`;
	}
	if (market.unavailable) return 'prices unavailable — base values';
	return `prices ${marketAge(market.asOf, now)} old`;
}
