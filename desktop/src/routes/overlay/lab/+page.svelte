<script lang="ts">
	/**
	 * Lab widget overlay — the shared lab window (POE-231). It hosts the timer,
	 * compass, path strip, gem comparator and font alerts. The in-lab rule lives in each widget's
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
	import { fontAlertsDefaultPlacement } from '$lib/compass/font-alerts-geometry';
	import { LAB_WINDOW_LABEL } from '$lib/overlay/manager';
	import type { OverlayDefaultGeometry } from '$lib/overlay/overlay-defaults';
	import type { HostFrame } from '$lib/overlay/widgets/widget-geometry';
	import type { WidgetSpec } from '$lib/overlay/widgets/widget-registry';
	import { ssot } from '$lib/stores/ssot.svelte';
	import { setContext } from 'svelte';
	import { createLabSession, LAB_SESSION } from '$lib/compass/lab-session.svelte';

	const labSession = createLabSession();
	setContext(LAB_SESSION, labSession);

	/**
	 * Where the font alert stack ships for a player who has not placed it:
	 * beside Rust's font OCR region (POE-328). Null on the empty-set rule — no
	 * placements yet or an unresolved scale factor — and when no side fits;
	 * the widget is `derivedDefaultOnly`, so null means it is not drawn. A
	 * stored placement never reaches this: the host consults a default only
	 * for an unstored widget.
	 */
	function fontAlertsDefaults(spec: WidgetSpec, frame: HostFrame): OverlayDefaultGeometry | null {
		if (spec.id !== 'lab.fontalerts') return null;
		const placed = fontAlertsDefaultPlacement({
			font: ssot.placements?.lab.font ?? null,
			gem: ssot.placements?.lab.gem ?? null,
			scaleFactor: frame.scaleFactor,
			box: { w: spec.defaults.w, h: spec.defaults.h },
			host: frame.host
		});
		return placed === null ? null : { ...spec.defaults, x: placed.x, y: placed.y };
	}
</script>

<WidgetHost module={LAB_WINDOW_LABEL} defaultsFor={fontAlertsDefaults}>
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
