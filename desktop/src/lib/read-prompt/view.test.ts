import { describe, expect, it } from 'vitest';
import { readPromptText } from './view';
import type { ReadHotkeySettings, ReadPromptMechanic } from '$lib/stores/ssot.svelte';

const CTRL_A: ReadHotkeySettings = { shortcut: { key: 65, shift: false, ctrl: true, alt: false }, delayMs: 0 };
const MOUSE_4: ReadHotkeySettings = { shortcut: { key: 5, shift: false, ctrl: false, alt: false }, delayMs: 0 };

function text(mechanic: ReadPromptMechanic | null, readHotkey: ReadHotkeySettings | null) {
	return readPromptText({ readPrompt: { mechanic, seq: 1 }, readHotkey });
}

describe('readPromptText', () => {
	it.each([
		['divineFont', 'Detected Divine Font - press Ctrl + A to read'],
		['incursion', 'Detected Incursion - press Ctrl + A to read'],
		['mercenary', 'Detected Mercenary - press Ctrl + A to read']
	] as const)('names %s and the bound keyboard key', (mechanic, expected) => {
		expect(text(mechanic, CTRL_A)).toBe(expected);
	});

	it.each([
		['divineFont', 'Detected Divine Font - press Mouse 4 to read'],
		['incursion', 'Detected Incursion - press Mouse 4 to read'],
		['mercenary', 'Detected Mercenary - press Mouse 4 to read']
	] as const)('names %s and a bound mouse button', (mechanic, expected) => {
		expect(text(mechanic, MOUSE_4)).toBe(expected);
	});

	it('follows a rebound read key for the same armed prompt', () => {
		expect(text('incursion', CTRL_A)).toBe('Detected Incursion - press Ctrl + A to read');
		expect(text('incursion', MOUSE_4)).toBe('Detected Incursion - press Mouse 4 to read');
	});

	it('draws nothing while no mechanic is armed', () => {
		expect(text(null, CTRL_A)).toBeNull();
	});

	it('draws nothing before the read key setting has arrived', () => {
		// A prompt without the real key would tell the player to press nothing.
		expect(text('incursion', null)).toBeNull();
	});
});
