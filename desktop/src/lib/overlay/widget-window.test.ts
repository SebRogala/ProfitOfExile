import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const h = vi.hoisted(() => {
	type Handler = (payload?: unknown) => unknown;
	const ops: string[] = [];
	const gates = { setPosition: false, setSize: false };
	const registry = new Map<string, FakeWindow>();
	const instances: FakeWindow[] = [];

	class FakeWindow {
		static getByLabel = async (label: string) => registry.get(label) ?? null;
		readonly handlers = new Map<string, Handler>();
		/** A sticky window keeps its label through close/destroy. */
		sticky = false;
		closeFailures = 0;
		readonly setPosition = vi.fn(async (_p: unknown) => {
			ops.push('setPosition');
			if (gates.setPosition) throw new Error('setPosition refused');
		});
		readonly setSize = vi.fn(async (_s: unknown) => {
			ops.push('setSize');
			if (gates.setSize) throw new Error('setSize refused');
		});
		readonly hide = vi.fn(async () => {
			ops.push('hide');
		});
		readonly close = vi.fn(async () => {
			ops.push('close');
			if (this.closeFailures > 0) {
				this.closeFailures--;
				throw new Error('close refused');
			}
		});
		readonly destroy = vi.fn(async () => {
			ops.push('destroy');
			if (!this.sticky && registry.get(this.label) === this) registry.delete(this.label);
		});
		constructor(
			public readonly label: string,
			public readonly options: Record<string, unknown> = {}
		) {
			ops.push('ctor');
			registry.set(label, this);
			instances.push(this);
		}
		once(event: string, handler: Handler) {
			this.handlers.set(event, handler);
			return Promise.resolve(() => {});
		}
	}

	return { ops, gates, registry, instances, FakeWindow };
});

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/dpi', () => ({
	PhysicalPosition: class {
		constructor(public x: number, public y: number) {}
	},
	PhysicalSize: class {
		constructor(public width: number, public height: number) {}
	}
}));
vi.mock('@tauri-apps/api/webviewWindow', () => ({ WebviewWindow: h.FakeWindow }));
vi.mock('@tauri-apps/api/window', () => ({
	availableMonitors: vi.fn(),
	currentMonitor: vi.fn(),
	primaryMonitor: vi.fn()
}));

import { invoke } from '@tauri-apps/api/core';
import { availableMonitors, currentMonitor, primaryMonitor } from '@tauri-apps/api/window';
import '@tauri-apps/api/dpi';
import '@tauri-apps/api/webviewWindow';
import { createWidgetWindow, type WidgetWindow } from './widget-window';
import type { GameMonitorInfo } from './monitor-choice';

type FakeWindow = InstanceType<typeof h.FakeWindow>;
type Monitor = { position: { x: number; y: number }; size: { width: number; height: number }; scaleFactor: number };

const LABEL = 'temple';
const MON_A: Monitor = { position: { x: 0, y: 0 }, size: { width: 3840, height: 2160 }, scaleFactor: 1.5 };
const MON_B: Monitor = { position: { x: 3840, y: 0 }, size: { width: 1920, height: 1080 }, scaleFactor: 1 };
const GAME_A: GameMonitorInfo = { id: 11, x: 0, y: 0, width: 3840, height: 2160 };
const GAME_B: GameMonitorInfo = { id: 22, x: 3840, y: 0, width: 1920, height: 1080 };

/** One answer per `get_game_monitor` call, in order; the last one repeats. */
type GameAnswer = GameMonitorInfo | null | Error;
let gameAnswers: GameAnswer[];
let gameQueries: { built: boolean }[];
let statusAnswer: unknown;
let clickthroughFails: boolean;
let appLogFails: boolean;
let appLogs: string[];
let wanted: boolean;
let configLive: boolean;
let debug: boolean;
let ww: WidgetWindow;

const invokeMock = vi.mocked(invoke);

