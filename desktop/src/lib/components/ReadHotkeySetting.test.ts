/**
 * ReadHotkeySetting: the editor's writes through `read_hotkey_set_settings`, and the
 * rows rendered with `svelte/server` from `ssot.readHotkey`.
 */
import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import { ssot, type ReadHotkeySettings } from '$lib/stores/ssot.svelte';
import ReadHotkeySetting, { createReadHotkeyEditor } from './ReadHotkeySetting.svelte';

const stored: ReadHotkeySettings = { shortcut: { key: 120, shift: true, ctrl: false, alt: false }, delayMs: 0 };
const key = (k: string, mods: Partial<{ ctrlKey: boolean; altKey: boolean; shiftKey: boolean }> = {}) =>
	({ key: k, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...mods });

function editor(reject?: string) {
	const calls: { command: string; args: Record<string, unknown> }[] = [];
	let refreshed = 0;
	const e = createReadHotkeyEditor({
		current: () => stored,
		call: async (command, args) => { calls.push({ command, args }); if (reject) throw reject; },
		refresh: async () => { refreshed++; }
	});
	return { e, calls, refreshed: () => refreshed };
}

afterEach(() => { ssot.readHotkey = null; });

describe('ReadHotkeySetting editor', () => {
	it('sends the pressed key as the new shortcut with the stored delay', async () => {
		const { e, calls, refreshed } = editor();
		e.listen();
		await e.captureKey(key('r', { ctrlKey: true }));
		expect(calls).toEqual([{ command: 'read_hotkey_set_settings',
			args: { settings: { shortcut: { key: 82, shift: false, ctrl: true, alt: false }, delayMs: 0 } } }]);
		expect(refreshed()).toBe(1);
		expect(e.listening).toBe(false);
	});

	it('sends a mouse button as the new shortcut', async () => {
		const { e, calls } = editor();
		e.listen();
		await e.captureMouse({ button: 3, ctrlKey: false, altKey: false, shiftKey: true, metaKey: false });
		expect(calls[0].args).toEqual({ settings: { shortcut: { key: 5, shift: true, ctrl: false, alt: false }, delayMs: 0 } });
	});

	it('cancels on Escape without sending', async () => {
		const { e, calls } = editor();
		e.listen();
		await e.captureKey(key('Escape'));
		expect(calls).toEqual([]);
		expect(e.listening).toBe(false);
	});

	it('sends the edited delay with the stored shortcut', async () => {
		const { e, calls } = editor();
		await e.setDelay(250);
		expect(calls[0].args).toEqual({ settings: { shortcut: stored.shortcut, delayMs: 250 } });
	});

	it('sends the longest delay', async () => {
		const { e, calls } = editor();
		await e.setDelay(2000);
		expect(calls[0].args).toEqual({ settings: { shortcut: stored.shortcut, delayMs: 2000 } });
	});

	it('refuses a delay past the longest without sending', async () => {
		const { e, calls } = editor();
		await e.setDelay(2001);
		expect(calls).toEqual([]);
		expect(e.error).toBe('Delay must be 0–2000 ms');
	});

	it('refuses a negative delay without sending', async () => {
		const { e, calls } = editor();
		await e.setDelay(-1);
		expect(calls).toEqual([]);
		expect(e.error).toContain('Delay');
	});

	it('keeps the command rejection as the error and does not refresh', async () => {
		const { e, refreshed } = editor('Read screen key is the boss-loot capture key');
		await e.captureKey(key('F8', { shiftKey: true }));
		expect(e.error).toBe('Read screen key is the boss-loot capture key');
		expect(refreshed()).toBe(0);
	});
});

describe('ReadHotkeySetting rows', () => {
	it('show the SSOT key and delay', () => {
		ssot.readHotkey = { shortcut: { key: 82, shift: false, ctrl: true, alt: false }, delayMs: 250 };
		const html = render(ReadHotkeySetting, { props: { editor: editor().e } }).body;
		expect(html).toContain('aria-label="Read screen: Ctrl + R"');
		expect(html).toMatch(/aria-label="Read delay"[^>]*value="250"|value="250"[^>]*aria-label="Read delay"/);
	});

	it('show the command rejection in the error row', async () => {
		ssot.readHotkey = stored;
		const { e } = editor('Read screen key is the boss-loot preview key');
		await e.setDelay(10);
		const html = render(ReadHotkeySetting, { props: { editor: e } }).body;
		expect(html).toContain('<span class="setting-error');
		expect(html).toContain('Read screen key is the boss-loot preview key');
	});
});
