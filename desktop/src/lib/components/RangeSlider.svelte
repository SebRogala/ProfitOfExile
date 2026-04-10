<script lang="ts">
	let { value = $bindable(), min = 0, max = 100, step = 1, formatValue }: {
		value: number;
		min?: number;
		max?: number;
		step?: number;
		formatValue?: (v: number) => string;
	} = $props();

	let display = $derived(formatValue ? formatValue(value) : String(value));
</script>

<div class="range-row">
	<input type="range" bind:value={value} {min} {max} {step} class="range-input" />
	<span class="range-value">{display}</span>
</div>

<style>
	.range-row {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		flex: 1;
	}

	.range-input {
		flex: 1;
		height: 4px;
		-webkit-appearance: none;
		appearance: none;
		background: var(--border);
		border-radius: 2px;
		outline: none;
		cursor: pointer;
	}

	.range-input::-webkit-slider-thumb {
		-webkit-appearance: none;
		width: 14px;
		height: 14px;
		border-radius: 50%;
		background: var(--accent);
		cursor: pointer;
		border: none;
	}

	.range-value {
		font-size: 0.75rem;
		font-family: 'Consolas', 'Courier New', monospace;
		color: var(--text-muted);
		min-width: 36px;
		text-align: right;
		flex-shrink: 0;
	}
</style>
