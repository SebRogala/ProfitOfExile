/**
 * The text of the global "press <key> to read" prompt (POE-388).
 *
 * Rust decides WHICH mechanic is armed (`read_prompt.rs`, `ssot.readPrompt`);
 * this file only words it, with the key the player actually bound
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

/**
 * `Detected <mechanic> - press <key> to read`, or null when nothing is armed
 * or the read key setting has not arrived yet.
 */
export function readPromptText(source: {
	readPrompt: ReadPromptSnapshot;
	readHotkey: ReadHotkeySettings | null;
}): string | null {
	if (source.readPrompt.noMatch) return NO_MATCH_TEXT;
	const mechanic = source.readPrompt.mechanic;
	if (mechanic === null || source.readHotkey === null) return null;
	return `Detected ${MECHANIC_WORDS[mechanic]} - press ${shortcutLabel(source.readHotkey.shortcut)} to read`;
}
