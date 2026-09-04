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
	EdgeId,
	LayoutView,
	OfferView,
	RankedView,
	SlotId,
	SlotView,
	TempleSlice,
	TempleStatus
} from './slice';

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
 * The statuses the DOOR widget stays up for (POE-244).
 *
 * One more than the board's, and the extra one is the point. The door is opened
 * INSIDE the room, during the timed incursion, and the layout panel is gone by
 * then — so every surface keyed on the panel being readable disappears at
 * exactly the moment the player has to act on it. `panel_not_visible` is the
 * module armed and LOOKING with nothing on screen, which since POE-246 is what
 * the whole incursion looks like: the arm survives on `PANEL_TAIL_MS` from the
 * last sighting, and when it finally lapses the status becomes `waiting`, which
 * is NOT in this list. That is what bounds how stale the room on the widget can
 * be — a board from two minutes ago at the outside, not one from last map.
 *
 * `off`, `idle`, `unavailable` and `error` are excluded for the same reasons
 * they are excluded from the board's list.
 */
export const DOOR_VISIBLE_STATUSES: readonly TempleStatus[] = [
	...OVERLAY_VISIBLE_STATUSES,
	'panel_not_visible'
];

/** Whether the door widget stays up for this status. */
export function overlayShowsDoors(status: TempleStatus): boolean {
	return DOOR_VISIBLE_STATUSES.includes(status);
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
 * - `uncertain` — reported open, but the current room's selection frame covers
 *   it, so the reader could not settle it. Dashed.
 * - `unresolved` — incident to the current room and settled by NOTHING, which
 *   only happens on the diamond-read fallback. Marked, because "we could not
 *   see it" must not render the same as "it is shut".
 * - `closed` — everything else.
 */
export type EdgeState = 'open' | 'uncertain' | 'unresolved' | 'closed';

/**
 * Which of the four states a corridor is in, for one published layout.
 *
 * `unresolved` wins outright: it is the honesty guard, and its set is a subset
 * of `uncertain`, so testing it second would never fire.
 */
export function edgeState(id: EdgeId, layout: LayoutView | null): EdgeState {
	if (layout === null) return 'closed';
	if (layout.unresolvedIncident.includes(id)) return 'unresolved';
	const open = layout.doors.includes(id);
	if (open && layout.uncertain.includes(id)) return 'uncertain';
	return open ? 'open' : 'closed';
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
	uncertain: 'reported open, hidden behind the selection frame',
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

/** What the kill callout says. Null when there is no ranked move to say it about. */
export interface KillCallout {
	/** `"KILL Quipolatl → Armoury"` — the architect to click and the room the
	 *  kill actually BUILDS. The arrow half is dropped when the printed target
	 *  did not resolve, and the whole title falls back to the advisor's own
	 *  headline when the ranking named no architect — which is `"kill either"`,
	 *  and is already the whole instruction. */
	title: string;
	/** The one reason the box has room for. */
	reason: string | null;
	/** `"only architect read"` when the kill was forced, else null. */
	forced: string | null;
	/** The block to point the arrow at, or null for a read that carried no
	 *  boxes — which is not the same as no architect, and both draw the box
	 *  without an arrow. */
	target: OfferView | null;
}

/**
 * `KILL <architect> → <room the kill builds>`.
 *
 * The architect leads because it is what is printed on screen beside the block
 * the player has to click. The target follows because without it the overlay
 * says WHICH block and never says what taking it does — the ranked action would
 * be absent from every overlay surface, which is the thing the old advice
 * widget's headline did carry.
 *
 * `displayName` and not `printedTarget`: POE-169's whole point is that the two
 * differ, and Contested Development prints one line while building
 * `currentTier + 1` of it. When nothing resolved there is no room to name and
 * the title is the architect alone — `offerBuilds()` says so in full on the
 * page, which has the space for it.
 */
function killTitle(target: OfferView): string {
	const head = `KILL ${target.architectName}`;
	return target.displayName === null ? head : `${head} → ${target.displayName}`;
}

/**
 * The kill, as the pointer callout says it (POE-244).
 *
 * The owner's ask is that the overlay be seen rather than read: the name and
 * the target lead, and the reason follows as the one line that justifies the
 * choice. Everything else the old advice widget printed — the gamble, the
 * unread-plate badge, the advisor's warning list — stays on the Temple page,
 * which is the surface for reading. The one honesty line that did NOT move is
 * `doorWarning()`; see its note.
 */
export function killCallout(slice: TempleSlice): KillCallout | null {
	const move = topRecommendation(slice.advice);
	if (move === null) return null;
	const target = chosenOffer(slice);
	return {
		title: target === null ? move.headline : killTitle(target),
		reason: leadReason(move),
		forced: forcedKillNote(slice.advice),
		target
	};
}

/** The corridors the top recommendation wants opened. Empty is a real answer —
 *  R3 can recommend a kill with no door. */
export function suggestedDoors(advice: AdviceView | null): EdgeId[] {
	return topRecommendation(advice)?.doors ?? [];
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
