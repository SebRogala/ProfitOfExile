import { describe, expect, it } from 'vitest';
import { readPromptNotice } from './view';
import type { ReadHotkeySettings, ReadPromptMechanic } from '$lib/stores/ssot.svelte';

const CTRL_A: ReadHotkeySettings = { shortcut: { key: 65, shift: false, ctrl: true, alt: false }, delayMs: 0, recalibrateSeconds: 5 };
const MOUSE_4: ReadHotkeySettings = { shortcut: { key: 5, shift: false, ctrl: false, alt: false }, delayMs: 0, recalibrateSeconds: 5 };

function notice(mechanic: ReadPromptMechanic | null, readHotkey: ReadHotkeySettings | null) {
	return readPromptNotice({ readPrompt: { mechanic, noMatch: false, recalibrating: false, seq: 1 }, readHotkey });
}

describe('readPromptNotice', () => {
	// keep:happy
	it.each([
		[null, false],
		[null, true],
		['incursion', false]
	] as const)('words the recalibration sweep status first (mechanic %s, noMatch %s)', (mechanic, noMatch) => {
		expect(readPromptNotice({ readPrompt: { mechanic, noMatch, recalibrating: true, seq: 3 }, readHotkey: CTRL_A }))
			.toEqual({ kind: 'info', header: 'No mechanics detected', body: 'Running a recalibration sweep…', progress: true });
	});

	it.each([CTRL_A, MOUSE_4, null])('shows the exact no-match message independently of the read key (%s)', (readHotkey) => {
		expect(readPromptNotice({ readPrompt: { mechanic: null, noMatch: true, recalibrating: false, seq: 2 }, readHotkey }))
			.toEqual({ kind: 'warning', header: 'Nothing detected after recalibrating', body: 'Check Settings → OCR Regions → Preview' });
	});
	it.each([
		[CTRL_A, 'Press Ctrl + A within 5 s to recalibrate', 'Ctrl + A'],
		[{ ...MOUSE_4, recalibrateSeconds: 10 }, 'Press Mouse 4 within 10 s to recalibrate', 'Mouse 4']
	])('offers recalibration with the bound key and its window (%#)', (readHotkey, body, keys) => {
		expect(readPromptNotice({ readPrompt: { mechanic: null, noMatch: true, recalibrating: false, seq: 2, recalibrateOffer: true }, readHotkey }))
			.toEqual({ kind: 'warning', header: 'No mechanics detected', body, keys });
	});

	it.each([
		['divineFont', 'Divine Font detected'],
		['incursion', 'Incursion detected'],
		['mercenary', 'Mercenary detected']
	] as const)('names %s and the bound keyboard key', (mechanic, header) => {
		expect(notice(mechanic, CTRL_A)).toEqual({ kind: 'info', header, body: 'Press Ctrl + A to read', keys: 'Ctrl + A' });
	});

	it.each([
		['divineFont', 'Divine Font detected'],
		['incursion', 'Incursion detected'],
		['mercenary', 'Mercenary detected']
	] as const)('names %s and a bound mouse button', (mechanic, header) => {
		expect(notice(mechanic, MOUSE_4)).toEqual({ kind: 'info', header, body: 'Press Mouse 4 to read', keys: 'Mouse 4' });
	});

	it('follows a rebound read key for the same armed prompt', () => {
		expect(notice('incursion', CTRL_A)).toMatchObject({ body: 'Press Ctrl + A to read', keys: 'Ctrl + A' });
		expect(notice('incursion', MOUSE_4)).toMatchObject({ body: 'Press Mouse 4 to read', keys: 'Mouse 4' });
	});

	it.each([
		[[true, false, false], 'Gem 1 unclear'],
		[[true, false, true], 'Gems 1 and 3 unclear'],
		[[true, true, true], 'Gems 1, 2 and 3 unclear']
	] as const)('names the uncertain gem slots %j and asks for another press of the bound key', (gemUncertain, header) => {
		expect(readPromptNotice({ readPrompt: { mechanic: null, noMatch: false, recalibrating: false, seq: 4, gemUncertain: [...gemUncertain] }, readHotkey: MOUSE_4 }))
			.toEqual({ kind: 'warning', header, body: 'Press Mouse 4 after the Font reveals, or while hovering the gem', keys: 'Mouse 4' });
	});

	it('words uncertain gem slots over an armed mechanic', () => {
		expect(readPromptNotice({ readPrompt: { mechanic: 'incursion', noMatch: false, recalibrating: false, seq: 4, gemUncertain: [false, true, false] }, readHotkey: CTRL_A }))
			.toMatchObject({ kind: 'warning', header: 'Gem 2 unclear' });
	});

	it('draws nothing for uncertain gem slots before the read key setting has arrived', () => {
		expect(readPromptNotice({ readPrompt: { mechanic: null, noMatch: false, recalibrating: false, seq: 4, gemUncertain: [true, false, false] }, readHotkey: null }))
			.toBeNull();
	});

	it('draws nothing while no mechanic is armed', () => {
		expect(notice(null, CTRL_A)).toBeNull();
	});

	it('draws nothing before the read key setting has arrived', () => {
		// A prompt without the real key would tell the player to press nothing.
		expect(notice('incursion', null)).toBeNull();
	});
});