beforeEach(() => {
	vi.useFakeTimers();
	vi.spyOn(console, 'warn').mockImplementation(() => {});
	vi.spyOn(console, 'error').mockImplementation(() => {});
	h.ops.length = 0;
	h.instances.length = 0;
	h.registry.clear();
	h.gates.setPosition = false;
	h.gates.setSize = false;
	gameAnswers = [GAME_A];
	gameQueries = [];
	statusAnswer = { game_focused: true };
	clickthroughFails = false;
	appLogFails = false;
	appLogs = [];
	wanted = true;
	configLive = false;
	debug = false;
	vi.mocked(primaryMonitor).mockResolvedValue(MON_A as never);
	vi.mocked(currentMonitor).mockResolvedValue(null);
	vi.mocked(availableMonitors).mockResolvedValue([MON_A, MON_B] as never);
	invokeMock.mockImplementation((async (cmd: string, args?: { msg?: string }) => {
		switch (cmd) {
			case 'app_log_from_frontend':
				appLogs.push(args?.msg ?? '');
				if (appLogFails) throw new Error('log channel down');
				return;
			case 'get_game_monitor': {
				gameQueries.push({ built: ww.driver.built() });
				const answer = gameAnswers.length > 1 ? gameAnswers.shift()! : gameAnswers[0];
				if (answer instanceof Error) throw answer;
				return answer;
			}
			case 'get_status':
				h.ops.push('get_status');
				if (statusAnswer instanceof Error) throw statusAnswer;
				return statusAnswer;
			case 'set_overlay_clickthrough':
				h.ops.push('clickthrough');
				if (clickthroughFails) throw new Error('clickthrough refused');
				return;
		}
		throw new Error(`unexpected command ${cmd}`);
	}) as never);
	ww = createWidgetWindow({
		label: LABEL,
		moduleId: 'temple-module',
		wanted: () => wanted,
		configLive: () => configLive,
		debug: () => debug
	});
});

afterEach(() => {
	vi.useRealTimers();
	vi.restoreAllMocks();
});

/** Drain microtasks only — no timer fires. */
async function microtasks(): Promise<void> {
	for (let round = 0; round < 3; round++) {
		await vi.dynamicImportSettled();
		for (let i = 0; i < 50; i++) await Promise.resolve();
	}
}

function lastWindow(): FakeWindow {
	const win = h.instances.at(-1);
	if (!win) throw new Error('no window was constructed');
	return win;
}

async function fire(win: FakeWindow, event: 'tauri://created' | 'tauri://error', payload?: unknown) {
	const handler = win.handlers.get(event);
	if (!handler) throw new Error(`no ${event} handler registered`);
	await handler(payload);
	await microtasks();
}

/** setDesired(true) and run the create up to its constructor. */
async function startBuild(): Promise<FakeWindow> {
	ww.driver.setDesired(true);
	await microtasks();
	return lastWindow();
}

/** A complete build, its reconcile included. */
async function settledBuild(): Promise<FakeWindow> {
	const win = await startBuild();
	await fire(win, 'tauri://created');
	await vi.advanceTimersByTimeAsync(0);
	return win;
}

const builtOnB = (win: FakeWindow) =>
	win.options.x === MON_B.position.x && win.options.width === MON_B.size.width;

const logsMatching = (text: string) => appLogs.filter((m) => m.includes(text));

describe('widget window build', () => {
	it('passes the constructor logical geometry rounded from the physical monitor', async () => {
		vi.mocked(primaryMonitor).mockResolvedValue({
			position: { x: 1500, y: 301 },
			size: { width: 3001, height: 1800 },
			scaleFactor: 1.5
		} as never);
		gameAnswers = [null];
		const win = await startBuild();
		expect(win.options).toMatchObject({ x: 1000, y: 201, width: 2001, height: 1200 });
	});

	it('applies the exact physical position and size once created', async () => {
		vi.mocked(primaryMonitor).mockResolvedValue({
			position: { x: 1500, y: 301 },
			size: { width: 3001, height: 1800 },
			scaleFactor: 1.5
		} as never);
		gameAnswers = [null];
		const win = await startBuild();
		await fire(win, 'tauri://created');
		expect(win.setPosition).toHaveBeenCalledWith({ x: 1500, y: 301 });
		expect(win.setSize).toHaveBeenCalledWith({ width: 3001, height: 1800 });
	});

	it('adds ?debug to the route when debug is on', async () => {
		debug = true;
		const win = await startBuild();
		expect(win.options.url).toBe('/overlay/temple?debug');
	});

	it('builds the plain route when debug is off', async () => {
		const win = await startBuild();
		expect(win.options.url).toBe('/overlay/temple');
	});

	it('builds without taking focus', async () => {
		const win = await startBuild();
		expect(win.options.focus).toBe(false);
	});

	it('positions, sizes, then registers click-through before reporting built', async () => {
		const win = await startBuild();
		expect(ww.driver.built()).toBe(false);
		await fire(win, 'tauri://created');
		expect(h.ops.filter((o) => ['setPosition', 'setSize', 'clickthrough'].includes(o))).toEqual([
			'setPosition',
			'setSize',
			'clickthrough'
		]);
		expect(ww.driver.built()).toBe(true);
	});
});

