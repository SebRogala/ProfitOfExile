/**
 * TypeScript mirror of the Rust `temple` SSOT slice (POE-171).
 *
 * The Rust structs (`src-tauri/src/temple/slice.rs`, published through
 * `ssot.rs`) own this shape — nothing in the webview writes it. Field names are
 * camelCase because the wire structs carry `#[serde(rename_all = "camelCase")]`;
 * `TempleStatus` is `snake_case` on the wire, so the union below spells the wire
 * strings and not the Rust variant names.
 *
 * **`AnchorCalibration` is the exception, and it is deliberate**: that struct
 * carries no `rename_all`, so its fields stay `screen_w` / `screen_h` INSIDE a
 * camelCase parent. Renaming them here to look tidy would make the page read
 * `undefined` for the calibration it renders.
 *
 * Two Rust serde tests pin the same strings from the other side —
 * `the_default_slice_json_is_pinned_for_the_typescript_mirror` and
 * `a_populated_slice_json_is_pinned_for_the_typescript_mirror`. Their literals
 * are copied verbatim into `slice.test.ts`, so a rename on either side fails a
 * test rather than blanking a control on the page.
 *
 * There is no constructor for a read here on purpose — a board only ever
 * arrives from Rust. `templeSliceDefault()` exists because the store needs
 * something to show before the first poll answers.
 */

/**
 * What the module is doing, in the one field a page can switch on.
 *
 * `off` (module disabled — the SSOT composer forces it) and `unavailable` (no
 * capture or no OCR engine on this host) outrank everything the loop publishes.
 * `no_current_room` is the panel open between rooms: a layout, no advice.
 * `waiting` (POE-242) is the module running and NOT capturing — nothing in
 * Client.txt has put an incursion in scope — which is where a session spends
 * nearly all of its time; `panel_not_visible` is the module having looked and
 * seen nothing, which is a different answer to "why is nothing happening?".
 */
export type TempleStatus =
	| 'off'
	| 'idle'
	| 'waiting'
	| 'panel_not_visible'
	| 'reading'
	| 'read'
	| 'no_current_room'
	| 'unavailable'
	| 'error';

/**
 * Every status as a runtime value, and the guard that tests against it.
 *
 * The union above is erased at compile time, so nothing in it can stop a
 * malformed payload — the guard is what does. The membership table is a total
 * `Record`, which means a status added to the union and not here fails
 * `npm run check` rather than being silently rejected at runtime by the store.
 */
const TEMPLE_STATUS_MEMBERS: Record<TempleStatus, true> = {
	off: true,
	idle: true,
	waiting: true,
	panel_not_visible: true,
	reading: true,
	read: true,
	no_current_room: true,
	unavailable: true,
	error: true
};

/**
 * Whether a value is one of the nine wire statuses.
 *
 * `Object.hasOwn` rather than `in`: `in` walks the prototype chain, so a
 * payload carrying `"toString"` (or `"constructor"`, or any other
 * `Object.prototype` key) would pass the guard and be published as a status no
 * surface has a branch for — the exact failure this guard exists to stop.
 */
export function isTempleStatus(value: unknown): value is TempleStatus {
	return typeof value === 'string' && Object.hasOwn(TEMPLE_STATUS_MEMBERS, value);
}

/** `"A0"`…`"E2"` — one of the 13 board positions, in `Slot::ALL` order. */
export type SlotId =
	| 'A0'
	| 'B0'
	| 'B1'
	| 'C0'
	| 'C1'
	| 'C2'
	| 'D0'
	| 'D1'
	| 'D2'
	| 'D3'
	| 'E0'
	| 'E1'
	| 'E2';

/** `"C1-C2"` — a corridor, endpoints in `SlotId` order, joined by a hyphen. */
export type EdgeId = string;

/** One of the 13 plates, as read. */
export interface SlotView {
	slot: SlotId;
	/** The game's own name for what was read; null for an unread plate. */
	name: string | null;
	/** 0 for the Entrance, the Apex, a filler and an unread plate. */
	tier: number;
	/** The name matched the vocabulary exactly, as opposed to fuzzily. */
	exact: boolean;
	/** False means the plate is unread — draw it as such, never guess. */
	known: boolean;
	current: boolean;
}

/** One plate centre in capture px — `[x, y]`. */
export type PlateCentre = [number, number];

/**
 * A rectangle on screen in CAPTURE px — `[x, y, w, h]`.
 *
 * The same unit as `LayoutView.origin` and `centres`: whole-primary-monitor
 * px, which is also window-relative px for a monitor-sized overlay, so no
 * conversion. NOT reference px and NOT CSS px (divide by `scaleFactor()` for
 * those).
 */
export type CaptureRect = [number, number, number, number];

/** Exactly the 13 plate centres Rust publishes, in `Slot::ALL` order.
 *
 *  A TUPLE, not an array: Rust's `LayoutView.centres` is `[[i32; 2]; 13]` and
 *  the board has thirteen plates in every league, so a consumer indexing a
 *  fourteenth is a mistake the type can catch here instead of an `undefined`
 *  reaching an SVG transform. */
