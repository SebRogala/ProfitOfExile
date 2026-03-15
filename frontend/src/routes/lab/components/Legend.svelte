<script lang="ts">
	let expanded = $state(false);
</script>

<section class="section">
	<button class="legend-toggle" onclick={() => { expanded = !expanded; }}>
		Legend {expanded ? '▼' : '▶'}
	</button>

	{#if expanded}
		<div class="legend-content">
			<div class="legend-grid">
				<div class="legend-col">
					<h4 class="legend-heading">Signals</h4>
					<div class="legend-item"><span class="sig green">▲ STABLE</span> steady price+lst</div>
					<div class="legend-item"><span class="sig green">▲ RISING</span> price increasing</div>
					<div class="legend-item"><span class="sig red">▼ FALLING</span> price decreasing</div>
					<div class="legend-item"><span class="sig yellow">⚠ HERD</span> price+lst both up</div>
					<div class="legend-item"><span class="sig red">⚠ DUMPING</span> price↓ listings↑</div>
					<div class="legend-item"><span class="sig purple">↻ RECOVERY</span> price↓ listings↓</div>
					<div class="legend-item"><span class="sig red">⚠ TRAP</span> CV>100% avoid</div>
				</div>

				<div class="legend-col">
					<h4 class="legend-heading">Window Lifecycle</h4>
					<div class="legend-item"><span class="sig muted">○ CLOSED</span> no opportunity</div>
					<div class="legend-item"><span class="sig blue">◆ BREWING</span> forming (~2h)</div>
					<div class="legend-item"><span class="sig blue">◆ OPENING</span> base draining</div>
					<div class="legend-item"><span class="sig green">● OPEN</span> farm now!</div>
					<div class="legend-item"><span class="sig yellow">● CLOSING</span> herd arriving</div>
					<div class="legend-item"><span class="sig muted">○ EXHAUSTED</span> no bases</div>
				</div>

				<div class="legend-col">
					<h4 class="legend-heading">Advanced Signals</h4>
					<div class="legend-item"><span class="sig purple">↻ COMEBACK</span> crashed + recovering</div>
					<div class="legend-item"><span class="sig cyan">◇ POTENTIAL</span> rising, unnoticed</div>
					<div class="legend-item"><span class="sig red">⚠ MANIPULATION</span> fake price floor</div>

					<h4 class="legend-heading" style="margin-top: 16px;">Liquidity Tiers</h4>
					<div class="legend-item"><span class="sig green">HIGH</span> safe farm</div>
					<div class="legend-item"><span class="sig yellow">MED</span> oscillating</div>
					<div class="legend-item"><span class="sig red">LOW</span> drain risk</div>
				</div>
			</div>

			<div class="legend-metrics">
				<h4 class="legend-heading">Metrics</h4>
				<div class="metrics-grid">
					<span><strong>ROI</strong> — profit in chaos (transfigured - base price)</span>
					<span><strong>ROI%</strong> — return on investment % (ROI / base × 100)</span>
					<span><strong>CV</strong> — coefficient of variation. &lt;25% safe, &gt;100% trap</span>
					<span><strong>EV</strong> — expected value from Font. pWin × avg winner price</span>
					<span><strong>pWin</strong> — probability of winner from font pool (3 picks, hypergeometric)</span>
					<span><strong>Liq</strong> — base gem liquidity vs market average. Predicts drain speed</span>
					<span><strong>Δ2h</strong> — change over last 2 hours (4 snapshots)</span>
				</div>
			</div>

			<div class="legend-footer">
				Data refreshes every ~30 min from poe.ninja.
			</div>
		</div>
	{/if}
</section>

<style>
	.section {
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		padding: 14px 24px;
		margin-bottom: 32px;
	}
	.legend-toggle {
		background: none;
		border: none;
		color: var(--color-lab-text);
		font-size: 0.9375rem;
		font-weight: 700;
		cursor: pointer;
		padding: 0;
		font-family: inherit;
	}
	.legend-toggle:hover {
		color: var(--color-lab-blue);
	}
	.legend-content {
		margin-top: 16px;
	}
	.legend-grid {
		display: grid;
		grid-template-columns: 1fr 1fr 1fr;
		gap: 20px;
	}
	.legend-col {
		min-width: 0;
	}
	.legend-heading {
		font-size: 0.8125rem;
		font-weight: 700;
		color: var(--color-lab-text);
		margin: 0 0 8px 0;
	}
	.legend-item {
		font-size: 0.75rem;
		color: var(--color-lab-text-secondary);
		line-height: 1.7;
	}
	.sig {
		font-weight: 600;
		margin-right: 4px;
	}
	.green { color: var(--color-lab-green); }
	.red { color: var(--color-lab-red); }
	.yellow { color: var(--color-lab-yellow); }
	.blue { color: var(--color-lab-blue); }
	.purple { color: var(--color-lab-purple); }
	.cyan { color: #22d3ee; }
	.muted { color: var(--color-lab-text-secondary); }
	.legend-metrics {
		margin-top: 16px;
		border-top: 1px solid var(--color-lab-border);
		padding-top: 14px;
	}
	.metrics-grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 4px 24px;
		font-size: 0.75rem;
		color: var(--color-lab-text-secondary);
	}
	.metrics-grid strong {
		color: var(--color-lab-text);
	}
	.legend-footer {
		margin-top: 14px;
		font-size: 0.75rem;
		color: var(--color-lab-text-secondary);
		border-top: 1px solid var(--color-lab-border);
		padding-top: 10px;
	}
</style>
