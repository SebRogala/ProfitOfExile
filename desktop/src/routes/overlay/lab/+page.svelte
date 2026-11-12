<script lang="ts">
	/**
	 * Lab widget overlay — the shared lab window (POE-231). It hosts the timer,
	 * compass, path strip and gem comparator. The in-lab rule lives in each widget's
	 * content, not in this window. The compass, path strip and timer share one lab
	 * nav session (lab-session) that this page owns and provides through context;
	 * each widget keeps only its per-view state.
	 *
	 * The host is mounted unconditionally so its widget-config listener is
	 * present when Settings raises the window in config mode, even when every
	 * widget's normal content is currently hidden.
	 */
	import WidgetHost from '$lib/overlay/widgets/WidgetHost.svelte';
	import LabCompassWidget from '$lib/compass/LabCompassWidget.svelte';
	import LabPathstripWidget from '$lib/compass/LabPathstripWidget.svelte';
	import LabTimerWidget from '$lib/compass/LabTimerWidget.svelte';
	import LabComparatorWidget from '$lib/compass/LabComparatorWidget.svelte';
	import { LAB_WINDOW_LABEL } from '$lib/overlay/manager';
	import { setContext } from 'svelte';
	import { createLabSession, LAB_SESSION } from '$lib/compass/lab-session.svelte';

	const labSession = createLabSession();
	setContext(LAB_SESSION, labSession);
</script>

<WidgetHost module={LAB_WINDOW_LABEL}>
	{#snippet content(spec, configMode)}
		{#if spec.id === 'lab.compass'}
			<LabCompassWidget {configMode} label={spec.label} />
		{:else if spec.id === 'lab.pathstrip'}
			<LabPathstripWidget {configMode} label={spec.label} />
		{:else if spec.id === 'lab.timer'}
			<LabTimerWidget {configMode} label={spec.label} />
		{:else if spec.id === 'lab.comparator'}
			<LabComparatorWidget {configMode} label={spec.label} />
		{/if}
	{/snippet}
</WidgetHost>
