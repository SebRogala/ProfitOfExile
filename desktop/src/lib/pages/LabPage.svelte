<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import { listen } from '@tauri-apps/api/event';
	import { untrack } from 'svelte';
	import { store } from '$lib/stores/status.svelte';
	import { ssot, fetchSsot, setNormalVariant, setDedicationSelection } from '$lib/stores/ssot.svelte';
	import { setDivineRate } from '$lib/price.svelte';
	import {
		fetchStatus,
		fetchBestPlays,
		VARIANTS,
		DEDICATION_VARIANTS,
		DEDICATION_POOLS,
		DEDICATION_POOL_LABELS,
		fetchMarketOverview,
		connectMercure,
		type StatusData,
		type GemPlay,
		type MarketOverviewData,
		type MercureConnection,
	} from '$lib/api';
	import type { TradeLookupResult } from '$lib/tradeApi';
	import SegmentedButtons from '$lib/components/SegmentedButtons.svelte';
	import Button from '$lib/components/Button.svelte';

	import Header from '../../routes/(app)/components/Header.svelte';
	import Comparator from '../../routes/(app)/components/Comparator.svelte';
	import SessionQueue from '../../routes/(app)/components/SessionQueue.svelte';
	import type { QueueItem } from '../../routes/(app)/components/SessionQueue.svelte';
	import ByVariant from '../../routes/(app)/components/ByVariant.svelte';
	import MarketOverview from '../../routes/(app)/components/MarketOverview.svelte';
	import FontEVCompare from '../../routes/(app)/components/FontEVCompare.svelte';
	import PlannerPage from './PlannerPage.svelte';
	import RunHistoryPage from './RunHistoryPage.svelte';

	const TABS = ['Session', 'Rankings', 'Font EV', 'Market', 'Planner', 'Runs'] as const;
	type Tab = typeof TABS[number];
	let activeTab = $state<Tab>('Session');

	// --- Lab mode ---
	const LAB_MODES = ['Normal', 'Dedication'] as const;
	type LabMode = typeof LAB_MODES[number];
	let labMode = $state<LabMode>('Normal');
	let isDedication = $derived(labMode === 'Dedication');
	let labModeForChild = $derived<'normal' | 'dedication'>(isDedication ? 'dedication' : 'normal');

	// The market lives in the ssot store (POE-163) — this page reads `ssot.*` and
	// writes through its setters. Lab mode is the only selection still restored
	// from Rust here: it is not part of the SSOT snapshot.
	function restoreFromRust() {
		invoke<string>('get_lab_mode')
			.then((mode) => {
				if (mode === 'Normal' || mode === 'Dedication') labMode = mode;
			})
			.catch(e => console.warn('[LabPage] get_lab_mode failed:', e));
	}

	restoreFromRust();

	// Reset Everything rewrites mode and both markets in Rust. The store's poll
	// would pick the markets up within an interval, but the top bar would show
	// the pre-reset selection until then, so pull both immediately.
	$effect(() => {
		let cancelled = false;
		const unlistenPromise = listen('settings-reset', () => {
			if (cancelled) return;
			restoreFromRust();
			fetchSsot();
		});
		return () => {
			cancelled = true;
			unlistenPromise.then(unlisten => unlisten());
		};
	});

	const LAB_MODE_OPTIONS = LAB_MODES.map((m) => ({ value: m, label: m }));
	const NORMAL_MARKET_OPTIONS = VARIANTS.map((v) => ({ value: v, label: v }));
	const DEDICATION_MARKET_OPTIONS = DEDICATION_VARIANTS.map((v) => ({ value: v, label: v }));
	const POOL_OPTIONS = DEDICATION_POOLS.map((p) => ({ value: p, label: DEDICATION_POOL_LABELS[p] }));

	const MARKET_TOOLTIP =
		'Market you are farming. Rankings and Pool Overview follow it, and the run is stamped with it when it uploads.';

	// --- Discard the pending font session ---
	// The stamp is taken once, at send time, so any market click before the send
	// re-stamps the whole accumulated session. Two-step confirm because this
	// button sits beside the frequently clicked market buttons and a discarded
	// run cannot be recaptured.
	let discardArmed = $state(false);
	let discardTimer: ReturnType<typeof setTimeout> | null = null;
	const DISCARD_ARM_MS = 4000;

	function handleDiscardFontSession() {
		if (discardArmed) {
			if (discardTimer) clearTimeout(discardTimer);
			discardTimer = null;
			discardArmed = false;
			invoke('discard_font_session')
				.catch(e => console.warn('[LabPage] discard_font_session failed:', e));
			return;
		}
		discardArmed = true;
		if (discardTimer) clearTimeout(discardTimer);
		discardTimer = setTimeout(() => {
			discardTimer = null;
			discardArmed = false;
		}, DISCARD_ARM_MS);
	}

	$effect(() => () => {
		if (discardTimer) clearTimeout(discardTimer);
		discardTimer = null;
	});

	// Rankings generation guard + failure surfacing. A re-fetch that loses a race
	// or fails silently would leave the previous mode's rows on screen, and the
	// error banner is the only thing that says the numbers are not current.
	let bestPlaysGeneration = 0;
	let bestPlaysError = $state('');

	function refreshBestPlays() {
		const generation = ++bestPlaysGeneration;
		bestPlaysError = '';
		loadBestPlays(isDedication)
			.then(bp => {
				if (generation !== bestPlaysGeneration) return;
				bestPlays = bp;
			})
			.catch(e => {
				if (generation !== bestPlaysGeneration) return;
				console.warn('[LabPage] re-fetch bestPlays failed:', e);
				bestPlays = [];
				bestPlaysError = 'Rankings failed to load for this market — retry to see current numbers.';
			});
	}

	// Rankings are fetched per variant, not as one global top-100. The server ranks
	// across all variants, so a single capped list let the best-ranking variants
	// crowd the others out and By Variant showed a starved slice (POE-132). The
	// merged result is a superset of the old global list.
	//
	// Dedication mode splits the same way, over its two corrupted markets: the
	// rankings show both, filtered client-side by market and pool.
	async function loadBestPlays(dedication: boolean): Promise<GemPlay[]> {
		if (dedication) {
			const perVariant = await Promise.all(
				DEDICATION_VARIANTS.map(v => fetchBestPlays(v, undefined, undefined, 100, undefined, 'dedication')),
			);
			return perVariant.flat();
		}
		const perVariant = await Promise.all(
			VARIANTS.map(v => fetchBestPlays(v, undefined, undefined, 100)),
		);
		return perVariant.flat();
	}

	function handleLabModeChange(mode: LabMode) {
		labMode = mode;
		invoke('set_lab_mode', { mode }).catch(e => console.warn('[LabPage] set_lab_mode failed:', e));
		// Re-fetch rankings for the new mode.
		refreshBestPlays();
	}

	let status = $state<StatusData | null>(null);
	let bestPlays = $state<GemPlay[]>([]);
	let marketOverview = $state<MarketOverviewData | null>(null);
	let loading = $state(true);
	let error = $state('');
	let mercure = $state<MercureConnection | null>(null);
	let refreshKey = $state(0);

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
	let mercureDebounceTimer: ReturnType<typeof setTimeout> | null = null;
	const MERCURE_DEBOUNCE_MS = 2000;
	const MERCURE_JITTER_MS = 4000;

	function debouncedMercureUpdate() {
		if (mercureDebounceTimer) clearTimeout(mercureDebounceTimer);
		// Re-rolled on every fire, not once per session: a fixed per-client offset
		// would put the same clients in the same slot on every publish, which only
		// spreads the herd once instead of on each tick.
		const delay = MERCURE_DEBOUNCE_MS + Math.random() * MERCURE_JITTER_MS;
		mercureDebounceTimer = setTimeout(() => {
			mercureDebounceTimer = null;
			refreshKey++;
			loadAll();
		}, delay);
	}

	// --- Mercure connection guard ---
	// $state flag that flips once (false → true) when store.status first arrives.
	// Used as the sole dependency for the Mercure effect so it fires exactly once,
	// not on every store.status mutation (game focus, lab state, etc.).
	let statusReady = $state(false);

	// --- Session Queue state ---
	let sessionQueue = $state<QueueItem[]>([]);
	let autoClearMinutes = $state(2);
	// Load persisted autoclear from Rust settings
	$effect(() => {
		invoke<number>('get_autoclear_minutes').then(m => { autoClearMinutes = m; })
			.catch(() => {});
	});
	let autoClearSecondsLeft = $state(0);
	let autoClearTimeout: ReturnType<typeof setTimeout> | null = null;
	let autoClearInterval: ReturnType<typeof setInterval> | null = null;

	function resetAutoClearTimer() {
		if (autoClearTimeout) clearTimeout(autoClearTimeout);
		if (autoClearInterval) clearInterval(autoClearInterval);
		const totalSeconds = autoClearMinutes * 60;
		autoClearSecondsLeft = totalSeconds;
		autoClearInterval = setInterval(() => {
			autoClearSecondsLeft = Math.max(0, autoClearSecondsLeft - 1);
		}, 1000);
		autoClearTimeout = setTimeout(() => {
			sessionQueue = [];
			if (autoClearInterval) clearInterval(autoClearInterval);
			autoClearSecondsLeft = 0;
		}, totalSeconds * 1000);
	}

	function handleQueueGem(gem: string, variant: string, roi: number, tradeData: TradeLookupResult | null) {
		if (sessionQueue.some((q) => q.gem === gem && q.variant === variant)) return;

		const item: QueueItem = {
			gem,
			variant,
			pickedAt: new Date(),
			snapshotROI: roi,
			snapshotFloor: tradeData?.priceFloor ?? 0,
			snapshotFloorOriginal: tradeData?.listings[0]?.price ?? tradeData?.priceFloor ?? 0,
			snapshotCurrency: tradeData?.listings[0]?.currency ?? 'chaos',
			snapshotDivineRate: tradeData?.divinePrice ?? 0,
		};

		sessionQueue = [...sessionQueue, item];
	}

	async function handleRefreshQueue() {
		// Mark all items as refreshing
		sessionQueue = sessionQueue.map((item) => ({ ...item, refreshing: true }));

		await Promise.allSettled(
			sessionQueue.map(async (item) => {
				// Rows are addressed by (gem, variant), the same key handleQueueGem
				// dedupes on — not by index. A row removed mid-refresh shifts every
				// index after it, and a session can hold both markets at once, so an
				// index write lands on a different gem in a different market.
				const patch = (fn: (q: QueueItem) => QueueItem) => {
					sessionQueue = sessionQueue.map((q) =>
						q.gem === item.gem && q.variant === item.variant ? fn(q) : q
					);
				};
				try {
					// Mode comes from the queued item's own variant, not from the
					// current lab mode: the snapshot floor was taken against a
					// corrupted market, so a refresh without it would search
					// uncorrupted listings and difference two different markets.
					// The divine rate must match too — without it Rust leaves
					// divine-priced listings unnormalized, so the refreshed floor
					// and the snapshot are quoted in different currencies.
					const result = await invoke<TradeLookupResult>('trade_lookup', {
						gem: item.gem,
						variant: item.variant,
						divineRate: status?.divinePrice || undefined,
						mode: (DEDICATION_VARIANTS as readonly string[]).includes(item.variant) ? 'dedication' : undefined,
					});

					if (result) {
						patch(q => ({
							...q,
							currentFloor: result.priceFloor,
							currentFloorOriginal: result.listings[0]?.price ?? result.priceFloor,
							currentCurrency: result.listings[0]?.currency ?? 'chaos',
							priceDelta: result.priceFloor - q.snapshotFloor,
							refreshing: false,
						}));
					} else {
						patch(q => ({ ...q, refreshing: false }));
					}
				} catch {
					patch(q => ({ ...q, refreshing: false }));
				}
			})
		);

	}

	function handleRemoveFromQueue(index: number) {
		sessionQueue = sessionQueue.filter((_, i) => i !== index);
	}

	function handleClearQueue() {
		sessionQueue = [];
		if (autoClearTimeout) clearTimeout(autoClearTimeout);
		if (autoClearInterval) clearInterval(autoClearInterval);
		autoClearTimeout = null;
		autoClearInterval = null;
		autoClearSecondsLeft = 0;
	}

	function handleAutoClearChange(mins: number) {
		autoClearMinutes = mins;
		invoke('set_autoclear_minutes', { minutes: mins }).catch(() => {});
		resetAutoClearTimer();
	}

	async function loadAll() {
		// Rankings go through the same generation counter as a manual refresh.
		// This path awaits three requests where refreshBestPlays awaits one, so it
		// is systematically the slower of the two: without sharing the counter, a
		// Mercure reload started before a market switch lands after it and puts
		// the old market's rows back under the new heading.
		const generation = ++bestPlaysGeneration;
		try {
			error = '';
			const [s, bp, mo] = await Promise.all([
				fetchStatus(),
				loadBestPlays(isDedication),
				fetchMarketOverview(),
			]);
			status = s;
			// One rate for every price on screen — see $lib/price.svelte.
			setDivineRate(s?.divinePrice ?? 0);
			if (generation === bestPlaysGeneration) {
				bestPlays = bp;
				bestPlaysError = '';
			}
			marketOverview = mo;

			// Update connection status from Mercure
			if (mercure) {
				status = { ...status, connected: mercure.connected };
			}
		} catch (e: any) {
			error = e?.message || 'Failed to load dashboard data';
		} finally {
			loading = false;
		}
	}


	// Detect when store.status first becomes available.
	// statusReady flips once (false → true) and never changes again,
	// so effects that depend on it run exactly once.
	$effect(() => {
		if (store.status && !statusReady) {
			statusReady = true;
		}
	});

	// Initial data load — fires once when status is ready.
	// Subsequent reloads come from Mercure events via debouncedMercureUpdate.
	// untrack prevents Svelte from tracking isDedication (read inside loadAll),
	// which would re-trigger the full reload on mode change and overwrite
	// status.connected, causing a disconnect flash.
	$effect(() => {
		if (!statusReady) return;
		// No longer waits on the persisted market: the rankings fetch both, so
		// nothing the first load reads depends on the restore finishing.
		untrack(() => loadAll());
	});

	// Mercure connection — connects once when status is ready.
	// statusReady only changes once, so this effect never re-runs and the
	// EventSource is never torn down by Svelte's cleanup-on-rerun cycle.
	$effect(() => {
		if (!statusReady) return;

		const connection = connectMercure(debouncedMercureUpdate, (connected) => {
			store.serverConnected = connected;
			if (status) {
				status = { ...status, connected };
			}
		}, (data) => {
			// Layout updated or reset on server — notify overlays
			import('@tauri-apps/api/event').then(({ emit }) => {
				emit('lab-layout-updated', { difficulty: data?.difficulty, action: data?.action })
					.catch(e => console.warn('[mercure] failed to emit layout update:', e));
			}).catch(() => {}); // expected: not in Tauri context (web dashboard)
		});
		mercure = connection;

		return () => {
			connection?.close();
			if (mercureDebounceTimer) clearTimeout(mercureDebounceTimer);
		};
	});
