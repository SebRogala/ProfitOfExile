<script lang="ts">
	/**
	 * The global read prompt: detected mechanic/key or completed no-match guidance.
	 * Content of `prompt.read`, a PLACEABLE widget in the app-level
	 * `prompt` window, so `WidgetHost` positions it and this file has no
	 * placement logic. The window is excluded from the grab (ADR-023), so the
	 * box never lands in a frame a module reads.
	 *
	 * It took over the look of the temple's "waiting for the temple panel"
	 * notice (POE-249). Deliberately neither purple nor cyan: on the overlays
	 * both are VERDICTS, and this is the absence of one, so it is the neutral
	 * text colour in a dark box with a muted 1 px border.
	 */
	let { text, noMatch = false }: { text: string; noMatch?: boolean } = $props();
</script>

<p class="prompt" class:no-match={noMatch}>{text}</p>

<style>
	/* The offer boxes' dark ground and radius, in the neutral palette. */
	.prompt {
		padding: 6px 10px;
		font-size: 14px;
		color: var(--color-lab-text);
		background: rgb(15 17 23 / 88%);
		border: 1px solid var(--color-lab-text-muted);
		border-radius: 6px;
		white-space: nowrap;
		pointer-events: none;
	}
	.no-match {
		white-space: normal;
	}
</style>
