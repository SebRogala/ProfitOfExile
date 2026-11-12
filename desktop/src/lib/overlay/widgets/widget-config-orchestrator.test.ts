/**
 * The widget-config flow against fake ports: every port call lands in one
 * ordered log, the clock is vitest's fake one, and the sessions go through the
 * real `widgetConfigStart`/`widgetConfigEnd` reducers. Assertions are on that
 * log and on the sessions the flow leaves behind.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
	createWidgetConfigOrchestrator,
	WIDGET_CONFIG_SESSION_MAX_MS,
	type ConfigWindow,
	type WidgetConfigPorts
} from './widget-config-orchestrator';
import { widgetConfigSessionsInit, type WidgetConfigSessions } from './widget-config-session';

const MODULE = 'temple';
const LABEL = 'temple';

type Call = [string, ...unknown[]];

interface Harness {
	ports: WidgetConfigPorts;
	/** Every port call except the app-log mirror, in order. */
	calls: Call[];
	/** Every line sent to `warn`. */
	warnings: string[];
	errors: unknown[][];
	appLogs: string[];
	sessions: () => WidgetConfigSessions;
	world: {
		built: boolean;
		visible: boolean;
		gameFocused: boolean;
		lookupRejects: boolean;
		importRejects: boolean;
		reject: Partial<Record<string, unknown>>;
	};
}

/** `reject` keys: 'show', 'hide', 'isVisible', 'get_status', 'app_log',
 *  'set_overlay_config_mode:on', 'set_overlay_config_mode:off',
 *  'emitTo:on', 'emitTo:off' (both off sends), 'emitTo:off#1' / 'emitTo:off#2'
 *  (first / repeated off send only), and any `emitToMain` event name. */
function harness(init: Partial<Harness['world']> = {}): Harness {
	const world: Harness['world'] = {
		built: true,
		visible: false,
		gameFocused: false,
		lookupRejects: false,
		importRejects: false,
		reject: {},
		...init
	};
	const calls: Call[] = [];
	const warnings: string[] = [];
	const errors: unknown[][] = [];
	const appLogs: string[] = [];
	let sessions = widgetConfigSessionsInit();
	let offSends = 0;
	const fail = (key: string) =>
		key in world.reject ? Promise.reject(world.reject[key]) : Promise.resolve();

	const win: ConfigWindow = {
		isVisible: async () => {
			calls.push(['isVisible']);
			await fail('isVisible');
			return world.visible;
		},
		show: async () => {
			calls.push(['show']);
			await fail('show');
		},
		hide: async () => {
			calls.push(['hide']);
			await fail('hide');
		}
	};

	const ports: WidgetConfigPorts = {
		loadWindowApi: async () => {
			if (world.importRejects) throw 'import boom';
		},
		getWindowByLabel: async (label) => {
			calls.push(['getWindowByLabel', label]);
			if (world.lookupRejects) throw 'lookup boom';
			return world.built ? win : null;
		},
		invoke: async (cmd, args) => {
			if (cmd === 'app_log_from_frontend') {
				appLogs.push((args as { msg: string }).msg);
				await fail('app_log');
				return;
			}
			calls.push(['invoke', cmd, args]);
			if (cmd === 'get_status') {
				await fail('get_status');
				return { game_focused: world.gameFocused };
			}
			if (cmd === 'set_overlay_config_mode') {
				await fail(`set_overlay_config_mode:${(args as { on: boolean }).on ? 'on' : 'off'}`);
			}
		},
		emitTo: async (label, event, payload) => {
			calls.push(['emitTo', label, event, payload]);
			const on = (payload as { on: boolean }).on;
			if (!on) {
				offSends += 1;
				await fail(`emitTo:off#${offSends}`);
			}
			await fail(`emitTo:${on ? 'on' : 'off'}`);
		},
		emitToMain: async (event, payload) => {
			calls.push(['emitToMain', event, payload]);
			await fail(event);
		},
		getSessions: () => sessions,
		setSessions: (next) => {
			calls.push(['setSessions', next]);
			sessions = next;
		},
		targets: () => ({ [MODULE]: { label: LABEL, built: () => world.built } }),
		setTimer: (cb, ms) => setTimeout(cb, ms),
		clearTimer: (t) => clearTimeout(t as ReturnType<typeof setTimeout>),
		wait: (ms) => new Promise<void>((r) => setTimeout(r, ms)),
		now: () => Date.now(),
		warn: (msg) => warnings.push(String(msg)),
		error: (...a) => errors.push(a)
	};
	return { ports, calls, warnings, errors, appLogs, sessions: () => sessions, world };
}

