import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { compileModule } from 'svelte/compiler';
import ts from 'typescript';
import source from './widget-dismissal.svelte.ts?raw';
// @ts-expect-error Svelte's source entry has no declaration file.
import { flushSync, tick, untrack } from '../../../../node_modules/svelte/src/index-client.js';
// @ts-expect-error Svelte's internal source entry has no declaration file.
import * as client from '../../../../node_modules/svelte/src/internal/client/index.js';
// The approved filename belongs to the server-compiled unit project. Compile the actual
// helper for the client so these assertions exercise its effects without changing config.
const compiled = compileModule(ts.transpileModule(source, {
	compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.ESNext }
}).outputText, { filename: 'widget-dismissal.svelte.js', generate: 'client', dev: false }).js.code;
const { createDismissal, createDismissalHost } = new Function('$', 'tick', 'untrack',
	compiled.replace(/^import .*;$/gm, '').replace(/export function /g, 'function ') +
	'\nreturn { createDismissal, createDismissalHost };'
)(client, tick, untrack) as typeof import('./widget-dismissal.svelte');
import { WIDGETS } from './widget-registry';
import type { DismissSettings, PendingDismiss } from '$lib/stores/ssot.svelte';

const esc = { key: 27, shift: false, ctrl: false, alt: false };
const f = { ...esc, key: 70 };
const cleanups: (() => void)[] = [];
beforeEach(() => vi.useFakeTimers());
afterEach(() => { cleanups.splice(0).forEach((stop) => stop()); vi.useRealTimers(); });
const settle = async () => { flushSync(); await tick(); await Promise.resolve(); flushSync(); };
function root(run: () => void) { cleanups.push(client.effect_root(run)); flushSync(); }
function setup(id = 'temple.offers', loaded = true) {
	const input = client.proxy({ shown: true, key: 1, contentReady: true, settings: loaded ? {
		timeoutSeconds: 10, shortcut: esc, overlays: {}
	} as DismissSettings : null });
	let dismissal!: ReturnType<typeof createDismissal>;
	root(() => { dismissal = createDismissal({
		spec: WIDGETS.find((spec) => spec.id === id)!,
		shown: () => input.shown, contentKey: () => input.key, settings: () => input.settings,
		contentReady: () => input.contentReady
	}); });
	return { input, dismissal };
}

