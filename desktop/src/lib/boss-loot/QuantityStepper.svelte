<script lang="ts">
	let { id, name, quantity, disabled = false }: { id: string; name: string; quantity: number; disabled?: boolean } = $props()
</script>

<div class="quantity">
	<button type="button" data-action="loot.decrement" data-item={id} {disabled} aria-label="One fewer {name}">−</button>
	<span aria-live="polite">{quantity}</span>
	<button type="button" data-action="loot.increment" data-item={id} {disabled} aria-label="One more {name}">+</button>
</div>

<style>
	.quantity { display: flex; align-items: center; justify-content: center; gap: 2px; padding: 0 4px 5px; }
	button { width: 22px; height: 20px; padding: 0; border: 1px solid var(--color-lab-text-muted); border-radius: 3px; background: var(--color-lab-bg); color: var(--color-lab-text); font-family: inherit; font-size: 13px; line-height: 1; cursor: pointer; }
	button:focus-visible { outline: 1px solid var(--color-lab-blue); outline-offset: -2px; }
	button:disabled { cursor: default; opacity: .45; }
	span { min-width: 22px; text-align: center; font: 600 12px 'Consolas', 'Monaco', monospace; }
</style>
