<script lang="ts">
	let {
		checked = $bindable(),
		onchange
	}: {
		checked?: boolean;
		/**
		 * Called with the new value after a click.
		 *
		 * For a caller whose truth is not a variable it can `bind:` to — a value
		 * read out of a keyed map, say, which has to be written back through a
		 * command rather than assigned. Optional, so every existing
		 * `bind:checked` caller is unaffected.
		 */
		onchange?: (checked: boolean) => void;
	} = $props();
</script>

<button
	class="toggle"
	class:on={checked}
	role="switch"
	aria-checked={checked}
	aria-label={checked ? 'Disable' : 'Enable'}
	onclick={() => { checked = !checked; onchange?.(checked); }}
>
	<span class="knob"></span>
</button>

<style>
	.toggle {
		position: relative;
		width: 36px;
		height: 20px;
		border-radius: 10px;
		border: 1px solid var(--border);
		background: var(--bg);
		cursor: pointer;
		padding: 0;
		flex-shrink: 0;
		transition: background 0.15s, border-color 0.15s;
	}

	.toggle.on {
		background: var(--accent);
		border-color: var(--accent);
	}

	.knob {
		position: absolute;
		top: 2px;
		left: 2px;
		width: 14px;
		height: 14px;
		border-radius: 50%;
		background: var(--text-muted);
		transition: transform 0.15s, background 0.15s;
	}

	.toggle.on .knob {
		transform: translateX(16px);
		background: var(--bg);
	}
</style>
