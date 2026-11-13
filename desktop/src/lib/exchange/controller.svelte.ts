import { listen } from '@tauri-apps/api/event';
import { fetchCurrencyExchangePlays, type CurrencyExchangeResponse, type ExchangePageParams } from '$lib/api';
import { onLeagueChange, ssot } from '$lib/stores/ssot.svelte';
import { parseMode, parseHorizon, refetchDelay } from './view';

/** One paged request: the mode/horizon picks plus `exchangePageParams`. */
export interface ExchangeQuery {
	mode: string;
	horizon: string;
	params: ExchangePageParams;
}

interface ExchangeDependencies {
	selection: () => ExchangeQuery;
	load: typeof fetchCurrencyExchangePlays;
	listen: (event: string, handler: () => void) => Promise<() => void>;
}

/** How long a typed input (search, gate, bound) settles before it fetches. */
const TYPED_DEBOUNCE_MS = 300;

export function createExchangeController(dependencies: ExchangeDependencies) {
	let result = $state<CurrencyExchangeResponse | null>(null);
	// The last response this league answered, whatever query it was for: the
	// filter bar and counters read it so they stay mounted while a changed query
	// is pending or failed. Never a source of rows.
	let settled = $state<CurrencyExchangeResponse | null>(null);
	let lastFetchedAt = $state<Date | null>(null);
	let lastError = $state<string | null>(null);
	let loading = $state(false);
	let now = $state(new Date());
	let generation = 0;
	let disposed = false;
	let timer: ReturnType<typeof setTimeout> | null = null;
	let typedTimer: ReturnType<typeof setTimeout> | null = null;
	let requested = '';
	function clearTypedTimer() {
		if (typedTimer) clearTimeout(typedTimer);
		typedTimer = null;
	}
	async function load() {
		const mine = ++generation;
		const { mode, horizon, params } = dependencies.selection();
		requested = JSON.stringify({ mode, horizon, params });
		loading = true;
		try {
			const response = await dependencies.load(parseMode(mode), parseHorizon(horizon), params);
			if (disposed || mine !== generation) return;
			// First resolution emits no change notification. Keep the initial HTTP read usable while SSOT is cold.
			if (ssot.league !== null && response.league !== ssot.league) return;
			result = response;
			settled = response;
			lastFetchedAt = new Date();
			lastError = null;
		} catch (error) {
			if (disposed || mine !== generation) return;
			lastError = String((error as Error)?.message ?? error);
		} finally {
			if (!disposed && mine === generation) loading = false;
		}
	}
	/**
	 * Fetch when the picks changed. A changed query withdraws the previous
	 * query's page at once, so rows are only ever shown under the query they
	 * were fetched for; `debounce` delays only the network fetch.
	 */
	function syncSelection(options: { debounce?: boolean } = {}) {
		const { mode, horizon, params } = dependencies.selection();
		const key = JSON.stringify({ mode, horizon, params });
		if (key === requested) return;
		requested = key;
		generation++;
		result = null;
		lastFetchedAt = null;
		lastError = null;
		loading = true;
		clearTypedTimer();
		if (!options.debounce) {
			void load();
			return;
		}
		typedTimer = setTimeout(() => {
			typedTimer = null;
			void load();
		}, TYPED_DEBOUNCE_MS);
	}
	function start() {
		disposed = false;
		const stopLeague = onLeagueChange(() => {
			if (timer) clearTimeout(timer);
			timer = null;
			clearTypedTimer();
			generation++;
			result = null;
			settled = null;
			lastFetchedAt = null;
			lastError = null;
			void load();
		});
		void load();
		const clock = setInterval(() => { now = new Date(); }, 30_000);
		const pending = dependencies.listen('currency-exchange-updated', () => {
			if (disposed) return;
			if (timer) clearTimeout(timer);
			timer = setTimeout(() => { timer = null; void load(); }, refetchDelay());
		});
		return () => {
			disposed = true;
			generation++;
			stopLeague();
			clearInterval(clock);
			if (timer) clearTimeout(timer);
			clearTypedTimer();
			pending.then((stop) => stop()).catch(() => {});
		};
	}
	return {
		get result() { return result; },
		get settled() { return settled; },
		get lastFetchedAt() { return lastFetchedAt; },
		get lastError() { return lastError; },
		get loading() { return loading; },
		get now() { return now; },
		load, syncSelection, start
	};
}

export function productionDependencies(selection: ExchangeDependencies['selection']): ExchangeDependencies {
	return { selection, load: fetchCurrencyExchangePlays, listen: (event, handler) => listen(event, handler) };
}
