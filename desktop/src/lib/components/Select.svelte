<script lang="ts">
	let {
		value = $bindable(''),
		options,
		onchange,
		id,
	}: {
		value: string;
		options: { value: string; label: string }[];
		onchange?: () => void;
		id?: string;
	} = $props();
</script>

<div class="select-container">
	<select {id} bind:value onchange={onchange}>
		{#each options as opt}
			<option value={opt.value}>{opt.label}</option>
		{/each}
	</select>
	<svg class="chevron" viewBox="0 0 12 8" fill="none" xmlns="http://www.w3.org/2000/svg">
		<path d="M1 1.5L6 6.5L11 1.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
	</svg>
</div>

<style>
	.select-container {
		position: relative;
		display: inline-flex;
		align-items: center;
	}
	select {
		background: var(--color-lab-bg);
		border: 1px solid var(--color-lab-border);
		color: var(--color-lab-text);
		padding: 6px 36px 6px 12px;
		font-size: 0.9375rem;
		font-family: inherit;
		cursor: pointer;
		width: 100%;
		-webkit-appearance: none;
		-moz-appearance: none;
		appearance: none;
	}
	select:hover {
		border-color: var(--color-lab-text-secondary);
	}
	.chevron {
		position: absolute;
		right: 12px;
		width: 12px;
		height: 8px;
		color: var(--color-lab-text-secondary);
		pointer-events: none;
	}
</style>