export type PlateCentres = [
	PlateCentre,
	PlateCentre,
	PlateCentre,
	PlateCentre,
	PlateCentre,
	PlateCentre,
	PlateCentre,
	PlateCentre,
	PlateCentre,
	PlateCentre,
	PlateCentre,
	PlateCentre,
	PlateCentre
];

/** The board, as pixels gave it. */
export interface LayoutView {
	/** 13 entries, in `Slot::ALL` order. */
	slots: SlotView[];
	/** Corridors to act on — settled, or `doors − uncertain` on the fallback. */
	doors: EdgeId[];
	/** Every corridor incident to the current room — a DIAGNOSTIC about the read,
	 *  never a door state (POE-248).
	 *
	 *  The beam sampler puts all of them here unconditionally, before any
	 *  open/closed judgement, because the gold selection frame covers their
	 *  midpoints (`doors.rs`). What settles them is the diamond read, and its
	 *  answer is already in `doors`. `edgeState` used to test this list and
	 *  coloured a corridor the seals had read GREEN as an unsettled grey —
	 *  see its note. No surface may read it as a state again; the honest
	 *  "nothing settled this" signal is `unresolvedIncident`. */
	uncertain: EdgeId[];
	/** Corridors incident to the current room that NOTHING settled. Populated
	 *  only on the diamond-read fallback: surfaced, never guessed. */
	unresolvedIncident: EdgeId[];
	/** Why the corridors are unresolved, when they are. */
	markerError: string | null;
	current: SlotId | null;
	scale: number;
	ncc: number;
	/** `"high"` or `"low"` — low means nothing should act on the door sets. */
	confidence: string;
	/** Entrance plate centre in CAPTURE px — the origin the board hangs off
	 *  (POE-227). Capture px is whole-primary-monitor px, which is also
	 *  window-relative px for a monitor-sized overlay: no conversion. NOT
	 *  reference px and NOT CSS px (divide by `scaleFactor()` for those). It is
	 *  the Entrance plate's centre, so it is also `centres[Slot::ENTRANCE]`. */
	origin: PlateCentre;
	/** The 13 plate centres in capture px, in `Slot::ALL` order — the same order
	 *  and unit as `origin`, and the same order as `slots`, so index `i` of one
	 *  describes the plate at index `i` of the other. Published by Rust from the
	 *  lattice the board was actually read off; do not re-derive them from
	 *  `scale` here, which would be a second answer to where a plate is. */
	centres: PlateCentres;
	/** Every rectangle this read took its INPUT from, in capture px (POE-244) —
	 *  the never-cover set. 42 on a full board: the side panel, the panel's own
	 *  diamond, the incursion-budget line, one per plate and one per corridor.
	 *
	 *  A surface drawing over the game must keep clear of all of them, because
	 *  the module reads them again on the next tick: a panel drawn over one is
	 *  OCR input the app wrote itself. `overlay-geometry.ts` is what applies
	 *  that rule; nothing here re-derives a rect, and nothing should — five
	 *  different Rust constants own these and a TypeScript copy of any of them
	 *  would drift with nothing to fail.
	 *
	 *  OPTIONAL on the wire, and normalised to `[]` by `normaliseTemple`: a
	 *  snapshot from a build before POE-244 carries neither field, and a
	 *  consumer reading `undefined.length` inside an overlay window fails with
	 *  no devtools to see it. Consumers may treat it as always present. */
	rois?: RoiView[];
	/** The current room's own isometric diamond, or null between rooms. Same
	 *  wire-optionality rule as `rois`. */
	diamond?: DiamondView | null;
}

/** One rectangle the read takes input from. */
export interface RoiView {
	/** `"panel"`, `"diamond"`, `"remaining"`, `"plate"` or `"corridor"`. The
	 *  never-cover rule treats all five the same; the kind is for naming one. */
	kind: string;
	/** A slot key for `plate`, an edge id for `corridor`, null for the three
	 *  panel regions. */
	of: string | null;
	rect: CaptureRect;
}

/**
 * The room's isometric diamond, as the side panel draws it.
 *
 * A UNIT shape, not a screen rectangle — the panel's own diamond has its rect
 * in `rois`. This is the geometry a widget needs to draw the SAME shape
 * somewhere else at whatever size the user dragged it to, which is the whole
 * point: during the incursion the panel and its diamond are gone, and the door
 * the advisor named still has to be identifiable.
 *
 * A ROTATED RECTANGLE since POE-248, not a rhombus: the game draws the room in
 * isometric view with two long walls carrying two doors each and two short
 * walls with one, which is what makes a six-door room readable at a glance.
 *
 * Every field is in one space — centre at the origin, `+y` down — so a consumer
 * fits `corners` into its box and puts every seal and both icon spots through
 * the same transform.
 */