const LIVE = (shown: boolean) => ['setSessions', { [MODULE]: { shown } }];
const DROPPED = ['setSessions', {}];

const OPEN_SEQUENCE = (shown: boolean): unknown[] => [
	['emitToMain', 'widget-config-opening', { module: MODULE }],
	['getWindowByLabel', LABEL],
	['isVisible'],
	LIVE(shown),
	['show'],
	['invoke', 'set_overlay_config_mode', { label: LABEL, on: true }],
	['emitTo', LABEL, 'widget-config', { module: MODULE, on: true }],
	['emitToMain', 'widget-config-open', { module: MODULE }]
];

const ABANDON_HEAD: unknown[] = [
	['emitTo', LABEL, 'widget-config', { module: MODULE, on: false }],
	['invoke', 'set_overlay_config_mode', { label: LABEL, on: false }],
	['emitTo', LABEL, 'widget-config', { module: MODULE, on: false }],
	['invoke', 'get_status', undefined]
];

/** The abandon of a session that forced a show, with the game unfocused. */
const ABANDON_WITH_HIDE: unknown[] = [
	...ABANDON_HEAD,
	['getWindowByLabel', LABEL],
	['hide'],
	DROPPED,
	['emitToMain', 'widget-config-end', { module: MODULE }]
];

const ABANDON_NO_HIDE: unknown[] = [
	...ABANDON_HEAD,
	DROPPED,
	['emitToMain', 'widget-config-end', { module: MODULE }]
];

function opened(h: Harness) {
	return createWidgetConfigOrchestrator(h.ports);
}

beforeEach(() => {
	vi.useFakeTimers();
	vi.setSystemTime(0);
});
afterEach(() => {
	vi.useRealTimers();
});

describe('opening a config session on a built window', () => {
	it('runs the ordered steps and acknowledges to Settings', async () => {
		const h = harness();
		await opened(h).start(MODULE);
		expect(h.calls).toEqual(OPEN_SEQUENCE(true));
		expect(h.sessions()).toEqual({ [MODULE]: { shown: true } });
	});

	it('records that nothing was forced when the window was already visible', async () => {
		const h = harness({ visible: true });
		await opened(h).start(MODULE);
		expect(h.calls).toEqual(OPEN_SEQUENCE(false));
	});
});

describe('waiting for the window to be built', () => {
	it('records the session before the first wait', async () => {
		const h = harness({ built: false });
		void opened(h).start(MODULE);
		await vi.advanceTimersByTimeAsync(0);
		expect(h.calls).toEqual([
			['emitToMain', 'widget-config-opening', { module: MODULE }],
			['getWindowByLabel', LABEL],
			LIVE(true)
		]);
	});

	it('opens a window that is built at 9,900 ms', async () => {
		const h = harness({ built: false });
		void opened(h).start(MODULE);
		await vi.advanceTimersByTimeAsync(9_899);
		h.world.built = true;
		const before = h.calls.length;
		await vi.advanceTimersByTimeAsync(1);
		expect(h.calls.slice(before)).toEqual([
			['getWindowByLabel', LABEL],
			['show'],
			['invoke', 'set_overlay_config_mode', { label: LABEL, on: true }],
			['emitTo', LABEL, 'widget-config', { module: MODULE, on: true }],
			['emitToMain', 'widget-config-open', { module: MODULE }]
		]);
	});

	it('still opens a window built at 10,000 ms on the next poll', async () => {
		const h = harness({ built: false });
		void opened(h).start(MODULE);
		await vi.advanceTimersByTimeAsync(10_000);
		h.world.built = true;
		await vi.advanceTimersByTimeAsync(50);
		expect(h.calls.at(-1)).toEqual(['emitToMain', 'widget-config-open', { module: MODULE }]);
		expect(h.calls).not.toContainEqual(ABANDON_HEAD[0]);
	});

	it('leaves a window still missing at 10,000 ms pending', async () => {
		const h = harness({ built: false });
		void opened(h).start(MODULE);
		await vi.advanceTimersByTimeAsync(10_049);
		expect(h.calls).not.toContainEqual(ABANDON_HEAD[0]);
		expect(h.calls).not.toContainEqual(['emitToMain', 'widget-config-end', { module: MODULE }]);
	});

	it('abandons at the first poll after expiry, 10,050 ms', async () => {
		const h = harness({ built: false });
		void opened(h).start(MODULE);
		await vi.advanceTimersByTimeAsync(10_049);
		const before = h.calls.length;
		await vi.advanceTimersByTimeAsync(1);
		// No window to hide: the lookup at the end finds none.
		expect(h.calls.slice(before)).toEqual([
			...ABANDON_HEAD,
			['getWindowByLabel', LABEL],
			DROPPED,
			['emitToMain', 'widget-config-end', { module: MODULE }]
		]);
	});
});

