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
 * on the panel, in the panel's own order, carrying the resolved room and kind,
 * the line's bonus ladder and architect, the value explanation and the
 * advisor's pointer. `killCallout()`, `KillCallout` and `killTitle()` are GONE
 * with it — they said one thing about one block and left the block the player
 * is choosing against unnamed. The Temple page keeps its own `offerHeadline`
 * and `offerBuilds`; boxes use `offerBoxHeadline` and `offerBoxBuilds` because
 * their header answers a different question.
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
	DiamondView,
	EdgeId,
	ExitLabelView,
	LayoutView,
	MarketView,
	OfferView,
	RankedView,
	SlotId,
	SlotView,
	TempleSlice,
	TempleStatus
} from './slice';
import { formatCount } from './values';
export {
	FULL_DRIVER_ROWS,
	formatDropRate,
	offerBoxBuilds,
	offerBoxes,
	offerBoxHeadline,
	offerBoxSignature,
	offerChaos,
	offerUniqueDrops
} from './offer-box';
export type {
	BoxMark,
	DriverMark,
	OfferBonus,
	OfferBox,
	OfferDriver,
	OfferDriverKind,
	OfferLadder,
	OfferValueState
} from './offer-box';

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
 *
 * **It gates the DIAMOND, not the whole widget, since POE-276.** The widget
 * also draws a `reading…` line while a read is in progress, which is a status
 * rule and deliberately a separate one: `doorWidget()` below composes the two,
 * and this function still answers only "is there a move and a room to draw it
 * on".
 */
export function overlayShowsDoors(slice: TempleSlice): boolean {
	return slice.advice !== null && (slice.layout?.diamond ?? null) !== null;
}

/** The door widget's in-progress line (POE-276). Short on purpose: it is a
 *  pulse to glance at, the way the merc strip's status line is, not a report. */
const DOOR_READING_LINE = 'reading…';

/** What the door widget draws — `doorWidget()`'s answer. */
export interface DoorWidgetView {
	/** The room's shape, or null when there is no move or no room to draw it
	 *  on (`overlayShowsDoors`) — the first read of an incursion has neither. */
	diamond: DiamondView | null;
	/** The muted `reading…` line, or null when no read is in progress or the
	 *  read is a retry round (`readRetry`). */
	reading: string | null;
}

/**
 * What the door widget draws, or null for nothing at all (POE-276).
 *
 * Two independent halves, because they answer two different questions:
 *
 * - the DIAMOND is `overlayShowsDoors` — the advice, which lives with the
 *   INCURSION (POE-248) — and nothing about it changes here;
 * - the LINE is the status alone: `reading` is what Rust publishes from the
 *   tick that anchors a board until that read's own projection replaces it
 *   (`temple::run::next_status`, `TickOutcome::Anchored`). A reopen of an
 *   already-read board answers `Reshown` and never passes through `reading`,
 *   so it draws no line.
 *
 * So the first read of an incursion is the line alone, in the widget's frame
 * — there is no advice and no room yet, and without it the player cannot tell
 * a working read from a broken one for the seconds before the verdict. A
 * RE-READ (the next room) is the previous diamond WITH the line: the slice
 * still carries the last advice and layout while `reading` is published, and
 * blanking them would take the only surface left in the room off the screen
 * for the length of the read.
 *
 * A RETRY ROUND draws no line (owner, 2026-09-11, reversing WI-1's acceptance
 * of the line coming back under the room it had just drawn). A retry is one
 * of the partial rounds an unclean board is owed (`RETRIES` in
 * `temple/run.rs`), and Rust flags it in the same publish as its `reading`
 * (`slice.readRetry`, decided by the gate's own same-board rule): the verdict
 * it refines is already on the widget, so the widget stays exactly as it is —
 * the room, or nothing when round 1 settled no room. A first read, a Re-arm, a
 * settings change and a new board (another epoch, a walked or moved sheet)
 * are not retries and show the line as above.
 *
 * Every non-reading case is exactly `overlayShowsDoors`: null wherever it
 * answers false, the diamond with no line wherever it answers true.
 */
export function doorWidget(slice: TempleSlice): DoorWidgetView | null {
	const diamond = overlayShowsDoors(slice) ? (slice.layout?.diamond ?? null) : null;
	const reading = slice.status === 'reading' && !slice.readRetry ? DOOR_READING_LINE : null;
	if (diamond === null && reading === null) return null;
	return { diamond, reading };
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
 * The resolved name and tier come from Rust temple::facts::landing_tier.
 * When the read cannot resolve an offer, there is no built name to show.
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
