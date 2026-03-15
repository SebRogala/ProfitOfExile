<script lang="ts">
	let expanded = $state(false);
</script>

<section class="section">
	<button class="legend-toggle" onclick={() => { expanded = !expanded; }}>
		Legend {expanded ? '\u25bc' : '\u25b6'}
	</button>

	{#if expanded}
		<div class="legend-content">
			<div class="legend-grid">
				<div class="legend-col">
					<h4 class="legend-heading">Signals</h4>
					<div class="legend-item"><span class="sig green">\u25b2 STABLE</span> steady price+lst</div>
					<div class="legend-item"><span class="sig green">\u25b2 RISING</span> price increasing</div>
					<div class="legend-item"><span class="sig red">\u25bc FALLING</span> price decreasing</div>
					<div class="legend-item"><span class="sig yellow">\u26a0 HERD</span> price+lst both up</div>
					<div class="legend-item"><span class="sig red">\u26a0 DUMPING</span> price\u2193 listings\u2191</div>
					<div class="legend-item"><span class="sig purple">\u21bb RECOVERY</span> price\u2193 listings\u2193</div>
					<div class="legend-item"><span class="sig red">\u26a0 TRAP</span> CV>100% avoid</div>
				</div>

				<div class="legend-col">
					<h4 class="legend-heading">Window Lifecycle</h4>
					<div class="legend-item"><span class="sig muted">CLOSED</span> no opportunity</div>
					<div class="legend-item"><span class="sig blue">BREWING</span> forming (~2h)</div>
					<div class="legend-item"><span class="sig blue">OPENING</span> base draining</div>
					<div class="legend-item"><span class="sig green">OPEN</span> farm now!</div>
					<div class="legend-item"><span class="sig yellow">CLOSING</span> herd arriving</div>
					<div class="legend-item"><span class="sig red">EXHAUSTED</span> no bases</div>
				</div>

				<div class="legend-col">
					<h4 class="legend-heading">Advanced Signals</h4>
					<div class="legend-item"><span class="sig purple">COMEBACK</span> crashed + recovering</div>
					<div class="legend-item"><span class="sig blue">POTENTIAL</span> rising, unnoticed</div>
					<div class="legend-item"><span class="sig red">MANIPULATION</span> fake price floor</div>

					<h4 class="legend-heading" style="margin-top: 12px;">Liquidity Tiers</h4>
					<div class="legend-item"><span class="sig green">HIGH</span> safe farm</div>
					<div class="legend-item"><span class="sig yellow">MED</span> oscillating</div>
					<div class="legend-item"><span class="sig red">LOW</span> drain risk</div>
				</div>
			</div>

			<div class="legend-metrics">
				<h4 class="legend-heading">Metrics</h4>
				<div class="metrics-grid">
					<span><strong>ROI</strong> \u2014 profit in chaos (transfigured - base price)</span>
					<span><strong>ROI%</strong> \u2014 return on investment % (ROI / base \u00d7 100)</span>
					<span><strong>CV</strong> \u2014 coefficient of variation. &lt;25% safe, &gt;100% trap</span>
					<span><strong>EV</strong> \u2014 expected value from Font. pWin \u00d7 avg winner price</span>
					<span><strong>pWin</strong> \u2014 probability of winner from font pool (3 picks, hypergeometric)</span>
					<span><strong>Liq</strong> \u2014 base gem liquidity vs market average. Predicts drain speed</span>
					<span><strong>\u03942h</strong> \u2014 change over last 2 hours (4 snapshots)</span>
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
		padding: 10px 20px;
		margin-bottom: 16px;
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
		margin-top: 12px;
	}
	.legend-grid {
		display: grid;
		grid-template-columns: 1fr 1fr 1fr;
		gap: 16px;
	}
	.legend-col {
		min-width: 0;
	}
	.legend-heading {
		font-size: 0.8125rem;
		font-weight: 700;
		color: var(--color-lab-text);
		margin: 0 0 6px 0;
	}
	.legend-item {
		font-size: 0.75rem;
		color: var(--color-lab-text-secondary);
		line-height: 1.6;
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
	.muted { color: var(--color-lab-text-secondary); }
	.legend-metrics {
		margin-top: 12px;
		border-top: 1px solid var(--color-lab-border);
		padding-top: 10px;
	}
	.metrics-grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 2px 20px;
		font-size: 0.75rem;
		color: var(--color-lab-text-secondary);
	}
	.metrics-grid strong {
		color: var(--color-lab-text);
	}
	.legend-footer {
		margin-top: 10px;
		font-size: 0.75rem;
		color: var(--color-lab-text-secondary);
		border-top: 1px solid var(--color-lab-border);
		padding-top: 8px;
	}
</style>