export interface DiamondView {
	/** The outline, four corners in ring order. */
	corners: [
		[number, number],
		[number, number],
		[number, number],
		[number, number]
	];
	/** One seal per corridor the current room has. */
	seals: SealView[];
	/** The architect icon spot in the room's TOP-RIGHT half, in `corners`'
	 *  units (POE-248) — the one the panel's first (topmost) architect block
	 *  belongs to, and where the overlay marks the kill.
	 *
	 *  Published rather than derived here for the reason `corners` is: it is a
	 *  MEASUREMENT of the panel (`markers::ARCHITECT_ICON_OFFSET`), and a
	 *  TypeScript copy would be a second answer a re-measure leaves behind.
	 *
	 *  Named for the HALF and not for a kind of kill: which architect's icon
	 *  the game draws where is what the measurement does NOT settle, so
	 *  `killGlyphs` keys the kill marks on the blocks' own OCR rects.
	 *
	 *  OPTIONAL on the wire and normalised to `null` by `normaliseTemple`, the
	 *  same rule `rois` and `diamond` follow: a snapshot from a build before
	 *  POE-248 carries neither icon, and the glyph is simply not drawn. */
	topIcon?: [number, number] | null;
	/** The spot in the room's BOTTOM-LEFT half — the mirror of `topIcon`
	 *  through the room's centre, and the second block's. Same wire-optionality
	 *  rule. */
	bottomIcon?: [number, number] | null;
}

/**
 * One seal on the room's diamond.
 *
 * Deliberately carries no colour and no recommendation. Open or not is
 * `edgeState(seal.edge, layout)` — the rule every temple surface already
 * shares — and whether the advisor wants this door opened is membership of
 * `recommendations[0].doors`. Repeating either here would be a second answer to
 * a question the slice already answers.
 */
export interface SealView {
	/** The slot this corridor leads to — `"C2"`. */
	neighbour: SlotId;
	/** The corridor itself — `"C1-C2"`, the key `doors` and `uncertain` use. */
	edge: EdgeId;
	/** `[x, y]` ON THE ROOM'S WALL, in `corners`' units (POE-248).
	 *
	 *  Not a unit vector: the room is a rectangle, a door is a hole in one of
	 *  its four walls, and this is where the corridor's own direction leaves
	 *  the outline. The two same-row corridors land at exactly 1.0 — the
	 *  midpoint of a short wall — and the four diagonals at 0.938 and 1.034,
	 *  two to each long wall. */
	pos: [number, number];
}

/** One architect block, resolved. */
export interface OfferView {
	/** Position in the panel, so a surface can point at the right block. */
	index: number;
	architectName: string;
	/** `"change"` or `"upgrade"`. */
	kind: string;
	/** What the panel printed. Kept because it is what the player sees. */
	printedTarget: string;
	/** What the kill actually builds — null when the printed name did not
	 *  resolve. NOT the printed name: Contested Development turns a `change`
	 *  into `currentTier + 1` of the named line (POE-169). */
	displayName: string | null;
	/** The tier the kill guarantees. An `upgrade` also rolls one more at 50%. */
	builtTier: number | null;
	/** Vertolka's letter for the LINE this kill builds into, as his sheet spells
	 *  it — `"A++"`, `"C-"` (POE-249). It grades the FAMILY, so a kill landing on
	 *  tier 2 carries the same letter as one landing on tier 3; `lineTop` is the
	 *  room it was given for. Null when the printed target did not resolve —
	 *  there is no line to have a grade. */
	grade: string | null;
	/** The tier-3 room of that line — what `grade` is a grade OF. Null on the
	 *  same failure. */
	lineTop: string | null;
	/** Where the block sits on screen — the union of the boxes of the OCR lines
	 *  it was read from (POE-243). Null when the read carried no boxes, which
	 *  is what a surface must test before drawing: a missing rect is not the
	 *  screen origin. */
	rect: CaptureRect | null;
	/** What this kill's room is worth in chaos, from the SAME table the ranking
	 *  used (POE-257). Null when the printed target did not resolve — the same
	 *  silence as `displayName` and `grade`.
	 *
	 *  Optional on the wire: a snapshot from a build before POE-257 carries no
	 *  field at all, so read it as `offer.value ?? null`. */
	value?: RoomValueView | null;
	/** The vial upgrade for the unique this kill's LINE drops, priced off the
	 *  same read as `value` — base + vial → upgraded (POE-260).
	 *
	 *  Null far more often than not, and the box draws nothing for every one
	 *  of the reasons: eighteen lines drop no unique, Locus of Corruption's
	 *  Shadowstitch is nobody's recipe base, and a payload that has never
	 *  reached the server carries no recipe table to look in.
	 *
	 *  On the OFFER rather than inside `RoomValueView`, because it is a fact
	 *  about the LINE and not about a tier's sum — the same three prices would
	 *  otherwise ride on all three rows of every `temple_value_table` line,
	 *  where nothing reads them.
	 *
	 *  Optional on the wire and normalised to `null` by `normaliseTemple`. */
	recipe?: RecipeView | null;
}

