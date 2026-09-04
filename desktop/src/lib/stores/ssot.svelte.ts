/**
 * App-wide cross-window SSOT store (POE-128 chunk 4) — webview delivery layer.
 *
 * Delivery to overlay windows is Rust-backed **polling** of the `get_ssot`
 * command, NOT reliance on the `ssot-changed` JavaScript event: WebView2
 * cross-window events return stale data / fail silently (see
 * docs/OVERLAY-GUIDE.md "Runtime-earned observations"). The `ssot-changed`
 * listener here is only an *optional eager nudge* that triggers an immediate
 * `get_ssot` re-fetch — its payload is never trusted as truth.
 *
 * League is low-churn, so the poll interval is lazy (seconds), not the
 * comparator overlay's 500 ms.
 *
 * The farming market (normal variant, dedication variant, dedication pool) is
 * owned here too (POE-163): every surface reads the same rune and writes through
 * the exported setters, so there is no per-component mirror to drift.
 *
 * The module enabled flags (POE-128) are the same deal, with dynamic keys: the
 * ids come from the Rust registry (src-tauri/src/modules.rs), so the slice is a
 * record rather than named fields. It reports **intent, not liveness** — a
 * module that panicked still reports enabled.
 *
 * Usage:
 *   import { ssot, startSsotStore } from '$lib/stores/ssot.svelte';
 *   // Read: ssot.league  (string | null; null until first successful get_ssot)
 *   // Read: ssot.normalVariant / ssot.dedicationVariant / ssot.dedicationPool
 *   // Read: ssot.modules['mercenary'] ?? false  (absent key = not yet known)
 *   // Read: ssot.mercenary  (Merc OCR status, last capture, enabled-guide echo)
 *   // Read: ssot.temple  (temple board, advice and settings echo; no direct write)
 *   // Write: setNormalVariant(v) / setDedicationSelection(variant, pool)
 *   // Write: setModuleEnabled(id, enabled)
 *   // Write: setMercSourcesOff(offList) — which guides take part in the verdict
 *   // Write: setTempleKeys(n) / setTempleConfig(c) / setTempleProfile(p) / rearmTemple()
 *   //   (these four return the rejection message instead of throwing — Rust
 *   //    validates them, so the page renders what it said no to)
 *   // Main window: call startSsotStore() top-level (like initStatusStore()).
 *   // Overlay windows: call startSsotStore() from an $effect and return its
 *   //   cleanup (onMount is unreliable in overlay windows).
 */
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { DEDICATION_VARIANTS, VARIANTS } from '$lib/api';
import { mercenarySliceDefault, type MercenarySlice } from '$lib/mercenaries/capture';
import {
	isTempleStatus,
	templeSliceDefault,
	type TempleConfig,
	type TempleDebugReport,
	type TempleProfile,
	type TempleSlice
} from '$lib/temple/slice';

/**
 * The dedication pools, in Rust/settings/DB canon spelling (singular `skill`).
 * `api.ts` has no pool constant — the JSON API uses the plural `skills` for the
 * same pool, and translating between the two is the consumer's job.
 */
const POOLS = ['skill', 'transfigured'] as const;

/**
 * Which cue measured `ScreenSlice.uiScale` (POE-214) — the wire strings Rust's
 * `ScreenScaleSource` serialises to, pinned from the Rust side by a serde test.
 *
 * A statement about the CUE, not a rank. Rust reads it to decide whether a new
 * measurement may replace the standing one (`ssot::accepts`, POE-240): a reading
 * that agrees with the standing value inside the drift band is refused rather
 * than ranked below it, whether it came from the merc OCR line pitch or — since
 * POE-234 — from a temple anchor converted through the two units' coefficient.
 * Nothing here does that arithmetic — the webview reads the slice Rust settled
 * on.
 */
export type ScreenScaleSource = 'merc-frame' | 'merc-ocr' | 'temple-anchor' | 'remembered';

