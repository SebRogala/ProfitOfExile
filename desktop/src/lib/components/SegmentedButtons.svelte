<script lang="ts">
	let {
		value,
		options,
		onselect,
		title,
	}: {
		value: string;
		options: { value: string; label: string }[];
		onselect: (value: string) => void;
		title?: string;
	} = $props();
</script>

<div class="segmented" role="group" {title}>
	{#each options as option (option.value)}
		<button
			class="segmented-btn"
			class:active={option.value === value}
			aria-pressed={option.value === value}
			onclick={() => onselect(option.value)}
		>
			{option.label}
		</button>
	{/each}
</div>

<style>
	.segmented {
		display: flex;
		gap: 0;
		border: 1px solid var(--color-lab-border);
		border-radius: 4px;
		overflow: hidden;
	}
	.segmented-btn {
		background: transparent;
		border: none;
		color: var(--color-lab-text-secondary);
		padding: 4px 10px;
		font-size: 0.6875rem;
		font-weight: 600;
		cursor: pointer;
		font-family: inherit;
		transition: background 0.15s, color 0.15s;
	}
	.segmented-btn:hover {
		color: var(--color-lab-text);
		background: rgba(255, 255, 255, 0.05);
	}
	.segmented-btn.active {
		color: var(--color-lab-text);
		background: rgba(99, 102, 241, 0.2);
	}
</style>
