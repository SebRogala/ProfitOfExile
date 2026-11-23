/**
 * The text of the global "press <key> to read" prompt (POE-388).
 *
 * Rust owns the detected/no-match presentation (`read_prompt.rs`, `ssot.readPrompt`);
 * this file words it, with the key the player actually bound
 * (`ssot.readHotkey`). Display only: nothing here can start a read.
 */
import { shortcutLabel } from '$lib/boss-loot/native-adapters';
import type { ReadHotkeySettings, ReadPromptMechanic, ReadPromptSnapshot } from '$lib/stores/ssot.svelte';

/** The word the prompt uses for each mechanic. */
export const MECHANIC_WORDS: Record<ReadPromptMechanic, string> = {
	divineFont: 'Divine Font',
	incursion: 'Incursion',
	mercenary: 'Mercenary'
};

export const NO_MATCH_TEXT = 'Nothing detected. If a mechanic is open, check Settings → OCR Regions → Preview, and press Recalibrate if a region is off.';

export const RECALIBRATING_TEXT = 'No mechanics found - running a recalibration sweep now.';

/**
 * Recalibration sweep status, no-match guidance, detected mechanic/key, or null
 * while none is present (detected text also waits for the bound read key).
 */
export function readPromptText(source: {
	readPrompt: ReadPromptSnapshot;
	readHotkey: ReadHotkeySettings | null;
}): string | null {
	if (source.readPrompt.recalibrating) return RECALIBRATING_TEXT;
	if (source.readPrompt.noMatch) return NO_MATCH_TEXT;
	const mechanic = source.readPrompt.mechanic;
	if (mechanic === null || source.readHotkey === null) return null;
	return `Detected ${MECHANIC_WORDS[mechanic]} - press ${shortcutLabel(source.readHotkey.shortcut)} to read`;
}
