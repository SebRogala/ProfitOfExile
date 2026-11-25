/** The latest Read exchange press (POE-477), kept for the app's lifetime. */
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { ChaosDivCurrency, ChaosDivMode, ChaosDivOneSide } from './chaos-div';

/** The Tauri event every exchange press emits once (`read_hotkey.rs`). */
export const EXCHANGE_READ_EVENT = 'exchange-read';

export type ExchangeReadErrorCode =
	| 'panel-not-found'
	| 'ratio-unreadable'
	| 'names-unreadable'
	| 'no-currency-side'
	| 'grab-failed'
	| 'privacy-refused';

export interface ExchangeSelection {
	mode: ChaosDivMode;
	currency: ChaosDivCurrency;
	ratio: number;
	oneSide: ChaosDivOneSide;
	left: number;
	right: number;
	stock: number;
	wantName: string;
	haveName: string;
}

export type ExchangeReadEvent =
	| { pressedAtMs: number; selection: ExchangeSelection }
	| { pressedAtMs: number; error: ExchangeReadErrorCode };

let latest = $state.raw<ExchangeReadEvent | null>(null);
let stopStore: (() => void) | null = null;

/** Registers the app's one `exchange-read` listener; returns its cleanup. */
export function startExchangeReadStore(): () => void {
	if (stopStore) return stopStore;
	let stopped = false;
	let unlisten: UnlistenFn | null = null;
	const stop = () => {
		if (stopped) return;
		stopped = true;
		unlisten?.();
		if (stopStore === stop) stopStore = null;
		latest = null;
	};
	stopStore = stop;
	void listen<ExchangeReadEvent>(EXCHANGE_READ_EVENT, (event) => {
		if (!stopped) latest = event.payload;
	}).then((cleanup) => {
		if (stopped) cleanup();
		else unlisten = cleanup;
	}).catch((error) => {
		stop();
		const msg = `Exchange read: listener failed: ${error}`;
		console.warn(msg);
		void invoke('app_log_from_frontend', { msg }).catch((logError) => {
			console.warn('[exchange-read] app_log_from_frontend failed:', logError);
		});
	});
	return stop;
}

/** The latest press result, or null before the first press. Reactive. */
export function latestExchangeRead(): ExchangeReadEvent | null {
	return latest;
}
