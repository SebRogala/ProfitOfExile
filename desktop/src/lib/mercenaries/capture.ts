/**
 * TypeScript mirror of the Rust `mercenary` SSOT slice.
 *
 * The Rust structs (`src-tauri/src/mercenary/mod.rs`, published through
 * `ssot.rs`) are the owners of this shape — nothing in the webview writes it.
 * Field names are camelCase because the Rust structs carry
 * `#[serde(rename_all = "camelCase")]`; the two enums are `snake_case` on the
 * wire, so their unions below spell the wire strings, not the Rust variant
 * names. A Rust serde test pins the same strings from the other side: if these
 * two ever disagree, one of the two tests is what fails, not the page.
 *
 * There is no constructor for a capture here on purpose — a capture only ever
 * arrives from Rust. `mercenarySliceDefault()` exists because the store needs
 * something to show before the first poll answers.
 */

/**
 * Whether the OCR module is running and whether it can see a recruit window.
 *
 * `off` (module disabled) and `unavailable` (non-Windows, or no OCR engine)
 * outrank the three running states: the page treats `status` as authoritative
 * over `capture.live`, because a capture is left behind with `live: false`
 * when the window goes away and is NOT cleaned up when the app exits.
 *
 * The running three are the trigger's own states (POE-198): `idle` is waiting
 * for a mercenary and runs NO OCR at all, `scanning` is an armed burst looking
 * for the window, `live` has one. `idle` therefore does not mean "watching".
 */
export type MercStatus = 'off' | 'idle' | 'scanning' | 'live' | 'unavailable';

/**
 * How much the reader trusts one skill name or one support cell.
 *
 * `matched` (OCR/template above threshold) and `confirmed` (the user hovered
 * the cell and the tooltip agreed) are the two confident states. The other
 * three are all "not read": `low_confidence` below threshold, `unknown` no
 * candidate at all, `ambiguous` a name that resolves to several stat ids.
 */
export type ReadState = 'matched' | 'low_confidence' | 'unknown' | 'confirmed' | 'ambiguous';

/** Recruit-window header. Every field is best-effort — missing means missing, never guessed. */
export interface MercHeader {
	name: string | null;
	class: string | null;
	level: number | null;
	wager: number | null;
}

/** One skill row's name read. `ids` is a set: one display name can resolve to several stat ids. */
export interface MercSkillRead {
	/** The OCR text as read, kept even when it matched nothing. */
	raw: string;
	ids: string[];
	name: string | null;
	score: number;
	state: ReadState;
}

/** One support cell of a skill row. */
export interface MercSupportRead {
	slot: number;
	/** `[x, y, w, h]` in screen pixels — the cell the reader looked at. */
	rect: [number, number, number, number];
	/** Icon family the template store matched, independent of the tier badge. */
	family: string | null;
	tier: number | null;
	ids: string[];
	name: string | null;
	score: number;
	state: ReadState;
	/** Names in play when `state` is `ambiguous`; empty otherwise. */
	candidates: string[];
}

export interface MercRow {
	index: number;
	skill: MercSkillRead;
	supports: MercSupportRead[];
}

export interface MercCapture {
	capturedAtMs: number;
	live: boolean;
	/** Runtime pixels per reference pixel, derived per capture from the row pitch. */
	scale: number;
	/** `[width, height]` of the screen the capture was taken from. */
	screen: [number, number];
	header: MercHeader;
	rows: MercRow[];
}

/** Where the runtime geometry came from — the debug report names the same two. */
export type MercGeometrySource = 'default' | 'file';

export interface MercenarySlice {
	status: MercStatus;
	/** The last capture, live or retired. Null until the module has seen one. */
	capture: MercCapture | null;
	/** Icon families the template store has learned, from hover confirmations. */
	learnedFamilies: string[];
	lastError: string | null;
	/** Whether `merc-geometry.json` overrode the built-in reference numbers. */
	geometrySource: MercGeometrySource;
}

/** What the store shows before Rust has answered a poll. */
export function mercenarySliceDefault(): MercenarySlice {
	return {
		status: 'off',
		capture: null,
		learnedFamilies: [],
		lastError: null,
		geometrySource: 'default'
	};
}