describe('half-built teardown', () => {
	const gates: [string, () => void][] = [
		['setPosition', () => (h.gates.setPosition = true)],
		['setSize', () => (h.gates.setSize = true)],
		['set_overlay_clickthrough', () => (clickthroughFails = true)]
	];

	it.each(gates)('tears the window down before the retry when %s rejects', async (_name, reject) => {
		reject();
		const win = await startBuild();
		void fire(win, 'tauri://created');
		await microtasks();
		// The teardown's own 100 ms re-check sleep is the only wait before settling.
		await vi.advanceTimersByTimeAsync(100);
		expect(win.close).toHaveBeenCalledTimes(1);
		expect(win.destroy).toHaveBeenCalledTimes(1);
		expect(h.registry.has(LABEL)).toBe(false);
		expect(ww.driver.built()).toBe(false);
		expect(h.instances).toHaveLength(1);

		await vi.advanceTimersByTimeAsync(499);
		expect(h.instances).toHaveLength(1);
		await vi.advanceTimersByTimeAsync(1);
		expect(h.instances).toHaveLength(2);
	});
});

describe('no monitor to build on', () => {
	const cases: [string, () => void][] = [
		['primary and current both null', () => {}],
		[
			'primary and current both throw',
			() => {
				vi.mocked(primaryMonitor).mockRejectedValue(new Error('no primary'));
				vi.mocked(currentMonitor).mockRejectedValue(new Error('no current'));
			}
		],
		[
			'the game lookup throws and no primary resolves',
			() => {
				gameAnswers = [new Error('no game')];
			}
		]
	];

	it.each(cases)('refuses to construct when %s', async (_name, arrange) => {
		vi.mocked(primaryMonitor).mockResolvedValue(null);
		vi.mocked(availableMonitors).mockResolvedValue([]);
		arrange();
		ww.driver.setDesired(true);
		await microtasks();
		expect(h.instances).toHaveLength(0);
		expect(logsMatching('no monitor to build the overlay on')).toEqual([
			'[temple-overlay] no monitor to build the overlay on — not creating it'
		]);
	});
});

describe('initial focus check', () => {
	it('hides the window when the game is not focused', async () => {
		statusAnswer = { game_focused: false };
		const win = await startBuild();
		await fire(win, 'tauri://created');
		expect(win.hide).toHaveBeenCalledTimes(1);
	});

	it('leaves the window shown when the game is focused', async () => {
		const win = await startBuild();
		await fire(win, 'tauri://created');
		expect(win.hide).not.toHaveBeenCalled();
	});

	it('leaves the window visible and logs when get_status throws', async () => {
		statusAnswer = new Error('status down');
		const win = await startBuild();
		await fire(win, 'tauri://created');
		expect(win.hide).not.toHaveBeenCalled();
		expect(logsMatching('initial focus check failed')).toEqual([
			'[temple-overlay] initial focus check failed, window left visible: Error: status down'
		]);
	});

	it('skips the focus check during a widget-config session', async () => {
		configLive = true;
		statusAnswer = { game_focused: false };
		const win = await startBuild();
		await fire(win, 'tauri://created');
		expect(h.ops).not.toContain('get_status');
		expect(win.hide).not.toHaveBeenCalled();
	});
});