/** One line's vial upgrade: base unique + vial → upgraded unique (POE-260). */
export interface RecipeView {
	/** The unique this line's tier-3 chest drops. */
	base: RecipeItemView;
	/** The vial that transforms it. NOT necessarily the vial this line's own
	 *  architect rolls for — on Locus of Corruption they are different items,
	 *  which is why Rust keys the lookup on `base`. */
	vial: RecipeItemView;
	/** What the two become. */
	upgraded: RecipeItemView;
}

/** One priced member of a `RecipeView`. */
export interface RecipeItemView {
	/** poe.ninja's own name — the price's join key, and what `/api/gem-icon/`
	 *  is asked for. */
	name: string;
	/** Chaos, or null where this read priced nothing for it. Never `0` standing
	 *  in for a missing price. */
	chaos: number | null;
}

/** One room-tier's chaos value and the terms behind it. */
export interface RoomValueView {
	/** Chaos. The number the advisor ranked this room on — every line, with no
	 *  exception: the two instrumental lines (upgrade, explosives) are priced
	 *  at their grade rung and copied into the ranking like any other. */
	total: number;
	/** `"market"` (every term this room names turned into chaos), `"partial"`
	 *  (some did), `"fallback"` (none did, so the grade ladder stood in),
	 *  `"instrumental"` (one of the two lines whose worth is what it DOES —
	 *  the tiers the upgrade line lifts, the rooms the explosives line clears
	 *  — so it is priced at its letter rather than at its own drops) or
	 *  `"override"` (the player stated this number outright in the Custom
	 *  table). A flag for the box, never a filter. */
	priced: string;
	/** Whether any term that actually contributed chaos rests on somebody's
	 *  estimate — a guessed drop count, one of the two unmeasured bonus rates,
	 *  or the grade ladder. */
	guessed: boolean;
	/** The league these prices came from, `""` on a cold read. A price is
	 *  league-local, so a value shown without one cannot be checked. */
	league: string;
	/** Unix ms of the market read behind this value. Null on a cold read AND on
	 *  a stale one — a stale read prices nothing, so it has no age to show. */
	asOf: number | null;
	/** The tier-3 total this row was scaled from, or null when this IS the
	 *  tier-3 row.
	 *
	 *  **Branch on this before touching `drivers`.** On a tier-3 row the
	 *  drivers sum to `total`. On a tier-1 or tier-2 row they are the TIER-3
	 *  row's terms, copied unscaled, plus one `tier_fraction` driver whose
	 *  `chaos` is the whole total — a lower tier is worth a fraction of the
	 *  LINE, not of its own separate drops (epic lock L2). Summing that list
	 *  would print a Locus tier-1 room as 846 + 676.80. */
	scaledFromTier3: number | null;
	/** Every term of the sum, including the ones that contributed nothing — a
	 *  term silently dropped from a total is indistinguishable from a term
	 *  worth zero. */
	drivers: DriverView[];
}

/** One term of a `RoomValueView`'s sum. */
export interface DriverView {
	/** `"sale"`, `"unique_drop"`, `"vial_drop"`, `"mod_item"`,
	 *  `"quantity_bonus"`, `"rarity_bonus"`, `"tier_fraction"`,
	 *  `"grade_fallback"`, `"instrumental"` or `"custom_override"`.
	 *  `snake_case`, this app's convention for enum variants on the wire.
	 *
	 *  `"instrumental"` carries the whole total for the upgrade and explosives
	 *  lines and has no count and no unit price: the room is valued at its
	 *  letter because its worth is the tiers it lifts / the rooms it clears,
	 *  and that mechanical effect is modelled in the advisor's rollout rather
	 *  than priced here. */
	kind: string;
	/** What is being priced, as its source spells it — the poe.ninja room or
	 *  item name, the game's own bonus wording, or the grade letter. */
	name: string;
	/** Expected count per run, the bonus percentage, or the tier fraction. Null
	 *  where the term has no count, or where nobody has stated one. */
	count: number | null;
	/** Chaos per unit of `count`. Null where nothing priced it. */
	unitPrice: number | null;
	/** What this term added. Null when it added nothing because a count or a
	 *  price was missing — which is why it is listed at all. */
	chaos: number | null;
	/** Whether this term rests on an estimate rather than on measured data. */
	guessed: boolean;
	/** POE-131's thin-market flag on the price behind this term. */
	lowConfidence: boolean;
	/** POE-252: that price is a trailing-window median, not the newest print. */
	windowPriced: boolean;
}

