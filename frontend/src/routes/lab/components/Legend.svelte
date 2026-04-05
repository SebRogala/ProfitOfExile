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
					<div class="legend-item"><span class="sig green-muted">━ STABLE</span> steady market</div>
					<div class="legend-item"><span class="sig green">🛒 DEMAND</span> listings draining, buyers active</div>
					<div class="legend-item"><span class="sig yellow">⚡ HERD</span> price+listings both rising</div>
					<div class="legend-item"><span class="sig red">⏬ DUMPING</span> price↓ listings↑</div>
					<div class="legend-item"><span class="sig purple">🔄 RECOVERY</span> thin market bottoming</div>
					<div class="legend-item"><span class="sig yellow">⚠ CAUTION</span> volatile, check history</div>
					<div class="legend-item"><span class="sig muted">? UNCERTAIN</span> no clear pattern</div>
				</div>

				<div class="legend-col">
					<h4 class="legend-heading">Window Lifecycle</h4>
					<div class="legend-item"><span class="sig muted">⭘ CLOSED</span> no opportunity</div>
					<div class="legend-item"><span class="sig blue">🔹 BREWING</span> forming (~2h)</div>
					<div class="legend-item"><span class="sig blue">🔷 OPENING</span> base draining</div>
					<div class="legend-item"><span class="sig green">🟢 OPEN</span> farm now!</div>
					<div class="legend-item"><span class="sig yellow">🟡 CLOSING</span> herd arriving</div>
					<div class="legend-item"><span class="sig muted">⭘ EXHAUSTED</span> no bases</div>
				</div>

				<div class="legend-col">
					<h4 class="legend-heading">Advanced Signals</h4>
					<div class="legend-item"><span class="sig purple">🔄 COMEBACK</span> crashed + recovering</div>
					<div class="legend-item"><span class="sig cyan">💎 POTENTIAL</span> rising, unnoticed</div>
					<div class="legend-item"><span class="sig red">🚩 MANIPULATION</span> fake price floor</div>
					<div class="legend-item"><span class="sig yellow">🌊 CASCADE</span> buyout→undercut cycle</div>
					<div class="legend-item"><span class="sig green">🚀 BREAKOUT</span> breaking resistance</div>

					<h4 class="legend-heading" style="margin-top: 16px;">Sell Confidence</h4>
					<div class="legend-item"><span class="sig green">✓ SAFE</span> liquid, sells near listed price</div>
					<div class="legend-item"><span class="sig yellow">• FAIR</span> may need patience or undercut</div>
					<div class="legend-item"><span class="sig red">✗ RISKY</span> thin market, crash risk</div>
				</div>
			</div>

			<div class="legend-grid" style="margin-top: 16px;">
				<div class="legend-col">
					<h4 class="legend-heading">Sell Urgency</h4>
					<div class="legend-item"><span class="sig red">SELL_NOW</span> dump immediately</div>
					<div class="legend-item"><span class="sig orange">UNDERCUT</span> undercut to sell fast</div>
					<div class="legend-item"><span class="sig green">HOLD</span> price is stable/rising</div>
					<div class="legend-item"><span class="sig muted">WAIT</span> no rush</div>
				</div>

				<div class="legend-col">
					<h4 class="legend-heading">Sellability</h4>
					<div class="legend-item"><span class="sig green">FAST SELL</span> 80+ score</div>
					<div class="legend-item"><span class="sig green-muted">GOOD</span> 60-79 score</div>
					<div class="legend-item"><span class="sig yellow">MODERATE</span> 40-59 score</div>
					<div class="legend-item"><span class="sig orange">SLOW</span> 20-39 score</div>
					<div class="legend-item"><span class="sig red">UNLIKELY</span> 0-19 score</div>
				</div>

				<div class="legend-col">
					<h4 class="legend-heading">Price Tiers (per variant, dynamic)</h4>
					<div class="legend-item"><span class="sig gold">TOP</span> monopoly outliers (gap-detected)</div>
					<div class="legend-item"><span class="sig orange">HIGH</span> within 30% of top gem</div>
					<div class="legend-item"><span class="sig purple">MID-HIGH</span> above 50% of HIGH boundary</div>
					<div class="legend-item"><span class="sig silver">MID</span> natural gap above LOW</div>
					<div class="legend-item"><span class="sig muted">LOW</span> marginal profit above FLOOR</div>
					<div class="legend-item"><span class="sig dim">FLOOR</span> below 8% of top-5 avg price</div>
					<div class="legend-note">Thin-market gems excluded from boundary computation. Tiers adapt automatically to market prices.</div>

					<h4 class="legend-heading" style="margin-top: 16px;">Font EV Hit Tiers</h4>
					<div class="legend-item"><span class="sig teal">Safe</span> any gem above FLOOR (not low-confidence)</div>
					<div class="legend-item"><span class="sig purple">Premium</span> HIGH + MID-HIGH + TOP gems</div>
					<div class="legend-item"><span class="sig gold">Jackpot</span> TOP-tier outliers only</div>
					<div class="legend-note">Low-confidence gems excluded from EV but counted in pool size. EV uses raw listed prices.</div>

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
					<span><strong>EV</strong> — expected income per Font (listed price of best gem from 3 draws)</span>
					<span><strong>pWin</strong> — probability of winner from font pool (3 picks, hypergeometric)</span>
					<span><strong>Liq</strong> — base gem liquidity vs market average. Predicts drain speed</span>
					<span><strong>Δ12h</strong> — change over last 12 hours</span>
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
		font-size: 0.875rem;
		font-weight: 700;
		color: var(--color-lab-text);
		margin: 0 0 8px 0;
	}
	.legend-item {
		font-size: 0.8125rem;
		color: var(--color-lab-text-secondary);
		line-height: 1.7;
	}
	.sig {
		font-weight: 600;
		margin-right: 4px;
	}
	.green { color: var(--color-lab-green); }
	.green-muted { color: var(--color-lab-green-muted); }
	.red { color: var(--color-lab-red); }
	.yellow { color: var(--color-lab-yellow); }
	.blue { color: var(--color-lab-blue); }
	.purple { color: var(--color-lab-purple); }
	.cyan { color: #22d3ee; }
	.orange { color: #f97316; }
	.gold { color: #fbbf24; }
	.silver { color: #9ca3af; }
	.muted { color: var(--color-lab-text-secondary); }
	.dim { color: #475569; }
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
