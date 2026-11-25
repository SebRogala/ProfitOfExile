<script lang="ts">
	/**
	 * The global read prompt window (POE-388): one placeable widget,
	 * `prompt.read`, stacking the lab-owned missing-layout notice above an
	 * armed mechanic, completed no-match or uncertain Divine Font gem slots. Display only —
	 * the read itself is the global read key (POE-387).
	 */
	import ReadPrompt from '$lib/read-prompt/ReadPrompt.svelte';
	import { createReadPromptPresentation } from '$lib/read-prompt/presentation.svelte';
	import { invoke } from '@tauri-apps/api/core';
	import { emit, listen } from '@tauri-apps/api/event';
	import { LAB_LAYOUT_NOTICE_EVENT, LAB_LAYOUT_NOTICE_REQUEST, type LabLayoutNoticeSnapshot } from '$lib/compass/lab-session.svelte';
	import WidgetHost from '$lib/overlay/widgets/WidgetHost.svelte';
	import { PROMPT_WINDOW_LABEL } from '$lib/overlay/manager';
	import { ssot } from '$lib/stores/ssot.svelte';

	let layoutSnapshot = $state.raw<LabLayoutNoticeSnapshot>({ revision: 0, notice: null });
	$effect(() => {
		let disposed = false;
		let unlisten: (() => void) | undefined;
		void (async () => {
			try {
				unlisten = await listen<LabLayoutNoticeSnapshot>(LAB_LAYOUT_NOTICE_EVENT, ({ payload }) => {
					if (!disposed && payload.revision > layoutSnapshot.revision) layoutSnapshot = payload;
				});
				if (disposed) { unlisten(); return; }
				await emit(LAB_LAYOUT_NOTICE_REQUEST);
			} catch (error) {
				const msg = `[prompt-overlay] layout notice reception failed: ${error}`;
				void invoke('app_log_from_frontend', { msg }).catch((cause) => console.warn(msg, cause));
			}
		})();
		return () => { disposed = true; unlisten?.(); };
	});

	const presentation = createReadPromptPresentation({
		source: () => ssot, listen, invoke,
		missingLayout: () => layoutSnapshot.notice,
		frame: () => new Promise((resolve) => requestAnimationFrame(() => resolve()))
	});
	const notice = $derived(presentation.notice);
	const layoutNotice = $derived(presentation.layoutNotice);
</script>

<WidgetHost module={PROMPT_WINDOW_LABEL}>
	{#snippet content(spec, configMode)}
		{#if spec.id === 'prompt.read' && (layoutNotice || notice)}
			<div class="notices">
				{#if layoutNotice}<div data-slot="layout"><ReadPrompt notice={layoutNotice} /></div>{/if}
				{#if notice}<div data-slot="read"><ReadPrompt {notice} /></div>{/if}
			</div>
		{:else if configMode}
			<p class="placeholder">{spec.label}</p>
		{/if}
	{/snippet}
</WidgetHost>

<style>
	.notices {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		justify-content: flex-start;
		gap: 4px;
	}
	.placeholder {
		margin: 0;
		padding: 6px 10px;
		font-size: 13px;
		color: var(--color-lab-text-muted);
	}
</style>
