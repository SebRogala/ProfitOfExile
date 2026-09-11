<script lang="ts">
	/**
	 * Lab widget overlay — the shared lab window (POE-231). Today it hosts the
	 * timer; the comparator, compass and path strip move in with POE-231's next
	 * WIs and are separate windows until then. The in-lab rule lives in each
	 * widget's content, not in this window: the comparator is shown outside the
	 * lab today, and the timer reconstructs its own state from lab-nav events and
	 * catch-up replay.
	 *
	 * The host is mounted unconditionally so its widget-config listener is
	 * present when Settings raises the window in config mode, even when every
	 * widget's normal content is currently hidden.
	 */
	import WidgetHost from '$lib/overlay/widgets/WidgetHost.svelte';
	import LabTimerWidget from '$lib/compass/LabTimerWidget.svelte';
	import { LAB_WINDOW_LABEL } from '$lib/overlay/manager';
</script>

<WidgetHost module={LAB_WINDOW_LABEL}>
	{#snippet content(spec, configMode)}
		{#if spec.id === 'lab.timer'}
			<LabTimerWidget {configMode} label={spec.label} />
		{/if}
	{/snippet}
</WidgetHost>
