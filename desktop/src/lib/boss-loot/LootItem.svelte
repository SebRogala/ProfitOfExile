<script lang="ts">
	import ItemIcon from '$lib/components/ItemIcon.svelte'
	import Tooltip from '$lib/components/Tooltip.svelte'
	import QuantityStepper from './QuantityStepper.svelte'
	import type { ItemView } from './overlay-view'

	let { item, icon, locked = false }: { item: ItemView; icon: string | null; locked?: boolean } = $props()
</script>

<div class="tile" class:selected={item.selected} class:ambiguous={item.ambiguous} class:category={item.category} class:off={!item.selected}>
	<button type="button" class="hit" data-action="loot.toggle" data-item={item.id}
		disabled={item.disabled || locked} title={item.title}
		role={item.radio ? 'radio' : undefined} aria-checked={item.radio ? item.selected : undefined}
		aria-pressed={item.radio ? undefined : item.selected}>
		{#if item.tag}<span class="tag" class:question={item.ambiguous}>{item.tag}</span>{/if}
		{#if item.selected}
			<span class="check" aria-hidden="true"><svg width="9" height="9" viewBox="0 0 10 10"><path d="M1.5 5.2 4 7.6 8.6 2.4" fill="none" stroke="currentColor" stroke-width="2" /></svg></span>
		{/if}
		<span class="art">
			{#if item.category}<span class="placeholder">any</span>
			{:else if icon}<ItemIcon src={icon} alt="" size={38} />
			{:else}<span class="placeholder">art</span>{/if}
		</span>
		<span class="name">{item.name}</span>
		{#if item.disabled}<span class="not-counted">not counted</span>
		{:else}<Tooltip text="Wiki estimates are shown when no boss runs have been logged."><span class="rate"><b>{item.rate}</b> {item.provenance}</span></Tooltip>{/if}
	</button>
	{#if item.showQuantity}<QuantityStepper id={item.id} name={item.name} quantity={item.quantity} disabled={locked} />{/if}
</div>

<style>
	.tile { position: relative; display: flex; flex-direction: column; border: 1px solid var(--color-lab-border); border-radius: 5px; background: var(--color-lab-surface); }
	.selected { border: 2px solid var(--color-lab-blue); background: var(--loot-selected); }
	.ambiguous { border: 2px dashed var(--color-lab-yellow); }
	.category { border-style: dashed; border-color: var(--color-lab-text-muted); }
	.category.selected { border-style: solid; border-color: var(--color-lab-blue); }
	.hit { all: unset; box-sizing: border-box; display: flex; flex-direction: column; align-items: center; gap: 3px; width: 100%; min-height: 92px; padding: 6px 4px 5px; text-align: center; color: var(--color-lab-text); cursor: pointer; }
	.hit:focus-visible { outline: 1px solid var(--color-lab-blue); outline-offset: -2px; }
	.hit:disabled { cursor: default; }
	.art, .placeholder { display: flex; align-items: center; justify-content: center; width: 38px; height: 38px; }
	.off .art { opacity: .4; }
	.placeholder { border: 1px dashed var(--color-lab-text-muted); border-radius: 4px; color: var(--color-lab-text-secondary); font-size: 10px; }
	.name { font-size: 11px; line-height: 13px; max-height: 39px; overflow: hidden; }
	.rate { color: var(--color-lab-text-secondary); font-size: 11px; }
	.rate b { font: 600 11px 'Consolas', 'Monaco', monospace; color: var(--color-lab-text); }
	.tag { position: absolute; top: 3px; left: 3px; padding: 0 3px; border-radius: 3px; color: var(--color-lab-text-secondary); background: var(--loot-tag); font-size: 10px; line-height: 12px; }
	.question, .not-counted { color: var(--color-lab-yellow); background: var(--loot-warning); }
	.not-counted { padding: 0 6px; border-radius: 8px; font-size: 11px; line-height: 16px; }
	.check { position: absolute; top: 3px; right: 3px; display: flex; align-items: center; justify-content: center; width: 14px; height: 14px; border-radius: 50%; background: var(--color-lab-blue); color: var(--color-lab-bg); }
</style>