describe('a window that never appears', () => {
	it('logs that it did not appear', async () => {
		const h = harness({ built: false });
		void opened(h).start(MODULE);
		await vi.advanceTimersByTimeAsync(10_050);
		expect(h.warnings).toContain(
			'[widget-config] temple: could not open config mode: Error: the temple window did not appear'
		);
	});

	it('never turns config mode on', async () => {
		const h = harness({ built: false });
		void opened(h).start(MODULE);
		await vi.advanceTimersByTimeAsync(10_050);
		expect(h.calls).not.toContainEqual(['invoke', 'set_overlay_config_mode', { label: LABEL, on: true }]);
	});
});

describe.each([
	['show', 'show'],
	['set_overlay_config_mode on', 'set_overlay_config_mode:on'],
	['widget-config on', 'emitTo:on']
])('a rejecting %s', (_name, key) => {
	async function run() {
		const h = harness({ reject: { [key]: 'boom' } });
		const o = opened(h);
		await o.start(MODULE);
		return h;
	}

	it('runs the ordered abandon after logging the failure', async () => {
		const h = await run();
		const at = h.calls.findIndex((c) => c[0] === 'emitTo' && (c[3] as { on: boolean }).on === false);
		expect(h.calls.slice(at)).toEqual(ABANDON_WITH_HIDE);
		expect(h.warnings).toContain('[widget-config] temple: could not open config mode: boom');
	});

	it('never acknowledges the open', async () => {
		const h = await run();
		expect(h.calls).not.toContainEqual(['emitToMain', 'widget-config-open', { module: MODULE }]);
	});

	it('arms no deadline', async () => {
		const h = await run();
		const before = h.calls.length;
		await vi.advanceTimersByTimeAsync(WIDGET_CONFIG_SESSION_MAX_MS + 1);
		expect(h.calls.slice(before)).toEqual([]);
	});
});

describe('a rejecting window lookup', () => {
	it('logs the failure', async () => {
		const h = harness({ lookupRejects: true });
		void opened(h).start(MODULE);
		await vi.advanceTimersByTimeAsync(0);
		expect(h.warnings).toContain('[widget-config] temple: window lookup failed: lookup boom');
	});

	it('counts as no window, so the open waits', async () => {
		const h = harness({ lookupRejects: true });
		void opened(h).start(MODULE);
		await vi.advanceTimersByTimeAsync(0);
		expect(h.calls).toEqual([
			['emitToMain', 'widget-config-opening', { module: MODULE }],
			['getWindowByLabel', LABEL],
			LIVE(true),
			['getWindowByLabel', LABEL]
		]);
	});

	it('opens on the next poll whose lookup resolves', async () => {
		const h = harness({ lookupRejects: true });
		void opened(h).start(MODULE);
		await vi.advanceTimersByTimeAsync(300);
		h.world.lookupRejects = false;
		await vi.advanceTimersByTimeAsync(150);
		expect(h.calls.at(-1)).toEqual(['emitToMain', 'widget-config-open', { module: MODULE }]);
	});
});

describe('the session deadline', () => {
	it('does not fire 1 ms before 10 min', async () => {
		const h = harness();
		await opened(h).start(MODULE);
		const before = h.calls.length;
		await vi.advanceTimersByTimeAsync(WIDGET_CONFIG_SESSION_MAX_MS - 1);
		expect(h.calls.slice(before)).toEqual([]);
	});

	it('abandons the session at 10 min', async () => {
		const h = harness();
		await opened(h).start(MODULE);
		await vi.advanceTimersByTimeAsync(WIDGET_CONFIG_SESSION_MAX_MS - 1);
		const before = h.calls.length;
		await vi.advanceTimersByTimeAsync(1);
		expect(h.calls.slice(before)).toEqual(ABANDON_WITH_HIDE);
		expect(h.warnings).toContain(
			'[widget-config] temple: still in configuration after 10 min — ending the session from here'
		);
	});

	it('restarts from a second successful open', async () => {
		const h = harness();
		const o = opened(h);
		await o.start(MODULE);
		await vi.advanceTimersByTimeAsync(5 * 60_000);
		await o.start(MODULE);
		const before = h.calls.length;
		await vi.advanceTimersByTimeAsync(WIDGET_CONFIG_SESSION_MAX_MS - 1);
		expect(h.calls.slice(before)).toEqual([]);
		await vi.advanceTimersByTimeAsync(1);
		expect(h.calls.slice(before)).toEqual(ABANDON_WITH_HIDE);
	});

	it('is cleared by an end before expiry', async () => {
		const h = harness();
		const o = opened(h);
		await o.start(MODULE);
		await o.end(MODULE);
		const before = h.calls.length;
		await vi.advanceTimersByTimeAsync(WIDGET_CONFIG_SESSION_MAX_MS + 1);
		expect(h.calls.slice(before)).toEqual([]);
	});
});