/**
 * The screen the game is drawn on and the game-UI scale measured on it
 * (POE-214). TypeScript mirror of Rust's `ScreenSlice`.
 *
 * **Unit.** `uiScale` is game-UI px per px of the reference fixture: a
 * 1920x1200 screen is 1.0 by definition, and 1080p measures 0.90 = 1080/1200 —
 * the game's UI scales with screen HEIGHT. The temple's
 * `AnchorCalibration.scale` is a DIFFERENT unit (relative to its own 1374-px
 * reference width), so the two numbers must not be substituted for each other.
 * The ratio between them is Rust's `temple::anchor::TEMPLE_SCALE_PER_UI_SCALE`,
 * good to about a per cent, and Rust is the only side that converts through it.
 *
 * **Reader rule.** A non-merc consumer reads THIS slice, never
 * `ssot.mercenary.capture.scale`. Both are written from the same settled scale
 * on the same Rust tick, so they never disagree — but the capture is null until
 * a recruit window opens and is retired again when it closes, so a reader keyed
 * on it would lose the screen's scale every time the player shuts the window.
 *
 * Rust-owned and read-only here. Projected onto `ssot.screen` for the Settings
 * "Screen geometry" card (POE-227). Both READERS POE-214 named now exist in
 * Rust: the Lab `CaptureRegion`s (POE-233) and the temple, which reads this as
 * its anchor hint and writes back what it anchors (POE-234). The lifecycle —
 * what is remembered, what verifies it, and the three events that drop it — is
 * normative in `desktop/src/lib/README.md` → "Screen Geometry (SSOT)".
 */
export interface ScreenSlice {
	/** Captured screen width in physical px. */
	width: number;
	/** Captured screen height in physical px. */
	height: number;
	/** Game-UI px per reference-fixture px — see the unit note above. */
	uiScale: number;
	/** What measured `uiScale`. A label, not a precedence. */
	source: ScreenScaleSource;
	/** Unix ms the measurement was taken at. */
	measuredAtMs: number;
	/**
	 * Whether a cue that VERIFIES the screen produced this value in the CURRENT
	 * app run (POE-240) — the merc gold frame does, the OCR line pitch and a
	 * startup load from settings do not.
	 *
	 * `false` is not "wrong", it is "trusted from last session, unconfirmed".
	 * It is what the lifecycle's remaining blind spot — an in-game UI-scale
	 * change with no verifying panel on screen — surfaces as, since neither the
	 * dimensions nor the display Rust prunes on can see one. (The other blind
	 * spot, a different monitor of the same resolution, is `monitorId`'s since
	 * POE-237.) Never persisted: a restart always starts unverified.
	 */
	verifiedThisSession: boolean;
	/**
	 * WHICH display it was measured on (POE-237) — Rust's `Capture.monitor_id`,
	 * a Win32 `HMONITOR` truncated to 32 bits.
	 *
	 * `0` means UNKNOWN: a scale persisted before POE-237, or a handle that
	 * truncated to zero. Never compare it as an identity without excluding `0`
	 * first — Rust's `ssot::different_monitor` is the rule, and it declines to
	 * answer on a zero.
	 *
	 * NOT the id `availableMonitors()` reports; the two enumerations do not
	 * share an id space, which is why `overlay/monitor-choice.ts` matches a
	 * display on its POSITION.
	 */
	monitorId: number;
	/**
	 * That display's top-left in virtual-desktop PHYSICAL px, as `[x, y]`, so a
	 * rect measured inside a capture can be placed on the desktop. `[0, 0]` for
	 * the primary monitor and for an unknown one — which is why `monitorId`,
	 * not this, is the identity.
	 */
	origin: [number, number];
}

/** Serialized Rust `AppSsotSnapshot` — `league.name` is `string | null`. */
export interface SsotSnapshot {
	league: { name: string | null };
	/** A league resolve is in flight (bounded-retry fetch task running). */
	resolving?: boolean;
	/** The resolver has failed enough times to treat the server as unreachable
	 *  (still retrying). Only meaningful while `resolving` is true. */
	unreachable?: boolean;
	/** Normal-mode farming market, e.g. "20/20". */
	normalVariant?: string;
	/** Dedication-mode farming market, e.g. "21/23". */
	dedicationVariant?: string;
	/** Dedication pool, canon spelling: "skill" | "transfigured". */
	dedicationPool?: string;
	/** Per-module enabled flags, keyed by registry id (Rust owns the keys). */
	modules?: Record<string, boolean>;
	/** Merc OCR module state + the last capture (POE-165). Rust-owned, read-only here. */
	mercenary?: MercenarySlice;
	/** Temple builder module state + the last board read (POE-171). Rust-owned,
	 *  read-only here — the four `setTemple*` writers below go through commands
	 *  that write `temple_settings`, and Rust echoes the result back into this
	 *  slice. Nothing in the webview assigns it except `applyTemple`. */
	temple?: TempleSlice;
	/** The screen and its measured game-UI scale (POE-214). Rust-owned; `null`
	 *  until something has measured one, and a consumer must NOT read `null` as
	 *  1.0 — that is a real measurement (a 1920x1200 screen) and assuming it
	 *  mis-scales every rect on a 1080p machine by 11%. Projected onto
	 *  `ssot.screen`, which the Settings "Screen geometry" card reads (POE-227).
	 *
	 *  **`null`, not absent, is what an unmeasured screen sends.** Rust's
	 *  `Option<ScreenSlice>` carries no `skip_serializing_if`, so the key is
	 *  always on the wire — a consumer must test `== null` (or truthiness), NOT
	 *  `=== undefined`. Same shape and same reason as `MercenarySlice.capture`.
	 *
	 *  The `?` is a TEST-LITERAL affordance, not a wire claim: `resolving`,
	 *  `mercenary` and `temple` are not even `Option` in Rust and carry it for
	 *  the same reason — it is what lets a case build the one-field snapshot it
	 *  is about. Only the `| null` above says anything about the payload. */
	screen?: ScreenSlice | null;
}

