<script lang="ts">
	import Button from './Button.svelte';

	let {
		label,
		value,
		unit = '',
		copyTitle,
		copyLabel,
		oncopy,
		split
	}: {
		label: string;
		value: string | null;
		unit?: string;
		copyTitle: string;
		copyLabel: string;
		oncopy: () => void;
		split?: string | null;
	} = $props();
</script>

<div class="total-card">
	<div class="caption">{label}</div>
	<div class="total-value">
		<span class="number mono">{value ?? '—'}{#if value !== null}<span class="unit">{unit}</span>{/if}</span>
		<Button disabled={value === null} title={copyTitle} onclick={oncopy}>{copyLabel}</Button>
	</div>
	{#if split !== undefined}<div class="split">Whole split <span class="mono">{split ?? '—'}</span></div>{/if}
</div>

<style>
	.total-card {
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 12px 14px;
		min-width: 0;
	}

	.caption {
		font-size: 0.6875rem;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		color: var(--color-lab-text-secondary);
	}

	.mono {
		font-family: 'Consolas', 'Monaco', monospace;
		font-weight: 600;
	}

	.total-value {
		display: flex;
		align-items: center;
		justify-content: space-between;
		flex-wrap: wrap;
		gap: 8px;
	}

	.number {
		font-size: 1.375rem;
		overflow-wrap: anywhere;
	}

	.unit {
		font-size: 0.8125rem;
		color: var(--color-lab-text-secondary);
	}

	.split {
		font-size: 0.6875rem;
		color: var(--color-lab-text-secondary);
	}

	.split .mono {
		color: var(--color-lab-text);
	}
</style>
