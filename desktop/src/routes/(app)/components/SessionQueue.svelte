<script lang="ts">
	import GemIcon from './GemIcon.svelte';
	import Tooltip from '$lib/components/Tooltip.svelte';

	export interface QueueItem {
		gem: string;
		variant: string;
		pickedAt: Date;
		snapshotROI: number;
		snapshotFloor: number;
		snapshotFloorOriginal: number;
		snapshotCurrency: string;
		snapshotDivineRate: number;
		currentFloor?: number;
		currentFloorOriginal?: number;
		currentCurrency?: string;
		priceDelta?: number;
		recommendation?: string;
		refreshing?: boolean;
	}

	let {
		queue,
		onRemove,
		onClear,
		onRefresh,
	}: {
		queue: QueueItem[];
		onRemove: (index: number) => void;
		onClear: () => void;
		onRefresh: () => void;
	} = $props();

	let anyRefreshing = $derived(queue.some((item) => item.refreshing));

	function fmtPrice(v: number): string {
		return Number.isInteger(v) ? v.toString() : v.toFixed(1);
	}

	function formatSnapshotPrice(item: QueueItem): string {
		if (item.snapshotCurrency === 'divine') {
			return `${fmtPrice(item.snapshotFloorOriginal)} div (${fmtPrice(item.snapshotFloor)}c)`;
		}
		return `${fmtPrice(item.snapshotFloor)}c`;
	}

	function formatCurrentPrice(item: QueueItem): string {
		if (item.currentFloor == null) return '\u2014';
		if (item.currentCurrency === 'divine' && item.currentFloorOriginal != null) {
			return `${fmtPrice(item.currentFloorOriginal)} div (${fmtPrice(item.currentFloor)}c)`;
		}
		return `${fmtPrice(item.currentFloor)}c`;
	}

	function formatDelta(item: QueueItem): string {
		if (item.priceDelta == null) return '\u2014';
		const sign = item.priceDelta > 0 ? '+' : '';
		const arrow = item.priceDelta > 0 ? '\u2191' : item.priceDelta < 0 ? '\u2193' : '';
		return `${arrow} ${sign}${fmtPrice(item.priceDelta)}c`;
	}

	function deltaClass(item: QueueItem): string {
		if (item.priceDelta == null || item.priceDelta === 0) return '';
		return item.priceDelta > 0 ? 'delta-positive' : 'delta-negative';
	}

	function formatPickedAt(date: Date): string {
		return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
	}

	function recClass(rec: string): string {
		if (rec === 'BEST') return 'rec-best';
		if (rec === 'AVOID') return 'rec-avoid';
		return 'rec-ok';
	}
</script>

