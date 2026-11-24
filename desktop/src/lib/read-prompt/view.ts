/**
 * The notice for the global "press <key> to read" prompt (POE-388).
 *
 * Rust owns the detected/no-match presentation (`read_prompt.rs`, `ssot.readPrompt`);
 * this file words it, with the key the player actually bound
 * (`ssot.readHotkey`). Display only: nothing here can start a read.
 */
import { shortcutLabel } from '$lib/boss-loot/native-adapters';
import type { OverlayNoticeProps } from '$lib/overlay/OverlayNotice.svelte';
import type { ReadHotkeySettings, ReadPromptMechanic, ReadPromptSnapshot } from '$lib/stores/ssot.svelte';

/** The word the prompt uses for each mechanic. */
export const MECHANIC_WORDS: Record<ReadPromptMechanic, string> = {
	divineFont: 'Divine Font',
	incursion: 'Incursion',
	mercenary: 'Mercenary'
};

/** 1-based numbers of the Divine Font gem slots the last read left uncertain. */
export function uncertainGemSlots(readPrompt: ReadPromptSnapshot): number[] {
	return (readPrompt.gemUncertain ?? []).flatMap((uncertain, i) => (uncertain ? [i + 1] : []));
}

function gemList(slots: number[]): string {
	return slots.length === 1 ? `Gem ${slots[0]}` : `Gems ${slots.slice(0, -1).join(', ')} and ${slots[slots.length - 1]}`;
}

/**
 * Recalibration sweep status, the no-match result (its recalibration offer, or
 * guidance once a confirming press also found nothing), uncertain gem slots,
 * detected mechanic/key, or null while none is present (the offer and the last
 * two wait for the bound read key).
 */
export function readPromptNotice(source: {
	readPrompt: ReadPromptSnapshot;
	readHotkey: ReadHotkeySettings | null;
}): OverlayNoticeProps | null {
	if (source.readPrompt.recalibrating) return {
		kind: 'info', header: 'No mechanics detected', body: 'Running a recalibration sweep…', progress: true
	};
	if (source.readPrompt.noMatch && !source.readPrompt.recalibrateOffer) return {
		kind: 'warning', header: 'Nothing detected after recalibrating', body: 'Check Settings → OCR Regions → Preview'
	};
	if (source.readHotkey === null) return null;
	const key = shortcutLabel(source.readHotkey.shortcut);
	if (source.readPrompt.noMatch) return {
		kind: 'warning', header: 'No mechanics detected',
		body: `Press ${key} within ${source.readHotkey.recalibrateSeconds} s to recalibrate`, keys: key
	};
	const slots = uncertainGemSlots(source.readPrompt);
	if (slots.length) {
		return {
			kind: 'warning', header: `${gemList(slots)} unclear`,
			body: `Press ${key} after the Font reveals, or while hovering the gem`, keys: key
		};
	}
	const mechanic = source.readPrompt.mechanic;
	if (mechanic === null) return null;
	return { kind: 'info', header: `${MECHANIC_WORDS[mechanic]} detected`, body: `Press ${key} to read`, keys: key };
}
