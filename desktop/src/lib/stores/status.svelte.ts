/**
 * Shared app status store — event-driven, no polling.
 *
 * Rust backend emits "status-changed" and "logs-changed" events on every
 * state mutation. The frontend subscribes once on app boot.
 *
 * Usage:
 *   import { store, initStatusStore } from '$lib/stores/status.svelte';
 *   // Read: store.status, store.logs
 *   // Call initStatusStore() once from root layout
 */
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

/** Reactive store — mutate properties, never reassign the export. */
export const store = $state({
	/** Full app status from Rust AppState. */
	status: null as any,
	/** Log entries from Rust. */
	logs: [] as string[],
});

/**
 * Initialize the status store — loads initial state, subscribes to events.
 * Call once from the root (app) layout.
 * Returns a cleanup function to unsubscribe.
 */
export async function initStatusStore(): Promise<() => void> {
	// Subscribe to backend events first (so we don't miss any)
	const unlistenStatus = await listen('status-changed', (event) => {
		console.log('[store] status-changed event received');
		store.status = event.payload;
	});

	const unlistenLogs = await listen('logs-changed', (event) => {
		store.logs = event.payload as string[];
	});

	// Then do initial load as fallback
	try {
		const s = await invoke('get_status');
		console.log('[store] initial get_status:', s);
		store.status = s;
	} catch (e) {
		console.warn('[store] initial get_status failed:', e);
	}
	try {
		store.logs = await invoke('get_logs') as string[];
	} catch (e) {
		console.warn('[store] initial get_logs failed:', e);
	}

	return () => {
		unlistenStatus();
		unlistenLogs();
	};
}
