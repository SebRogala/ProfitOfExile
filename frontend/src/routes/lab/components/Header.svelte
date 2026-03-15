<script lang="ts">
	import type { StatusData } from '$lib/api';

	let {
		status,
		selectedLab,
		onLabChange,
	}: {
		status: StatusData;
		selectedLab: string;
		onLabChange: (lab: string) => void;
	} = $props();

	const LABS = ['Merciless', 'Uber', 'Gift', 'Dedication', 'Tribute'];

	function formatTimeAgo(isoStr: string): string {
		const diff = Date.now() - new Date(isoStr).getTime();
		const mins = Math.floor(diff / 60000);
		if (mins < 1) return 'just now';
		return `${mins} min ago`;
	}

	function formatTime(isoStr: string): string {
		return new Date(isoStr).toLocaleTimeString(undefined, {
			hour: '2-digit',
			minute: '2-digit',
		});
	}

	function minutesUntil(isoStr: string): number {
		return Math.max(0, Math.round((new Date(isoStr).getTime() - Date.now()) / 60000));
	}
</script>

<header class="header">
	<div class="header-top">
		<h1 class="title">ProfitOfExile — Lab Farming Dashboard</h1>
	</div>
	<div class="header-row">
		<div class="lab-selector">
			<span class="label">Lab:</span>
			{#each LABS as lab}
				<button
					class="lab-btn"
					class:active={selectedLab === lab}
					onclick={() => onLabChange(lab)}
				>
					{#if selectedLab === lab}<span class="dot">&#9679;</span>{/if}
					{lab}
				</button>
			{/each}
		</div>
	</div>
	<div class="header-row meta-row">
		<span class="meta">
			Last update: {formatTime(status.lastUpdate)} ({formatTimeAgo(status.lastUpdate)})
		</span>
		<span class="meta-sep">|</span>
		<span class="meta">
			Next: ~{formatTime(status.nextFetch)} ({minutesUntil(status.nextFetch)} min)
		</span>
		<span class="meta-sep">|</span>
		<span class="connection" class:connected={status.connected} class:disconnected={!status.connected}>
			<span class="conn-dot">{status.connected ? '\u25cf' : '\u25cf'}</span>
			{status.connected ? 'Live \u2014 connected to event stream' : 'Disconnected'}
		</span>
	</div>
</header>

<style>
	.header {
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		padding: 16px 20px;
		margin-bottom: 16px;
	}
	.header-top {
		margin-bottom: 10px;
	}
	.title {
		font-size: 1.125rem;
		font-weight: 700;
		color: var(--color-lab-text);
		margin: 0;
	}
	.header-row {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-wrap: wrap;
	}
	.meta-row {
		margin-top: 8px;
	}
	.lab-selector {
		display: flex;
		align-items: center;
		gap: 4px;
	}
	.label {
		color: var(--color-lab-text-secondary);
		font-size: 0.875rem;
		margin-right: 4px;
	}
	.lab-btn {
		background: transparent;
		border: 1px solid var(--color-lab-border);
		color: var(--color-lab-text-secondary);
		padding: 4px 12px;
		font-size: 0.8125rem;
		cursor: pointer;
		font-family: inherit;
	}
	.lab-btn:hover {
		color: var(--color-lab-text);
		border-color: var(--color-lab-text-secondary);
	}
	.lab-btn.active {
		color: var(--color-lab-text);
		border-color: var(--color-lab-blue);
		background: rgba(59, 130, 246, 0.1);
	}
	.dot {
		color: var(--color-lab-blue);
		margin-right: 2px;
	}
	.meta {
		color: var(--color-lab-text-secondary);
		font-size: 0.8125rem;
	}
	.meta-sep {
		color: var(--color-lab-border);
		font-size: 0.8125rem;
	}
	.connection {
		font-size: 0.8125rem;
	}
	.connected {
		color: var(--color-lab-green);
	}
	.disconnected {
		color: var(--color-lab-red);
	}
	.conn-dot {
		font-size: 0.625rem;
		margin-right: 3px;
	}
</style>
