/**
 * Shared app status store — event-driven, no polling.
 *
 * Rust backend emits "status-changed" and "logs-changed" events on every
 * state mutation. The frontend subscribes once on app boot.
 *
 * Usage:
 *   import { appStatus, appLogs, initStatusStore } from '$lib/stores/status.svelte';
 *   // Call initStatusStore() once from root layout
 */
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

/** Full app status from Rust AppState. Updates on every state change. */
export let appStatus = $state<any>(null);

/** Log entries from Rust. Updates on every new log. */
export let appLogs = $state<string[]>([]);

/**
 * Initialize the status store — loads initial state, subscribes to events.
 * Call once from the root (app) layout.
 * Returns a cleanup function to unsubscribe.
 */
export async function initStatusStore(): Promise<() => void> {
	// Initial load
	try {
		appStatus = await invoke('get_status');
	} catch {
		// Tauri not ready yet — events will catch up
	}
	try {
		appLogs = await invoke('get_logs') as string[];
	} catch {}

	// Subscribe to backend events
	const unlistenStatus = await listen('status-changed', (event) => {
		appStatus = event.payload;
	});

	const unlistenLogs = await listen('logs-changed', (event) => {
		appLogs = event.payload as string[];
	});

	return () => {
		unlistenStatus();
		unlistenLogs();
	};
}
