<script lang="ts">
	/**
	 * Currency Exchange — the ranked arbitrage plays the server computes each
	 * hour (POE-175), as a table with one filter over it.
	 *
	 * Deliberately thin. Every derivation the header and the cells need lives in
	 * `$lib/exchange/view`, which is pure and unit-tested; this file owns only
	 * the four loose variables that view module reads, the fetch that fills
	 * them, and the markup. A `.svelte` file has no unit-test harness here, so
	 * anything with a rule in it belongs on the other side of that import.
	 *
	 * It does not open its own Mercure connection: `LabPage.svelte` owns the one
	 * connection for the whole app (ADR-014 keeps every page mounted) and
	 * re-emits the Currency Exchange payloads as a Tauri `currency-exchange-updated`
	 * event, which this page listens for and answers with a jittered refetch.
	 *
	 * A failed fetch never blanks the table: the last plays stay on screen and
	 * the header says how old they are. The alternative — an empty page on a
	 * dropped connection — throws away the only data the user has while the
	 * server is the thing that is down.
	 */
	import { listen } from '@tauri-apps/api/event';
	import { fetchCurrencyExchangePlays, type CurrencyExchangeResponse } from '$lib/api';
	import {
		MODE_OPTIONS,
		deriveState,
		formatEdge,
		formatTime,
		formatVolume,
		legLabel,
		parseMode,
		refetchDelay
	} from '$lib/exchange/view';
	import { persisted } from '$lib/prefs.svelte';
	import SegmentedButtons from '$lib/components/SegmentedButtons.svelte';
	import Tooltip from '$lib/components/Tooltip.svelte';

	/** Persisted (ADR-013): the mode filter survives restarts. */
	const mode = persisted('currencyExchangeMode', 'all');

	let result = $state<CurrencyExchangeResponse | null>(null);
	let lastFetchedAt = $state<Date | null>(null);
	let lastError = $state<string | null>(null);
	let loading = $state(false);

	/**
	 * The clock the relative strings are measured against. Ticked rather than
	 * read inline: "updated 3 min ago" is derived once and would otherwise stay
	 * frozen at its fetch-time wording until the next Mercure publish, which on
	 * a quiet hour is an hour away.
	 */
	let now = $state(new Date());
	const NOW_TICK_MS = 30_000;

	const viewState = $derived(deriveState({ result, lastFetchedAt, lastError, now }));

	/**
	 * Which `load()` call owns the state. Two fetches are routinely in flight at
	 * once — a cold start fires `all` and then the restored mode a tick later,
	 * and a Mercure refetch can be overtaken by a mode switch — and the requests
	 * are not guaranteed to answer in the order they were sent. Without the
	 * counter the slower one wins and the table shows one mode's plays under the
	 * other mode's button. Same pattern as `LabPage.svelte`'s
	 * `bestPlaysGeneration`.
	 *
	 * A plain `let`, not `$state`: nothing renders it, and making it reactive
	 * would re-run the effects that call `load()`.
	 */
	let loadGeneration = 0;

	async function load() {
		const generation = ++loadGeneration;
		loading = true;
		try {
			const response = await fetchCurrencyExchangePlays(parseMode(mode.value));
			if (generation !== loadGeneration) return;
			result = response;
			lastFetchedAt = new Date();
			lastError = null;
		} catch (e: any) {
			if (generation !== loadGeneration) return;
			// Keep `result`: the header downgrades to `stale` and the table stays.
			lastError = String(e?.message ?? e);
		} finally {
			// A superseded fetch leaves `loading` to the call that overtook it.
			if (generation === loadGeneration) loading = false;
		}
	}

	// Fetch on mount and on every mode change — one effect for both.
	$effect(() => {
		// The explicit read is the dependency: reactivity must not rest on
		// `load()` happening to read `mode.value` before its first await.
		void mode.value;
		load();
	});

	// Re-derive the relative strings on a wall clock the data does not move.
	$effect(() => {
		const timer = setInterval(() => {
			now = new Date();
		}, NOW_TICK_MS);
		return () => clearInterval(timer);
	});

	/**
	 * Mercure fan-out from LabPage. The refetch is debounced AND jittered
	 * (`refetchDelay`) because every client receives the same publish within
	 * milliseconds — see the constants' comment in `$lib/exchange/view`.
	 */
	let refetchTimer: ReturnType<typeof setTimeout> | null = null;

	$effect(() => {
		let cancelled = false;
		const promise = listen('currency-exchange-updated', () => {
			if (cancelled) return;
			if (refetchTimer) clearTimeout(refetchTimer);
			refetchTimer = setTimeout(() => {
				refetchTimer = null;
				load();
			}, refetchDelay());
		});
		return () => {
			cancelled = true;
			if (refetchTimer) {
				clearTimeout(refetchTimer);
				refetchTimer = null;
			}
			// Expected to reject outside a Tauri context (the browser dev server).
			promise.then((unlisten) => unlisten()).catch(() => {});
		};
	});
