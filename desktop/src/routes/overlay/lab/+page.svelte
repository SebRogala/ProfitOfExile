<script lang="ts">
	/**
	 * Lab widget overlay — the shared lab window (POE-231). It hosts the timer,
	 * compass, path strip and gem comparator. The in-lab rule lives in each widget's
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
	import LabComparatorWidget from '$lib/compass/LabComparatorWidget.svelte';
	import { LAB_WINDOW_LABEL } from '$lib/overlay/manager';

	let comparatorWidget: { handleAction: (action: string, target: HTMLElement | null) => void } | undefined;

	function handleComparatorAction(action: string, target: HTMLElement): void {
		comparatorWidget?.handleAction(action, target);
	}
</script>

<WidgetHost module={LAB_WINDOW_LABEL} onAction={handleComparatorAction}>
	{#snippet content(spec, configMode)}
		{#if spec.id === 'lab.compass'}
			<LabCompassWidget {configMode} label={spec.label} />
		{:else if spec.id === 'lab.pathstrip'}
			<LabPathstripWidget {configMode} label={spec.label} />
		{:else if spec.id === 'lab.timer'}
			<LabTimerWidget {configMode} label={spec.label} />
		{:else if spec.id === 'lab.comparator'}
			<LabComparatorWidget bind:this={comparatorWidget} {configMode} label={spec.label} />
		{/if}
	{/snippet}
</WidgetHost>
