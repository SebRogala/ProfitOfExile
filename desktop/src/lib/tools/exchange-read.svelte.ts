/** The latest Read exchange press (POE-477), kept for the app's lifetime. */
import { listen } from '@tauri-apps/api/event';
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

/** Registers the app's one `exchange-read` listener; returns its cleanup. */
export function startExchangeReadStore(): () => void {
	const unlisten = listen<ExchangeReadEvent>(EXCHANGE_READ_EVENT, () => {});
	return () => {
		void unlisten.then((stop) => stop());
	};
}

/** The latest press result, or null before the first press. Reactive. */
export function latestExchangeRead(): ExchangeReadEvent | null {
	return null;
}