/** The side panel, as text gave it. */
export interface PanelView {
	room: string | null;
	/** Where the title line sits on screen — same unit and same null rule as
	 *  `OfferView.rect`. Null also when the title itself was unread. */
	roomRect: CaptureRect | null;
	offers: OfferView[];
	/** Null means the line was not legible — every rollout then terminates
	 *  immediately and the scores are the board as it stands. */
	incursionsRemaining: number | null;
}

/** One ranked move. */
export interface RankedView {
	/** `"upgrade → Locus of Corruption"`, or `"kill either"`. */
	headline: string;
	/** `"C1-C2, B0-C1"`, or `"no door"`. */
	doorsLabel: string;
	doors: EdgeId[];
	/** Which architect block to point at. */
	architectIndex: number | null;
	ev: number;
	/** Fraction of rollouts that finished below the profile's "lost the room"
	 *  threshold. Null on the recommended side — RV did not exclude it. */
	risk: number | null;
	/** One line per rule that put the option here. A bare score cannot be
	 *  audited, so these are the audit trail and every surface shows them. */
	reasons: string[];
}

/** The convenience door and the walk it shortens, in words — see
 *  `AdviceView.convenience`. */
export interface ConvenienceView {
	/** `"B0-C1"`. */
	door: EdgeId;
	/** Rust's one line, naming the door and the walk: `convenience door B0-C1:
	 *  shortens the Entrance → Apex walk, 5 → 4 hops`. */
	reason: string;
}

/** The recommended exit's label: which corridor, and the room behind it — see
 *  `AdviceView.recommendedExit`. */
export interface ExitLabelView {
	/** `"C1-C2"` — the corridor the top recommendation opens. */
	door: EdgeId;
	/** The game's own name for the plate behind it, at the tier THAT read gave
	 *  the plate. Rust's string, drawn as it arrives: no truncation here and no
	 *  second lookup — a name shortened on the wire cannot be lengthened by a
	 *  wider widget, and a lookup here would be a second answer to what the
	 *  board says the room is. */
	name: string;
}

/** The decision, with everything needed to justify it. */
export interface AdviceView {
	/** Best first. */
	recommendations: RankedView[];
	/** The RV-excluded options, best first, each with its measured risk. */
	gambles: RankedView[];
	/** The corridor a SECOND Stone of Passage would buy, given the top
	 *  recommendation — `"B1-C1"`, or null (POE-248).
	 *
	 *  The CONDITIONAL answer, not the two-key one: Rust ranks only the two-key
	 *  sets that contain the door already recommended and publishes the other
	 *  member of the best (`advisor::conditional_second_door`, which owns every
	 *  reason it is null). The overlay draws it as a faint purple seal beside
	 *  the bright suggested one, so a player who finds a second stone
	 *  mid-incursion acts without having configured `keys` first.
	 *
	 *  NOT a member of `recommendations[0].doors`, and a surface must not merge
	 *  it into them: those are the doors to open NOW, and with one key in hand
	 *  the second is a door the player cannot buy.
	 *
	 *  OPTIONAL on the wire, the same rule `rois` follows: a payload from a
	 *  build before POE-248 carries no field at all. `secondDoor()` in `view.ts`
	 *  is the reader, and it coerces `undefined` to null. */
	secondaryDoor?: string | null;
	/** The door to open with the key when the top recommendation opens
	 *  NOTHING, and the walk it shortens in words (owner, 2026-09-05: *"if all
	 *  rooms have the connections, app doesn't suggest to open the doors
	 *  anymore at all"*).
	 *
	 *  Rust's `advisor::convenience` owns the ranking — Entrance → Apex,
	 *  Entrance → the wanted rooms, the wanted rooms → Apex, then the longest
	 *  open loop — and every reason it is null, RU's veto included. Exclusive
	 *  with `secondaryDoor` by construction: that one needs a primary door to
	 *  be second to, this one needs there to be none. The overlay draws it
	 *  with the SAME faint seal (`faintDoor()` in `view.ts`), because it is
	 *  the same kind of statement: not the move, but what to do with a key the
	 *  move has no use for.
	 *
	 *  OPTIONAL on the wire for the same reason `secondaryDoor` is;
	 *  `convenienceDoor()` / `convenienceNote()` coerce `undefined` to null. */
	convenience?: ConvenienceView | null;
	/** The room the top recommendation's door opens into, named by RUST
	 *  (POE-261, owner: *"put the name of the exit (the next room) to the solid
	 *  purple exit (only to that recommended one)"*).
	 *
	 *  `slice.rs`'s `recommended_exit` is the ONE place the name is decided,
	 *  and this side must not second-guess it: the name depends on the far
	 *  plate's READ TIER, so deriving one here from `layout.slots` would be a
	 *  second answer to what the board says is behind that door — the same rule
	 *  `secondaryDoor` and `convenience` keep about the doors themselves.
	 *
	 *  Null is an ANSWER: the move opens no door, or the plate behind it did
	 *  not resolve. The widget then draws the purple seal unlabelled rather
	 *  than guessing.
	 *
	 *  OPTIONAL on the wire for the reason `secondaryDoor` is;
	 *  `recommendedExit()` in `view.ts` coerces `undefined` to null. */
	recommendedExit?: ExitLabelView | null;
	/** `"continue"` or `"leaveMap"` — R5's verdict for the top recommendation.
	 *  Note the camelCase: `MapAction` is projected through a hand-written
	 *  `match`, not through `rename_all`, so this one string is NOT snake_case
	 *  like `TempleStatus`. Never `"leaveMap"` while `R5_WITHHELD` in `slice.rs`
	 *  holds (owner, 2026-09-07); `leaveMapBanner()` stays for the day it is lifted. */
	mapAction: string;
	warnings: string[];
	/** Whether the kill on the top recommendation is the ONLY kill the read saw
	 *  — the panel prints two architect blocks and only one was read (POE-243).
	 *
	 *  The typed half of the partial-read warning: `warnings` carries the prose
	 *  a surface PRINTS, this carries the fact a surface BRANCHES on, so no
	 *  surface has to recognise a warning by its wording. False when nothing
	 *  was read at all — there is then no kill on screen to call forced. */
	forcedKill: boolean;
}

