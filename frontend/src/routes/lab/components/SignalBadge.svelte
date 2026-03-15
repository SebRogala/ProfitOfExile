<script lang="ts">
	import {
		SIGNAL_TOOLTIPS,
		WINDOW_TOOLTIPS,
		ADVANCED_TOOLTIPS,
	} from '$lib/tooltips';

	let { signal, type = 'signal' }: { signal: string; type?: 'signal' | 'window' | 'advanced' } =
		$props();

	const SIGNAL_STYLES: Record<string, { prefix: string; cssClass: string }> = {
		STABLE: { prefix: '▲', cssClass: 'badge-green' },
		RISING: { prefix: '▲', cssClass: 'badge-green' },
		FALLING: { prefix: '▼', cssClass: 'badge-red' },
		HERD: { prefix: '⚠', cssClass: 'badge-yellow' },
		DUMPING: { prefix: '⚠', cssClass: 'badge-red' },
		RECOVERY: { prefix: '↻', cssClass: 'badge-purple' },
		TRAP: { prefix: '⚠', cssClass: 'badge-red' },
	};

	const WINDOW_STYLES: Record<string, { prefix: string; cssClass: string }> = {
		CLOSED: { prefix: '○', cssClass: 'badge-muted' },
		BREWING: { prefix: '◆', cssClass: 'badge-blue' },
		OPENING: { prefix: '◆', cssClass: 'badge-blue' },
		OPEN: { prefix: '●', cssClass: 'badge-green' },
		CLOSING: { prefix: '●', cssClass: 'badge-yellow' },
		EXHAUSTED: { prefix: '○', cssClass: 'badge-muted' },
	};

	const ADVANCED_STYLES: Record<string, { prefix: string; cssClass: string }> = {
		COMEBACK: { prefix: '↻', cssClass: 'badge-purple' },
		POTENTIAL: { prefix: '◇', cssClass: 'badge-cyan' },
		PRICE_MANIPULATION: { prefix: '⚠', cssClass: 'badge-red' },
	};

	function getStyle() {
		if (type === 'window') return WINDOW_STYLES[signal] || { prefix: '', cssClass: 'badge-muted' };
		if (type === 'advanced')
			return ADVANCED_STYLES[signal] || { prefix: '', cssClass: 'badge-muted' };
		return SIGNAL_STYLES[signal] || { prefix: '', cssClass: 'badge-muted' };
	}

	function getTooltip() {
		if (type === 'window') return WINDOW_TOOLTIPS[signal] || '';
		if (type === 'advanced') return ADVANCED_TOOLTIPS[signal] || '';
		return SIGNAL_TOOLTIPS[signal] || '';
	}

	let style = $derived(getStyle());
	let tooltip = $derived(getTooltip());
</script>

{#if signal}
	<span class="signal-badge {style.cssClass}" title={tooltip}>
		{#if style.prefix}<span class="badge-icon">{style.prefix}</span> {/if}{signal}
	</span>
{/if}

<style>
	.signal-badge {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 2px 8px;
		font-size: 0.75rem;
		font-weight: 600;
		letter-spacing: 0.02em;
		white-space: nowrap;
		cursor: help;
	}
	.badge-icon {
		font-size: 0.6875rem;
	}
	.badge-green {
		color: var(--color-lab-green);
		background: rgba(34, 197, 94, 0.12);
	}
	.badge-red {
		color: var(--color-lab-red);
		background: rgba(239, 68, 68, 0.12);
	}
	.badge-yellow {
		color: var(--color-lab-yellow);
		background: rgba(234, 179, 8, 0.12);
	}
	.badge-blue {
		color: var(--color-lab-blue);
		background: rgba(59, 130, 246, 0.12);
	}
	.badge-purple {
		color: var(--color-lab-purple);
		background: rgba(168, 85, 247, 0.12);
	}
	.badge-cyan {
		color: #22d3ee;
		background: rgba(34, 211, 238, 0.12);
	}
	.badge-muted {
		color: var(--color-lab-text-secondary);
		background: rgba(156, 163, 175, 0.08);
	}
</style>