describe('destroy ladder', () => {
	it('gives the label up after five close/destroy rounds 100 ms apart', async () => {
		const stuck = new h.FakeWindow(LABEL);
		stuck.sticky = true;
		ww.driver.setDesired(true);
		await microtasks();
		expect(stuck.destroy).toHaveBeenCalledTimes(1);
		for (let round = 2; round <= 5; round++) {
			await vi.advanceTimersByTimeAsync(100);
			expect(stuck.close).toHaveBeenCalledTimes(round);
			expect(stuck.destroy).toHaveBeenCalledTimes(round);
		}
		expect(logsMatching('giving the label up')).toHaveLength(0);
		await vi.advanceTimersByTimeAsync(100);
		expect(stuck.close).toHaveBeenCalledTimes(5);
		expect(stuck.destroy).toHaveBeenCalledTimes(5);
		expect(logsMatching('giving the label up')).toEqual([
			'[temple-overlay] window still present after 5 close/destroy rounds — giving the label up'
		]);
	});

	it('still calls destroy in the round whose close throws', async () => {
		const leftover = new h.FakeWindow(LABEL);
		leftover.closeFailures = 1;
		ww.driver.setDesired(true);
		await microtasks();
		expect(leftover.destroy).toHaveBeenCalledTimes(1);
		expect(h.registry.get(LABEL)).not.toBe(leftover);
		expect(logsMatching('close attempt 1 failed')).toHaveLength(1);
	});
});

describe('post-build monitor reconcile', () => {
	it('rebuilds once onto the queried game monitor when the game moved during the build', async () => {
		gameAnswers = [GAME_A, GAME_B];
		const first = await startBuild();
		await fire(first, 'tauri://created');
		await vi.advanceTimersByTimeAsync(0);
		await vi.advanceTimersByTimeAsync(100);
		expect(first.destroy).toHaveBeenCalledTimes(1);
		expect(h.instances).toHaveLength(2);
		const second = lastWindow();
		expect(builtOnB(second)).toBe(true);
		await fire(second, 'tauri://created');
		await vi.advanceTimersByTimeAsync(1000);
		expect(h.instances).toHaveLength(2);
	});

	it('falls back to the notice recorded during the build when the re-check fails', async () => {
		gameAnswers = [GAME_A, new Error('lookup down'), GAME_B];
		const first = await startBuild();
		ww.onGameMonitorChanged(GAME_B);
		await fire(first, 'tauri://created');
		await vi.advanceTimersByTimeAsync(100);
		expect(h.instances).toHaveLength(2);
		expect(builtOnB(lastWindow())).toBe(true);
	});

	it('trusts the re-check over a recorded notice', async () => {
		gameAnswers = [GAME_A];
		const first = await startBuild();
		ww.onGameMonitorChanged(GAME_B);
		await fire(first, 'tauri://created');
		await vi.advanceTimersByTimeAsync(1000);
		expect(h.instances).toHaveLength(1);
		expect(first.destroy).not.toHaveBeenCalled();
	});

	it('re-checks exactly once, on the timer turn after the driver has settled', async () => {
		const first = await startBuild();
		await fire(first, 'tauri://created');
		expect(gameQueries).toHaveLength(1);
		await vi.advanceTimersByTimeAsync(0);
		await vi.advanceTimersByTimeAsync(1000);
		expect(gameQueries).toEqual([{ built: false }, { built: true }]);
	});

	it('clears a notice recorded by a failed build before the next build', async () => {
		gameAnswers = [GAME_A, GAME_A, new Error('lookup down')];
		const first = await startBuild();
		ww.onGameMonitorChanged(GAME_B);
		await fire(first, 'tauri://error', { payload: 'boom' });
		// 500 ms backoff, then the retry's sweep of the failed window (100 ms).
		await vi.advanceTimersByTimeAsync(600);
		expect(h.instances).toHaveLength(2);
		await fire(lastWindow(), 'tauri://created');
		await vi.advanceTimersByTimeAsync(1000);
		expect(h.instances).toHaveLength(2);
		expect(ww.driver.built()).toBe(true);
	});

	it('corrects a stale build even while a config session is live', async () => {
		configLive = true;
		gameAnswers = [GAME_A, GAME_B];
		const first = await startBuild();
		await fire(first, 'tauri://created');
		await vi.advanceTimersByTimeAsync(100);
		expect(h.instances).toHaveLength(2);
		expect(builtOnB(lastWindow())).toBe(true);
	});

	it('leaves a stale window alone when nothing wants it any more', async () => {
		gameAnswers = [GAME_A, GAME_B];
		const first = await startBuild();
		await fire(first, 'tauri://created');
		wanted = false;
		await vi.advanceTimersByTimeAsync(1000);
		expect(h.instances).toHaveLength(1);
		expect(first.destroy).not.toHaveBeenCalled();
		expect(logsMatching('nothing wants the window any more')).toHaveLength(1);
	});

	it('does not rebuild on a later notice for the display a reconciled window is on', async () => {
		gameAnswers = [GAME_A];
		const first = await startBuild();
		ww.onGameMonitorChanged(GAME_B);
		await fire(first, 'tauri://created');
		await vi.advanceTimersByTimeAsync(0);
		ww.onGameMonitorChanged(GAME_A);
		await vi.advanceTimersByTimeAsync(1000);
		expect(h.instances).toHaveLength(1);
		expect(first.destroy).not.toHaveBeenCalled();
	});

	it('schedules no re-check after a failed build', async () => {
		const first = await startBuild();
		await fire(first, 'tauri://error', { payload: 'boom' });
		await vi.advanceTimersByTimeAsync(499);
		expect(gameQueries).toHaveLength(1);
		expect(h.instances).toHaveLength(1);
	});
});

