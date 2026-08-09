<script lang="ts">
	/**
	 * `onchange` decides who owns the state:
	 *
	 *  - supplied → pure delegation. The click reports the intended value and
	 *    mutates nothing; the parent's store write is what flips the visual. A
	 *    self-flip here would fight a store whose value can be rejected or
	 *    overwritten by a poll, showing a state nobody committed to.
	 *  - absent → the original `bind:checked` self-flip, unchanged.
	 *
	 * `ariaLabel` names *what* is being switched. Without it every toggle on a
	 * page shares the state-only name "Enable"/"Disable", so a screen-reader user
	 * hears the same control repeated; supply it whenever a view renders more
	 * than one toggle.
	 */
	let {
		checked = $bindable(),
		onchange,
		ariaLabel,
	}: {
		checked?: boolean;
		onchange?: (checked: boolean) => void;
		ariaLabel?: string;
	} = $props();
</script>

<button
	class="toggle"
	class:on={checked}
	role="switch"
	aria-checked={checked}
	aria-label={ariaLabel ?? (checked ? 'Disable' : 'Enable')}
	onclick={() => {
		if (onchange) { onchange(!checked); return; }
		checked = !checked;
	}}
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
