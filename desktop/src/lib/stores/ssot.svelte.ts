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
 * Usage:
 *   import { ssot, startSsotStore } from '$lib/stores/ssot.svelte';
 *   // Read: ssot.league  (string | null; null until first successful get_ssot)
 *   // Main window: call startSsotStore() top-level (like initStatusStore()).
 *   // Overlay windows: call startSsotStore() from an $effect and return its
 *   //   cleanup (onMount is unreliable in overlay windows).
 */
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

/** Serialized Rust `AppSsotSnapshot` — `league.name` is `string | null`. */
export interface SsotSnapshot {
	league: { name: string | null };
	/** A league resolve is in flight (bounded-retry fetch task running). */
	resolving?: boolean;
	/** The resolver has failed enough times to treat the server as unreachable
	 *  (still retrying). Only meaningful while `resolving` is true. */
	unreachable?: boolean;
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
});

/** Map the Rust snapshot shape (`snap.league.name`, `snap.resolving`,
 *  `snap.unreachable`) into the flat store fields. Missing/malformed fields fail
 *  closed (null / false). */
export function applySnapshot(snap: SsotSnapshot): void {
	ssot.league = snap.league?.name ?? null;
	ssot.resolving = snap.resolving ?? false;
	ssot.unreachable = snap.unreachable ?? false;
}

let pollInterval: ReturnType<typeof setInterval> | null = null;
let unlistenSsot: UnlistenFn | null = null;

/** Fetch the snapshot via the poll-target command and apply it. */
async function fetchSsot(): Promise<void> {
	try {
		applySnapshot(await invoke<SsotSnapshot>('get_ssot'));
	} catch (e) {
		console.warn('[ssot] get_ssot failed:', e);
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