/**
 * A remembered anchor scale for one capture size.
 *
 * snake_case keys — see the file header. `AnchorCalibration` carries no
 * `rename_all` in Rust.
 */
export interface AnchorCalibration {
	screen_w: number;
	screen_h: number;
	scale: number;
}

/** The two config flags. camelCase, from `TempleConfig`'s own `rename_all`. */
export interface TempleConfig {
	/** Atlas passive: *"Your Maps with Incursions always have four Incursions"*. */
	artefactsOfTheVaal: boolean;
	/** The Incursion Scarab of Timelines requires finishing every incursion in
	 *  the map, which takes R5 (leave the map) away. */
	scarabOfTimelines: boolean;
}

/** Which room valuation is in force (POE-257). `snake_case` wire strings, like
 *  `TempleStatus` and unlike the camelCase FIELDS around them. */
export type TemplePreset = 'default' | 'custom';

/** The Custom preset's rates and per-room chaos overrides.
 *
 *  Persisted separately from the preset choice, and echoed onto the slice
 *  whether or not Custom is in force: the table has to survive periods of not
 *  being used, or looking at Default once would lose the player's numbers. */
export interface TempleCustom {
	/** What a tier-1 or tier-2 room is worth as a fraction of its line's
	 *  tier-3 total (epic lock L2's 80 %). */
	tierFraction: number;
	/** Chaos per point of `increased Quantity of Items found in this Area`. A
	 *  guess — nobody has measured a rate. */
	cPerQuantity: number;
	/** Chaos per point of `increased Rarity of Items found in this Area`. Same
	 *  standing as `cPerQuantity`. */
	cPerRarity: number;
	/** Expected vials per run at tier 3 on a line whose raw poedb vial chance
	 *  is the anchor (Conduit, Crucible, Sanctum). Every other line scales off
	 *  it by that stat — Glittering Halls ×1.67, Locus and Throne ×0.012.
	 *  Vertolka's proposed 0.1; `0` removes every vial term. */
	vialsPerRun: number;
	/** A global multiplier on the whole drops term. A rusher who never opens a
	 *  chest sets it to 0, which reduces the ranking to sale value alone. */
	dropsWeight: number;
	/** What the Locus + Doryani pair is worth ON TOP of the sum of the two
	 *  rooms. 0 by default: the pair is the sum of its two above-floor deltas
	 *  and no more. */
	comboPremium: number;
	/** Per-room-tier chaos overrides, keyed by the room LINE's key, tier 1
	 *  first. `null` in a slot means "use the computed value for this tier". */
	rooms: Record<string, (number | null)[]>;
}

/** One room LINE's three tier values, as `temple_value_table` hands them over
 *  (POE-259).
 *
 *  NOT part of `TempleSlice`, and deliberately: the preset editor is the only
 *  thing that wants all 75 values and it wants them while it is open, so
 *  publishing them on every SSOT snapshot would put a payload nothing reads on
 *  the poll that carries the board. `templeValueTable()` asks for them.
 *
 *  The cells are the same `RoomValueView` an offer box carries, built by the
 *  same Rust projection, so a number in the editor and a number on the board
 *  cannot disagree about what a room is worth. */
export interface TempleValueRow {
	/** The room LINE's key — the string `TempleCustom.rooms` keys its
	 *  overrides by, which is what makes a cell addressable. */
	key: string;
	/** The TIER-3 room's name: what the player calls the family, and what
	 *  Vertolka graded. The tier-1 name is only its root. */
	name: string;
	/** Vertolka's letter for the line — what prices a room nothing else
	 *  priced. */
	grade: string;
	/** Tier 1, tier 2, tier 3, in that order. */
	tiers: [RoomValueView, RoomValueView, RoomValueView];
}

