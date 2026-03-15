<script lang="ts">
	import {
		SIGNAL_TOOLTIPS,
		WINDOW_TOOLTIPS,
		ADVANCED_TOOLTIPS,
	} from '$lib/tooltips';

	let { signal, type = 'signal' }: { signal: string; type?: 'signal' | 'window' | 'advanced' } =
		$props();

	const SIGNAL_STYLES: Record<string, { prefix: string; cssClass: string }> = {
		STABLE: { prefix: '\u25b2', cssClass: 'badge-green' },
		RISING: { prefix: '\u25b2', cssClass: 'badge-green' },
		FALLING: { prefix: '\u25bc', cssClass: 'badge-red' },
		HERD: { prefix: '\u26a0', cssClass: 'badge-yellow' },
		DUMPING: { prefix: '\u26a0', cssClass: 'badge-red' },
		RECOVERY: { prefix: '\u21bb', cssClass: 'badge-purple' },
		TRAP: { prefix: '\u26a0', cssClass: 'badge-red' },
	};

	const WINDOW_STYLES: Record<string, { prefix: string; cssClass: string }> = {
		CLOSED: { prefix: '', cssClass: 'badge-muted' },
		BREWING: { prefix: '\ud83c\udf7a', cssClass: 'badge-blue' },
		OPENING: { prefix: '', cssClass: 'badge-blue' },
		OPEN: { prefix: '\ud83d\udfe2', cssClass: 'badge-green' },
		CLOSING: { prefix: '', cssClass: 'badge-yellow' },
		EXHAUSTED: { prefix: '', cssClass: 'badge-red' },
	};

	const ADVANCED_STYLES: Record<string, { prefix: string; cssClass: string }> = {
		COMEBACK: { prefix: '\ud83d\udd04', cssClass: 'badge-purple' },
		POTENTIAL: { prefix: '\ud83d\udc8e', cssClass: 'badge-blue' },
		PRICE_MANIPULATION: { prefix: '\u26a0', cssClass: 'badge-red' },
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
		{#if style.prefix}{style.prefix} {/if}{signal}
	</span>
{/if}

<style>
	.signal-badge {
		display: inline-block;
		padding: 1px 6px;
		font-size: 0.75rem;
		font-weight: 600;
		letter-spacing: 0.02em;
		white-space: nowrap;
		cursor: help;
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
	.badge-muted {
		color: var(--color-lab-text-secondary);
		background: rgba(156, 163, 175, 0.08);
	}
</style>
