/**
 * The desktop's one Mercure connection (POE-336). The (app) layout starts it;
 * pages subscribe instead of connecting.
 *
 * The connection opens the first time `store.status` is non-null — the API base
 * and device headers `connectMercure` reads come from it — and never reopens on
 * later status replacements. Per-topic payloads are re-emitted as Tauri events
 * (overlay webviews and main-window listeners pick them up there); generic
 * updates and connection changes go to in-process subscribers.
 *
 * Main window only: an overlay is its own WebView and shares no module state.
 */
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import { connectMercure, type MercureConnection } from '$lib/api';
import { onStatusReady, store } from './status.svelte';

const updateSubscribers = new Set<() => void>();
const connectionSubscribers = new Set<(connected: boolean) => void>();
let connection: MercureConnection | null = null;
let stopGate: (() => void) | null = null;
let started = false;

function noteServerLeague(observed: string | null) {
	invoke('note_server_league', { observed })
		.catch(e => console.warn('[mercure] failed to reconcile league:', e));
}

function open() {
	connection = connectMercure({
		onUpdate: () => { for (const fn of updateSubscribers) fn(); },
		onConnectionChange: (connected) => {
			store.serverConnected = connected;
			for (const fn of connectionSubscribers) fn(connected);
		},
		onLayoutUpdate: (data) => {
			emit('lab-layout-updated', { difficulty: data?.difficulty, action: data?.action })
				.catch(e => console.warn('[mercure] failed to emit layout update:', e));
		},
		onCurrencyExchangeUpdate: (data) => {
			emit('currency-exchange-updated', data)
				.catch(e => console.warn('[mercure] failed to emit currency exchange update:', e));
		},
		onHarvestUpdate: (data) => {
			emit('harvest-updated', data)
				.catch(e => console.warn('[mercure] failed to emit harvest update:', e));
		},
		onOpen: () => noteServerLeague(null),
		onLeagueActive: (data) => noteServerLeague(typeof data?.league === 'string' ? data.league : null),
	});
}

function stopMercureStore() {
	stopGate?.();
	stopGate = null;
	connection?.close();
	connection = null;
	started = false;
}

/** Start the connection gate. Idempotent: a second call returns the same stop and opens nothing. */
export function startMercureStore(): () => void {
	if (started) return stopMercureStore;
	started = true;
	stopGate = onStatusReady(open);
	return stopMercureStore;
}

/** Generic (non-topic-specific) updates. Returns the unsubscribe. */
export function subscribeMercureUpdate(fn: () => void): () => void {
	updateSubscribers.add(fn);
	return () => { updateSubscribers.delete(fn); };
}

/** Connection changes (5 s debounced on the way down). Returns the unsubscribe. */
export function subscribeMercureConnection(fn: (connected: boolean) => void): () => void {
	connectionSubscribers.add(fn);
	return () => { connectionSubscribers.delete(fn); };
}

/** The live connection's `connected`; undefined before a connection exists. */
export function mercureConnected(): boolean | undefined {
	return connection?.connected;
}