/**
 * Lazy poll interval. League is low-churn, so poll slowly — this is NOT the
 * comparator overlay's 500 ms. Keep in the 2000–5000 ms band.
 */
const POLL_INTERVAL_MS = 3000;

/**
 * Reactive store — read `ssot.league`. Mutate the property, never reassign the
 * export. `null` means not-yet-fetched (fail-closed): it stays null until the
 * first successful `get_ssot`.
 */
export const ssot = $state({
	league: null as string | null,
	/** True while Rust is resolving the league (drives the Settings "Resolving…"
	 *  state and neutralises the Refresh button). Defaults false. */
	resolving: false,
	/** True when the resolver has given up reaching the server but is still
	 *  retrying — drives the "Server unreachable" state with an actionable
	 *  Refresh. Defaults false. */
	unreachable: false,
	/** Normal-mode farming market. Every surface that shows or picks the normal
	 *  market reads this field — it is what Rust stamps onto uploaded sessions.
	 *  The initial value matches Rust's default; the first poll corrects it. */
	normalVariant: '20/20',
	/** Dedication-mode farming market. Same ownership as `normalVariant`. */
	dedicationVariant: '21/23',
	/** Dedication pool in canon spelling ("skill" | "transfigured"). */
	dedicationPool: 'skill',
	/** Per-module enabled flags, keyed by the Rust registry id. Empty means
	 *  not-yet-polled, and an absent key means "unknown" — every surface supplies
	 *  its own fallback (`?? false`) rather than trusting a default here, because
	 *  the registry, not this file, owns which modules exist and what they
	 *  default to. Intent, not liveness. */
	modules: {} as Record<string, boolean>,
	/** Merc OCR module state, its last capture and the enabled-guide echo
	 *  (POE-165, POE-199). Rust owns every field; `setMercSourcesOff` writes
	 *  through a command and re-fetches rather than assigning here, so there is
	 *  no optimistic local value and no poll-vs-write guard to keep. Until the
	 *  first poll answers, this is `mercenarySliceDefault()`: module off, no
	 *  capture, every guide on. */
	mercenary: mercenarySliceDefault() as MercenarySlice,
	/** Temple builder module state and its last board read (POE-171). Rust owns
	 *  every field; the setters below never write it, so it needs no
	 *  poll-vs-write guard. Until the first poll answers, this is
	 *  `templeSliceDefault()` — the Rust derive default, pinned by a serde test
	 *  on both sides. */
	temple: templeSliceDefault() as TempleSlice,
	/** The screen and its measured game-UI scale (POE-214), or `null` when
	 *  nothing has measured one. Rust owns it; the only writer here is the
	 *  snapshot apply. `null` must NEVER be read as 1.0 — that is a real
	 *  measurement (a 1920x1200 screen) and assuming it mis-scales every rect on
	 *  a 1080p machine by 11%. Its consumer is the Settings "Screen geometry"
	 *  card (POE-227), which renders the `null` as "not measured yet". */
	screen: null as ScreenSlice | null,
});

/** The three market fields, which share the write-through + poll-guard machinery. */
type MarketField = 'normalVariant' | 'dedicationVariant' | 'dedicationPool';

/**
 * What a poll-vs-write guard record is keyed by.
 *
 * Market fields key on their own field name; module flags key on
 * `module:<id>` because their ids come from Rust at runtime and a bare id could
 * collide with a market field name, letting one slice's write suppress the
 * other's poll.
 */
type GuardKey = MarketField | `module:${string}`;

/** Guard key for one module's enabled flag. */
function moduleKey(id: string): GuardKey {
	return `module:${id}`;
}