{#if queue.length > 0}
	<section class="section">
		<div class="section-header">
			<h2 class="section-title">Session Queue</h2>
			<div class="header-actions">
				<button class="refresh-btn" onclick={onRefresh} disabled={anyRefreshing}>
					{#if anyRefreshing}
						<span class="btn-spinner"></span>
					{/if}
					Refresh Prices
				</button>
				<button class="clear-btn" onclick={onClear}>Clear Session</button>
			</div>
		</div>

		<div class="queue-table-wrapper">
			<table class="queue-table">
				<thead>
					<tr>
						<th class="col-gem">Gem</th>
						<th class="col-price">Snapshot</th>
						<th class="col-price">Current</th>
						<th class="col-delta">Delta</th>
						<th class="col-action">Action</th>
						<th class="col-remove"></th>
					</tr>
				</thead>
				<tbody>
					{#each queue as item, i}
						<tr class="queue-row">
							<td class="col-gem">
								<div class="gem-cell">
									<GemIcon name={item.gem} size={24} />
									<span class="gem-name">{item.gem}</span>
									<span class="variant-badge">{item.variant}</span>
								</div>
								<div class="picked-at">{formatPickedAt(item.pickedAt)}</div>
							</td>
							<td class="col-price">{formatSnapshotPrice(item)}</td>
							<td class="col-price">
								{#if item.refreshing}
									<span class="cell-spinner"></span>
								{:else}
									{formatCurrentPrice(item)}
								{/if}
							</td>
							<td class="col-delta {deltaClass(item)}">{formatDelta(item)}</td>
							<td class="col-action">
								{#if item.recommendation}
									<span class="rec-badge {recClass(item.recommendation)}">{item.recommendation}</span>
								{:else}
									<Tooltip text="Awaiting price refresh"><span class="no-action">&#9202;</span></Tooltip>
								{/if}
							</td>
							<td class="col-remove">
								<Tooltip text="Remove from queue"><button class="remove-btn" onclick={() => onRemove(i)}>&#215;</button></Tooltip>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	</section>
{/if}

<style>
	.section {
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		padding: 28px;
		margin-bottom: 32px;
	}
	.section-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 20px;
		gap: 16px;
	}
	.section-title {
		font-size: 1.125rem;
		font-weight: 700;
		color: var(--color-lab-text);
		margin: 0;
	}

	.header-actions {
		display: flex;
		gap: 10px;
	}
	.refresh-btn {
		background: rgba(34, 197, 94, 0.15);
		border: 1px solid rgba(34, 197, 94, 0.4);
		color: var(--color-lab-green);
		padding: 6px 14px;
		font-size: 0.875rem;
		font-weight: 600;
		cursor: pointer;
		font-family: inherit;
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.refresh-btn:hover:not(:disabled) {
		background: rgba(34, 197, 94, 0.25);
	}
	.refresh-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
	.clear-btn {
		background: rgba(239, 68, 68, 0.12);
		border: 1px solid rgba(239, 68, 68, 0.3);
		color: var(--color-lab-red);
		padding: 6px 14px;
		font-size: 0.875rem;
		font-weight: 600;
		cursor: pointer;
		font-family: inherit;
	}
	.clear-btn:hover {
		background: rgba(239, 68, 68, 0.2);
	}

	.btn-spinner {
		display: inline-block;
		width: 12px;
		height: 12px;
		border: 2px solid var(--color-lab-border);
		border-top-color: var(--color-lab-green);
		border-radius: 50%;
		animation: spin 0.8s linear infinite;
	}
	.cell-spinner {
		display: inline-block;
		width: 14px;
		height: 14px;
		border: 2px solid var(--color-lab-border);
		border-top-color: var(--color-lab-text);
		border-radius: 50%;
		animation: spin 0.8s linear infinite;
	}
	@keyframes spin {
		to { transform: rotate(360deg); }
	}

	.queue-table-wrapper {
		overflow-x: auto;
	}
	.queue-table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.9375rem;
	}
	.queue-table thead th {
		text-align: left;
		font-size: 0.6875rem;
		font-weight: 700;
		color: var(--color-lab-text-secondary);
		text-transform: uppercase;
		letter-spacing: 0.04em;
		padding: 8px 10px;
		background: rgba(42, 45, 55, 0.6);
		border-bottom: 1px solid var(--color-lab-border);
	}
	.queue-row td {
		padding: 10px;
		border-bottom: 1px solid rgba(42, 45, 55, 0.4);
		vertical-align: middle;
	}
	.queue-row:last-child td {
		border-bottom: none;
	}

	.col-gem {
		min-width: 200px;
	}
	.gem-cell {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.gem-name {
		color: var(--color-lab-text);
		font-weight: 600;
	}
	.variant-badge {
		font-size: 0.75rem;
		color: var(--color-lab-text-secondary);
		background: rgba(42, 45, 55, 0.8);
		padding: 2px 6px;
		white-space: nowrap;
	}
	.picked-at {
		font-size: 0.75rem;
		color: var(--color-lab-text-secondary);
		opacity: 0.7;
		margin-top: 3px;
		padding-left: 32px;
	}

	.col-price {
		color: var(--color-lab-text);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}

	.col-delta {
		font-weight: 600;
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
		color: var(--color-lab-text-secondary);
	}
	.delta-positive {
		color: var(--color-lab-green);
	}
	.delta-negative {
		color: var(--color-lab-red);
	}

	.col-action {
		text-align: center;
	}
	.rec-badge {
		font-size: 0.75rem;
		font-weight: 700;
		padding: 2px 8px;
		text-transform: uppercase;
		letter-spacing: 0.03em;
	}
	.rec-best {
		color: var(--color-lab-green);
		background: rgba(34, 197, 94, 0.1);
	}
	.rec-ok {
		color: var(--color-lab-yellow);
		background: rgba(234, 179, 8, 0.1);
	}
	.rec-avoid {
		color: var(--color-lab-red);
		background: rgba(239, 68, 68, 0.12);
	}
	.no-action {
		color: var(--color-lab-text-secondary);
	}

	.col-remove {
		text-align: center;
		width: 40px;
	}
	.remove-btn {
		background: none;
		border: none;
		color: var(--color-lab-text-secondary);
		font-size: 1.125rem;
		cursor: pointer;
		padding: 2px 6px;
		font-family: inherit;
		line-height: 1;
	}
	.remove-btn:hover {
		color: var(--color-lab-red);
	}
</style>
