/**
 * Harvest Flipping page controller — the page's workflow, unmounted.
 *
 * Owns both owners' last results, the fetch clock and error, the wall clock
 * and the copied regex text; reads and writes the three persisted picks
 * (D10). Every number and label still comes from `view.ts`'s `pageView()`:
 * this file decides WHEN to fetch and WHAT the player changed, never what the
 * page says.
 *
 * Fetch follows `CurrencyExchangePage.svelte`'s `load`: one generation
 * counter, so a superseded response is dropped, and the last results kept on
 * an error, so the page downgrades to `stale` instead of blanking. Harvest
 * data is taken as soon as it answers on the first load only (reference 08
 * §1: tabs render while prices load); after that both owners are replaced
 * together, so one view never mixes two refreshes.
 *
 * D5: `currentDivineRate` is the one rate the status line and the engine's
 * divine-scale line read. `dependencies.divineRate()` wins when it answers:
 * production reads POE-284's shared store (`currentDivineRate()`, the top-bar
 * chip's rate), and only while that store is cold (`null`) does the exchange
 * seam's `divineChaosRate` stand in.
 */
import { listen } from '@tauri-apps/api/event';
import { refetchDelay, type CurrencyExchangeHorizon } from '$lib/exchange/view';
import { persisted, type PersistedString } from '$lib/prefs.svelte';
import { currentDivineRate } from '$lib/stores/divine-rate.svelte';
import type { Pick } from './engine';
import type { ExchangePriceRead, HarvestFamilyData, LoadExchangePrices, LoadHarvestFamilies } from './seam';
import { loadExchangePrices } from './sources/exchange-fixture';
import { loadHarvestFamilies } from './sources/harvest-fixture';
import {
	DEFAULT_FAMILY_ID,
	applyPick,
	moveTier as moveTierPicks,
	pageView,
	parseHarvestHorizon,
	parsePicks,
	resetFamily as resetFamilyPicks,
	resolveFamily,
	serializePicks,
	type Chip,
	type Column,
	type HarvestPicks,
	type PageView
} from './view';

/** D10 pref keys and defaults. */
export const PICKS_PREF = 'harvestFlippingPicks';
export const HORIZON_PREF = 'harvestFlippingHorizon';
export const FAMILY_PREF = 'harvestFlippingFamily';

/** Same cadence as `CurrencyExchangePage.svelte`'s `NOW_TICK_MS`: "updated N min ago" moves on its own. */
const NOW_TICK_MS = 30_000;

export type RegexKind = 'feeders' | 'keepers';

export interface HarvestPrefs {
	picks: PersistedString;
	horizon: PersistedString;
	family: PersistedString;
}

export interface HarvestControllerDependencies {
	loadExchangePrices: LoadExchangePrices;
	loadHarvestFamilies: LoadHarvestFamilies;
	prefs: HarvestPrefs;
	listen: (event: string, handler: () => void) => Promise<() => void>;
	refetchDelay: () => number;
	setTimeout: (fn: () => void, ms: number) => ReturnType<typeof setTimeout>;
	clearTimeout: (timer: ReturnType<typeof setTimeout>) => void;
	setInterval: (fn: () => void, ms: number) => ReturnType<typeof setInterval>;
	clearInterval: (timer: ReturnType<typeof setInterval>) => void;
	writeClipboard: (text: string) => Promise<void>;
	now: () => Date;
	/** D5: the shared divine rate, `null` when unknown. Production reads `$lib/stores/divine-rate.svelte`, read-only. */
	divineRate: () => number | null;
}

/** Built per controller: `persisted()` starts its prefs load, so nothing runs at import. */
export function productionDependencies(): HarvestControllerDependencies {
	return {
		loadExchangePrices,
		loadHarvestFamilies,
		prefs: {
			picks: persisted(PICKS_PREF, '{}'),
			horizon: persisted(HORIZON_PREF, 'day'),
			family: persisted(FAMILY_PREF, DEFAULT_FAMILY_ID)
		},
		listen: (event, handler) => listen(event, handler),
		refetchDelay: () => refetchDelay(),
		setTimeout: (fn, ms) => setTimeout(fn, ms),
		clearTimeout: (timer) => clearTimeout(timer),
		setInterval: (fn, ms) => setInterval(fn, ms),
		clearInterval: (timer) => clearInterval(timer),
		writeClipboard: (text) => navigator.clipboard.writeText(text),
		now: () => new Date(),
		divineRate: () => currentDivineRate()
	};
}

function errorMessage(error: unknown): string {
	return String((error as { message?: string } | null)?.message ?? error);
}

function columnChips(column: Column): Chip[] {
	return [...column.cards.flatMap((c) => c.chips), ...column.untiered];
}