/**
 * Poll-vs-write ordering guard.
 *
 * Every write bumps `writeSeq` and stamps it into `lastWriteSeq[key]`;
 * `fetchSsot` captures the counter *before* awaiting `get_ssot` and hands it to
 * `applySnapshot`. A snapshot is then ignored for a key when either:
 *
 *  - `inFlight[key] > 0` — the write round-trip has not settled yet, so the
 *    snapshot cannot contain it; or
 *  - `lastWriteSeq[key] > dispatchedAtSeq` — the snapshot was dispatched
 *    before the write and merely arrived after it. The in-flight clause alone
 *    misses this ordering, because the write can settle first.
 *
 * `inFlight` counts rather than flags: two rapid clicks overlap, and the first
 * response must not unguard the second write.
 *
 * Both records are keyed dynamically (module ids are only known at runtime), so
 * a never-written key is simply absent and reads as zero — the same value the
 * market fields used to be initialised to, which is why generalising the keys
 * leaves market-field behaviour untouched.
 */
let writeSeq = 0;
const lastWriteSeq: Partial<Record<GuardKey, number>> = {};
const inFlight: Partial<Record<GuardKey, number>> = {};

/** Whether a snapshot must leave `key` alone — see the guard doc above. */
function isGuarded(key: GuardKey, dispatchedAtSeq: number): boolean {
	if ((inFlight[key] ?? 0) > 0) return true;
	return (lastWriteSeq[key] ?? 0) > dispatchedAtSeq;
}

const MARKET_DOMAINS: Record<MarketField, readonly string[]> = {
	normalVariant: VARIANTS,
	dedicationVariant: DEDICATION_VARIANTS,
	dedicationPool: POOLS,
};

/**
 * Apply one market field from a snapshot, fail-closed.
 *
 * An absent, empty or out-of-domain value keeps the last known good value —
 * never a hardcoded fallback, which would silently move the farming market to
 * one nobody picked. An out-of-domain *non-empty* value additionally heals the
 * Rust side: the UI refuses to display it, but Rust keeps stamping it onto
 * uploaded font sessions, so it is written back with the value we do show. The
 * write makes the next poll return the healed value, so this branch stops
 * matching by itself — no repeat-suppression flag needed.
 */
function applyMarketField(field: MarketField, incoming: string | undefined, dispatchedAtSeq: number): void {
	if (isGuarded(field, dispatchedAtSeq)) return;
	if (!incoming) return;
	if (MARKET_DOMAINS[field].includes(incoming)) {
		ssot[field] = incoming;
		return;
	}
	console.warn(`[ssot] ignoring unknown persisted ${field}:`, incoming, '— healing Rust to', ssot[field]);
	void writeField(field, ssot[field]);
}

/**
 * Apply the module map from a snapshot, per key.
 *
 * The map is applied key by key rather than assigned wholesale, so an in-flight
 * toggle keeps its optimistic value while every other module in the same
 * snapshot still updates. Within a key the snapshot is the whole truth: an id
 * Rust stopped reporting (module unregistered by a downgrade, settings reset)
 * is dropped, so a stale toggle cannot linger with nothing behind it.
 *
 * An absent map is "not yet known", NOT "no modules" — same fail-closed rule as
 * the market fields. Rust always sends the map (empty at worst), so absence
 * means a malformed or older payload, and wiping the local record on one would
 * blank every toggle for a poll interval.
 */
function applyModules(incoming: Record<string, boolean> | undefined, dispatchedAtSeq: number): void {
	if (!incoming) return;
	for (const [id, enabled] of Object.entries(incoming)) {
		if (isGuarded(moduleKey(id), dispatchedAtSeq)) continue;
		ssot.modules[id] = enabled;
	}
	for (const id of Object.keys(ssot.modules)) {
		if (Object.hasOwn(incoming, id)) continue;
		if (isGuarded(moduleKey(id), dispatchedAtSeq)) continue;
		delete ssot.modules[id];
	}
}

/**
 * Apply the mercenary slice from a snapshot.
 *
 * Taken whole rather than field by field: Rust is the only writer, so a
 * snapshot that carries the slice carries all of it, and merging would let a
 * retired capture's rows survive under a newer capture's header. An ABSENT
 * slice keeps what we have — same fail-closed rule as the market fields, and
 * the only payload that lacks it is a malformed or older one, where blanking
 * the last capture would be a lie about what the reader saw.
 */
function applyMercenary(incoming: MercenarySlice | undefined): void {
	if (!incoming) return;
	ssot.mercenary = incoming;
}

/**
 * Apply the temple slice from a snapshot.
 *
 * Whole, and fail-closed on absence — the same two rules as `applyMercenary`,
 * for the same two reasons. Rust is the only writer, so a snapshot that carries
 * the slice carries all of it, and merging would let a retired board's plates
 * survive under a newer read's advice. An ABSENT slice keeps the last known
 * one: the only payload that lacks it is a malformed or older one, and blanking
 * the board there would be a lie about what the reader saw.
 *
 * No write guard, unlike the market fields: the setters below do not write this
 * rune at all. They invoke a command, Rust echoes the new value onto the slice,
 * and the re-fetch they trigger brings it back — so there is no optimistic
 * local value for a poll to race.
 */