describe('widget dismissal', () => {
	it.each(['temple.offers', 'mercenary.verdict', 'lab.fontalerts'])('%s hides at the inherited deadline', (id) => {
		const { dismissal } = setup(id);
		vi.advanceTimersByTime(9999); expect(dismissal.hidden).toBe(false);
		vi.advanceTimersByTime(1); expect(dismissal.hidden).toBe(true);
	});
	it.each(['temple.offers', 'mercenary.verdict', 'lab.fontalerts'])('%s hides only for its effective shortcut', (id) => {
		const { dismissal } = setup(id);
		dismissal.applyKey(f); expect(dismissal.hidden).toBe(false);
		dismissal.applyKey(esc); expect(dismissal.hidden).toBe(true);
	});
	it('an inheriting widget follows a global timeout change from its original show time', () => {
		const { input, dismissal } = setup();
		vi.advanceTimersByTime(4000);
		input.settings!.timeoutSeconds = 5; flushSync();
		vi.advanceTimersByTime(999); expect(dismissal.hidden).toBe(false);
		vi.advanceTimersByTime(1); expect(dismissal.hidden).toBe(true);
	});
	it('an inheriting widget follows a changed global key', () => {
		const { input, dismissal } = setup();
		input.settings!.shortcut = f; flushSync();
		dismissal.applyKey(esc); expect(dismissal.hidden).toBe(false);
		dismissal.applyKey(f); expect(dismissal.hidden).toBe(true);
	});
	it('an override retains its own deadline across global updates', () => {
		const { input, dismissal } = setup();
		input.settings!.overlays['temple.offers'] = { timeout: { override: 20 }, shortcut: { override: f } };
		flushSync(); input.settings!.timeoutSeconds = 5; flushSync();
		vi.advanceTimersByTime(19999); expect(dismissal.hidden).toBe(false);
		vi.advanceTimersByTime(1); expect(dismissal.hidden).toBe(true);
	});
	it('an override retains its key across global updates', () => {
		const { input, dismissal } = setup();
		input.settings!.overlays['temple.offers'] = { timeout: 'inherit', shortcut: { override: f } };
		input.settings!.shortcut = { ...esc, key: 65 }; flushSync();
		dismissal.applyKey(input.settings!.shortcut); expect(dismissal.hidden).toBe(false);
		dismissal.applyKey(f); expect(dismissal.hidden).toBe(true);
	});
	it('timeout off never hides', () => {
		const { input, dismissal } = setup(); input.settings!.timeoutSeconds = 0; flushSync();
		vi.advanceTimersByTime(60000); expect(dismissal.hidden).toBe(false);
		expect(vi.getTimerCount()).toBe(0);
	});
	it('a widget without dismissal flags preserves its own mechanism', () => {
		const { dismissal } = setup('temple.door');
		dismissal.applyKey(esc); vi.advanceTimersByTime(60000); expect(dismissal.hidden).toBe(false);
	});
	it('a fresh show restores a dismissed widget', () => {
		const { input, dismissal } = setup(); dismissal.applyKey(esc);
		input.shown = false; flushSync(); input.shown = true; flushSync();
		expect(dismissal.hidden).toBe(false);
		vi.advanceTimersByTime(10000); expect(dismissal.hidden).toBe(true);
	});
	it.each(['key', 'timeout'] as const)('a later identical lab read restores an alert dismissed by %s', (trigger) => {
		const { input, dismissal } = setup('lab.fontalerts');
		if (trigger === 'key') dismissal.applyKey(esc); else vi.advanceTimersByTime(10000);
		expect(dismissal.hidden).toBe(true);
		input.key++; flushSync(); expect(dismissal.hidden).toBe(false);
		vi.advanceTimersByTime(9999); expect(dismissal.hidden).toBe(false);
		vi.advanceTimersByTime(1); expect(dismissal.hidden).toBe(true);
	});
	it('a lab read restores the identical alert when the empty intermediate reset was not polled', () => {
		const { input, dismissal } = setup('lab.fontalerts'); dismissal.applyKey(esc);
		input.key += 2; flushSync(); expect(dismissal.hidden).toBe(false);
	});
	it('unchanged polls retain the original deadline', () => {
		const { input, dismissal } = setup('lab.fontalerts');
		vi.advanceTimersByTime(6000);
		input.settings = { ...input.settings! }; flushSync();
		expect(vi.getTimerCount()).toBe(1);
		vi.advanceTimersByTime(4000); expect(dismissal.hidden).toBe(true);
	});
	it('unchanged polls keep a dismissed read hidden without another timer', () => {
		const { input, dismissal } = setup('lab.fontalerts'); dismissal.applyKey(esc); flushSync();
		input.settings = { ...input.settings! }; flushSync(); vi.advanceTimersByTime(60000);
		expect(dismissal.hidden).toBe(true); expect(vi.getTimerCount()).toBe(0);
	});
});

function host(ack = true, loaded = true, id = 'temple.offers') {
	const { input, dismissal } = setup(id, loaded);
	const state: { pending: PendingDismiss[] } = client.proxy({ pending: [] });
	const frames: (() => void)[] = [];
	let deliver!: (event: { payload: PendingDismiss }) => void;
	const unlisten = vi.fn();
	const atAck: { gen: unknown; shown: boolean; hidden: boolean }[] = [];
	const invoke = vi.fn(async (command: string, args: Record<string, unknown>) => {
		if (command === 'overlay_dismiss_ack') atAck.push({ gen: args.gen, shown: input.shown, hidden: dismissal.hidden });
	});
	root(() => createDismissalHost({
		window: id.split('.')[0], dismissals: [dismissal], ack,
		pending: () => state.pending,
		listen: async (_event: string, handler: typeof deliver) => { deliver = handler; return unlisten; },
		invoke, frame: () => new Promise<void>((resolve) => frames.push(resolve))
	}));
	const advanceFrame = async () => { expect(frames.length).toBe(1); frames.shift()!(); await settle(); };
	const acks = () => invoke.mock.calls.filter(([cmd]) => cmd === 'overlay_dismiss_ack').map(([, args]) => args.gen);
	return { input, dismissal, state, deliver: (gen: number, shortcut = esc) => deliver({ payload: { gen, shortcut } }), invoke, frames, advanceFrame, acks, atAck, unlisten };
}

