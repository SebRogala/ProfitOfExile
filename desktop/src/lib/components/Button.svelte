<script lang="ts">
	import type { Snippet } from 'svelte';

	let {
		variant = 'default',
		disabled = false,
		title,
		onclick,
		children,
	}: {
		variant?: 'default' | 'save' | 'danger';
		disabled?: boolean;
		title?: string;
		onclick?: (e: MouseEvent) => void;
		children: Snippet;
	} = $props();
</script>

<button
	class="btn"
	class:save={variant === 'save'}
	class:danger={variant === 'danger'}
	{disabled}
	{title}
	{onclick}
>
	{@render children()}
</button>

<style>
	.btn {
		background: transparent;
		border: 1px solid var(--border);
		color: var(--text-muted);
		padding: 0.2rem 0.5rem;
		border-radius: 4px;
		font-size: 0.75rem;
		cursor: pointer;
		white-space: nowrap;
		flex-shrink: 0;
	}

	.btn:hover:not(:disabled) {
		border-color: var(--accent);
		color: var(--text);
	}

	.btn:disabled {
		opacity: 0.35;
		cursor: default;
	}

	.btn.save {
		border-color: var(--success);
		color: var(--success);
	}

	.btn.save:hover:not(:disabled) {
		background: rgba(74, 222, 128, 0.1);
	}

	.btn.danger {
		border: 1px solid rgba(239, 68, 68, 0.5);
		color: #ef4444;
		padding: 0.3rem 0.7rem;
	}

	.btn.danger:hover:not(:disabled) {
		background: rgba(239, 68, 68, 0.1);
		border-color: #ef4444;
	}
</style>
