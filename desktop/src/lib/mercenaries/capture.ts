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
 * The running four are the trigger's own states (POE-198 + the 2026-08-25
 * smoke): `idle` is waiting for a mercenary and runs NO OCR at all, `scanning`
 * is an armed burst looking for the window, `live` has one and is still reading
 * it, and `done` has one that is fully read — the re-detect is PAUSED down to a
 * 10 s liveness check, while the hover tick keeps running so a tooltip can
 * still correct a confident wrong match. `idle` therefore does not mean
 * "watching", and `done` does not mean the window is gone: a `done` capture is
 * on screen exactly as a `live` one is.
 */
import type { MercTradeResult } from '$lib/tradeApi';

export type MercStatus = 'off' | 'idle' | 'scanning' | 'live' | 'done' | 'unavailable';

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

/**
 * How the last shared-pool pull ended (POE-201).
 *
 * `failed` is not an error state for the module — the pool is an optimisation,
 * and a device that cannot reach it runs on its own templates exactly as it did
 * before the pool existed.
 */
export type MercPullResult = 'never' | 'merged' | 'unchanged' | 'failed';

/** What the page says about the shared icon-template pool (POE-201). */
export interface MercSyncStatus {
	/** Unix ms of the last finished pull, or null when none has finished. */
	lastPullMs: number | null;
	lastPull: MercPullResult;
	/** Samples in the store that came from the pool rather than from a hover. */
	pooledSamples: number;
	/** Local samples still waiting to be offered to the pool. */
	queuedUploads: number;
	/**
	 * Why the last pool call failed. Deliberately separate from the slice's own
	 * `lastError`: a pool the app cannot reach is not a capture failure, and
	 * showing it as one would send the user looking for an OCR problem.
	 */
	lastError: string | null;
}

/**
 * Where the captured mercenary's own trade search stands (POE-202).
 *
 * Wire strings, not Rust variant names: the Rust enum carries
 * `#[serde(rename_all = "kebab-case")]`, so `waiting-league` is spelled with a
 * hyphen here. Unlike `MercStatus` and `ReadState` — pinned from the other side
 * by `mercenary/mod.rs`'s `every_read_state_and_status_wire_string_is_pinned` —
 * these strings have no Rust-side pin yet, so a `rename_all` drop would leave
 * both sides compiling while the badge goes blank.
 *
 * `off` is not one of the search's own states — it is the module being off,
 * written by `compose_snapshot` over whatever the trade state held. Only the
 * status is forced: a retained `result`/`url` still renders on `off` (like the
 * capture and verdict cards), the badge says "module off", and the two
 * settings stay browsable (ADR-014).
 */
export type MercTradeStatus =
	| 'off'
	| 'idle'
	| 'waiting-league'
	| 'queued'
	| 'searching'
	| 'done'
	| 'error';

/**
 * The trade half of the merc slice — Rust-owned like everything else here.
 *
 * `queryHash` is the identity `result` and `url` answer for, never "the newest
 * hash the loop has seen": Rust discards a lookup that comes back carrying a
 * different hash, so a state whose `result` is set is always describing the
 * query named by `queryHash`.
 */
export interface MercTradeState {
	status: MercTradeStatus;
	/** The capture identity `result` and `url` answer for. */
	queryHash: string | null;
	/** The trade-site link for that query, or null until the league resolves. */
	url: string | null;
	result: MercTradeResult | null;
	/** Why the last lookup failed. Set only alongside `status: 'error'`. */
	error: string | null;
	/** Searches this capture session has spent, out of Rust's ceiling of 3. */
	searchesUsed: number;
}

