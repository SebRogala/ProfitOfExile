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

/** The no-match result while it offers recalibration to the next press (POE-459). */
export function recalibrateOfferText(key: string, seconds: number): string {
	return `No mechanics detected - press ${key} within ${seconds} s to recalibrate`;
}

/** 1-based numbers of the Divine Font gem slots the last read left uncertain. */
export function uncertainGemSlots(readPrompt: ReadPromptSnapshot): number[] {
	return (readPrompt.gemUncertain ?? []).flatMap((uncertain, i) => (uncertain ? [i + 1] : []));
}

function gemList(slots: number[]): string {
	return slots.length === 1 ? `Gem ${slots[0]}` : `Gems ${slots.slice(0, -1).join(', ')} and ${slots[slots.length - 1]}`;
}

/** Whether the text is guidance that wraps rather than a one-line prompt. */
export function readPromptWraps(readPrompt: ReadPromptSnapshot): boolean {
	return readPrompt.recalibrating || readPrompt.noMatch || uncertainGemSlots(readPrompt).length > 0;
}

/**
 * Recalibration sweep status, the no-match result (its recalibration offer, or
 * guidance once a confirming press also found nothing), uncertain gem slots,
 * detected mechanic/key, or null while none is present (the offer and the last
 * two wait for the bound read key).
 */
export function readPromptText(source: {
	readPrompt: ReadPromptSnapshot;
	readHotkey: ReadHotkeySettings | null;
}): string | null {
	if (source.readPrompt.recalibrating) return RECALIBRATING_TEXT;
	if (source.readPrompt.noMatch && !source.readPrompt.recalibrateOffer) return NO_MATCH_TEXT;
	if (source.readHotkey === null) return null;
	const key = shortcutLabel(source.readHotkey.shortcut);
	if (source.readPrompt.noMatch) return recalibrateOfferText(key, source.readHotkey.recalibrateSeconds);
	const slots = uncertainGemSlots(source.readPrompt);
	if (slots.length) {
		return `${gemList(slots)} unclear - wait for the Font to finish revealing, then press ${key} again, or hover the gem and press ${key}`;
	}
	const mechanic = source.readPrompt.mechanic;
	if (mechanic === null) return null;
	return `Detected ${MECHANIC_WORDS[mechanic]} - press ${key} to read`;
}
