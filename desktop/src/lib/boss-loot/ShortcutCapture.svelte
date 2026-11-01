<script lang="ts">
	let { value, label, listening, disabled = false, keycaps = false, onlisten, onkey, onmouse }: {
		value: string; label: string; listening: boolean; disabled?: boolean; keycaps?: boolean
		onlisten: () => void
		onkey: (event: KeyboardEvent) => void
		onmouse: (event: MouseEvent) => void
	} = $props()
	let button: HTMLButtonElement | undefined = $state()

	$effect(() => {
		let consumedMouse: number | null = null
		const consume = (event: Event) => { event.preventDefault(); event.stopImmediatePropagation() }
		const key = (event: KeyboardEvent) => {
			if (!listening || disabled || event.repeat || !button?.getClientRects().length) return
			consumedMouse = null
			consume(event)
			onkey(event)
		}
		const mouse = (event: MouseEvent) => {
			consumedMouse = null
			if (!listening || disabled || !button?.getClientRects().length || event.button < 0 || event.button > 4) return
			consumedMouse = event.button
			consume(event)
			onmouse(event)
		}
		// Consume the click following a captured mouse-down, so it cannot reactivate this button or toggle another setting.
		const click = (event: MouseEvent) => {
			if (consumedMouse !== event.button) return
			consume(event)
			consumedMouse = null
		}
		window.addEventListener('keydown', key, true)
		window.addEventListener('mousedown', mouse, true)
		window.addEventListener('click', click, true)
		window.addEventListener('auxclick', click, true)
		window.addEventListener('contextmenu', click, true)
		return () => {
			window.removeEventListener('keydown', key, true)
			window.removeEventListener('mousedown', mouse, true)
			window.removeEventListener('click', click, true)
			window.removeEventListener('auxclick', click, true)
			window.removeEventListener('contextmenu', click, true)
		}
	})
</script>

<span class="shortcut">
	<button bind:this={button} type="button" class:listening {disabled} aria-label="{label}: {listening ? 'Press a key or mouse button…' : value}" onclick={() => { if (!listening) onlisten() }}>
		{#if listening}Press a key or mouse button…
		{:else if keycaps}{#each value.split(' + ') as key, index}{#if index} + {/if}<kbd>{key}</kbd>{/each}
		{:else}{value}{/if}
	</button>
	{#if listening}<span class="cancel">Esc cancels</span>{/if}
</span>

<style>
	.shortcut { display: inline-flex; gap: 6px; align-items: center; }
	button { display: inline-flex; align-items: center; gap: 4px; padding: 4px 8px; border: 1px solid var(--color-lab-border); border-radius: 4px; background: var(--color-lab-bg); color: var(--color-lab-text); cursor: pointer; min-width: 180px; font: inherit; font-size: 14px; }
	button.listening { border-color: var(--color-lab-blue); color: var(--color-lab-text-secondary); }
	button:focus-visible { outline: 1px solid var(--color-lab-blue); outline-offset: 2px; }
	button:disabled { opacity: .35; cursor: default; }
	kbd { border: 1px solid var(--color-lab-text-muted); border-radius: 2px; padding: 1px 4px; font: 11px Consolas, Monaco, monospace; }
	.cancel { color: var(--color-lab-text-secondary); font-size: 11px; white-space: nowrap; }
</style>