</script>

<div class="exchange-page" aria-busy={loading}>
	<div class="page-head">
		<h1>Currency Exchange</h1>
		{#if result}
			<span class="league">{result.league}</span>
		{/if}
		<div class="spacer"></div>
		<SegmentedButtons
			value={parseMode(mode.value)}
			options={MODE_OPTIONS}
			onselect={(v) => (mode.value = parseMode(v))}
			title="Which plays to show: every one, single-swap only, or two-swap only."
		/>
	</div>

	<div class="status-line">
		{#if viewState.kind === 'loading'}
			Loading…
		{:else if viewState.kind === 'warming'}
			Waiting for the first Currency Exchange hour…
		{:else if viewState.kind === 'ready'}
			{#if viewState.updatedAgo}
				<Tooltip text={formatTime(result?.lastUpdated ?? null)} position="below">
					<span>updated {viewState.updatedAgo}</span>
				</Tooltip>
				<span class="dot">·</span>
			{/if}
			<span>{result?.count ?? 0} plays</span>
		{:else if viewState.kind === 'stale'}
			<span class="warn">stale since {viewState.staleSince} — server unreachable</span>
		{:else}
			<span class="warn">Couldn't reach the server</span>
		{/if}
	</div>

	{#if result && result.plays.length > 0}
		<div class="table-wrap">
			<table>
				<thead>
					<tr>
						<th class="num">#</th>
						<th>Mode</th>
						<th>Play</th>
						<th class="num">Edge</th>
						<th class="num">Depth</th>
						<th class="num">Hours</th>
					</tr>
				</thead>
				<tbody>
					{#each result.plays as play, i}
						<tr>
							<td class="num mono">{i + 1}</td>
							<td><span class="pill">{play.mode}</span></td>
							<td>
								<div class="legs">
									{#each play.legs as leg}
										<span class="leg" title="{leg.item} | {leg.quote}">{legLabel(leg)}</span>
									{/each}
								</div>
							</td>
							<td class="num mono edge">{formatEdge(play.edge)}</td>
							<td class="num mono">{formatVolume(play.depth)}/h</td>
							<td class="num mono">{play.hoursSeen}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{:else if viewState.kind === 'ready'}
		<div class="empty">No plays pass the filters right now.</div>
	{/if}
</div>

<style>
	.exchange-page {
		display: flex;
		flex-direction: column;
		gap: 12px;
		height: 100%;
		color: var(--color-lab-text);
	}

	.page-head {
		display: flex;
		align-items: center;
		gap: 10px;
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		padding: 10px 16px;
	}

	.page-head h1 {
		font-size: 1rem;
		font-weight: 700;
		color: var(--color-lab-text);
		margin: 0;
	}

	.league {
		font-size: 0.6875rem;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		color: var(--color-lab-text-secondary);
	}

	.spacer {
		flex: 1;
	}

	.status-line {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 0.75rem;
		color: var(--color-lab-text-secondary);
		padding: 0 2px;
	}

	.status-line .warn {
		color: var(--color-lab-yellow);
	}

	.dot {
		opacity: 0.5;
	}

	.table-wrap {
		flex: 1;
		overflow-y: auto;
		/* Three nowrap leg chips on a 1-hop play outrun a narrow window. */
		overflow-x: auto;
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
	}

	table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.8125rem;
	}

	thead {
		position: sticky;
		top: 0;
		background: var(--color-lab-surface);
	}

	th {
		text-align: left;
		padding: 8px 10px;
		font-size: 0.6875rem;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		color: var(--color-lab-text-secondary);
		border-bottom: 1px solid var(--color-lab-border);
		font-weight: 600;
	}

	td {
		padding: 6px 10px;
		color: var(--color-lab-text);
		border-bottom: 1px solid var(--color-lab-border);
		vertical-align: top;
	}

	th.num,
	td.num {
		text-align: right;
		white-space: nowrap;
	}

	td.mono {
		font-family: 'Consolas', 'Monaco', monospace;
		font-weight: 600;
	}

	td.edge {
		color: var(--color-lab-green);
	}

	tr:hover {
		background: rgba(255, 255, 255, 0.02);
	}

	.pill {
		display: inline-block;
		font-size: 0.6rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		border-radius: 3px;
		border: 1px solid currentcolor;
		padding: 0 4px;
		white-space: nowrap;
		color: var(--color-lab-text-secondary);
	}

	.legs {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}

	.leg {
		background: var(--color-lab-bg);
		border: 1px solid var(--color-lab-border);
		border-radius: 3px;
		padding: 1px 6px;
		font-size: 0.75rem;
		white-space: nowrap;
	}

	.empty {
		text-align: center;
		padding: 40px;
		color: var(--color-lab-text-secondary);
	}
</style>