/**
 * The unknown status already reported, so a stuck one is not reported again.
 *
 * A bad status is not a one-off event: whatever produced it is on the other
 * side of a 3-second poll, so the same payload comes back twenty times a
 * minute. One line per distinct value says everything the log can say; the
 * nineteen repeats only bury it. Cleared by the first good payload, so a
 * recurrence after a recovery is reported afresh.
 */
let reportedBadTempleStatus: string | null = null;

function applyTemple(incoming: TempleSlice | undefined): void {
	if (!incoming) return;
	if (!isTempleStatus(incoming.status)) {
		// `status` is the field every surface switches on, and an unrecognised
		// one falls through every branch — the overlay would decide it has no
		// board to draw and the page would render an empty badge, both silently.
		// The last known slice is the honest thing to keep: it is what the
		// reader last actually saw.
		const seen = JSON.stringify(incoming.status) ?? 'undefined';
		if (seen !== reportedBadTempleStatus) {
			reportedBadTempleStatus = seen;
			void sliceCommandFailed(
				'temple',
				'get_ssot',
				`temple slice carried an unknown status ${seen} — payload ignored, keeping the last known board`
			);
		}
		return;
	}
	reportedBadTempleStatus = null;
	ssot.temple = normaliseTemple(incoming);
}

/**
 * Take a payload field by field, so the rune matches its declared type.
 *
 * Whole-replace is preserved exactly: every value comes from `incoming` or from
 * a fresh default, and NOTHING is carried over from the slice being replaced —
 * a board that is gone must not be filled back in from the previous read.
 *
 * What this adds is that a field the payload omits lands as the `null` (or the
 * empty list) the type promises rather than as `undefined`. The types are read
 * as a guarantee by every consumer — `slice.unknownRooms.length` in
 * `unknownRoomsBadge`, drawn on the overlay — and an older or truncated payload
 * used to make that guarantee false.
 */
function normaliseTemple(incoming: TempleSlice): TempleSlice {
	const fresh = templeSliceDefault();
	return {
		status: incoming.status,
		// A build before POE-249 sends no flag at all, and `undefined` here
		// would read as "not waiting" by accident rather than by contract.
		waitingForPanel: incoming.waitingForPanel ?? false,
		// The nested layout is normalised too, and only for the two POE-244
		// fields, because only those two are consumed as a GUARANTEE the way
		// `unknownRooms.length` is: `neverCoverRects` iterates `rois` and
		// `diamondGeometry` reads `corners`, and an older or truncated payload
		// reaching either as `undefined` throws inside an overlay window with
		// no devtools. An absent `rois` normalises to `[]`, which every caller
		// already reads as "place nothing yet" (`overlay-geometry.ts`).
		layout:
			incoming.layout == null
				? null
				: {
						...incoming.layout,
						rois: incoming.layout.rois ?? [],
						// The diamond's own two POE-248 fields go through the
						// same rule, one level deeper: `killGlyphs` draws from
						// them, and a payload from before POE-248 has a diamond
						// with no icon spots on it.
						diamond:
							incoming.layout.diamond == null
								? null
								: {
										...incoming.layout.diamond,
										topIcon: incoming.layout.diamond.topIcon ?? null,
										bottomIcon: incoming.layout.diamond.bottomIcon ?? null
									}
					},
		// The panel's OFFERS go through the same rule one level deeper
		// (POE-249): `grade` and `lineTop` are `serde(default)` on the Rust
		// side, so a payload from a build before them carries neither, and
		// `offerBoxes` tests `grade === null` to decide whether the box prints
		// a rating line at all. `undefined` there is falsy by accident rather
		// than by contract, and it makes the declared `string | null` a lie for
		// anything that later reads it as one. Nothing else on the offer is
		// touched: the rest has been on the wire since POE-243.
		panel:
			incoming.panel == null
				? null
				: {
						...incoming.panel,
						offers: (incoming.panel.offers ?? []).map((offer) => ({
							...offer,
							grade: offer.grade ?? null,
							lineTop: offer.lineTop ?? null
						}))
					},
		advice: incoming.advice ?? null,
		mode: incoming.mode ?? null,
		keys: incoming.keys ?? fresh.keys,
		config: incoming.config ?? fresh.config,
		profile: incoming.profile ?? fresh.profile,
		unknownRooms: incoming.unknownRooms ?? [],
		lastReadAt: incoming.lastReadAt ?? null,
		calibration: incoming.calibration ?? null,
		readNotice: incoming.readNotice ?? null,
		lastError: incoming.lastError ?? null
	};
}

