<script lang="ts">
	import type { FontEVResponse, FontColor } from '$lib/api';
	import InfoTooltip from './InfoTooltip.svelte';

	let { data }: { data: FontEVResponse } = $props();

	const COLOR_LABELS: Record<string, { icon: string; cssClass: string; colorClass: string }> = {
		RED: { icon: '\u25CF', cssClass: 'font-red', colorClass: 'icon-red' },
		GREEN: { icon: '\u25CF', cssClass: 'font-green', colorClass: 'icon-green' },
		BLUE: { icon: '\u25CF', cssClass: 'font-blue', colorClass: 'icon-blue' },
	};

	const FIXED_ORDER = ['RED', 'GREEN', 'BLUE'];

	// Find best EV color (EV is the same across modes, use safe).
	let bestColor = $derived(data.bestColorSafe);

	// Build lookup maps: color -> FontColor for each mode.
	let safeMap = $derived(
		Object.fromEntries((data.safe || []).map(fc => [fc.color, fc]))
	);
	let premiumMap = $derived(
		Object.fromEntries((data.premium || []).map(fc => [fc.color, fc]))
	);
	let jackpotMap = $derived(
		Object.fromEntries((data.jackpot || []).map(fc => [fc.color, fc]))
	);

	function tierLine(fc: FontColor | undefined): string {
		if (!fc || fc.winners === 0 || !fc.fontsToHit) return '\u2014 none';
		return `~${fc.fontsToHit}/hit \u00B7 ${fc.avgWin}c`;
	}
</script>

<div class="font-ev">
	<div class="font-header">
		<h4 class="font-title">Font EV<InfoTooltip text="Expected income per Font of Divine Skill usage.<br><br><b>EV</b>: Average income picking the best gem from 3 random draws.<br><br><b>Safe (LOW+)</b>: Hit rate for decent+ gems.<br><b>Premium (MID-HIGH+)</b>: Hit rate for valuable gems.<br><b>Jackpot (TOP)</b>: Hit rate for top-tier gems only.<br><br><b>~N/hit</b>: Expected fonts per qualifying hit.<br><b>Nc</b>: Average value when you hit." /></h4>
	</div>
	<div class="color-cards">
		{#each FIXED_ORDER as color}
			{@const cl = COLOR_LABELS[color]}
			{@const safe = safeMap[color]}
			{@const premium = premiumMap[color]}
			{@const jackpot = jackpotMap[color]}
			{@const ev = safe?.ev ?? premium?.ev ?? 0}
			{@const pool = safe?.pool ?? premium?.pool ?? 0}
			{#if cl}
				<div class="color-card {cl.cssClass}" class:best-red={color === bestColor && color === 'RED'} class:best-green={color === bestColor && color === 'GREEN'} class:best-blue={color === bestColor && color === 'BLUE'}>
					<div class="color-header">
						<span class="color-icon {cl.colorClass}">{cl.icon}</span>
						<span class="color-name">{color}</span>
						{#if color === bestColor}
							<span class="best-badge best-badge-{color.toLowerCase()}">BEST</span>
						{/if}
					</div>
					<div class="ev-line">
						<span class="ev-value">{ev}c/font</span>
						<span class="pool-label">pool {pool}</span>
					</div>
					<div class="tier-lines">
						<div class="tier-row">
							<span class="tier-label tier-safe">Safe</span>
							<span class="tier-detail tier-safe-text">{tierLine(safe)}</span>
						</div>
						<div class="tier-row">
							<span class="tier-label tier-premium">Premium</span>
							<span class="tier-detail tier-premium-text">{tierLine(premium)}</span>
						</div>
						<div class="tier-row">
							<span class="tier-label tier-jackpot">Jackpot</span>
							<span class="tier-detail tier-jackpot-text">{tierLine(jackpot)}</span>
						</div>
					</div>
				</div>
			{/if}
		{/each}
	</div>
</div>

<style>
	.font-ev {
		margin-top: 20px;
		border-top: 1px solid var(--color-lab-border);
		padding-top: 18px;
	}
	.font-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 12px;
	}
	.font-title {
		font-size: 1rem;
		font-weight: 700;
		color: var(--color-lab-text);
		margin: 0;
	}
	.color-cards {
		display: flex;
		gap: 14px;
	}
	.color-card {
		flex: 1;
		border: 1px solid var(--color-lab-border);
		padding: 16px 18px;
		background: var(--color-lab-bg);
	}
	.color-card.best-red {
		border-color: var(--color-lab-red);
		background: rgba(239, 68, 68, 0.04);
	}
	.color-card.best-green {
		border-color: var(--color-lab-green);
		background: rgba(34, 197, 94, 0.04);
	}
	.color-card.best-blue {
		border-color: var(--color-lab-blue);
		background: rgba(59, 130, 246, 0.04);
	}
	.color-header {
		display: flex;
		align-items: center;
		gap: 10px;
		margin-bottom: 8px;
	}
	.color-icon {
		font-size: 0.875rem;
	}
	.icon-red { color: var(--color-lab-red); }
	.icon-green { color: var(--color-lab-green); }
	.icon-blue { color: var(--color-lab-blue); }
	.color-name {
		font-weight: 700;
		font-size: 0.9375rem;
	}
	.font-red .color-name { color: var(--color-lab-red); }
	.font-green .color-name { color: var(--color-lab-green); }
	.font-blue .color-name { color: var(--color-lab-blue); }
	.best-badge {
		font-size: 0.625rem;
		font-weight: 700;
		padding: 1px 6px;
		letter-spacing: 0.05em;
		border: 1px solid;
	}
	.best-badge-red { color: var(--color-lab-red); border-color: var(--color-lab-red); }
	.best-badge-green { color: var(--color-lab-green); border-color: var(--color-lab-green); }
	.best-badge-blue { color: var(--color-lab-blue); border-color: var(--color-lab-blue); }
	.ev-line {
		display: flex;
		align-items: baseline;
		gap: 12px;
		margin-bottom: 10px;
	}
	.ev-value {
		font-weight: 700;
		font-size: 1.25rem;
		color: var(--color-lab-text);
	}
	.pool-label {
		font-size: 0.8125rem;
		color: var(--color-lab-text-secondary);
	}
	.tier-lines {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.tier-row {
		display: flex;
		align-items: baseline;
		gap: 8px;
		font-size: 0.8125rem;
	}
	.tier-label {
		font-weight: 600;
		min-width: 56px;
	}
	.tier-detail {
		color: var(--color-lab-text-secondary);
	}
	.tier-safe, .tier-safe-text { color: #5eead4; }
	.tier-premium, .tier-premium-text { color: #c084fc; }
	.tier-jackpot, .tier-jackpot-text { color: #fbbf24; }
</style>