export function createHarvestController(
	dependencies: HarvestControllerDependencies = productionDependencies()
) {
	const { prefs } = dependencies;

	let exchange = $state.raw<ExchangePriceRead | null>(null);
	let harvest = $state.raw<HarvestFamilyData | null>(null);
	let lastFetchedAt = $state<Date | null>(null);
	let lastError = $state<string | null>(null);
	let now = $state(dependencies.now());
	let copiedText = $state<Record<RegexKind, string | null>>({ feeders: null, keepers: null });

	const horizon = $derived(parseHarvestHorizon(prefs.horizon.value));
	/** The family on screen: a stored id no family carries falls back (`resolveFamily`), so picks land on the shown tab. */
	const family = $derived(harvest ? resolveFamily(harvest.families, prefs.family.value) : null);
	const familyId = $derived(family?.id ?? prefs.family.value);
	const picks = $derived<HarvestPicks>(harvest ? parsePicks(prefs.picks.value, harvest.families) : {});
	const currentDivineRate = $derived(dependencies.divineRate() ?? exchange?.divineChaosRate ?? null);
	const view = $derived<PageView>(
		pageView({
			exchange,
			harvest,
			lastFetchedAt,
			lastError,
			now,
			familyId,
			picks,
			horizon,
			divineChaosRate: currentDivineRate
		})
	);

	/** A plain counter, as CX's `loadGeneration`: nothing renders it. */
	let loadGeneration = 0;
	/** The horizon of the newest load; plain, so a load started from a read writes no state. */
	let requested: CurrencyExchangeHorizon | null = null;
	let started = false;

	async function load(): Promise<void> {
		const generation = ++loadGeneration;
		const asked = horizon;
		requested = asked;
		const families = dependencies.loadHarvestFamilies(asked).then((read) => {
			if (generation === loadGeneration && harvest === null) harvest = read;
			return read;
		});
		try {
			const [prices, data] = await Promise.all([dependencies.loadExchangePrices(asked), families]);
			if (generation !== loadGeneration) return;
			exchange = prices;
			harvest = data;
			lastFetchedAt = dependencies.now();
			lastError = null;
		} catch (error) {
			if (generation !== loadGeneration) return;
			// Keep both results: the page downgrades to `stale` and every number stays.
			lastError = errorMessage(error);
		}
	}

	function writePicks(next: HarvestPicks): void {
		prefs.picks.value = serializePicks(next);
	}

	/** Writes the pref only: `syncHorizon()`, run by the page's horizon effect, owns the reload. */
	function setHorizon(value: string): void {
		prefs.horizon.value = parseHarvestHorizon(value);
	}

	function setFamily(id: string): void {
		prefs.family.value = id;
	}

	/** Prototype line 417: a chip click always moves the chip to the other column, picked or not. */
	function togglePick(itemId: string): void {
		const body = view.body;
		if (!exchange || body?.kind !== 'family') return;
		const keeper = columnChips(body.keepers).find((c) => c.itemId === itemId);
		const chip = keeper ?? columnChips(body.feeders).find((c) => c.itemId === itemId);
		if (!chip) return;
		const pick: Pick = keeper ? 'reroll' : 'keep';
		writePicks(applyPick(picks, familyId, itemId, pick, exchange.prices));
	}

	/** Without `to`, a tier with any feeder moves to the keepers, else to the feeders. */
	function moveTier(tier: string, to?: Pick): void {
		const body = view.body;
		if (!exchange || !family || body?.kind !== 'family') return;
		const side = to ?? (body.feeders.cards.some((c) => c.tier === tier) ? 'keep' : 'reroll');
		writePicks(moveTierPicks(picks, family, tier, side, exchange.prices));
	}

	function resetFamily(): void {
		writePicks(resetFamilyPicks(picks, familyId));
	}

	async function copyRegex(kind: RegexKind): Promise<void> {
		const body = view.body;
		if (body?.kind !== 'family') return;
		const text = body.regex[kind].text;
		try {
			await dependencies.writeClipboard(text);
			copiedText = { ...copiedText, [kind]: text };
		} catch (error) {
			console.warn('[harvest] clipboard write failed:', errorMessage(error));
		}
	}

	/** "Copied" holds only while the copied text is still the box's regex. */
	function isCopied(kind: RegexKind): boolean {
		const body = view.body;
		return body?.kind === 'family' && copiedText[kind] !== null && copiedText[kind] === body.regex[kind].text;
	}

	/**
	 * Load again when the normalized horizon is not the one last asked for —
	 * after a pick, and after a late `persisted()` restore, which replaces the
	 * pref's value without any action. The page calls it from a `$effect` that
	 * reads `horizon` (CX's horizon effect); idempotent through `requested`, so
	 * the effect's first run after `start()` asks for nothing. Nothing before `start()`.
	 */
	function syncHorizon(): void {
		if (started && horizon !== requested) void load();
	}

	/**
	 * The first load, the wall clock and the Mercure refetch
	 * (`currency-exchange-updated`, LabPage's re-emit) after a jittered
	 * `refetchDelay()`. Returns the disposer.
	 */
	function start(): () => void {
		let cancelled = false;
		let refetchTimer: ReturnType<typeof setTimeout> | null = null;
		started = true;
		void load();
		const tick = dependencies.setInterval(() => {
			now = dependencies.now();
		}, NOW_TICK_MS);
		const unlisten = dependencies.listen('currency-exchange-updated', () => {
			if (cancelled) return;
			if (refetchTimer) dependencies.clearTimeout(refetchTimer);
			refetchTimer = dependencies.setTimeout(() => {
				refetchTimer = null;
				void load();
			}, dependencies.refetchDelay());
		});
		return () => {
			cancelled = true;
			started = false;
			dependencies.clearInterval(tick);
			if (refetchTimer) dependencies.clearTimeout(refetchTimer);
			refetchTimer = null;
			// Expected to reject outside a Tauri context (the browser dev server).
			unlisten.then((stop) => stop()).catch(() => {});
		};
	}

	return {
		get view() {
			return view;
		},
		get exchange() {
			return exchange;
		},
		get harvest() {
			return harvest;
		},
		get horizon() {
			return horizon;
		},
		get familyId() {
			return familyId;
		},
		get picks() {
			return picks;
		},
		get currentDivineRate() {
			return currentDivineRate;
		},
		get lastError() {
			return lastError;
		},
		get copied(): Record<RegexKind, boolean> {
			return { feeders: isCopied('feeders'), keepers: isCopied('keepers') };
		},
		load,
		syncHorizon,
		setHorizon,
		setFamily,
		togglePick,
		moveTier,
		resetFamily,
		copyRegex,
		start
	};
}

export type HarvestController = ReturnType<typeof createHarvestController>;