/** Map the Rust snapshot shape (`snap.league.name`, `snap.resolving`,
 *  `snap.unreachable`) into the flat store fields. Missing/malformed fields fail
 *  closed (null / false for league state, last known good for markets).
 *
 *  `dispatchedAtSeq` is the write counter as of the moment this snapshot was
 *  requested; it defaults to "now" so direct callers get no guard suppression. */
export function applySnapshot(snap: SsotSnapshot, dispatchedAtSeq: number = writeSeq): void {
	ssot.league = snap.league?.name ?? null;
	ssot.resolving = snap.resolving ?? false;
	ssot.unreachable = snap.unreachable ?? false;
	applyMarketField('normalVariant', snap.normalVariant, dispatchedAtSeq);
	applyMarketField('dedicationVariant', snap.dedicationVariant, dispatchedAtSeq);
	applyMarketField('dedicationPool', snap.dedicationPool, dispatchedAtSeq);
	applyModules(snap.modules, dispatchedAtSeq);
	applyMercenary(snap.mercenary);
	applyTemple(snap.temple);
	// NOT fail-closed on absence, unlike the two module slices above — this one
	// is deliberately DROPPABLE. `ssot::drop_if_mismatched` clears it on the
	// first capture whose dimensions disagree with the remembered measurement
	// (POE-227), and "keep the last known one" would leave the Settings card
	// showing a scale the app has just thrown away. Rust never omits the key
	// (`Option<ScreenSlice>` carries no `skip_serializing_if`), so an absent one
	// only ever means a payload older than the field.
	ssot.screen = snap.screen ?? null;
}

let pollInterval: ReturnType<typeof setInterval> | null = null;
let unlistenSsot: UnlistenFn | null = null;

/** Fetch the snapshot via the poll-target command and apply it. */
export async function fetchSsot(): Promise<void> {
	// Capture the write counter before awaiting: anything written after this
	// point is newer than whatever the response carries.
	const dispatchedAtSeq = writeSeq;
	try {
		applySnapshot(await invoke<SsotSnapshot>('get_ssot'), dispatchedAtSeq);
	} catch (e) {
		console.warn('[ssot] get_ssot failed:', e);
	}
}

/** Command + argument name backing each market field's write path. */
const MARKET_COMMANDS: Record<MarketField, { command: string; arg: string }> = {
	normalVariant: { command: 'set_normal_variant', arg: 'variant' },
	dedicationVariant: { command: 'set_dedication_variant', arg: 'variant' },
	dedicationPool: { command: 'set_dedication_pool', arg: 'pool' },
};

/**
 * Open the poll-vs-write guard on every key a single user action writes.
 *
 * One `writeSeq` bump for the whole action, so a multi-key action is ordered
 * against polls as one unit rather than as N independent writes.
 */
function beginWrite(keys: readonly GuardKey[]): void {
	writeSeq += 1;
	for (const key of keys) {
		lastWriteSeq[key] = writeSeq;
		inFlight[key] = (inFlight[key] ?? 0) + 1;
	}
}

/** Close the guard opened by `beginWrite`. Always call from a `finally`. */
function endWrite(keys: readonly GuardKey[]): void {
	for (const key of keys) inFlight[key] = (inFlight[key] ?? 0) - 1;
}

/**
 * Write-through one market field: guard, mutate the rune synchronously, then
 * invoke. The synchronous mutation is what makes same-window surfaces update in
 * the same frame instead of waiting a poll interval.
 *
 * Never throws — matches `fetchSsot`'s catch-and-warn contract. On failure the
 * optimistic value stays: the next poll restores Rust truth once the guard
 * clears, and reverting here would flash a value the user did not pick.
 */
async function writeField(field: MarketField, value: string): Promise<void> {
	const { command, arg } = MARKET_COMMANDS[field];
	beginWrite([field]);
	ssot[field] = value;
	try {
		await invoke(command, { [arg]: value });
	} catch (e) {
		console.warn(`[ssot] ${command} failed:`, e);
	} finally {
		endWrite([field]);
	}
}

/** Set the normal-mode farming market. */
export function setNormalVariant(variant: string): Promise<void> {
	return writeField('normalVariant', variant);
}

/** Set the dedication-mode farming market on its own.
 *
 *  Dedication surfaces that pick a market and a pool together must use
 *  `setDedicationSelection` instead — this setter guards only its own field. */
