<script lang="ts">
	import type { StatusData } from '$lib/api';
	import Tooltip from '$lib/components/Tooltip.svelte';

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

	// Tick every 1s to keep timers live
	let now = $state(Date.now());
	$effect(() => {
		const interval = setInterval(() => { now = Date.now(); }, 1_000);
		return () => clearInterval(interval);
	});

	function formatTime(isoStr: string): string {
		if (!isoStr) return '--:--';
		return new Date(isoStr).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
	}

	function formatTimeAgo(isoStr: string, _now: number): string {
		if (!isoStr) return 'pending...';
		const diff = _now - new Date(isoStr).getTime();
		const mins = Math.floor(diff / 60000);
		if (isNaN(mins) || mins < 0) return 'pending...';
		if (mins < 1) return 'just now';
		if (mins < 60) return `${mins}m ago`;
		const hrs = Math.floor(mins / 60);
		if (hrs < 24) return `${hrs}h ${mins % 60}m ago`;
		return `${Math.floor(hrs / 24)}d ${hrs % 24}h ago`;
	}

	function nextFetchDisplay(nextFetch: string, _now: number): string {
		if (!nextFetch) return 'pending...';
		const target = new Date(nextFetch).getTime();
		if (isNaN(target)) return 'pending...';
		const diff = target - _now;
		const mins = Math.round(diff / 60000);
		if (mins <= 0) return `any moment (${formatTime(nextFetch)})`;
		return `~${mins}m (${formatTime(nextFetch)})`;
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
			{#if status.divinePrice > 0}
				<Tooltip text="Current Divine Orb price"><span class="meta divine-rate">1 div = {Math.round(status.divinePrice)}c</span></Tooltip>
				<span class="meta-sep">|</span>
			{/if}
			<Tooltip text={status.lastUpdate ? formatTime(status.lastUpdate) : ''}><span class="meta">
				{formatTimeAgo(status.lastUpdate, now)}
			</span></Tooltip>
			<span class="meta-sep">|</span>
			<span class="meta">
				Next: {nextFetchDisplay(status.nextFetch, now)}
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
	.divine-rate {
		color: var(--color-lab-yellow, #eab308);
		font-weight: 600;
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
