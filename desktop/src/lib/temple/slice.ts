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
	/** Where the block sits on screen — the union of the boxes of the OCR lines
	 *  it was read from (POE-243). Null when the read carried no boxes, which
	 *  is what a surface must test before drawing: a missing rect is not the
	 *  screen origin. */
	rect: CaptureRect | null;
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
	/** `"continue"` or `"leaveMap"` — R5's verdict for the top recommendation.
	 *  Note the camelCase: `MapAction` is projected through a hand-written
	 *  `match`, not through `rename_all`, so this one string is NOT snake_case
	 *  like `TempleStatus`. */
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

/** The four tunable fields of the strategy profile. */
export interface TempleProfile {
	/** What the Apex is worth on its own. */
	apexScore: number;
	/** Run-time traversal weight per BFS hop from the Entrance. 0 for the Rush. */
	pathCost: number;
	/** Prefer `change` over `upgrade` while no favourable line exists. */
	rerollUntilFavourable: boolean;
	/** R4's carve-out: keep a slot in the drop pool while an adjacent upgrade
	 *  room can still hit it. */
	r4KeepUpgradeTargets: boolean;
}

/** The `temple` SSOT slice. Rust-owned; read-only in the webview. */
export interface TempleSlice {
	status: TempleStatus;
	layout: LayoutView | null;
	panel: PanelView | null;
	/** Null whenever there is no decision to make — no board, or no current
	 *  room. Dropped, along with `mode`, when the module is switched off. */
	advice: AdviceView | null;
	/** `"chase"` or `"scarab"`, from the profile's own selector. */
	mode: string | null;
	/** The user's key count, echoed so a surface renders its control from one
	 *  source. Settings, not a reading: it survives the module being off. */
	keys: number;
	/** The config flags in force. Same ownership as `keys`. */
	config: TempleConfig;
	/** The four tunable profile fields in force. Same ownership as `keys`. */
	profile: TempleProfile;
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
 * character by the Rust side and re-asserted in `slice.test.ts`. In particular
 * `keys` is **0**, not the shipped 1: the derive default is what a window sees
 * before `apply_to_state` seeds the echo, and claiming 1 here would be a
 * different number from the one Rust would send.
 */
export function templeSliceDefault(): TempleSlice {
	return {
		status: 'idle',
		layout: null,
		panel: null,
		advice: null,
		mode: null,
		keys: 0,
		config: { artefactsOfTheVaal: true, scarabOfTimelines: false },
		profile: {
			apexScore: 2.0,
			pathCost: 0.0,
			rerollUntilFavourable: false,
			r4KeepUpgradeTargets: true
		},
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
