// @vitest-environment happy-dom
/**
 * ExchangeReadSetting (POE-477) mounted in the client runtime under happy-dom with
 * the real ShortcutCapture and `ssot.exchangeRead`. The stubbed Tauri `invoke` is
 * the native side: `exchange_read_set_settings` stores or refuses the binding, and
 * `get_ssot` answers with the binding native now holds.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import type { BossLootShortcut } from '$lib/stores/ssot.svelte';

const native = vi.hoisted(() => ({
	/** Every `exchange_read_set_settings` argument object, in call order. */
	saves: [] as Record<string, unknown>[],
	/** The message native refuses the save with; null stores it. */
	refusal: null as string | null,
	/** The binding `get_ssot` reports. */
	stored: null as unknown
}));

vi.mock('@tauri-apps/api/core', () => ({
	invoke: vi.fn(async (command: string, args: Record<string, unknown>) => {
		if (command === 'exchange_read_set_settings') {
			native.saves.push(args);
			if (native.refusal) throw native.refusal;
			native.stored = (args as { settings: unknown }).settings;
			return null;
		}
		if (command === 'get_ssot') return { exchangeRead: native.stored };
		return null;
	})
}));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

import { ssot } from '$lib/stores/ssot.svelte';
import ExchangeReadSetting from './ExchangeReadSetting.svelte';

/** The Rust `ExchangeReadSettings::default()` binding: Alt + Right click. */
const ALT_RIGHT_CLICK: BossLootShortcut = { key: 2, shift: false, ctrl: false, alt: true };

let mounted: Record<string, unknown> | null = null;

beforeEach(() => {
	native.saves = [];
	native.refusal = null;
	native.stored = { shortcut: ALT_RIGHT_CLICK };
	ssot.exchangeRead = { shortcut: ALT_RIGHT_CLICK };
	mounted = mount(ExchangeReadSetting, { target: document.body });
	flushSync();
});

afterEach(() => {
	if (mounted) unmount(mounted);
	mounted = null;
	ssot.exchangeRead = null;
	document.body.innerHTML = '';
});

/** The Read exchange shortcut button, found by its accessible name. */
function shortcutButton(): HTMLButtonElement {
	const button = [...document.body.querySelectorAll('button')].find((b) =>
		(b.getAttribute('aria-label') ?? '').startsWith('Read exchange: ')
	);
	if (!button) throw new Error(`no Read exchange shortcut button in: ${document.body.innerHTML}`);
	return button;
}

function startListening() {
	shortcutButton().click();
	flushSync();
	expect(shortcutButton().getAttribute('aria-label')).toBe('Read exchange: Press a key or mouse button…');
}

type Press = { kind: 'mouse'; button: number; mods: Partial<MouseEventInit> } | { kind: 'key'; key: string; mods: Partial<KeyboardEventInit> };

function press(input: Press) {
	const event = input.kind === 'mouse'
		? new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: input.button, ...input.mods })
		: new KeyboardEvent('keydown', { bubbles: true, cancelable: true, key: input.key, ...input.mods });
	document.body.dispatchEvent(event);
	flushSync();
}

describe('ExchangeReadSetting', () => {
	it('keep:happy: displays the native default Read exchange binding', () => {
		const labels = [...document.body.querySelectorAll('.setting-label')].map((el) => el.textContent?.trim());
		expect(labels).toContain('Read exchange');
		expect(shortcutButton().getAttribute('aria-label')).toBe('Read exchange: Alt + Right click');
	});

	it.each([
		{ name: 'Ctrl + Mouse 4', input: { kind: 'mouse', button: 3, mods: { ctrlKey: true } } as Press,
			shortcut: { key: 5, shift: false, ctrl: true, alt: false } },
		{ name: 'Shift + F9', input: { kind: 'key', key: 'F9', mods: { shiftKey: true } } as Press,
			shortcut: { key: 120, shift: true, ctrl: false, alt: false } }
	])('keep:happy: refreshes the native binding from captured $name', async ({ name, input, shortcut }) => {
		startListening();
		press(input);

		await vi.waitFor(() => expect(shortcutButton().getAttribute('aria-label')).toBe(`Read exchange: ${name}`));
		expect(native.saves).toEqual([{ settings: { shortcut } }]);
		expect(ssot.exchangeRead).toEqual({ shortcut });
	});

	const refusedCaptures = [
		{ refusal: 'Read exchange shortcut must differ from the read screen and boss loot shortcuts',
			input: { kind: 'key', key: 'F9', mods: { shiftKey: true } } as Press },
		{ refusal: 'Plain left or right click cannot be the read exchange shortcut',
			input: { kind: 'mouse', button: 2, mods: {} } as Press }
	];

	// Silent: hiding a refusal would leave the player trusting an unsaved shortcut.
	it.each(refusedCaptures)('keep:critical: displays the exact refusal "$refusal"', async ({ refusal, input }) => {
		native.refusal = refusal;
		startListening();
		press(input);

		await vi.waitFor(() => expect(document.body.textContent).toContain(refusal));
	});

	// Silent: displaying the captured binding would tell the player a shortcut is
	// active while native still reads on the old one.
	it.each(refusedCaptures)('keep:critical: preserves the native binding on refusal "$refusal"', async ({ refusal, input }) => {
		native.refusal = refusal;
		startListening();
		press(input);

		await vi.waitFor(() => expect(shortcutButton().disabled).toBe(false));
		expect(native.saves).toHaveLength(1);
		expect(shortcutButton().getAttribute('aria-label')).toBe('Read exchange: Alt + Right click');
		expect(ssot.exchangeRead).toEqual({ shortcut: ALT_RIGHT_CLICK });
	});
});
