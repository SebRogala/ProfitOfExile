<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import { listen } from '@tauri-apps/api/event';
	import { untrack } from 'svelte';
	import { store } from '$lib/stores/status.svelte';
	import { ssot, fetchSsot, setNormalVariant, setDedicationSelection } from '$lib/stores/ssot.svelte';
	import { currentDivineRate } from '$lib/stores/divine-rate.svelte';
	import { mercureConnected, subscribeMercureConnection, subscribeMercureUpdate } from '$lib/stores/mercure.svelte';
	import {
		fetchStatus,
		VARIANTS,
		DEDICATION_VARIANTS,
		DEDICATION_POOLS,
		DEDICATION_POOL_LABELS,
		fetchMarketOverview,
		fetchRankingsPage,
	} from '$lib/api';
	import type { TradeLookupResult } from '$lib/tradeApi';
	import SegmentedButtons from '$lib/components/SegmentedButtons.svelte';
	import Button from '$lib/components/Button.svelte';

	import Header from '../../routes/(app)/components/Header.svelte';
	import Comparator from '../../routes/(app)/components/Comparator.svelte';
	import SessionQueue from '../../routes/(app)/components/SessionQueue.svelte';
	import { createLabQueueController, createLabController } from '$lib/comparator/controller.svelte';
	import { floorQuote, type QueueItem } from '$lib/session-queue';
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

	$effect(() => {
		let cancelled = false;
		const unlistenPromise = listen('lab-mode-changed', async () => {
			try {
				const mode = await invoke<string>('get_lab_mode');
				if (cancelled || (mode !== 'Normal' && mode !== 'Dedication')) return;
				labMode = mode;
			} catch (e) {
				console.warn('[LabPage] lab-mode-changed failed:', e);
			}
		});
		return () => {
			cancelled = true;
			unlistenPromise.then(unlisten => unlisten());
		};
	});

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

	const dashboard = createLabController({
		status: fetchStatus, rankings: fetchRankingsPage,
		market: fetchMarketOverview,
		connected: mercureConnected,
		onReload: () => { refreshKey++; }
	});
	const bestPlaysError = $derived(dashboard.state.bestPlaysError);
	const refreshBestPlays = dashboard.refreshBestPlays;
	const loadAll = dashboard.loadAll;

	function handleLabModeChange(mode: LabMode) {
		labMode = mode;
		invoke('set_lab_mode', { mode }).catch(e => console.warn('[LabPage] set_lab_mode failed:', e));
	}

	const status = $derived(dashboard.state.status);
	const bestPlays = $derived(dashboard.state.bestPlays);
	const marketOverview = $derived(dashboard.state.marketOverview);
	const loading = $derived(dashboard.state.loading);
	const error = $derived(dashboard.state.error);
	let refreshKey = $state(0);

	// --- Initial-load guard ---
	// $state flag that flips once (false → true) when store.status first arrives.
	// Used as the sole dependency for the initial load so it fires exactly once,
	// not on every store.status mutation (game focus, lab state, etc.).
	let statusReady = $state(false);

	// --- Session Queue state ---
	let sessionQueue = $state<QueueItem[]>([]);
	const queueController = createLabQueueController({
		queue: {
			get items() { return sessionQueue; },
			set items(value) { sessionQueue = value; }
		},
		lookup: (item) => invoke<TradeLookupResult>('trade_lookup', {
			// Refresh the queued item's market and normalize divine listings with the current rate.
			gem: item.gem, variant: item.variant, divineRate: currentDivineRate() ?? undefined,
			mode: (DEDICATION_VARIANTS as readonly string[]).includes(item.variant) ? 'dedication' : undefined
		})
	});
	$effect(() => dashboard.start(() => {
		handleClearQueue();
		refreshKey++;
	}));
	$effect(() => () => {
		queueController.clear();
		if (autoClearTimeout) clearTimeout(autoClearTimeout);
		if (autoClearInterval) clearInterval(autoClearInterval);
	});
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
			queueController.clear();
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
			snapshot: tradeData ? floorQuote(tradeData) : null,
			snapshotDivineRate: tradeData?.divinePrice ?? 0,
		};

		sessionQueue = [...sessionQueue, item];
	}

	async function handleRefreshQueue() {
		await queueController.refresh();
	}

	function handleRemoveFromQueue(index: number) {
		sessionQueue = sessionQueue.filter((_, i) => i !== index);
	}

	function handleClearQueue() {
		queueController.clear();
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

	// Detect when store.status first becomes available.
	// statusReady flips once (false → true) and never changes again,
	// so effects that depend on it run exactly once.
	$effect(() => {
		if (store.status && !statusReady) {
			statusReady = true;
		}
	});

	// Initial data load — fires once when status is ready.
	// Subsequent reloads come from Mercure events via dashboard.onMercureUpdate.
	// untrack keeps loadAll's reads out of this effect, so a later state change
	// cannot re-trigger the full reload and overwrite status.connected, causing a
	// disconnect flash.
	$effect(() => {
		if (!statusReady) return;
		// No longer waits on the persisted market: the rankings query arrives from
		// ByVariant, so nothing the first load reads depends on the restore finishing.
		untrack(() => loadAll());
	});

	// The app's one Mercure connection lives in the mercure store, started by the layout.
	$effect(() => {
		const stopUpdates = subscribeMercureUpdate(dashboard.onMercureUpdate);
		const stopConnection = subscribeMercureConnection(dashboard.setConnected);
		return () => { stopUpdates(); stopConnection(); dashboard.dispose(); };
	});
</script>

<div class="dashboard">
	<!-- Tab bar + lab state -->
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
			<span class="scan-state" class:picking={store.status?.state === 'PickingGems'}>{store.status?.state || '...'}</span>
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
				divineRate={currentDivineRate() ?? 0}
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
			<ByVariant allPlays={bestPlays} rankingsPending={dashboard.state.rankingsPending} onQuery={dashboard.setRankingsQuery} labMode={labModeForChild} />
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
	/* Tab bar + lab state */
	.tab-bar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		/* Wrap: this row holds six nav tabs, the lab-mode buttons, and in
		   Dedication mode a market picker AND a pool picker, plus the lab
		   state label. Nothing in it can shrink below its content, and the
		   default window is 1024px (src-tauri/tauri.conf.json), so a no-wrap
		   row pushed its right end out of reach. */
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
	.scan-state {
		font-size: 0.75rem;
		font-weight: 600;
		color: var(--color-lab-text-secondary);
	}
	.scan-state.picking {
		color: var(--color-lab-green);
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
