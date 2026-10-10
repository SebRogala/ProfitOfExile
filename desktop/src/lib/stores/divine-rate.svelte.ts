/**
 * The desktop's one divine:chaos rate (POE-284) — what the top-bar chip shows
 * and what a surface converts chaos ↔ divine with (see the Store API in the
 * lib README). Fed only through the `$lib/api` port `fetchDivineRate()`, so
 * going live swaps that function's body and nothing here.
 *
 * Refresh: one read at init, then one debounced read per
 * `currency-exchange-updated` burst — LabPage's re-emit of the exchange's
 * Mercure topic, so this opens no second connection. A rejected read keeps
 * the last snapshot. The state comes from the source; this store derives no
 * liveness of its own.
 *
 * Main window only: an overlay is its own WebView and shares no module state.
 */
import { listen } from '@tauri-apps/api/event';
import { fetchDivineRate } from '$lib/api';
import { refetchDelay } from '$lib/exchange/view';
import { deriveDivineRateState, normalizeDivineRate, type DivineRateSnapshot, type DivineRateState } from './divine-rate';

/** How often `now` advances, so "updated N min ago" moves while the app is open. */
const CLOCK_MS = 30_000;

/** The one reactive snapshot. Read reactively; only this module writes it. */
export const divineRate = $state<DivineRateSnapshot & { now: number }>({
	divineChaosRate: null,
	priceHour: null,
	updatedAt: null,
	state: 'cold',
	unreachableSince: null,
	now: Date.now()
});

/** The rate to convert with: a finite number > 0, also while stale; `null` while cold. */
export function currentDivineRate(): number | null {
	return divineRate.divineChaosRate;
}

export function divineRateState(): DivineRateState {
	return deriveDivineRateState(divineRate);
}

/** One read through the port; a rejected read leaves the snapshot unchanged. */
export async function refreshDivineRate(): Promise<void> {
	try {
		const next = normalizeDivineRate(await fetchDivineRate());
		divineRate.divineChaosRate = next.divineChaosRate;
		divineRate.priceHour = next.priceHour;
		divineRate.updatedAt = next.updatedAt;
		divineRate.state = next.state;
		divineRate.unreachableSince = next.unreachableSince;
		divineRate.now = Date.now();
	} catch (e) {
		console.warn('[divine-rate] read failed, keeping the last rate:', e);
	}
}

/** Call once from `routes/(app)/+layout.svelte`. Returns the cleanup. */
export function initDivineRateStore(): () => void {
	let cancelled = false;
	let refetchTimer: ReturnType<typeof setTimeout> | null = null;

	void refreshDivineRate();

	const clock = setInterval(() => {
		divineRate.now = Date.now();
	}, CLOCK_MS);

	// Debounced AND jittered (`refetchDelay`): every client receives the same
	// publish at once — see the constants' comment in `$lib/exchange/view`.
	const unlisten = listen('currency-exchange-updated', () => {
		if (cancelled) return;
		if (refetchTimer) clearTimeout(refetchTimer);
		refetchTimer = setTimeout(() => {
			refetchTimer = null;
			void refreshDivineRate();
		}, refetchDelay());
	});

	return () => {
		cancelled = true;
		clearInterval(clock);
		if (refetchTimer) {
			clearTimeout(refetchTimer);
			refetchTimer = null;
		}
		// Expected to reject outside a Tauri context (the browser dev server).
		unlisten.then((stop) => stop()).catch(() => {});
	};
}