export function setDedicationVariant(variant: string): Promise<void> {
	return writeField('dedicationVariant', variant);
}

/** Set the dedication pool on its own (canon spelling: "skill" | "transfigured").
 *
 *  Same caveat as `setDedicationVariant`: use `setDedicationSelection` when the
 *  user action changes both fields. */
export function setDedicationPool(pool: string): Promise<void> {
	return writeField('dedicationPool', pool);
}

/** The fields `setDedicationSelection` guards and mutates as one unit. */
const DEDICATION_FIELDS: readonly MarketField[] = ['dedicationVariant', 'dedicationPool'];

/**
 * Set the dedication market and pool as one user action.
 *
 * Both fields are guarded and mutated together, then the two Tauri commands are
 * sequenced, and both guards are held until both settle — so no poll tick lands
 * between them and reverts one half. Every Dedication surface writes through
 * this, never through the two single-field setters.
 *
 * What that does NOT buy is atomicity in Rust: the pair is two commands, so a
 * rejected second invoke (IPC failure, command error) leaves Rust holding the
 * new variant with the old pool, `persist_settings` writes that pair to disk,
 * and once the guards clear the next poll settles the UI onto it. Accepted:
 * making the pair atomic means a combined Rust command, and a failure between
 * two local IPC calls has not been observed. The catch below only warns, per
 * `writeField`'s never-throw contract.
 */
export async function setDedicationSelection(variant: string, pool: string): Promise<void> {
	beginWrite(DEDICATION_FIELDS);
	ssot.dedicationVariant = variant;
	ssot.dedicationPool = pool;
	try {
		await invoke(MARKET_COMMANDS.dedicationVariant.command, { variant });
		await invoke(MARKET_COMMANDS.dedicationPool.command, { pool });
	} catch (e) {
		console.warn('[ssot] set dedication selection failed:', e);
	} finally {
		endWrite(DEDICATION_FIELDS);
	}
}

/**
 * Switch a module on or off (POE-128): guard, mutate the rune synchronously,
 * then invoke. Same write-through shape as `writeField`, keyed on the module's
 * guard key.
 *
 * Never throws. Rust rejects an unregistered id with an `Err`, and the invoke
 * can fail on its own; either way the optimistic value stays and the next poll
 * settles the toggle onto Rust truth once the guard clears. A rejected toggle
 * therefore flips back by itself within a poll interval rather than reverting
 * mid-click.
 */
export async function setModuleEnabled(id: string, enabled: boolean): Promise<void> {
	const key = moduleKey(id);
	beginWrite([key]);
	ssot.modules[id] = enabled;
	try {
		await invoke('set_module_enabled', { id, enabled });
	} catch (e) {
		console.warn('[ssot] set_module_enabled failed:', e);
	} finally {
		endWrite([key]);
	}
}

// ------------------------------------------------- validated slice setters --

/**
 * Report a failed slice-setter command everywhere it can be read.
 *
 * `console.warn` alone is what the market setters do, and it is enough for
 * them: their only failure mode is IPC. These commands are different — Rust
 * REJECTS values (`validate_keys`, `TempleProfileSettings::validate`,
 * `validate_sources_off`), so a failure here is a thing the user did and must
 * be told about. So the message goes to the persistent app log (the LOGS
 * channel the README names as the one place a desktop error must reach) AND
 * comes back to the caller, which is how the page renders it next to the
 * control that produced it.
 *
 * `tag` is the module the line belongs to (`temple`, `merc`), so a log dump
 * says which feature refused rather than only which command.
 */
async function sliceCommandFailed(tag: string, command: string, e: unknown): Promise<string> {
	const message = `${e}`;
	console.warn(`[ssot] ${command} failed:`, e);
	try {
		await invoke('app_log_from_frontend', { msg: `[${tag}] ${command} failed: ${message}` });
	} catch (logError) {
		// The log channel itself is down. There is nowhere left to put this, so
		// the console is the last resort rather than a silent swallow.
		console.warn('[ssot] app_log_from_frontend failed:', logError);
	}
	return message;
}

/**
 * Run one validated command and bring the echo back.
 *
 * Returns null on success and the rejection message on failure — NOT void like
 * the market setters, because Rust validates these and the page has to show
 * what it said no to.
 *
 * On success it re-fetches immediately rather than mutating the rune: the
 * slices are Rust-owned and whole-replace, so an optimistic local write would
 * have to be reconciled with the next poll's whole slice. Rust echoes the
 * accepted value onto the slice in the same command, so one `get_ssot` is the
 * whole round trip and the control moves without waiting out a poll interval.
 */
