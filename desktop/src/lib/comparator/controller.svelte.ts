import { onLeagueChange, ssot } from '$lib/stores/ssot.svelte';
import { fetchCompare, fetchGemNames, type CompareGem, type GemPlay, type RankingsQuery, type StatusData, type MarketOverviewData } from '$lib/api';

export type RankingsPage = { rows: GemPlay[]; total: number };
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { isSource, type TradeLookupResult, type TradeQueueDisplay, type TradeQueueEvent } from '$lib/tradeApi';
import { floorQuote, quoteDelta, type QueueItem } from '$lib/session-queue';
import { createLeagueReadGuard } from './league-fence';

interface Selection { variant: string; mode: string; divineRate: number; autoTrade: boolean }
interface OverlayPayload {
	results: CompareGem[];
	tradeData: Record<string, TradeLookupResult | null>;
	tradeLoading: Record<string, boolean>;
	tradeError: Record<string, boolean>;
	divineRate: number;
	labMode: string;
}
interface ComparatorDependencies {
	selection: () => Selection;
	compare: typeof fetchCompare;
	names: typeof fetchGemNames;
	trade: (gem: string, selection: Selection) => Promise<TradeLookupResult>;
	listen: <T>(event: string, handler: (event: { payload: T }) => void) => Promise<() => void>;
	pushOverlay: (payload: OverlayPayload) => void;
	queue: (gem: string, variant: string, roi: number, trade: TradeLookupResult | null) => void;
	autoRefreshDue?: (gem: string) => boolean;
}

// Queue events carry no league/request identity, and settled promises do not
// prove event delivery has drained. Fence them for this WebView session after
// rollover, including across remounts; promises still own results/errors/loading.
const nativeEventProvenance = { generation: null as number | null, fenced: false };