describe('dismissal host', () => {
	it.each([0, 1])('hides initial lab content arriving after %s repaint frames before acknowledging', async (elapsed) => {
		const h = host(true, true, 'lab.fontalerts');
		h.input.shown = false; h.input.key = 0; h.input.settings!.timeoutSeconds = 0;
		await settle(); h.deliver(1); await settle();
		if (elapsed) await h.advanceFrame();
		h.input.shown = true; h.input.key = 7; await settle();
		for (let n = elapsed; n < 2; n++) await h.advanceFrame();
		expect(h.dismissal.hidden).toBe(true); expect(h.acks()).toEqual([]);
		await h.advanceFrame(); expect(h.acks()).toEqual([]);
		await h.advanceFrame();
		expect(h.atAck).toEqual([{ gen: 1, shown: true, hidden: true }]);
		h.deliver(1); await settle(); expect(h.frames).toHaveLength(0);
		h.input.key = 8; await settle(); expect(h.dismissal.hidden).toBe(false);
	});
	it.each([false, true])('waits for initial lab content to resolve (shown=%s)', async (shown) => {
		const h = host(true, true, 'lab.fontalerts');
		h.input.shown = false; h.input.contentReady = false;
		await settle(); h.deliver(1); await settle();
		expect(h.acks()).toEqual([]); expect(h.frames).toHaveLength(0);
		h.input.shown = shown; h.input.key = 7; h.input.contentReady = true;
		await settle(); expect(h.acks()).toEqual([]);
		await h.advanceFrame(); expect(h.acks()).toEqual([]);
		await h.advanceFrame();
		expect(h.atAck).toEqual([{ gen: 1, shown, hidden: shown }]);
	});
	it('acknowledges genuinely empty lab content', async () => {
		const h = host(true, true, 'lab.fontalerts'); h.input.shown = false;
		await settle(); h.deliver(1); await settle();
		await h.advanceFrame(); await h.advanceFrame();
		expect(h.atAck).toEqual([{ gen: 1, shown: false, hidden: false }]);
	});
	it('acknowledges a nonmatching shortcut while delayed lab content remains visible', async () => {
		const h = host(true, true, 'lab.fontalerts'); h.input.shown = false;
		await settle(); h.deliver(1, f); await settle();
		h.input.shown = true; h.input.key = 7; await settle();
		await h.advanceFrame(); await h.advanceFrame();
		expect(h.atAck).toEqual([{ gen: 1, shown: true, hidden: false }]);
	});
	it('registers the temple host on mount', async () => {
		const h = host(); await settle();
		expect(h.invoke).toHaveBeenCalledWith('overlay_dismiss_host', { window: 'temple' });
	});
	it.each(['temple.offers', 'lab.fontalerts'])('%s acknowledges only after hiding and two frames', async (id) => {
		const h = host(true, true, id); await settle(); h.deliver(1); await settle();
		expect(h.dismissal.hidden).toBe(true); expect(h.acks()).toEqual([]);
		await h.advanceFrame(); expect(h.acks()).toEqual([]);
		await h.advanceFrame(); expect(h.acks()).toEqual([1]);
	});
	it('applies an SSOT-only generation exactly once across duplicate events', async () => {
		const h = host(); await settle(); h.state.pending = [{ gen: 1, shortcut: esc }]; await settle();
		expect(h.dismissal.hidden).toBe(true);
		h.deliver(1); await h.advanceFrame(); await h.advanceFrame(); h.deliver(1); await settle();
		expect(h.acks()).toEqual([1]); expect(h.frames).toHaveLength(0);
	});
	it('a late earlier generation applies its own override shortcut', async () => {
		const h = host(); h.input.settings!.overlays['temple.offers'] = { timeout: 'inherit', shortcut: { override: f } };
		await settle(); h.deliver(2); await settle();
		expect(h.dismissal.hidden).toBe(false);
		await h.advanceFrame(); await h.advanceFrame(); expect(h.acks()).toEqual([2]);
		h.state.pending = [{ gen: 1, shortcut: f }, { gen: 2, shortcut: esc }]; await settle();
		expect(h.dismissal.hidden).toBe(true); expect(h.acks()).toEqual([2]);
		await h.advanceFrame(); expect(h.acks()).toEqual([2]);
		await h.advanceFrame(); expect(h.acks()).toEqual([2, 1]);
	});
	it('a generation absent from both delivery paths is never acknowledged', async () => {
		const h = host(); h.input.settings!.overlays['temple.offers'] = { timeout: 'inherit', shortcut: { override: f } };
		await settle(); h.deliver(2); await settle(); await h.advanceFrame(); await h.advanceFrame();
		expect(h.dismissal.hidden).toBe(false); expect(h.acks()).toEqual([2]);
	});
	it('a pending batch applies generations in ascending order', async () => {
		const h = host(); await settle(); h.state.pending = [{ gen: 2, shortcut: esc }, { gen: 1, shortcut: f }];
		await settle(); expect(h.dismissal.hidden).toBe(false);
		await h.advanceFrame(); await h.advanceFrame(); await settle();
		expect(h.dismissal.hidden).toBe(true); expect(h.acks()).toEqual([1]);
		await h.advanceFrame(); await h.advanceFrame(); expect(h.acks()).toEqual([1, 2]);
	});
	it('settings catch-up applies the effective override before any acknowledgement', async () => {
		const h = host(true, false); await settle(); h.deliver(1, f); await settle();
		expect(h.acks()).toEqual([]); expect(h.frames).toHaveLength(0);
		h.input.settings = { timeoutSeconds: 10, shortcut: esc, overlays: { 'temple.offers': { timeout: 'inherit', shortcut: { override: f } } } };
		await settle(); expect(h.dismissal.hidden).toBe(true); expect(h.acks()).toEqual([]);
		await h.advanceFrame(); await h.advanceFrame(); expect(h.acks()).toEqual([1]);
	});
	it('an excluded mercenary host dismisses without registration or acknowledgement', async () => {
		const h = host(false, true, 'mercenary.verdict'); await settle(); h.deliver(1); await settle();
		expect(h.dismissal.hidden).toBe(true); await h.advanceFrame(); await h.advanceFrame();
		expect(h.invoke).not.toHaveBeenCalled();
	});
	it('unmount cancels an acknowledgement waiting on repaint', async () => {
		const h = host(); await settle(); h.deliver(1); await settle();
		cleanups.splice(0).forEach((stop) => stop()); await h.advanceFrame();
		expect(h.acks()).toEqual([]); expect(h.unlisten).toHaveBeenCalledOnce();
	});
	it('a rejected acknowledgement is reported through the application log', async () => {
		const h = host(); await settle();
		const warning = vi.spyOn(console, 'warn').mockImplementation(() => {});
		try {
			h.invoke.mockRejectedValueOnce(new Error('ack unavailable'));
			h.deliver(1); await settle(); await h.advanceFrame(); await h.advanceFrame();
			expect(h.dismissal.hidden).toBe(true);
			expect(warning).toHaveBeenCalledWith('[temple-overlay] dismissal failed: Error: ack unavailable');
			expect(h.invoke).toHaveBeenCalledWith('app_log_from_frontend', {
				msg: '[temple-overlay] dismissal failed: Error: ack unavailable'
			});
		} finally { warning.mockRestore(); }
	});
});
