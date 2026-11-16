import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import OverlayDismissSetting, { createOverlayDismissEditor } from './OverlayDismissSetting.svelte';
import type { DismissSettings, DismissSnapshot } from '$lib/stores/ssot.svelte';

const esc = { key: 27, shift: false, ctrl: false, alt: false };
const f = { key: 70, shift: false, ctrl: false, alt: false };
const key = (name: string) => ({ key: name, shiftKey: false, ctrlKey: false, altKey: false, metaKey: false });
function fixture(initial: DismissSnapshot | null = { timeoutSeconds: 10, shortcut: esc, overlays: {} }) {
	let current = initial;
	let saved: DismissSettings | undefined;
	let refusal = '';
	let refreshes = 0;
	const writes: Record<string, unknown>[] = [];
	const editor = createOverlayDismissEditor({
		current: () => current,
		call: async (command, args) => {
			expect(command).toBe('overlay_dismiss_set_settings');
			writes.push(args);
			if (refusal) throw refusal;
			saved = args.settings as DismissSettings;
		},
		refresh: async () => { refreshes++; current = saved!; }
	});
	return { editor, writes, html: () => render(OverlayDismissSetting, { props: { editor } }).body,
		current: () => current, replace: (value: DismissSnapshot | null) => { current = value; },
		refuse: (message: string) => { refusal = message; }, refreshes: () => refreshes };
}

describe('overlay dismissal settings', () => {
	it('binds Escape as key 27', async () => {
		const { editor, writes } = fixture();
		editor.listen(); await editor.captureKey(key('Escape'));
		expect(writes).toEqual([{ settings: { timeoutSeconds: 10, shortcut: esc, overlays: {} } }]);
	});
	it('ignores key input outside listening', async () => {
		const { editor, writes } = fixture();
		await editor.captureKey(key('f'));
		expect(writes).toEqual([]);
	});
	it('cancels without changing the saved shortcut', async () => {
		const t = fixture();
		t.editor.listen(); t.editor.cancel(); await t.editor.captureKey(key('f'));
		expect(t.writes).toEqual([]); expect(t.current()?.shortcut).toEqual(esc);
	});
	it('resets the global binding to Esc', async () => {
		const t = fixture({ timeoutSeconds: 20, shortcut: f, overlays: {} });
		await t.editor.reset();
		expect(t.current()).toEqual({ timeoutSeconds: 20, shortcut: esc, overlays: {} });
	});
	it('preserves modifiers on Escape', async () => {
		const t = fixture();
		t.editor.listen(); await t.editor.captureKey({ ...key('Escape'), ctrlKey: true });
		expect(t.current()?.shortcut).toEqual({ ...esc, ctrl: true });
	});
	it('captures mouse buttons while listening', async () => {
		const t = fixture();
		t.editor.listen('temple.offers');
		await t.editor.captureMouse({ button: 3, ctrlKey: false, altKey: false, shiftKey: true, metaKey: false });
		expect(t.current()?.overlays['temple.offers']).toEqual({ timeout: 'inherit', shortcut: { override: { key: 5, shift: true, ctrl: false, alt: false } } });
	});
	it('keeps listening on a modifier-only key', async () => {
		const t = fixture(); t.editor.listen(); await t.editor.captureKey(key('Shift'));
		expect(t.writes).toEqual([]); expect(t.editor.listening).toBe('global');
	});
	it('writes Off as zero', async () => {
		const t = fixture(); await t.editor.setTimeout(0);
		expect(t.current()?.timeoutSeconds).toBe(0);
	});
	it('preserves timeout override when shortcut starts inheriting', async () => {
		const t = fixture({ timeoutSeconds: 10, shortcut: esc, overlays: { 'temple.offers': { timeout: { override: 30 }, shortcut: { override: f } } } });
		await t.editor.inheritShortcut('temple.offers');
		expect(t.current()?.overlays['temple.offers']).toEqual({ timeout: { override: 30 }, shortcut: 'inherit' });
	});
	it('preserves shortcut override when timeout starts inheriting', async () => {
		const t = fixture({ timeoutSeconds: 10, shortcut: esc, overlays: { 'temple.offers': { timeout: { override: 30 }, shortcut: { override: f } } } });
		await t.editor.setTimeout('inherit', 'temple.offers');
		expect(t.current()?.overlays['temple.offers']).toEqual({ timeout: 'inherit', shortcut: { override: f } });
	});
	it('writes only the selected timeout override', async () => {
		const t = fixture(); await t.editor.setTimeout(30, 'temple.offers');
		expect(t.current()).toEqual({ timeoutSeconds: 10, shortcut: esc, overlays: { 'temple.offers': { timeout: { override: 30 }, shortcut: 'inherit' } } });
	});
	it('follows changed global values in inherited labels', async () => {
		const t = fixture(); await t.editor.setTimeout(20);
		t.editor.listen(); await t.editor.captureKey(key('f'));
		expect(t.html()).toContain('Use global (20 s)'); expect(t.html()).toContain('Use global (F)');
	});
	it('preserves every overlay override on a global change', async () => {
		const overlays = { 'temple.offers': { timeout: { override: 30 }, shortcut: 'inherit' as const }, 'future.widget': { timeout: 'inherit' as const, shortcut: { override: f } } };
		const t = fixture({ timeoutSeconds: 10, shortcut: esc, overlays, pending: [{ gen: 9, shortcut: esc }] });
		await t.editor.setTimeout(5);
		expect(t.writes).toEqual([{ settings: { timeoutSeconds: 5, shortcut: esc, overlays } }]);
	});
	it('shows the Rust refusal while retaining saved values', async () => {
		const t = fixture(); t.refuse('Native dismissal settings refused');
		await t.editor.setTimeout(30);
		expect(t.html()).toContain('Native dismissal settings refused');
		expect(t.current()?.timeoutSeconds).toBe(10); expect(t.refreshes()).toBe(0);
	});
	it('shows the actual echo rather than an optimistic edit', async () => {
		let release: () => void = () => {};
		const barrier = new Promise<void>(resolve => { release = resolve; });
		let current: DismissSettings = { timeoutSeconds: 10, shortcut: esc, overlays: {} };
		const editor = createOverlayDismissEditor({ current: () => current, call: async () => {}, refresh: async () => { await barrier; current = { ...current, timeoutSeconds: 5 }; } });
		const saving = editor.setTimeout(30);
		expect(editor.settings?.timeoutSeconds).toBe(10);
		release(); await saving;
		expect(editor.settings?.timeoutSeconds).toBe(5);
	});
	it('lists exactly the three applicable registry widgets', () => {
		const t = fixture();
		expect(t.editor.widgets.map(widget => widget.id)).toEqual(['temple.offers', 'mercenary.verdict', 'lab.fontalerts']);
		expect(t.html().match(/data-dismiss-widget=/g)).toHaveLength(3);
	});
	it('leaves unknown settings disabled without invented defaults', async () => {
		const t = fixture(null); t.editor.listen(); await t.editor.setTimeout(10);
		expect(t.writes).toEqual([]); expect(t.editor.listening).toBe(null);
		expect(t.html()).toContain('disabled'); expect(t.html()).not.toContain('Use global (10 s)');
	});
	it('renders an explicit Cancel while listening', () => {
		const t = fixture(); t.editor.listen();
		expect(t.html()).toContain('Cancel'); expect(t.html()).not.toContain('Esc cancels');
	});
});