describe('a second start while one is in flight', () => {
	it('is logged and emits nothing', async () => {
		const h = harness({ built: false });
		const o = opened(h);
		void o.start(MODULE);
		await vi.advanceTimersByTimeAsync(0);
		const before = h.calls.length;
		await o.start(MODULE);
		expect(h.calls.slice(before)).toEqual([]);
		expect(h.warnings).toContain(
			'[widget-config] temple: a config session is already opening — ignoring the second press'
		);
	});

	it('does not abandon the first', async () => {
		const h = harness({ built: false });
		const o = opened(h);
		void o.start(MODULE);
		await vi.advanceTimersByTimeAsync(0);
		await o.start(MODULE);
		h.world.built = true;
		await vi.advanceTimersByTimeAsync(150);
		expect(h.calls.at(-1)).toEqual(['emitToMain', 'widget-config-open', { module: MODULE }]);
		expect(h.calls).not.toContainEqual(ABANDON_HEAD[0]);
	});

	it('is accepted again once the first has settled', async () => {
		const h = harness();
		const o = opened(h);
		await o.start(MODULE);
		const before = h.calls.length;
		await o.start(MODULE);
		expect(h.calls.slice(before)[0]).toEqual(['emitToMain', 'widget-config-opening', { module: MODULE }]);
	});

	it('is accepted again once the first was abandoned', async () => {
		const h = harness({ reject: { show: 'boom' } });
		const o = opened(h);
		await o.start(MODULE);
		const before = h.calls.length;
		await o.start(MODULE);
		expect(h.calls.slice(before)[0]).toEqual(['emitToMain', 'widget-config-opening', { module: MODULE }]);
	});

	it('does not block a different module', async () => {
		const h = harness({ built: false });
		const o = opened(h);
		void o.start(MODULE);
		await vi.advanceTimersByTimeAsync(0);
		const before = h.calls.length;
		await o.start('other');
		expect(h.calls.slice(before)[0]).toEqual(['emitToMain', 'widget-config-opening', { module: 'other' }]);
	});
});

describe('ending a session', () => {
	async function openThenEnd(world: Partial<Harness['world']>) {
		const h = harness(world);
		const o = opened(h);
		await o.start(MODULE);
		const before = h.calls.length;
		await o.end(MODULE);
		return { h, tail: h.calls.slice(before) };
	}

	it('hides a force-shown window before dropping the session', async () => {
		const { h, tail } = await openThenEnd({});
		expect(tail).toEqual([['invoke', 'get_status', undefined], ['getWindowByLabel', LABEL], ['hide'], DROPPED]);
		expect(h.sessions()).toEqual({});
	});

	it('does not hide a window that was already visible', async () => {
		const { tail } = await openThenEnd({ visible: true });
		expect(tail).toEqual([['invoke', 'get_status', undefined], DROPPED]);
	});

	it('does not hide when the game is focused', async () => {
		const { tail } = await openThenEnd({ gameFocused: true });
		expect(tail).toEqual([['invoke', 'get_status', undefined], DROPPED]);
	});

	it('treats a rejecting get_status as unfocused and hides', async () => {
		const { h, tail } = await openThenEnd({ gameFocused: true, reject: { get_status: 'nope' } });
		expect(tail).toContainEqual(['hide']);
		expect(h.warnings).toContain(
			'[widget-config] temple: could not read game focus, assuming unfocused: nope'
		);
	});

	it('leaves the sessions alone for a module with no session', async () => {
		const h = harness();
		await opened(h).end(MODULE);
		expect(h.calls).toEqual([['invoke', 'get_status', undefined], ['setSessions', {}]]);
		expect(h.sessions()).toEqual({});
	});
});

describe('an unknown module', () => {
	it('logs that no window is registered and reports the end', async () => {
		const h = harness();
		await opened(h).start('nope');
		expect(h.warnings).toContain('[widget-config] nope: no overlay window is registered for this module');
		expect(h.calls).toEqual([
			['emitToMain', 'widget-config-opening', { module: 'nope' }],
			['invoke', 'get_status', undefined],
			['setSessions', {}],
			['emitToMain', 'widget-config-end', { module: 'nope' }]
		]);
	});
});