async function sliceCommand(
	tag: string,
	command: string,
	args: Record<string, unknown>
): Promise<string | null> {
	try {
		await invoke(command, args);
	} catch (e) {
		return sliceCommandFailed(tag, command, e);
	}
	await fetchSsot();
	return null;
}

/**
 * Set which guides take NO part in the merc verdict (POE-199).
 *
 * The one merc writer in this file. It goes through Rust rather than through
 * the ADR-013 prefs map because the verdict overlay reads the same value: the
 * page writes, Rust echoes it onto `ssot.mercenary.sourcesOff`, and the
 * overlay's next poll evaluates the same capture against the same set. Rust
 * validates the ids, so this returns the rejection instead of throwing.
 */
export function setMercSourcesOff(sourcesOff: string[]): Promise<string | null> {
	return sliceCommand('merc', 'merc_set_sources_off', { sourcesOff });
}

/**
 * Turn the captured mercenary's auto trade search on or off (POE-202).
 *
 * Same reasoning as `setMercSourcesOff`: the value is Rust's, the page only
 * asks. It is not an ADR-013 view preference — the trigger loop in Rust reads
 * it every tick, so a webview-local copy would be able to disagree with the
 * loop that actually decides whether to search.
 */
export function setMercTradeAuto(auto: boolean): Promise<string | null> {
	return sliceCommand('merc', 'merc_set_trade_auto', { auto });
}

/**
 * Set how far below the read tier the merc query comps (1..=3, POE-202).
 *
 * Rust validates the range and refuses anything else, so the rejection comes
 * back to be shown rather than thrown — the same contract as the guide
 * toggles above. Changing it changes the query hash, which is what makes the
 * next tick search again.
 */
export function setMercTierFloor(floor: number): Promise<string | null> {
	return sliceCommand('merc', 'merc_set_tier_floor', { floor });
}

function templeCommand(command: string, args: Record<string, unknown>): Promise<string | null> {
	return sliceCommand('temple', command, args);
}

/** Set how many opening stones this incursion dropped (0, 1 or 2). */
export function setTempleKeys(keys: number): Promise<string | null> {
	return templeCommand('temple_set_keys', { keys });
}

/** Set the two temple config flags — the Atlas passive and the scarab. */
export function setTempleConfig(config: TempleConfig): Promise<string | null> {
	return templeCommand('temple_set_config', { config });
}

/** Set the four tunable strategy-profile fields. */
export function setTempleProfile(profile: TempleProfile): Promise<string | null> {
	return templeCommand('temple_set_profile', { profile });
}

/** Force the next tick to do a full read, whatever the read gate thinks. */
export function rearmTemple(): Promise<string | null> {
	return templeCommand('temple_rearm', {});
}

/**
 * Take a temple debug dump and hand back the report.
 *
 * The one temple command that returns a value rather than an echo, so it does
 * not go through `templeCommand`: there is nothing to re-fetch, and the report
 * is the whole point. Failures still reach the log channel.
 */
export async function templeDebugCapture(
	imagePath: string | null = null
): Promise<{ report: TempleDebugReport | null; error: string | null }> {
	try {
		return { report: await invoke<TempleDebugReport>('temple_debug_capture', { imagePath }), error: null };
	} catch (e) {
		return {
			report: null,
			error: await sliceCommandFailed('temple', 'temple_debug_capture', e)
		};
	}
}

/**
 * Start the poll loop + optional eager-nudge listener. Returns a cleanup that
 * stops both. Idempotent — calling again before stop is a no-op.
 */
export function startSsotStore(): () => void {
	if (pollInterval !== null) return stopSsotStore;

	// Immediate first fetch so the store leaves its null fail-closed state ASAP.
	fetchSsot();
	pollInterval = setInterval(fetchSsot, POLL_INTERVAL_MS);

	// Optional eager nudge: on ssot-changed, re-fetch via get_ssot. The event
	// payload is NOT trusted as truth — get_ssot is the source (WebView2 events
	// can be stale). Overlays rely on the poll above regardless.
	listen('ssot-changed', () => { fetchSsot(); })
		.then((unlisten) => {
			// If stop ran before the listener resolved, unlisten immediately.
			if (pollInterval === null) { unlisten(); return; }
			unlistenSsot = unlisten;
		})
		.catch((e) => console.warn('[ssot] ssot-changed listen failed:', e));

	return stopSsotStore;
}

/** Stop the poll loop and remove the eager-nudge listener. */
export function stopSsotStore(): void {
	if (pollInterval !== null) {
		clearInterval(pollInterval);
		pollInterval = null;
	}
	if (unlistenSsot) {
		unlistenSsot();
		unlistenSsot = null;
	}
}
