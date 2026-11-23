import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { compileModule } from 'svelte/compiler';
import ts from 'typescript';
import source from './presentation.svelte.ts?raw';
import dismissalSource from '$lib/overlay/widgets/widget-dismissal.svelte.ts?raw';
import { widgetsFor } from '$lib/overlay/widgets/widget-registry';
import { readPromptText } from './view';
import type { DismissSnapshot, PendingDismiss, ReadPromptSnapshot } from '$lib/stores/ssot.svelte';
// @ts-expect-error Svelte's source entry has no declaration file.
import { flushSync, tick, untrack } from '../../../node_modules/svelte/src/index-client.js';
// @ts-expect-error Svelte's internal source entry has no declaration file.
import * as client from '../../../node_modules/svelte/src/internal/client/index.js';

// This filename runs in the server unit project; compile the production modules
// for the client runtime to exercise their actual effects without changing config.
function compile(raw: string, filename: string) {
	return compileModule(ts.transpileModule(raw, {
		compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.ESNext }
	}).outputText, { filename, generate: 'client', dev: false }).js.code
		.replace(/^import .*;$/gm, '').replace(/export function /g, 'function ');
}
const { createDismissal, createDismissalHost } = new Function('$', 'tick', 'untrack',
	compile(dismissalSource, 'widget-dismissal.svelte.js') + '\nreturn { createDismissal, createDismissalHost };'
)(client, tick, untrack) as typeof import('$lib/overlay/widgets/widget-dismissal.svelte');
const { createReadPromptPresentation } = new Function('$', 'createDismissal', 'createDismissalHost', 'widgetsFor', 'PROMPT_WINDOW_LABEL', 'readPromptText',
	compile(source, 'presentation.svelte.js') + '\nreturn { createReadPromptPresentation };'
)(client, createDismissal, createDismissalHost, widgetsFor, 'prompt', readPromptText) as typeof import('./presentation.svelte');

const MESSAGE = 'Nothing detected. If a mechanic is open, check Settings → OCR Regions → Preview, and press Recalibrate if a region is off.';
const DETECTED = 'Detected Incursion - press Ctrl + A to read';
const esc = { key: 27, shift: false, ctrl: false, alt: false };
const f = { ...esc, key: 70 };
const cleanups: (() => void)[] = [];
beforeEach(() => vi.useFakeTimers());
afterEach(() => { cleanups.splice(0).forEach((stop) => stop()); vi.useRealTimers(); });
async function settle() {
	for (let n = 0; n < 12; n++) { flushSync(); await tick(); await Promise.resolve(); }
	flushSync();
}
function setup(loaded = true) {
	const input = client.proxy({
		readPrompt: { mechanic: null, noMatch: true, recalibrating: false, seq: 1 } as ReadPromptSnapshot,
		readHotkey: { shortcut: { key: 65, shift: false, ctrl: true, alt: false }, delayMs: 0, recalibrateSeconds: 5 },
		overlayDismiss: loaded ? { timeoutSeconds: 10, shortcut: esc, overlays: {}, pending: [] } as DismissSnapshot : null
	});
	let deliver!: (event: { payload: PendingDismiss }) => void;
	const unlisten = vi.fn();
	const invoke = vi.fn(async (_command: string, _args: Record<string, unknown>) => {});
	let presentation!: ReturnType<typeof createReadPromptPresentation>;
	const stop = client.effect_root(() => {
		presentation = createReadPromptPresentation({
			source: () => input,
			listen: async (_event, handler) => { deliver = handler; return unlisten; },
			invoke, frame: async () => {}
		});
	});
	cleanups.push(stop); flushSync();
	return { input, presentation, invoke, unlisten, stop,
		deliver: (gen: number, shortcut = esc) => deliver({ payload: { gen, shortcut } }) };
}

describe('read prompt presentation', () => {
	it('leaves the no-match lifetime to Rust: no view timer hides it', () => {
		const h = setup();
		vi.advanceTimersByTime(60000); expect(h.presentation.text).toBe(MESSAGE);
		expect(vi.getTimerCount()).toBe(0);
	});
	it('hides for the matching key', async () => {
		const h = setup(); await settle(); h.deliver(1); await settle();
		expect(h.presentation.text).toBeNull();
	});
	it('keeps visible for another key', async () => {
		const h = setup(); await settle(); h.deliver(1, f); await settle();
		expect(h.presentation.text).toBe(MESSAGE);
	});
	it('follows a changed global dismissal key', async () => {
		const h = setup(); h.input.overlayDismiss!.shortcut = f; await settle();
		h.deliver(1); await settle(); expect(h.presentation.text).toBe(MESSAGE);
		h.deliver(2, f); await settle(); expect(h.presentation.text).toBeNull();
	});
	it('keeps the same sequence hidden across polls after key dismissal', async () => {
		const h = setup(); await settle();
		h.deliver(1); await settle();
		flushSync(); h.input.readPrompt = { mechanic: null, noMatch: true, recalibrating: false, seq: 1 };
		h.input.overlayDismiss = { ...h.input.overlayDismiss! }; flushSync();
		expect(h.presentation.text).toBeNull(); expect(vi.getTimerCount()).toBe(0);
	});
	it('shows a fresh no-match sequence after a key dismissal', async () => {
		const h = setup(); await settle(); h.deliver(1); await settle();
		h.input.readPrompt = { mechanic: null, noMatch: true, recalibrating: false, seq: 2 }; flushSync();
		expect(h.presentation.text).toBe(MESSAGE);
	});
	it('a selected result replaces the no-match', () => {
		const h = setup();
		h.input.readPrompt = { mechanic: null, noMatch: false, recalibrating: false, seq: 2 }; flushSync();
		expect(h.presentation.text).toBeNull(); expect(vi.getTimerCount()).toBe(0);
	});
	it('keeps detected text outside the no-match key', async () => {
		const h = setup(); await settle(); h.deliver(1); await settle();
		h.input.readPrompt = { mechanic: 'incursion', noMatch: false, recalibrating: false, seq: 2 }; await settle();
		h.deliver(2); await settle(); vi.advanceTimersByTime(60000);
		expect(h.presentation.text).toBe(DETECTED); expect(vi.getTimerCount()).toBe(0);
	});
	it('catches up a queued key once global settings arrive', async () => {
		const h = setup(false); await settle(); h.deliver(1); await settle();
		expect(h.presentation.text).toBe(MESSAGE);
		h.input.overlayDismiss = { timeoutSeconds: 10, shortcut: esc, overlays: {}, pending: [] };
		await settle(); expect(h.presentation.text).toBeNull();
	});
	it('applies SSOT-only dismissal without registering or acknowledging a capture host', async () => {
		const h = setup(); await settle();
		h.input.overlayDismiss!.pending = [{ gen: 1, shortcut: esc }]; await settle();
		expect(h.presentation.text).toBeNull(); expect(h.invoke).not.toHaveBeenCalled();
	});
	it('disposal removes the listener', async () => {
		const h = setup(); await settle();
		h.stop(); cleanups.splice(cleanups.indexOf(h.stop), 1); await settle();
		expect(h.unlisten).toHaveBeenCalledOnce();
	});
});
