import { createDismissal, createDismissalHost } from '$lib/overlay/widgets/widget-dismissal.svelte';
import { widgetsFor } from '$lib/overlay/widgets/widget-registry';
import { PROMPT_WINDOW_LABEL } from '$lib/overlay/manager';
import type { DismissSnapshot, ReadHotkeySettings, ReadPromptSnapshot } from '$lib/stores/ssot.svelte';
import { readPromptText } from './view';

type HostPorts = Pick<Parameters<typeof createDismissalHost>[0], 'listen' | 'invoke' | 'frame'>;

export function createReadPromptPresentation({ source, listen, invoke, frame }: HostPorts & {
	source(): {
		readPrompt: ReadPromptSnapshot;
		readHotkey: ReadHotkeySettings | null;
		overlayDismiss: DismissSnapshot | null;
	};
}) {
	// Key only: Rust ends the no-match result when its recalibration window
	// does (POE-459), so a view timer would disagree with the press it offers.
	const dismissal = createDismissal({
		spec: { ...widgetsFor(PROMPT_WINDOW_LABEL).find((spec) => spec.id === 'prompt.read')!, dismiss: { key: true } },
		shown: () => source().readPrompt.noMatch,
		contentKey: () => source().readPrompt.seq,
		settings: () => source().overlayDismiss
	});
	createDismissalHost({
		window: PROMPT_WINDOW_LABEL, ack: false,
		dismissals: [dismissal],
		pending: () => source().overlayDismiss?.pending ?? [],
		listen, invoke, frame
	});
	return {
		get text() {
			const current = source();
			return current.readPrompt.noMatch && dismissal.hidden ? null : readPromptText(current);
		}
	};
}