describe('game-monitor-changed on a settled window', () => {
	it('rebuilds onto the display the notice names', async () => {
		gameAnswers = [GAME_A, GAME_A, GAME_B];
		const first = await settledBuild();
		ww.onGameMonitorChanged(GAME_B);
		await vi.advanceTimersByTimeAsync(100);
		expect(first.destroy).toHaveBeenCalledTimes(1);
		expect(h.instances).toHaveLength(2);
		expect(builtOnB(lastWindow())).toBe(true);
		expect(logsMatching('rebuilding the overlay there')).toHaveLength(1);
	});

	it('defers the rebuild during a config session and rebuilds on the next notice after it', async () => {
		gameAnswers = [GAME_A, GAME_A, GAME_B];
		await settledBuild();
		configLive = true;
		ww.onGameMonitorChanged(GAME_B);
		await vi.advanceTimersByTimeAsync(1000);
		expect(h.instances).toHaveLength(1);
		expect(logsMatching('deferring the rebuild')).toHaveLength(1);
		configLive = false;
		ww.onGameMonitorChanged(GAME_B);
		await vi.advanceTimersByTimeAsync(100);
		expect(h.instances).toHaveLength(2);
		expect(builtOnB(lastWindow())).toBe(true);
	});

	it('does not resurrect a window nothing wants', async () => {
		gameAnswers = [GAME_A, GAME_A, GAME_B];
		const first = await settledBuild();
		wanted = false;
		ww.onGameMonitorChanged(GAME_B);
		await vi.advanceTimersByTimeAsync(1000);
		expect(h.instances).toHaveLength(1);
		expect(first.destroy).not.toHaveBeenCalled();
		expect(logsMatching('nothing wants the window any more')).toHaveLength(1);
	});
});

describe('app log channel', () => {
	it('prefixes the message with the window label', async () => {
		ww.log('hello');
		await microtasks();
		expect(appLogs).toEqual(['[temple-overlay] hello']);
	});

	it('reports an unreachable app log on the console instead of rejecting', async () => {
		appLogFails = true;
		expect(() => ww.log('hello')).not.toThrow();
		await microtasks();
		expect(console.error).toHaveBeenCalledWith(
			'[overlay] temple: app log unreachable:',
			expect.objectContaining({ message: 'log channel down' })
		);
	});
});
