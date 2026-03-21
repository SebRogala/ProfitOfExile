<script lang="ts">
	let { text }: { text: string } = $props();
	let visible = $state(false);
	let pinned = $state(false);

	function handleClick(e: MouseEvent) {
		e.stopPropagation();
		if (pinned) {
			pinned = false;
			visible = false;
		} else {
			pinned = true;
			visible = true;
		}
	}

	function handleClickOutside() {
		if (pinned) {
			pinned = false;
			visible = false;
		}
	}

	$effect(() => {
		if (pinned) {
			const handler = () => handleClickOutside();
			document.addEventListener('click', handler);
			return () => document.removeEventListener('click', handler);
		}
	});
</script>

<span
	class="info-icon"
	role="button"
	tabindex="0"
	onmouseenter={() => { if (!pinned) visible = true; }}
	onmouseleave={() => { if (!pinned) visible = false; }}
	onclick={handleClick}
>
	<span class="icon-circle">i</span>
	{#if visible}
		<!-- svelte-ignore a11y_click_events_have_key_events -->
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="info-panel" onclick={(e) => e.stopPropagation()}>
			{@html text}
		</div>
	{/if}
</span>

<style>
	.info-icon {
		cursor: help;
		color: var(--color-lab-text-secondary);
		font-size: 0.875rem;
		position: relative;
		margin-left: 6px;
		display: inline-flex;
		align-items: baseline;
		vertical-align: baseline;
	}
	.icon-circle {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 16px;
		height: 16px;
		border: 1px solid currentColor;
		border-radius: 50%;
		font-size: 0.6875rem;
		font-weight: 700;
		font-style: italic;
		line-height: 1;
		opacity: 0.6;
		transition: opacity 0.15s ease;
		transform: translateY(-2px);
	}
	.info-icon:hover .icon-circle {
		color: var(--color-lab-text);
		opacity: 1;
	}
	.info-panel {
		cursor: default;
		position: absolute;
		top: calc(100% + 6px);
		left: 0;
		z-index: 100;
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		padding: 12px 16px;
		min-width: 380px;
		max-width: 520px;
		font-size: 0.8125rem;
		font-weight: 400;
		font-style: normal;
		line-height: 1.5;
		color: var(--color-lab-text);
		box-shadow: 0 8px 32px rgba(0, 0, 0, 0.6), 0 0 0 1px rgba(255, 255, 255, 0.08);
		white-space: normal;
		text-transform: none;
		letter-spacing: normal;
	}
</style>
