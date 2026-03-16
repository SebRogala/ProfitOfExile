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

	const LABS = ['Merciless'];

	function formatTimeAgo(isoStr: string): string {
		if (!isoStr) return 'unknown';
		const diff = Date.now() - new Date(isoStr).getTime();
		const mins = Math.floor(diff / 60000);
		if (isNaN(mins)) return 'unknown';
		if (mins < 1) return 'just now';
		return `${mins} min ago`;
	}

	function formatTime(isoStr: string): string {
		if (!isoStr) return '--:--';
		return new Date(isoStr).toLocaleTimeString(undefined, {
			hour: '2-digit',
			minute: '2-digit',
		});
	}

	function minutesUntil(isoStr: string): number {
		if (!isoStr) return 0;
		const result = Math.max(0, Math.round((new Date(isoStr).getTime() - Date.now()) / 60000));
		return isNaN(result) ? 0 : result;
	}
</script>

<header class="header">
	<div class="header-row">
		<h1 class="title">ProfitOfExile — Lab Farming Dashboard</h1>
		<div class="lab-selector">
			{#each LABS as lab}
				<button
					class="lab-btn"
					class:active={selectedLab === lab}
					onclick={() => onLabChange(lab)}
				>
					{lab}
				</button>
			{/each}
		</div>
		<div class="meta-row">
			<span class="meta">
				{formatTimeAgo(status.lastUpdate)}
			</span>
			<span class="meta-sep">|</span>
			<span class="meta">
				Next: {minutesUntil(status.nextFetch)} min
			</span>
			<span class="meta-sep">|</span>
			<span class="connection" class:connected={status.connected} class:disconnected={!status.connected}>
				<span class="conn-dot">●</span>
				{status.connected ? 'Live' : 'Disconnected'}
			</span>
		</div>
	</div>
</header>

<style>
	.header {
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		padding: 18px 28px;
		margin-bottom: 32px;
	}
	.title {
		font-size: 1.25rem;
		font-weight: 700;
		color: var(--color-lab-text);
		margin: 0;
	}
	.header-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
	}
	.lab-selector {
		display: flex;
		gap: 4px;
	}
	.lab-btn {
		background: transparent;
		border: 1px solid var(--color-lab-border);
		color: var(--color-lab-text-secondary);
		padding: 5px 14px;
		font-size: 0.875rem;
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
	.meta-row {
		display: flex;
		align-items: center;
		gap: 10px;
	}
	.meta {
		color: var(--color-lab-text-secondary);
		font-size: 0.9375rem;
	}
	.meta-sep {
		color: var(--color-lab-border);
		font-size: 0.9375rem;
	}
	.connection {
		font-size: 0.9375rem;
	}
	.connected {
		color: var(--color-lab-green);
	}
	.disconnected {
		color: var(--color-lab-red);
	}
	.conn-dot {
		font-size: 0.75rem;
		margin-right: 4px;
	}
</style>
