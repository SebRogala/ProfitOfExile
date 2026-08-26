<script lang="ts">
	/**
	 * The trade listings table — price, an optional caller-supplied detail
	 * cell, and how long ago the listing was indexed.
	 *
	 * Lives in `$lib/components/` rather than beside the Comparator because
	 * two consumers already want it in different windows: the gem surface in
	 * the main window, and the mercenary result (POE-202), which the merc
	 * overlay is expected to render later.
	 *
	 * Rows are `TradeListingRow` — the fields every listing carries. Whatever
	 * is specific to one kind of listing (a gem's level/quality/corrupted mark)
	 * is rendered by the `detail` snippet, which receives the row INDEX so the
	 * caller can reach back into its own richer array. Passing no snippet drops
	 * the middle column entirely rather than leaving an empty one.
	 */
	import type { Snippet } from 'svelte';
	import type { TradeListingRow } from '$lib/tradeApi';

	let {
		rows,
		detail,
		detailLabel = 'Detail'
	}: {
		rows: TradeListingRow[];
		detail?: Snippet<[number]>;
		detailLabel?: string;
	} = $props();

	function fmtPrice(v: number): string {
		return Number.isInteger(v) ? v.toString() : v.toFixed(1);
	}

	function formatTimeAgo(isoString: string): string {
		const diff = Date.now() - new Date(isoString).getTime();
		const mins = Math.floor(diff / 60000);
		if (mins < 60) return `${mins}m ago`;
		const hours = Math.floor(mins / 60);
		if (hours < 24) return `${hours}h ago`;
		return `${Math.floor(hours / 24)}d ago`;
	}
</script>

{#if rows.length > 0}
	<div class="trade-listings-table">
		<div class="trade-listings-header" class:with-detail={!!detail}>
			<span class="tl-col-price">Price</span>
			{#if detail}<span class="tl-col-detail">{detailLabel}</span>{/if}
			<span class="tl-col-time">Listed</span>
		</div>
		{#each rows as row, i}
			<div class="trade-listing-row" class:with-detail={!!detail}>
				<span class="tl-col-price">
					{#if row.currency === 'divine'}
						{fmtPrice(row.amount)} div
						<span class="tl-original">({fmtPrice(row.chaosPrice)}c)</span>
					{:else}
						{fmtPrice(row.amount)}c
					{/if}
				</span>
				{#if detail}
					<span class="tl-col-detail">{@render detail(i)}</span>
				{/if}
				<span class="tl-col-time">{formatTimeAgo(row.indexedAt)}</span>
			</div>
		{/each}
	</div>
{/if}

<style>
	.trade-listings-table {
		margin-top: 6px;
		font-size: 0.75rem;
		border: 1px solid var(--color-lab-border);
		overflow: hidden;
	}
	.trade-listings-header {
		display: grid;
		grid-template-columns: 1.4fr 0.6fr;
		gap: 4px;
		padding: 6px 8px;
		background: rgba(42, 45, 55, 0.6);
		color: var(--color-lab-text-secondary);
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.03em;
		font-size: 0.625rem;
	}
	.trade-listing-row {
		display: grid;
		grid-template-columns: 1.4fr 0.6fr;
		gap: 4px;
		padding: 5px 8px;
		border-top: 1px solid rgba(42, 45, 55, 0.4);
		color: var(--color-lab-text);
	}
	.trade-listings-header.with-detail,
	.trade-listing-row.with-detail {
		grid-template-columns: 1.4fr 0.7fr 0.6fr;
	}
	.trade-listing-row:hover {
		background: rgba(59, 130, 246, 0.05);
	}
	.tl-col-time {
		color: var(--color-lab-text-secondary);
	}
	.tl-original {
		color: var(--color-lab-text-secondary);
		font-size: 0.7rem;
		margin-left: 4px;
	}
</style>