export interface MercenarySlice {
	status: MercStatus;
	/** The last capture, live or retired. Null until the module has seen one. */
	capture: MercCapture | null;
	/** Icon families the template store has learned, from hover confirmations. */
	learnedFamilies: string[];
	/**
	 * The subset of `learnedFamilies` this device knows only from the shared
	 * pool — no local hover taught them (POE-201). Same `"<family>--<tier>"`
	 * shape, so one parse serves both lists.
	 */
	pooledFamilies: string[];
	/**
	 * Families the store knows only from the gem art it seeded itself with
	 * (POE-208) — FAMILY NAMES, not `"<family>--<tier>"` keys, because a seed is
	 * installed once per family under the family's lowest vocabulary tier and
	 * that tier is the store's business, not a read the player made.
	 *
	 * Not a subset of `learnedFamilies`: a hover confirm of a seeded family
	 * stores a local sample beside the seed, so the same family legitimately
	 * appears in both lists and gets a chip in both groups — one ✕ per thing
	 * that can be removed. The seed's own door is `merc_forget_seed(family)`.
	 *
	 * Declared REQUIRED, like `pooledFamilies`, because that is what every
	 * publisher of this slice sends. A Rust build older than POE-208 omits it,
	 * and `templateGroups` (`capture-view.ts`) defends against that at runtime
	 * rather than the type carrying an optionality no current writer produces —
	 * the store applies the slice WHOLE, so there is no field-wise place to
	 * default it either.
	 */
	seededFamilies: string[];
	lastError: string | null;
	/**
	 * Who the module HEARD, for the burst it is scanning under (2026-08-25).
	 *
	 * Non-null only alongside `scanning`, and only for a Client.txt burst —
	 * Scan now names nobody. Rust writes it in the same publish that arms the
	 * status, which is what lets the strip say "heard Fennik, of Unshakeable
	 * Faith · scanning…" the moment the voice line lands.
	 */
	burstSpeaker: string | null;
	/** Whether `merc-geometry.json` overrode the built-in reference numbers. */
	geometrySource: MercGeometrySource;
	/**
	 * The guides taking NO part in the verdict (POE-199) — Rust's settings echo,
	 * composed onto the slice at read time from `AppState.merc_sources_off`.
	 *
	 * It lives here rather than in the ADR-013 prefs map because TWO windows
	 * read it: the page and the verdict overlay evaluate the same capture, and
	 * a prefs map fetched once per webview left them free to disagree about one
	 * mercenary. Write it through `setMercSourcesOff` (`stores/ssot.svelte.ts`),
	 * never locally.
	 */
	sourcesOff: string[];
	/**
	 * The shared template pool's state (POE-201) — Rust's echo, composed onto
	 * the slice at read time from `AppState.merc_sync` for the same reason
	 * `sourcesOff` is: the pull and the uploader are tasks, not the capture
	 * loop, so the slice keeps a single writer.
	 */
	sync: MercSyncStatus;
	/**
	 * The captured mercenary's own trade search (POE-202).
	 *
	 * Unlike `sourcesOff` and `sync` this is STORED on the Rust slice rather
	 * than composed from settings at read time — it is the trigger's own state
	 * machine, not a settings echo. The overlay is expected to render it later,
	 * which is why it reaches every window through the slice instead of a
	 * page-local fetch.
	 */
	trade: MercTradeState;
	/** Whether the auto-search runs at all — Rust's `merc_trade_auto` echo,
	 *  composed like `sourcesOff`. Write it through `setMercTradeAuto`. */
	tradeAuto: boolean;
	/** How far below the read tier the query comps — Rust's `merc_tier_floor`
	 *  echo. 3 is the mercenary exactly as read. Write it through
	 *  `setMercTierFloor`. */
	tierFloor: 1 | 2 | 3;
}

/** What the store shows before Rust has answered a poll. */
export function mercenarySliceDefault(): MercenarySlice {
	return {
		status: 'off',
		capture: null,
		learnedFamilies: [],
		pooledFamilies: [],
		seededFamilies: [],
		lastError: null,
		burstSpeaker: null,
		geometrySource: 'default',
		sourcesOff: [],
		sync: {
			lastPullMs: null,
			lastPull: 'never',
			pooledSamples: 0,
			queuedUploads: 0,
			lastError: null
		},
		trade: {
			status: 'off',
			queryHash: null,
			url: null,
			result: null,
			error: null,
			searchesUsed: 0
		},
		tradeAuto: true,
		tierFloor: 3
	};
}