/** Owns the actual compare/native requests and the state also sent to the overlay. */
export function createComparatorController(dependencies: ComparatorDependencies) {
	if (ssot.leagueGeneration > 0) nativeEventProvenance.generation ??= ssot.leagueGeneration;
	const state = $state({
		selectedGems: [] as string[], results: [] as CompareGem[],
		selectedForQueue: null as string | null,
		searchQuery: '', suggestions: [] as string[], showDropdown: false, highlightedIndex: -1,
		tradeData: {} as Record<string, TradeLookupResult | null>,
		tradeLoading: {} as Record<string, boolean>,
		tradeError: {} as Record<string, boolean>,
		tradeErrorMsg: {} as Record<string, string>,
		tradeExpanded: {} as Record<string, boolean>,
		tradeQueue: null as TradeQueueDisplay | null
	});
	let generation = 0;
	let compareGeneration = 0;
	let tradeGeneration: Record<string, number> = {};
	let disposed = false;
	let compareAbort: AbortController | null = null;
	let debounce: ReturnType<typeof setTimeout> | null = null;
	let searchDebounce: ReturnType<typeof setTimeout> | null = null;
	let searchGeneration = 0;
	let loadedKey = '';
	function pushOverlay() {
		const { divineRate, mode } = dependencies.selection();
		dependencies.pushOverlay({ results: [...state.results], tradeData: { ...state.tradeData },
			tradeLoading: { ...state.tradeLoading }, tradeError: { ...state.tradeError },
			divineRate: state.results.length ? divineRate : 0, labMode: mode });
	}
	function dropTradeData() {
		state.tradeData = {}; state.tradeLoading = {}; state.tradeError = {};
		state.tradeErrorMsg = {}; state.tradeExpanded = {}; state.tradeQueue = null;
	}
	function clearAll(autoQueue = true) {
		if (autoQueue) handleNext(false);
		generation++;
		compareGeneration++;
		compareAbort?.abort();
		if (debounce) clearTimeout(debounce);
		debounce = null;
		state.selectedGems = []; state.results = []; state.selectedForQueue = null;
		resetSearch();
		dropTradeData();
		pushOverlay();
	}
	function resetSearch() {
		searchGeneration++;
		if (searchDebounce) clearTimeout(searchDebounce);
		searchDebounce = null;
		state.searchQuery = ''; state.suggestions = []; state.showDropdown = false; state.highlightedIndex = -1;
	}
	function handleInput(query: string) {
		resetSearch();
		state.searchQuery = query;
		if (query.length < 2) return;
		const mine = generation;
		const request = searchGeneration;
		searchDebounce = setTimeout(async () => {
			searchDebounce = null;
			const current = () => !disposed && mine === generation && request === searchGeneration;
			try {
				const names = await dependencies.names(query, dependencies.selection().mode === 'dedication' ? 'dedication' : undefined);
				if (!current()) return;
				state.suggestions = names.filter((name) => !state.selectedGems.includes(name));
				state.showDropdown = state.suggestions.length > 0;
				state.highlightedIndex = state.suggestions.length === 1 ? 0 : -1;
			} catch (error) {
				if (current()) console.warn('[Comparator] Failed to load gem names:', error);
			}
		}, 100);
	}
	async function loadResults() {
		const mine = generation;
		const request = ++compareGeneration;
		compareAbort?.abort();
		compareAbort = new AbortController();
		const { variant, mode, autoTrade } = dependencies.selection();
		if (!state.selectedGems.length) { state.results = []; pushOverlay(); return; }
		try {
			const rows = await dependencies.compare([...state.selectedGems], variant, compareAbort.signal, mode === 'dedication' ? mode : undefined);
			if (disposed || mine !== generation || request !== compareGeneration) return;
			state.results = rows.filter((row) => row.variant.replace(/c$/, '') === variant).map((row) =>
				row.trade && row.trade.league !== ssot.league ? { ...row, trade: undefined } : row);
			for (const row of state.results) {
				if (row.trade?.league === ssot.league) state.tradeData[row.name] = row.trade;
			}
			pushOverlay();
			if (autoTrade) for (const row of state.results) {
				if (!state.tradeData[row.name] || dependencies.autoRefreshDue?.(row.name)) void invokeRustTrade(row.name);
			}
		} catch (error) {
			if (disposed || mine !== generation || request !== compareGeneration || (error as Error)?.name === 'AbortError') return;
			console.error('[Comparator] Failed to load results:', error);
			state.results = []; pushOverlay();
		}
	}
	async function invokeRustTrade(gem: string) {
		const mine = generation;
		const request = (tradeGeneration[gem] ?? 0) + 1;
		tradeGeneration[gem] = request;
		const selection = dependencies.selection();
		const current = () => !disposed && mine === generation && tradeGeneration[gem] === request;
		nativeEventProvenance.generation ??= ssot.leagueGeneration;
		state.tradeLoading[gem] = true; state.tradeError[gem] = false; delete state.tradeErrorMsg[gem];
		pushOverlay();
		try {
			const result = await dependencies.trade(gem, selection);
			if (!current() || result.league !== ssot.league || result.variant !== dependencies.selection().variant) return;
			state.tradeData[gem] = result;
			state.tradeError[gem] = false;
			delete state.tradeErrorMsg[gem];
		} catch (error) {
			if (!current() || error === 'cancelled') return;
			console.warn(`[Trade] Rust lookup failed for ${gem}:`, error);
			state.tradeError[gem] = true;
			state.tradeErrorMsg[gem] = String((error as Error)?.message ?? error);
		} finally {
			if (current()) {
				state.tradeLoading[gem] = false;
				if (state.tradeQueue?.gem === gem) state.tradeQueue = null;
				pushOverlay();
			}
		}
	}
	function selectGem(name: string, debounceLoad = false) {
		if (!name || state.selectedGems.includes(name) || state.selectedGems.length >= 3) return;
		state.selectedGems = [...state.selectedGems, name];
		resetSearch();
		if (debounceLoad) {
			if (debounce) clearTimeout(debounce);
			debounce = setTimeout(() => { debounce = null; void loadResults(); }, 300);
		} else void loadResults();
	}
	function removeGem(index: number) {
		const gem = state.selectedGems[index];
		state.selectedGems = state.selectedGems.filter((_, i) => i !== index);
		tradeGeneration[gem] = (tradeGeneration[gem] ?? 0) + 1;
		delete state.tradeData[gem]; delete state.tradeLoading[gem]; delete state.tradeError[gem];
		delete state.tradeErrorMsg[gem]; delete state.tradeExpanded[gem];
		void loadResults();
	}
	function handleNext(clear = true) {
		const row = state.results.find((row) => row.name === state.selectedForQueue);
		if (!row) return;
		dependencies.queue(row.name, dependencies.selection().variant, row.roi, state.tradeData[row.name] ?? null);
		if (clear) clearAll(false);
	}
	function syncSelection() {
		const { variant, mode } = dependencies.selection();
		const key = `${variant}:${mode}`;
		if (loadedKey === key) return;
		loadedKey = key;
		generation++;
		state.results = [];
		dropTradeData(); pushOverlay(); void loadResults();
	}
	function start() {
		disposed = false;
		const stopLeague = onLeagueChange(() => {
			nativeEventProvenance.fenced = true;
			clearAll(false);
			void loadResults();
		});
		const subscriptions: Promise<() => void>[] = [];
		function on<T>(event: string, handler: (value: T) => void) {
			subscriptions.push(dependencies.listen<T>(event, ({ payload }) => { if (!disposed) handler(payload); }));
		}
		on<string>('gem-detected', (name) => selectGem(name, true));
		on('gems-cleared', () => clearAll());
		on<{ name: string }>('overlay-pick', ({ name }) => { state.selectedForQueue = name; handleNext(); });
		on<{ name: string }>('overlay-trade-refresh', ({ name }) => { if (state.selectedGems.includes(name)) void invokeRustTrade(name); });
		on('overlay-clear', () => clearAll());
		on<TradeQueueEvent>('trade-queue', (event) => {
			if (!isSource(event, 'gem')) return;
			if (nativeEventProvenance.fenced || nativeEventProvenance.generation !== ssot.leagueGeneration) return;
			if (event.kind === 'cancelled') { state.tradeQueue = null; return; }
			if (!('gem' in event) || !event.gem || !state.tradeLoading[event.gem]) return;
			if (event.kind === 'queued' || event.kind === 'waiting' || event.kind === 'fetching') {
				state.tradeQueue = { gem: event.gem, position: event.position, total: event.total,
					status: event.kind, waitSecs: event.kind === 'waiting' ? event.waitSecs : 0 };
			} else {
				state.tradeQueue = null;
				if (event.kind === 'error') { state.tradeError[event.gem] = true; state.tradeErrorMsg[event.gem] = event.error; }
			}
			pushOverlay();
		});
		return () => {
			disposed = true; generation++; compareGeneration++; compareAbort?.abort();
			if (debounce) clearTimeout(debounce);
			resetSearch();
			stopLeague();
			for (const subscription of subscriptions) subscription.then((stop) => stop()).catch(() => {});
		};
	}
	return { state, loadResults, invokeRustTrade, selectGem, removeGem, clearAll, handleNext, syncSelection, pushOverlay, handleInput, resetSearch, start };
}

