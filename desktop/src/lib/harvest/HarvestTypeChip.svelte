<script lang="ts">
	/**
	 * One type of a Harvest family as a chip: icon, short name, price, then the
	 * feeder's loop EV or the keeper's weight share (README § Screens 6).
	 *
	 * Presentation only: every string and flag comes from `view.ts`'s `Chip`.
	 * `kept` is the column the chip sits in (keeper fill); the click leaves
	 * through `onpick` and the controller owns what it does.
	 */
	import ItemIcon from '$lib/components/ItemIcon.svelte';
	import { iconSrc } from '$lib/exchange/view';
	import type { Chip } from './view';

	let {
		chip,
		apiBase,
		kept = false,
		onpick
	}: { chip: Chip; apiBase: string; kept?: boolean; onpick: (itemId: string) => void } = $props();
</script>

<button
	type="button"
	class="chip"
	class:kept
	class:picked={chip.picked}
	title={chip.title ?? undefined}
	onclick={() => onpick(chip.itemId)}
>
	<ItemIcon src={iconSrc(apiBase, chip.icon)} alt={chip.name} size={18} />
	<span class="name">{chip.shortName}</span>
	<span class="mono price">{chip.priceText}</span>
	{#if chip.unpriced}
		<span class="mark">UNPRICED</span>
	{/if}
	{#if chip.evText !== null}
		<span class="mono ev {chip.evTone ?? ''}">{chip.evText}</span>
	{/if}
	{#if chip.shareText !== null}
		<span class="share">{chip.shareText}</span>
	{/if}
</button>

<style>
	.chip {
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		padding: 0.25rem 0.45rem;
		background: var(--color-lab-bg);
		border: 1px solid var(--color-lab-border);
		border-radius: 3px;
		color: var(--color-lab-text);
		font-size: 0.8125rem;
		cursor: pointer;
		text-align: left;
	}

	.chip.kept {
		background: color-mix(in srgb, var(--color-lab-green) 10%, transparent);
		border-color: color-mix(in srgb, var(--color-lab-green) 40%, transparent);
	}

	.chip.picked {
		border: 1px dashed var(--color-lab-yellow);
	}

	.chip:hover {
		border-color: var(--color-lab-text-secondary);
	}

	.chip:focus-visible {
		outline: 2px solid var(--color-lab-blue);
		outline-offset: 1px;
	}

	.mono {
		font-family: 'Consolas', 'Monaco', monospace;
		font-weight: 600;
	}

	.price {
		color: var(--color-lab-text-secondary);
	}

	.ev.gain {
		color: var(--color-lab-green);
	}

	.ev.loss {
		color: var(--color-lab-red);
	}

	.ev.flat {
		color: var(--color-lab-text-secondary);
	}

	.share {
		color: var(--color-lab-text-secondary);
		font-size: 0.75rem;
	}

	.mark {
		padding: 0 0.3rem;
		border: 1px solid color-mix(in srgb, var(--color-lab-yellow) 45%, transparent);
		border-radius: 3px;
		color: var(--color-lab-yellow);
		font-size: 0.625rem;
		letter-spacing: 0.06em;
	}
</style>