</script>

<div class="dashboard">
	<!-- Tab bar + scan controls -->
	<div class="tab-bar">
		<div class="tabs">
			{#each TABS as tab}
				<button class="tab" class:active={activeTab === tab} onclick={() => { activeTab = tab; }}>
					{tab}
				</button>
			{/each}
		</div>
		<div class="bar-right">
			<SegmentedButtons
				value={labMode}
				options={LAB_MODE_OPTIONS}
				onselect={(mode) => handleLabModeChange(mode as LabMode)}
			/>
			{#if isDedication}
				<SegmentedButtons
					value={ssot.dedicationVariant}
					options={DEDICATION_MARKET_OPTIONS}
					onselect={(variant) => setDedicationSelection(variant, ssot.dedicationPool)}
					title={MARKET_TOOLTIP}
				/>
				<SegmentedButtons
					value={ssot.dedicationPool}
					options={POOL_OPTIONS}
					onselect={(pool) => setDedicationSelection(ssot.dedicationVariant, pool)}
					title="Gem pool you are farming — Rankings and Pool Overview follow it."
				/>
			{:else}
				<SegmentedButtons
					value={ssot.normalVariant}
					options={NORMAL_MARKET_OPTIONS}
					onselect={(variant) => setNormalVariant(variant)}
					title={MARKET_TOOLTIP}
				/>
			{/if}
			{#if (store.status?.font_session_rounds ?? 0) > 0}
				<Button
					variant={discardArmed ? 'danger' : 'default'}
					onclick={handleDiscardFontSession}
					title="The captured font session uploads stamped with the currently selected market. Discard it if it was captured against the wrong one."
				>
					{discardArmed ? 'Confirm discard' : `Discard font session (${store.status?.font_session_rounds ?? 0})`}
				</Button>
			{/if}
			<div class="scan-controls">
				<span class="scan-state" class:picking={store.status?.state === 'PickingGems'}>{store.status?.state || '...'}</span>
				{#if store.status?.state === 'PickingGems'}
					<button class="scan-btn scan-stop" onclick={() => invoke('stop_scanning').catch((e: any) => console.error('Stop scan failed:', e))}>Stop</button>
				{:else}
					<button class="scan-btn" onclick={() => invoke('start_scanning').catch((e: any) => console.error('Start scan failed:', e))}>Scan</button>
				{/if}
			</div>
		</div>
	</div>

	{#if status}
		<Header {status} />
	{/if}

	{#if loading}
		<div class="loading">
			<div class="loading-spinner"></div>
			<p>Loading dashboard data...</p>
		</div>
	{:else if error}
		<div class="error-banner">
			<p class="error-text">{error}</p>
			<button class="retry-btn" onclick={loadAll}>Retry</button>
		</div>
	{:else}
		<!-- Comparator + SessionQueue always mounted (event listeners must stay active).
		     Hidden via CSS when not on Session tab to avoid unmount/remount. -->
		<div class:tab-hidden={activeTab !== 'Session'}>
			<Comparator
				divineRate={status?.divinePrice || 0}
				onQueueGem={handleQueueGem}
				labMode={labModeForChild}
			/>
			<SessionQueue
				queue={sessionQueue}
				onRemove={handleRemoveFromQueue}
				onClear={handleClearQueue}
				onRefresh={handleRefreshQueue}
			/>
		</div>
		{#if activeTab === 'Rankings'}
			{#if bestPlaysError}
				<div class="error-banner">
					<p class="error-text">{bestPlaysError}</p>
					<button class="retry-btn" onclick={refreshBestPlays}>Retry</button>
				</div>
			{/if}
			<ByVariant allPlays={bestPlays} labMode={labModeForChild} />
		{:else if activeTab === 'Font EV'}
			<FontEVCompare {refreshKey} labMode={labModeForChild} />
		{:else if activeTab === 'Market'}
			{#if marketOverview}
				<MarketOverview data={marketOverview} />
			{/if}
		{:else if activeTab === 'Planner'}
			<PlannerPage />
		{:else if activeTab === 'Runs'}
			<RunHistoryPage />
		{/if}
	{/if}
</div>

<style>
	/* Tab bar + scan controls */
	.tab-bar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		/* Wrap: this row holds six nav tabs, the lab-mode buttons, and in
		   Dedication mode a market picker AND a pool picker, plus the scan
		   controls. Nothing in it can shrink below its content, and the default
		   window is 1024px (src-tauri/tauri.conf.json), so a no-wrap row pushed
		   the Scan button out of reach. */
		flex-wrap: wrap;
		gap: 8px;
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		padding: 0 16px;
		margin-bottom: 12px;
	}
	.tabs {
		display: flex;
		gap: 0;
	}
	.tab {
		background: transparent;
		border: none;
		border-bottom: 2px solid transparent;
		color: var(--color-lab-text-secondary);
		padding: 10px 16px;
		font-size: 0.8125rem;
		font-weight: 600;
		cursor: pointer;
		font-family: inherit;
		transition: color 0.15s, border-color 0.15s;
	}
	.tab:hover {
		color: var(--color-lab-text);
	}
	.tab.active {
		color: var(--color-lab-text);
		border-bottom-color: var(--color-lab-blue);
	}
	.bar-right {
		display: flex;
		align-items: center;
		gap: 12px;
	}
	.scan-controls {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.scan-state {
		font-size: 0.75rem;
		font-weight: 600;
		color: var(--color-lab-text-secondary);
	}
	.scan-state.picking {
		color: var(--color-lab-green);
	}
	.scan-btn {
		background: var(--color-lab-blue);
		border: none;
		color: #fff;
		padding: 4px 12px;
		font-size: 0.75rem;
		font-weight: 600;
		cursor: pointer;
		font-family: inherit;
	}
	.scan-btn:hover {
		opacity: 0.9;
	}
	.scan-stop {
		background: var(--color-lab-yellow);
		color: #1a1a2e;
	}

	.tab-hidden {
		display: none;
	}

	/* Dashboard — adapted for desktop viewport */
	.dashboard {
		max-width: 100%;
		margin: 0;
		padding: 0 0 16px;
	}
	/* Override child component section spacing for desktop density */
	.dashboard :global(section),
	.dashboard :global(.section) {
		padding: 16px !important;
		margin-bottom: 16px !important;
	}
	.dashboard :global(.section-header) {
		margin-bottom: 8px;
	}
	.dashboard :global(.comparator-input) {
		margin-bottom: 12px;
	}
	.dashboard :global(.search-input) {
		padding: 8px 12px;
	}
	/* Tighter table row padding */
	.dashboard :global(.plays-table td) {
		padding: 8px 6px;
	}
	.dashboard :global(.plays-table th) {
		padding: 6px 6px;
	}
	/* Let table auto-size instead of fixed layout — prevents text overlap */
	.dashboard :global(.plays-table) {
		table-layout: auto;
	}
	.dashboard :global(.col-name) {
		width: auto;
		min-width: 180px;
	}
	.dashboard :global(.gem-name) {
		white-space: normal;
		word-break: break-word;
	}
	.dashboard :global(.col-num) {
		width: auto;
	}
	.dashboard :global(.col-signal) {
		width: auto;
	}
	.dashboard :global(.col-signals) {
		width: auto;
	}
	.dashboard :global(.col-tier) {
		width: auto;
	}
	.dashboard :global(.col-sell) {
		width: auto;
	}
	.dashboard :global(.col-spark) {
		width: 80px;
	}
	/* Compact header for desktop */
	.dashboard :global(.header) {
		padding: 12px 16px !important;
		margin-bottom: 16px !important;
	}
	.dashboard :global(.header-row) {
		flex-wrap: wrap;
		gap: 8px;
	}
	.dashboard :global(.title) {
		font-size: 1rem;
	}
	/* Compact comparator cards — always 3 columns, compact for desktop */
	.dashboard :global(.compare-card) {
		padding: 10px;
		font-size: 0.8125rem;
	}
	.dashboard :global(.cards-row) {
		grid-template-columns: repeat(3, 1fr) !important;
		gap: 8px;
	}
	.dashboard :global(.card-name-row) {
		gap: 6px;
		margin-bottom: 8px;
	}
	.dashboard :global(.card-name) {
		font-size: 0.875rem;
	}
	.dashboard :global(.price-raw) {
		font-size: 1rem;
	}
	.dashboard :global(.price-risk-adj) {
		font-size: 0.6875rem;
	}
	.dashboard :global(.urgency-slot) {
		min-height: 50px;
		margin: 6px 0;
	}
	.dashboard :global(.urgency-banner) {
		padding: 6px 8px;
		font-size: 0.75rem;
	}
	.dashboard :global(.trade-section) {
		padding-top: 8px;
		margin: 8px 0;
	}
	.dashboard :global(.sparkline-row) {
		margin: 8px 0;
		padding: 4px 0;
	}
	.dashboard :global(.history-line) {
		font-size: 0.75rem;
		padding: 3px 0;
		gap: 4px;
	}
	.dashboard :global(.card-rec) {
		margin-top: 8px;
		padding: 4px 0;
		font-size: 0.875rem;
	}
	/* Tighter variant blocks */
	.dashboard :global(.variant-block) {
		padding: 14px 16px;
		margin-bottom: 12px;
	}
	/* Font EV table — compact for desktop (! needed to override scoped styles) */
	.dashboard :global(.ft) {
		border-spacing: 1px !important;
	}
	.dashboard :global(.ft th) {
		padding: 4px 4px !important;
		font-size: 0.875rem !important;
	}
	.dashboard :global(.ft td) {
		padding: 2px 4px 6px !important;
	}
	.dashboard :global(.var-header) {
		width: 65px !important;
		text-align: center !important;
		padding: 4px 2px !important;
		padding-left: 4px !important;
		white-space: nowrap;
	}
	/* 65px was sized for the old "Skill Gems" label, whose longest token is
	   "Skill". "Non-Transfigured" breaks to "Transfigured" (~90px), and with
	   border-collapse: separate and no table-layout: fixed, min-content beats the
	   width, so the column silently widened and squeezed the three colour
	   columns. Allow the wider label rather than pretend 65px holds it. */
	.dashboard :global(.var) {
		width: 110px !important;
		text-align: center !important;
		padding: 2px 2px !important;
		font-size: 1rem;
	}
	.dashboard :global(.ev) {
		font-size: 1.1rem;
		margin-bottom: 3px;
	}
	.dashboard :global(.tier-lines) {
		gap: 1px;
	}
	.dashboard :global(.tier-row) {
		font-size: 0.8rem;
		gap: 4px;
	}
	.dashboard :global(.buy-header),
	.dashboard :global(.buy-col) {
		width: 50px;
	}
	.dashboard :global(.buy-btn) {
		width: 44px;
		font-size: 0.6rem;
		padding: 2px 0;
	}
	/* Loading */
	.loading {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		padding: 40px 16px;
		color: var(--color-lab-text-secondary);
		font-size: 1rem;
	}
	.loading-spinner {
		width: 32px;
		height: 32px;
		border: 3px solid var(--color-lab-border);
		border-top-color: var(--color-lab-blue);
		border-radius: 50%;
		animation: spin 0.8s linear infinite;
		margin-bottom: 16px;
	}
	@keyframes spin {
		to { transform: rotate(360deg); }
	}

	/* Error */
	.error-banner {
		background: rgba(239, 68, 68, 0.1);
		border: 1px solid rgba(239, 68, 68, 0.3);
		padding: 12px 16px;
		margin-bottom: 16px;
		display: flex;
		align-items: center;
		justify-content: space-between;
	}
	.error-text {
		color: var(--color-lab-red);
		font-size: 0.9375rem;
		margin: 0;
	}
	.retry-btn {
		background: rgba(239, 68, 68, 0.15);
		border: 1px solid rgba(239, 68, 68, 0.4);
		color: var(--color-lab-red);
		padding: 8px 20px;
		font-size: 0.875rem;
		font-weight: 600;
		cursor: pointer;
		font-family: inherit;
	}
	.retry-btn:hover {
		background: rgba(239, 68, 68, 0.25);
	}

</style>