export function productionDependencies(
	selection: ComparatorDependencies['selection'], queue: ComparatorDependencies['queue'],
	autoRefreshDue: (gem: string) => boolean
): ComparatorDependencies {
	return {
		selection, queue, autoRefreshDue, compare: fetchCompare, names: fetchGemNames,
		trade: (gem, { variant, divineRate, mode }) => invoke<TradeLookupResult>('trade_lookup', {
			gem, variant, divineRate: divineRate || undefined, mode: mode === 'dedication' ? mode : undefined
		}),
		listen: (event, handler) => listen(event, handler),
		pushOverlay: (payload) => { void invoke('set_comparator_data', { payload }).catch((error) => console.warn('[comparator] push to overlay failed:', error)); }
	};
}

export function createLabQueueController(dependencies: {
	queue: { items: QueueItem[] };
	lookup: (item: QueueItem) => Promise<TradeLookupResult>;
}) {
	const guard = createLeagueReadGuard(() => ssot.league);
	function clear() {
		guard.invalidate();
		dependencies.queue.items = [];
	}
	async function refresh() {
		const current = guard.capture();
		dependencies.queue.items = dependencies.queue.items.map((item) => ({ ...item, refreshing: true }));
		await Promise.allSettled(dependencies.queue.items.map(async (item) => {
			const patch = (fn: (row: QueueItem) => QueueItem) => {
				if (!current()) return;
				// Match the queue's (gem, variant) identity: removal shifts array indices.
				dependencies.queue.items = dependencies.queue.items.map((row) =>
					row.gem === item.gem && row.variant === item.variant ? fn(row) : row);
			};
			try {
				const result = await dependencies.lookup(item);
				if (!current(result)) { patch((row) => ({ ...row, refreshing: false })); return; }
				const quote = floorQuote(result);
				patch((row) => ({ ...row, current: quote, priceDelta: quoteDelta(quote, row.snapshot), refreshing: false }));
			} catch {
				patch((row) => ({ ...row, refreshing: false }));
			}
		}));
	}
	return { clear, refresh };
}

/** Lab's retained dashboard reads share one league fence, including failure/finally paths. */
const MERCURE_DEBOUNCE_MS = 2000;
const MERCURE_JITTER_MS = 4000;

