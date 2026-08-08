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
 * Usage:
 *   import { ssot, startSsotStore } from '$lib/stores/ssot.svelte';
 *   // Read: ssot.league  (string | null; null until first successful get_ssot)
 *   // Read: ssot.normalVariant / ssot.dedicationVariant / ssot.dedicationPool
 *   // Write: setNormalVariant(v) / setDedicationSelection(variant, pool)
 *   // Main window: call startSsotStore() top-level (like initStatusStore()).
 *   // Overlay windows: call startSsotStore() from an $effect and return its
 *   //   cleanup (onMount is unreliable in overlay windows).
 */
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { DEDICATION_VARIANTS, VARIANTS } from '$lib/api';

/**
 * The dedication pools, in Rust/settings/DB canon spelling (singular `skill`).
 * `api.ts` has no pool constant — the JSON API uses the plural `skills` for the
 * same pool, and translating between the two is the consumer's job.
 */
const POOLS = ['skill', 'transfigured'] as const;

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
});

/** The three market fields, which share the write-through + poll-guard machinery. */
type MarketField = 'normalVariant' | 'dedicationVariant' | 'dedicationPool';

/**
 * Poll-vs-write ordering guard.
 *
 * Every write bumps `writeSeq` and stamps it into `lastWriteSeq[field]`;
 * `fetchSsot` captures the counter *before* awaiting `get_ssot` and hands it to
 * `applySnapshot`. A snapshot is then ignored for a field when either:
 *
 *  - `inFlight[field] > 0` — the write round-trip has not settled yet, so the
 *    snapshot cannot contain it; or
 *  - `lastWriteSeq[field] > dispatchedAtSeq` — the snapshot was dispatched
 *    before the write and merely arrived after it. The in-flight clause alone
 *    misses this ordering, because the write can settle first.
 *
 * `inFlight` counts rather than flags: two rapid clicks overlap, and the first
 * response must not unguard the second write.
 */
let writeSeq = 0;
const lastWriteSeq: Record<MarketField, number> = {
	normalVariant: 0,
	dedicationVariant: 0,
	dedicationPool: 0,
};
const inFlight: Record<MarketField, number> = {
	normalVariant: 0,
	dedicationVariant: 0,
	dedicationPool: 0,
};

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
	if (inFlight[field] > 0) return;
	if (lastWriteSeq[field] > dispatchedAtSeq) return;
	if (!incoming) return;
	if (MARKET_DOMAINS[field].includes(incoming)) {
		ssot[field] = incoming;
		return;
	}
	console.warn(`[ssot] ignoring unknown persisted ${field}:`, incoming, '— healing Rust to', ssot[field]);
	void writeField(field, ssot[field]);
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
 * Open the poll-vs-write guard on every field a single user action writes.
 *
 * One `writeSeq` bump for the whole action, so a multi-field action is ordered
 * against polls as one unit rather than as N independent writes.
 */
function beginWrite(fields: readonly MarketField[]): void {
	writeSeq += 1;
	for (const field of fields) {
		lastWriteSeq[field] = writeSeq;
		inFlight[field] += 1;
	}
}

/** Close the guard opened by `beginWrite`. Always call from a `finally`. */
function endWrite(fields: readonly MarketField[]): void {
	for (const field of fields) inFlight[field] -= 1;
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
