<script lang="ts">
	let { checked = $bindable() } = $props();
</script>

<button
	class="toggle"
	class:on={checked}
	role="switch"
	aria-checked={checked}
	onclick={() => { checked = !checked; }}
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
