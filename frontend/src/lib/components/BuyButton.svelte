<script lang="ts">
	let { label, href, colour, width = 'fixed', title }: {
		label: string;
		href: string | null;
		colour: 'red' | 'green' | 'blue';
		width?: 'fixed' | 'auto';
		title?: string;
	} = $props();
</script>

{#if href !== null}
	<a class="buy-btn buy-{colour}" class:buy-auto={width === 'auto'} {href} target="_blank" {title}>{label}</a>
{:else}
	<span class="buy-btn buy-{colour} buy-disabled" class:buy-auto={width === 'auto'} aria-disabled="true" {title}>{label}</span>
{/if}

<style>
	.buy-btn {
		display: block;
		width: 56px;
		padding: 3px 0;
		font-size: 0.6875rem;
		font-weight: 700;
		text-align: center;
		text-decoration: none;
		letter-spacing: 0.04em;
		border: 1px solid;
		cursor: pointer;
	}
	.buy-auto {
		display: inline-block;
		width: auto;
		padding: 3px 10px;
		white-space: nowrap;
	}
	.buy-disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
	.buy-red {
		color: var(--color-lab-red);
		border-color: rgba(239, 68, 68, 0.4);
		background: rgba(239, 68, 68, 0.08);
	}
	.buy-red:hover { background: rgba(239, 68, 68, 0.2); }
	.buy-green {
		color: var(--color-lab-green);
		border-color: rgba(34, 197, 94, 0.4);
		background: rgba(34, 197, 94, 0.08);
	}
	.buy-green:hover { background: rgba(34, 197, 94, 0.2); }
	.buy-blue {
		color: var(--color-lab-blue, #3b82f6);
		border-color: rgba(59, 130, 246, 0.4);
		background: rgba(59, 130, 246, 0.08);
	}
	.buy-blue:hover { background: rgba(59, 130, 246, 0.2); }
</style>