export function createLabController(dependencies: {
	status: () => Promise<StatusData>;
	rankings: (q: RankingsQuery) => Promise<RankingsPage>;
	market: () => Promise<MarketOverviewData>;
	rate: (value: number) => void;
	connected: () => boolean | undefined;
	/** Runs before each Mercure-driven reload (LabPage bumps its refreshKey). */
	onReload?: () => void;
	random?: () => number;
}) {
	const random = dependencies.random ?? Math.random;
	const state = $state({
		status: null as StatusData | null,
		bestPlays: [] as GemPlay[],
		rankingsTotal: 0,
		// True from a new query until its page (or its failure) arrives.
		rankingsPending: true,
		marketOverview: null as MarketOverviewData | null,
		bestPlaysError: '', error: '', loading: true
	});
	let generation = 0;
	let rankingsGeneration = 0;
	let loadGeneration = 0;
	// The rankings controls' current query: a league change or a Mercure reload
	// refetches it. Rows are only ever shown under the query they were fetched for.
	let rankingsQuery: RankingsQuery | null = null;
	let rankingsQueryKey = '';
	async function refreshBestPlays() {
		const q = rankingsQuery;
		if (!q) return;
		const mine = generation;
		const request = ++rankingsGeneration;
		state.bestPlaysError = '';
		try {
			const page = await dependencies.rankings(q);
			if (mine !== generation || request !== rankingsGeneration) return;
			state.bestPlays = page.rows;
			state.rankingsTotal = page.total;
			state.rankingsPending = false;
		} catch (error) {
			if (mine !== generation || request !== rankingsGeneration) return;
			console.warn('[LabPage] re-fetch bestPlays failed:', error);
			state.bestPlays = [];
			state.rankingsTotal = 0;
			state.rankingsPending = false;
			state.bestPlaysError = 'Rankings failed to load for this market — retry to see current numbers.';
		}
	}
	function setRankingsQuery(q: RankingsQuery) {
		const key = JSON.stringify(q);
		rankingsQuery = q;
		if (key !== rankingsQueryKey) {
			rankingsQueryKey = key;
			state.bestPlays = [];
			state.rankingsTotal = 0;
			state.rankingsPending = true;
		}
		return refreshBestPlays();
	}
	async function loadAll() {
		const mine = generation;
		const request = ++loadGeneration;
		state.error = '';
		try {
			const [status, , market] = await Promise.all([dependencies.status(), refreshBestPlays(), dependencies.market()]);
			if (mine !== generation || request !== loadGeneration) return;
			const connected = dependencies.connected();
			state.status = connected === undefined ? status : { ...status, connected };
			state.marketOverview = market;
			dependencies.rate(status?.divinePrice ?? 0);
		} catch (error) {
			if (mine !== generation || request !== loadGeneration) return;
			state.error = String((error as Error)?.message ?? 'Failed to load dashboard data');
		} finally {
			if (mine === generation && request === loadGeneration) state.loading = false;
		}
	}
	// --- Mercure debounce + jitter ---
	// The 2s debounce collapses a burst of publishes into a single reload instead
	// of one six-request loadAll() per event.
	//
	// The jitter is the part the debounce alone does not solve. A fixed per-client
	// delay does not spread a herd, it aligns one — every client receives the same
	// publish within milliseconds, so all of them would fire at publish + 2000ms
	// exactly. The random offset de-synchronises clients: arrivals land somewhere in
	// 2–6s rather than stacking on one tick. The server already debounces publishes
	// by 2s (lab.Throttler) and the collector cycle is minutes, so up to 6s of extra
	// delay is invisible on screen.
	//
	// Kept in step with frontend/src/routes/lab/+page.svelte, which runs the same
	// two constants against the same publish.
	let debounceTimer: ReturnType<typeof setTimeout> | null = null;
	function clearDebounce() {
		if (debounceTimer) clearTimeout(debounceTimer);
		debounceTimer = null;
	}
	function onMercureUpdate() {
		clearDebounce();
		// Re-rolled on every fire, not once per session: a fixed per-client offset
		// would put the same clients in the same slot on every publish, which only
		// spreads the herd once instead of on each tick.
		const delay = MERCURE_DEBOUNCE_MS + random() * MERCURE_JITTER_MS;
		debounceTimer = setTimeout(() => {
			debounceTimer = null;
			dependencies.onReload?.();
			void loadAll();
		}, delay);
	}
	function setConnected(connected: boolean) {
		const status = state.status;
		if (status && status.connected !== connected) state.status = { ...status, connected };
	}
	function start(invalidateQueue: () => void) {
		const stop = onLeagueChange(() => {
			generation++;
			state.bestPlays = []; state.rankingsTotal = 0; state.rankingsPending = true;
			state.status = null; state.marketOverview = null;
			state.bestPlaysError = ''; state.error = ''; state.loading = true;
			dependencies.rate(0);
			clearDebounce();
			invalidateQueue();
			void loadAll();
		});
		return () => { generation++; stop(); };
	}
	return { state, refreshBestPlays, setRankingsQuery, loadAll, start, onMercureUpdate, setConnected, dispose: clearDebounce };
}