/** The four tunable fields of the strategy profile. */
export interface TempleProfile {
	/** What the Apex is worth on its own, **in units where the top tier-3 room
	 *  is worth 9** — relative, not chaos.
	 *
	 *  2.0 means "the Apex is worth a bit under a quarter of the best room on
	 *  the board", a judgement that survives the currency moving and a league
	 *  ending. Rust multiplies it by the live valuation's scale, so the same
	 *  2.0 is 188 c beside a top room worth 846. A control for this field must
	 *  say so: a slider reading "2" next to three-figure chaos room values is a
	 *  slider nobody can set (POE-259). */
	apexScore: number;
	/** Run-time traversal weight per BFS hop from the Entrance, in the SAME
	 *  relative units as `apexScore`: 9 is the top tier-3 room. 0 for the
	 *  Rush. */
	pathCost: number;
	/** Prefer `change` over `upgrade` while no favourable line exists. */
	rerollUntilFavourable: boolean;
	/** R4's carve-out: keep a slot in the drop pool while an adjacent upgrade
	 *  room can still hit it. */
	r4KeepUpgradeTargets: boolean;
}

/**
 * What the prices behind a board are worth saying (POE-258).
 *
 * `marketNote()` in `view.ts` is the one place that turns it into words. The age
 * is a TIMESTAMP and not a rendered string on purpose — a "12 min old" composed
 * in Rust would stand frozen until the next poll five minutes later, while the
 * page re-derives it from the clock on every render. `staleAfterMs` rides along
 * for the same reason: the age moves between publishes, so the line it crosses
 * has to travel with it or the page could only re-derive half the sentence.
 *
 * The slice carries TWO of these and they answer different questions:
 * `TempleSlice.market` is the read on screen, `TempleSlice.pollMarket` is the
 * latest poll.
 */
export interface MarketView {
	/** Unix ms of the server's last observation, or null when nothing has been
	 *  read.
	 *
	 *  Set whether or not the read is `stale`, which is the difference from
	 *  `RoomValueView.asOf` (null on a stale read, because a stale read priced
	 *  the room at nothing). Here the age IS the message. */
	asOf: number | null;
	/** Rust's staleness answer at the moment this view was published.
	 *
	 *  `marketStale()` prefers the CLOCK whenever `asOf` is set — that is what
	 *  ages a board left on screen into `prices stale (3 h)` with no republish
	 *  behind it — and falls back to this flag only for a view carrying no
	 *  observation to measure. */
	stale: boolean;
	/** How old an observation may be before it stops pricing, in ms — Rust's
	 *  `market::STALE_AFTER_MS` (two hours), published so this side re-judges
	 *  `stale` against the same line rather than hard-coding it a second time.
	 *
	 *  A payload from a build before the field falls back to the constant on
	 *  both sides (Rust's `serde(default = ...)`, `normaliseTemple` here) rather
	 *  than to zero, which would read as "every observation is instantly stale"
	 *  — a claim about the market that an absent field is not making. */
	staleAfterMs: number;
	/** Whether the board WAS on the preset's base values when it was valued.
	 *  Every way to get there at once — nothing polled yet, a cold server, a
	 *  payload from another league, a stale read, an unusable floor — because
	 *  the player's question is whether the number is a price or a ladder rung
	 *  and the answer is the same in all five.
	 *
	 *  Read as a fact about the NUMBERS and not about the age, which is what
	 *  gates `marketNote`'s `— base values` suffix: a read that priced live and
	 *  has merely aged past `staleAfterMs` since carries `unavailable: false`
	 *  and gets no suffix, because its figures are real prices gone old. */
	unavailable: boolean;
}

