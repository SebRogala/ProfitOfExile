import { describe, expect, it } from 'vitest';
import { readPromptText } from './view';
import type { ReadHotkeySettings, ReadPromptMechanic } from '$lib/stores/ssot.svelte';

const CTRL_A: ReadHotkeySettings = { shortcut: { key: 65, shift: false, ctrl: true, alt: false }, delayMs: 0, recalibrateSeconds: 5 };
const MOUSE_4: ReadHotkeySettings = { shortcut: { key: 5, shift: false, ctrl: false, alt: false }, delayMs: 0, recalibrateSeconds: 5 };

function text(mechanic: ReadPromptMechanic | null, readHotkey: ReadHotkeySettings | null) {
	return readPromptText({ readPrompt: { mechanic, noMatch: false, recalibrating: false, seq: 1 }, readHotkey });
}

describe('readPromptText', () => {
	// keep:happy
	it.each([
		[null, false],
		[null, true],
		['incursion', false]
	] as const)('words the recalibration sweep status first (mechanic %s, noMatch %s)', (mechanic, noMatch) => {
		expect(readPromptText({ readPrompt: { mechanic, noMatch, recalibrating: true, seq: 3 }, readHotkey: CTRL_A }))
			.toBe('No mechanics found - running a recalibration sweep now.');
	});

	it.each([CTRL_A, MOUSE_4, null])('shows the exact no-match message independently of the read key (%s)', (readHotkey) => {
		expect(readPromptText({ readPrompt: { mechanic: null, noMatch: true, recalibrating: false, seq: 2 }, readHotkey }))
			.toBe('Nothing detected. If a mechanic is open, check Settings → OCR Regions → Preview, and press Recalibrate if a region is off.');
	});
	it.each([
		[CTRL_A, 'No mechanics detected - press Ctrl + A within 5 s to recalibrate'],
		[{ ...MOUSE_4, recalibrateSeconds: 10 }, 'No mechanics detected - press Mouse 4 within 10 s to recalibrate']
	])('offers recalibration with the bound key and its window (%#)', (readHotkey, expected) => {
		expect(readPromptText({ readPrompt: { mechanic: null, noMatch: true, recalibrating: false, seq: 2, recalibrateOffer: true }, readHotkey }))
			.toBe(expected);
	});

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

	it.each([
		[[true, false, false], 'Gem 1 unclear'],
		[[true, false, true], 'Gems 1 and 3 unclear'],
		[[true, true, true], 'Gems 1, 2 and 3 unclear']
	] as const)('names the uncertain gem slots %j and asks for another press of the bound key', (gemUncertain, gems) => {
		expect(readPromptText({ readPrompt: { mechanic: null, noMatch: false, recalibrating: false, seq: 4, gemUncertain: [...gemUncertain] }, readHotkey: MOUSE_4 }))
			.toBe(`${gems} - wait for the Font to finish revealing, then press Mouse 4 again, or hover the gem and press Mouse 4`);
	});

	it('words uncertain gem slots over an armed mechanic', () => {
		expect(readPromptText({ readPrompt: { mechanic: 'incursion', noMatch: false, recalibrating: false, seq: 4, gemUncertain: [false, true, false] }, readHotkey: CTRL_A }))
			.toMatch(/^Gem 2 unclear/);
	});

	it('draws nothing for uncertain gem slots before the read key setting has arrived', () => {
		expect(readPromptText({ readPrompt: { mechanic: null, noMatch: false, recalibrating: false, seq: 4, gemUncertain: [true, false, false] }, readHotkey: null }))
			.toBeNull();
	});

	it('draws nothing while no mechanic is armed', () => {
		expect(text(null, CTRL_A)).toBeNull();
	});

	it('draws nothing before the read key setting has arrived', () => {
		// A prompt without the real key would tell the player to press nothing.
		expect(text('incursion', null)).toBeNull();
	});
});