describe('non-fatal failures', () => {
	it('a rejecting opening report is logged and the open proceeds', async () => {
		const h = harness({ reject: { 'widget-config-opening': 'x' } });
		await opened(h).start(MODULE);
		expect(h.calls).toEqual(OPEN_SEQUENCE(true));
		expect(h.warnings).toContain('[widget-config] temple: could not report the start to Settings: x');
	});

	it('a rejecting open acknowledgement is logged and the deadline still arms', async () => {
		const h = harness({ reject: { 'widget-config-open': 'x' } });
		await opened(h).start(MODULE);
		expect(h.warnings).toContain('[widget-config] temple: could not confirm config mode to Settings: x');
		const before = h.calls.length;
		await vi.advanceTimersByTimeAsync(WIDGET_CONFIG_SESSION_MAX_MS);
		expect(h.calls.slice(before)).toEqual(ABANDON_WITH_HIDE);
	});

	describe.each([
		['the first leave request', { 'emitTo:off#1': 'e' }, 'could not ask the window to leave config mode: e'],
		['the direct config-mode clear', { 'set_overlay_config_mode:off': 'm' }, 'direct config-mode clear did not apply: m'],
		['the repeated leave request', { 'emitTo:off#2': 'e' }, 'could not repeat the leave request: e'],
		['the end report', { 'widget-config-end': 'z' }, 'could not report the failure to Settings: z']
	])('a rejecting abandon step: %s', (_name, reject, line) => {
		async function abandoned() {
			const h = harness({ built: false, reject });
			void opened(h).start(MODULE);
			await vi.advanceTimersByTimeAsync(10_050);
			return h;
		}

		it('is logged', async () => {
			const h = await abandoned();
			expect(h.warnings).toContain(`[widget-config] temple: ${line}`);
		});

		it('does not stop the abandon sequence', async () => {
			const h = await abandoned();
			const at = h.calls.findIndex((c) => c[0] === 'emitTo');
			expect(h.calls.slice(at)).toEqual([
				...ABANDON_HEAD,
				['getWindowByLabel', LABEL],
				DROPPED,
				['emitToMain', 'widget-config-end', { module: MODULE }]
			]);
		});
	});

	it('a rejecting hide is logged and the session is still dropped', async () => {
		const h = harness({ reject: { hide: 'h' } });
		const o = opened(h);
		await o.start(MODULE);
		await o.end(MODULE);
		expect(h.calls.at(-1)).toEqual(DROPPED);
		expect(h.warnings).toContain('[widget-config] temple: hide failed: h');
	});

	it('a rejecting isVisible is silent and counts as hidden', async () => {
		const h = harness({ visible: true, reject: { isVisible: 'v' } });
		const o = opened(h);
		await o.start(MODULE);
		expect(h.calls).toEqual(OPEN_SEQUENCE(true));
		expect(h.warnings).toEqual([]);
		expect(h.errors).toEqual([]);
		await o.end(MODULE);
		expect(h.calls).toContainEqual(['hide']);
	});

	it('mirrors every logged line to the app log', async () => {
		const h = harness();
		await opened(h).start('nope');
		expect(h.appLogs).toEqual(h.warnings);
	});

	it('sends a rejecting app log to error, not up', async () => {
		const h = harness({ reject: { app_log: 'down' } });
		await opened(h).start('nope');
		await vi.advanceTimersByTimeAsync(0);
		expect(h.errors).toContainEqual(['[widget-config] app log unreachable:', 'down']);
		expect(h.calls.at(-1)).toEqual(['emitToMain', 'widget-config-end', { module: 'nope' }]);
	});
});

describe('a rejecting window-API load', () => {
	it('rejects start before any session is recorded', async () => {
		const h = harness({ importRejects: true });
		await expect(opened(h).start(MODULE)).rejects.toBe('import boom');
		expect(h.sessions()).toEqual({});
		expect(h.warnings).toEqual([]);
	});

	it('rejects end before the session is dropped', async () => {
		const h = harness();
		const o = opened(h);
		await o.start(MODULE);
		h.world.importRejects = true;
		await expect(o.end(MODULE)).rejects.toBe('import boom');
		expect(h.sessions()).toEqual({ [MODULE]: { shown: true } });
	});

	it('releases the in-flight guard so the next start proceeds', async () => {
		const h = harness({ importRejects: true });
		const o = opened(h);
		await o.start(MODULE).catch(() => {});
		h.world.importRejects = false;
		await o.start(MODULE);
		expect(h.calls.at(-1)).toEqual(['emitToMain', 'widget-config-open', { module: MODULE }]);
	});
});