/** The `temple` SSOT slice. Rust-owned; read-only in the webview. */
export interface TempleSlice {
	status: TempleStatus;
	/**
	 * Whether Alva has started an incursion the module has not yet found the
	 * temple sheet for (POE-249).
	 *
	 * The only new lifecycle state, and a flag rather than a status because the
	 * lifecycle in `docs/TEMPLE-LIFECYCLE.md` is spelled by three fields
	 * together: `waiting` is this flag, `reading`/`read` are `status`, and
	 * `playing` is an advice that stands while the sheet is shut. Rust sets it
	 * on one of the three measured START phrases and clears it on any other
	 * Alva line, a zone change, a completed read, a sighting, a stand-down and
	 * the module being switched off.
	 *
	 * The notice's own gate is `overlayShowsWaiting`, not this field alone.
	 */
	waitingForPanel: boolean;
	layout: LayoutView | null;
	panel: PanelView | null;
	/** Null whenever there is no decision to make — no board, or no current
	 *  room. Dropped, along with `mode`, when the module is switched off. */
	advice: AdviceView | null;
	/** `"chase"` or `"scarab"`, from the profile's own selector. */
	mode: string | null;
	/** The config flags in force. Settings, not a reading: they survive the
	 *  module being off. */
	config: TempleConfig;
	/** The four tunable profile fields in force. Same ownership as `config`. */
	profile: TempleProfile;
	/** Which valuation preset is in force. Same ownership as `config`. */
	preset: TemplePreset;
	/** The Custom preset's rates and overrides, echoed whether or not Custom is
	 *  the preset in force — the page has to be able to show what switching
	 *  would give back. */
	custom: TempleCustom;
	/** The prices THIS board was valued against, or the absence of them.
	 *
	 *  Written by `slice::project` alone, from the very market read the board's
	 *  own valuation used, so a surface showing this board's numbers and this
	 *  line can never be made to disagree with itself by something that happened
	 *  after the read. Every per-read surface — the offer boxes above all —
	 *  reads THIS field. */
	market: MarketView;
	/** The prices the NEXT read will be valued against — the latest poll.
	 *
	 *  Written by `ssot::publish_market_view` alone, on every poll and on a
	 *  server-URL change, so it moves with nobody looking at a temple. The
	 *  Temple page's reader meta row reads this one: its question is "what is
	 *  the app priced against right now", not "what was that board priced
	 *  against". Seeded from the read's own market by `project`, which is exact
	 *  — at the moment of a read the two are the same view. */
	pollMarket: MarketView;
	/** Slots whose plate did not resolve, by key. Surfaced, never hidden. */
	unknownRooms: SlotId[];
	/** Unix ms of the last completed read. */
	lastReadAt: number | null;
	calibration: AnchorCalibration | null;
	/**
	 * Something the last read could not do, worded as a WARNING.
	 *
	 * Today: a text OCR region that fell entirely outside the capture, which
	 * produces an empty panel read that looks exactly like a panel with nothing
	 * printed on it. Deliberately not `lastError` — that belongs to the
	 * status/message machine and is rendered in red as "Last error", and a read
	 * that completed and published a board is not a failure. Rust sets and
	 * clears it in `slice::project`, so it describes the LAST read and never
	 * outlives it.
	 */
	readNotice: string | null;
	lastError: string | null;
}

/**
 * What the store shows before Rust has answered a poll.
 *
 * Every value here is `TempleSlice::default()`'s, pinned character for
 * character by the Rust side and re-asserted in `slice.test.ts`: the derive
 * default is what a window sees before `apply_to_state` seeds the echo, and a
 * value invented here would be one Rust never sends.
 */
export function templeSliceDefault(): TempleSlice {
	return {
		status: 'idle',
		waitingForPanel: false,
		layout: null,
		panel: null,
		advice: null,
		mode: null,
		config: { artefactsOfTheVaal: true, scarabOfTimelines: false },
		profile: {
			apexScore: 2.0,
			pathCost: 0.0,
			rerollUntilFavourable: false,
			r4KeepUpgradeTargets: true
		},
		preset: 'default',
		custom: {
			tierFraction: 0.8,
			cPerQuantity: 0.5,
			cPerRarity: 0.25,
			vialsPerRun: 0.1,
			dropsWeight: 1.0,
			comboPremium: 0.0,
			rooms: {}
		},
		// `unavailable: true` is the Rust default too, and it is the honest one:
		// a window that has not yet been answered by a poll is showing base
		// values, not prices. `staleAfterMs` is Rust's `STALE_AFTER_MS`, which
		// `MarketView::default()` carries because it is a constant and not a
		// reading.
		market: { asOf: null, stale: false, staleAfterMs: 7_200_000, unavailable: true },
		pollMarket: { asOf: null, stale: false, staleAfterMs: 7_200_000, unavailable: true },
		unknownRooms: [],
		lastReadAt: null,
		calibration: null,
		readNotice: null,
		lastError: null
	};
}

/** One timed step of a debug capture. */
export interface TempleDebugTiming {
	label: string;
	ms: number;
}

/**
 * What `temple_debug_capture` returns.
 *
 * A summary by design — the full detail is in `report.json` under `dumpDir`,
 * which is what a bug report attaches. Mirrors
 * `src-tauri/src/temple/commands.rs::TempleDebugReport`.
 */
export interface TempleDebugReport {
	dumpDir: string;
	/** `"screen"`, or the path of the image that was read instead. */
	source: string;
	screen: [number, number];
	anchored: boolean;
	scale: number | null;
	ncc: number | null;
	confidence: string | null;
	current: SlotId | null;
	/** `[x, y, w, h]` — the diamond rect this build used. Since POE-230 all
	 *  three rects below are placed from the Entrance origin and the anchor's
	 *  scale, so a wrong one is a wrong anchor or a constant to re-measure. */
	diamondRect: [number, number, number, number] | null;
	panelRect: [number, number, number, number] | null;
	remainingRect: [number, number, number, number] | null;
	markerError: string | null;
	ocrLines: number;
	unknownRooms: SlotId[];
	timings: TempleDebugTiming[];
	/** Only files that reached the disk — the report treats this as a claim. */
	files: string[];
	notes: string[];
}
