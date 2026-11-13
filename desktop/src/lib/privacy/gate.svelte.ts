/**
 * Whether this window may reach our server: the Data & Privacy notice is
 * accepted (POE-395, ADR-033). Rust owns the answer (`get_privacy_state`) and
 * announces an acceptance with the `privacy-accepted` event to every window.
 *
 * A leaf on purpose: it imports neither `$lib/api` nor `ssot.svelte`, because
 * `stores/server-base.svelte.ts` reads from here.
 *
 * Usage:
 *   initNoticeGate()                 // idempotent; server-base calls it on its first slice
 *   noticeGate.accepted / .known     // reactive
 *   await whenNoticeAccepted()       // resolves once accepted, never rejects
 *   onNoticeAccepted(fn)             // called once on acceptance; returns the unsubscribe
 */
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

/** `privacy::PrivacyState` on the Rust side. */
export interface PrivacyState {
	currentRevision: number;
	acceptedRevision: number | null;
	accepted: boolean;
	persisted: boolean;
}

/** Reactive store — mutate properties, never reassign the export. */
export const noticeGate = $state({ accepted: false, known: false });

const acceptedListeners = new Set<() => void>();
let started = false;

function markAccepted(): void {
	noticeGate.known = true;
	if (noticeGate.accepted) return;
	noticeGate.accepted = true;
	for (const fn of [...acceptedListeners]) fn();
	acceptedListeners.clear();
}

/**
 * Listen for `privacy-accepted`, then ask Rust once. Later calls do nothing.
 * The listener is registered first: an acceptance in another window between
 * the two would otherwise be missed for the session.
 */
export function initNoticeGate(): void {
	if (started) return;
	started = true;
	void (async () => {
		try {
			await listen('privacy-accepted', () => markAccepted());
		} catch (e) {
			console.error('[privacy] listen privacy-accepted failed:', e);
		}
		try {
			const state = await invoke<PrivacyState>('get_privacy_state');
			if (state.accepted) markAccepted();
			else noticeGate.known = true;
		} catch (e) {
			console.error('[privacy] get_privacy_state failed:', e);
		}
	})();
}

/** Call `fn` once when the notice is accepted (at once if it already is). */
export function onNoticeAccepted(fn: () => void): () => void {
	if (noticeGate.accepted) {
		fn();
		return () => {};
	}
	acceptedListeners.add(fn);
	return () => { acceptedListeners.delete(fn); };
}

export function whenNoticeAccepted(): Promise<void> {
	return new Promise((resolve) => { onNoticeAccepted(resolve); });
}

/** Tests only: accepted and started, so no Tauri call is made. */
export function acceptNoticeForTests(): void {
	started = true;
	markAccepted();
}

/** Tests only: back to "not started, not accepted". */
export function resetNoticeGateForTests(): void {
	noticeGate.accepted = false;
	noticeGate.known = false;
	acceptedListeners.clear();
	started = false;
}
