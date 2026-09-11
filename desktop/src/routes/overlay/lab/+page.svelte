<script lang="ts">
	/**
	 * Lab widget overlay — the shared lab window (POE-231). It hosts the timer,
	 * compass and path strip; the comparator moves in with POE-231's next WI and
	 * remains a separate window here. The in-lab rule lives in each widget's
	 * content, not in this window: each widget reconstructs its own state from
	 * lab-nav events and catch-up replay.
	 *
	 * The host is mounted unconditionally so its widget-config listener is
	 * present when Settings raises the window in config mode, even when every
	 * widget's normal content is currently hidden.
	 */
	import WidgetHost from '$lib/overlay/widgets/WidgetHost.svelte';
	import LabCompassWidget from '$lib/compass/LabCompassWidget.svelte';
	import LabPathstripWidget from '$lib/compass/LabPathstripWidget.svelte';
	import LabTimerWidget from '$lib/compass/LabTimerWidget.svelte';
	import { LAB_WINDOW_LABEL } from '$lib/overlay/manager';
</script>

<WidgetHost module={LAB_WINDOW_LABEL}>
	{#snippet content(spec, configMode)}
		{#if spec.id === 'lab.compass'}
			<LabCompassWidget {configMode} label={spec.label} />
		{:else if spec.id === 'lab.pathstrip'}
			<LabPathstripWidget {configMode} label={spec.label} />
		{:else if spec.id === 'lab.timer'}
			<LabTimerWidget {configMode} label={spec.label} />
		{/if}
	{/snippet}
</WidgetHost>
