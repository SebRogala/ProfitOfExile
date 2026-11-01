import { listen } from '@tauri-apps/api/event';
import { fetchCurrencyExchangePlays, type CurrencyExchangeResponse } from '$lib/api';
import { onLeagueChange, ssot } from '$lib/stores/ssot.svelte';
import { parseMode, parseHorizon, refetchDelay } from './view';

interface ExchangeDependencies {
	selection: () => { mode: string; horizon: string };
	load: typeof fetchCurrencyExchangePlays;
	listen: (event: string, handler: () => void) => Promise<() => void>;
}

export function createExchangeController(dependencies: ExchangeDependencies) {
	let result = $state<CurrencyExchangeResponse | null>(null);
	let lastFetchedAt = $state<Date | null>(null);
	let lastError = $state<string | null>(null);
	let loading = $state(false);
	let now = $state(new Date());
	let generation = 0;
	let disposed = false;
	let timer: ReturnType<typeof setTimeout> | null = null;
	let requested = '';
	async function load() {
		const mine = ++generation;
		const { mode, horizon } = dependencies.selection();
		requested = `${mode}:${horizon}`;
		loading = true;
		try {
			const response = await dependencies.load(parseMode(mode), parseHorizon(horizon));
			if (disposed || mine !== generation) return;
			// First resolution emits no change notification. Keep the initial HTTP read usable while SSOT is cold.
			if (ssot.league !== null && response.league !== ssot.league) return;
			result = response;
			lastFetchedAt = new Date();
			lastError = null;
		} catch (error) {
			if (disposed || mine !== generation) return;
			lastError = String((error as Error)?.message ?? error);
		} finally {
			if (!disposed && mine === generation) loading = false;
		}
	}
	function syncSelection() {
		const { mode, horizon } = dependencies.selection();
		if (`${mode}:${horizon}` !== requested) void load();
	}
	function start() {
		disposed = false;
		const stopLeague = onLeagueChange(() => {
			if (timer) clearTimeout(timer);
			timer = null;
			generation++;
			result = null;
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
			pending.then((stop) => stop()).catch(() => {});
		};
	}
	return {
		get result() { return result; },
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
