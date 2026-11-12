import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createLabSession, type CompassSettings, type LabNavEvent, type LabSessionHooks } from './lab-session.svelte';
import type { LabLayout, NavState } from './navigation';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

type Deferred<T> = { promise: Promise<T>; resolve: (v: T) => void; reject: (e: unknown) => void };
function deferred<T>(): Deferred<T> {
	let resolve!: (v: T) => void;
	let reject!: (e: unknown) => void;
	const promise = new Promise<T>((res, rej) => {
		resolve = res;
		reject = rej;
	});
	return { promise, resolve, reject };
}

function makeLayout(difficulty = 'Uber'): LabLayout {
	return {
		difficulty,
		date: '2026-11-12',
		weapon: 'Sword',
		phase1: '',
		phase2: '',
		trap1: '',
		trap2: '',
		rooms: [
			{ id: '1', name: 'Estate Path', x: '0', y: '0', areacode: '', contents: [], exits: { E: '2' } },
			{ id: '2', name: 'Estate Walkways', x: '1', y: '0', areacode: '', contents: [], exits: { W: '1' } }
		]
	};
}

const PLAZA: LabNavEvent = { type: 'PlazaEntered' };
const ROOM1: LabNavEvent = { type: 'RoomChanged', name: 'Estate Path' };
const ROOM2: LabNavEvent = { type: 'RoomChanged', name: 'Estate Walkways' };
const EXIT: LabNavEvent = { type: 'LabExited' };

async function flush(): Promise<void> {
	for (let i = 0; i < 30; i++) await Promise.resolve();
}

/**
 * Port fakes. `invoke` answers `get_compass_settings` with `settingsReply`
 * and `get_lab_catchup` with `catchupEvents`, unless a deferred was queued for
 * that command with `hold`. `fetchLayout` answers with `layoutReply` unless
 * held with `holdLayout`. `listen` registrations can be held with
 * `holdListen`.
 */
function makeHarness() {
	const order: string[] = [];
	const logs: string[] = [];
	const handlers = new Map<string, Set<(e: { payload: unknown }) => void>>();
	const unlistened: string[] = [];
	const held = new Map<string, Deferred<unknown>[]>();
	const heldLayouts: Deferred<LabLayout | null>[] = [];
	const heldListens = new Map<string, Deferred<() => void>>();
	const state = {
		settingsReply: {} as CompassSettings | Error,
		catchupEvents: [] as unknown[] | Error,
		layoutReply: makeLayout() as LabLayout | null
	};

	const invoke = vi.fn(async (command: string) => {
		order.push(command);
		const queue = held.get(command);
		if (queue?.length) return queue.shift()!.promise;
		if (command === 'get_compass_settings') {
			if (state.settingsReply instanceof Error) throw state.settingsReply;
			return { ...state.settingsReply };
		}
		if (command === 'get_lab_catchup') {
			if (state.catchupEvents instanceof Error) throw state.catchupEvents;
			return { events: state.catchupEvents };
		}
		return undefined;
	}) as any;

	const fetchLayout = vi.fn(async (_opts: { preferredDifficulty?: string; lockedDifficulty?: string | null }) => {
		order.push('fetchLayout');
		const next = heldLayouts.shift();
		if (next) return next.promise;
		return state.layoutReply;
	});

	const listen = vi.fn((event: string, handler: (e: { payload: unknown }) => void) => {
		order.push(`listen:${event}`);
		const set = handlers.get(event) ?? new Set();
		set.add(handler);
		handlers.set(event, set);
		const unlisten = () => {
			unlistened.push(event);
			set.delete(handler);
		};
		const hold = heldListens.get(event);
		if (hold) {
			heldListens.delete(event);
			return hold.promise.then(() => unlisten);
		}
		return Promise.resolve(unlisten);
	}) as any;

	const session = createLabSession({
		invoke,
		listen,
		fetchLayout: fetchLayout as any,
		setInterval: (cb, ms) => setInterval(cb, ms),
		clearInterval: (id) => clearInterval(id),
		log: (msg) => logs.push(msg)
	});

	return {
		session,
		state,
		order,
		logs,
		unlistened,
		invoke,
		fetchLayout,
		listen,
		hold<T>(command: string): Deferred<T> {
			const d = deferred<T>();
			const queue = held.get(command) ?? [];
			queue.push(d as Deferred<unknown>);
			held.set(command, queue);
			return d;
		},
		holdLayout(): Deferred<LabLayout | null> {
			const d = deferred<LabLayout | null>();
			heldLayouts.push(d);
			return d;
		},
		holdListen(event: string): Deferred<() => void> {
			const d = deferred<() => void>();
			heldListens.set(event, d);
			return d;
		},
		send(event: string, payload: unknown) {
			for (const handler of [...(handlers.get(event) ?? [])]) handler({ payload });
		},
		count(command: string) {
			return order.filter((c) => c === command).length;
		}
	};
}

/** A subscriber recording every hook delivery. */
function recorder() {
	const events: { event: LabNavEvent; navAfter: NavState }[] = [];
	const settings: CompassSettings[] = [];
	let resets = 0;
	const hooks: LabSessionHooks = {
		onNavEvent: (event, navAfter) => events.push({ event, navAfter }),
		onReset: () => {
			resets++;
		},
		onSettings: (s) => settings.push(s)
	};
	return {
		hooks,
		events,
		settings,
		get resets() {
			return resets;
		},
		types: () => events.map((e) => e.event.type)
	};
}

beforeEach(() => {
	vi.useFakeTimers();
});

afterEach(() => {
	vi.useRealTimers();
});

describe('lab session lifecycle', () => {
	it('calls no port when created', () => {
		const h = makeHarness();
		expect(h.order).toEqual([]);
	});

	it('registers both listeners and reads settings on the first acquire', () => {
		const h = makeHarness();
		h.session.acquire({});
		expect(h.order.slice(0, 3)).toEqual(['listen:lab-nav', 'listen:lab-layout-updated', 'get_compass_settings']);
	});

	it('adds no listener, interval or catch-up on a second acquire', async () => {
		const h = makeHarness();
		h.session.acquire({});
		await flush();
		h.session.acquire({});
		await flush();
		await vi.advanceTimersByTimeAsync(2000);
		expect(h.listen).toHaveBeenCalledTimes(2);
		expect(h.count('get_lab_catchup')).toBe(1);
		// immediate poll + init + one interval tick
		expect(h.count('get_compass_settings')).toBe(3);
	});

	it('keeps polling while one of two subscribers remains', async () => {
		const h = makeHarness();
		const releaseA = h.session.acquire({});
		h.session.acquire({});
		await flush();
		releaseA();
		const before = h.count('get_compass_settings');
		await vi.advanceTimersByTimeAsync(2000);
		expect(h.count('get_compass_settings')).toBe(before + 1);
	});

	it('stops polling and unlistens when the last subscriber releases', async () => {
		const h = makeHarness();
		const release = h.session.acquire({});
		await flush();
		release();
		await flush();
		const before = h.count('get_compass_settings');
		await vi.advanceTimersByTimeAsync(4000);
		expect(h.count('get_compass_settings')).toBe(before);
		expect(h.unlistened.sort()).toEqual(['lab-layout-updated', 'lab-nav']);
	});

	it('ignores a second call of the same release while another subscriber holds the session', async () => {
		const h = makeHarness();
		const releaseA = h.session.acquire({});
		h.session.acquire({});
		await flush();
		releaseA();
		releaseA();
		const before = h.count('get_compass_settings');
		await vi.advanceTimersByTimeAsync(2000);
		expect(h.count('get_compass_settings')).toBe(before + 1);
	});

	it('starts from scratch after every subscriber released', async () => {
		const h = makeHarness();
		h.state.catchupEvents = [PLAZA, ROOM1];
		const release = h.session.acquire({});
		await flush();
		expect(h.session.navState.inLab).toBe(true);
		release();
		h.state.catchupEvents = [];
		const b = recorder();
		h.session.acquire(b.hooks);
		expect(b.events).toEqual([]);
		expect(h.session.navState.inLab).toBe(false);
		await flush();
		expect(h.count('get_lab_catchup')).toBe(2);
	});

	it('ignores and unlistens a registration that resolves after the session stopped', async () => {
		const h = makeHarness();
		const hold = h.holdListen('lab-nav');
		const release = h.session.acquire({});
		await flush();
		release();
		const b = recorder();
		h.session.acquire(b.hooks);
		await flush();
		const oldLabNav = h.listen.mock.calls[0][1];
		hold.resolve(() => {});
		await flush();
		oldLabNav({ payload: PLAZA });
		expect(b.events).toEqual([]);
		expect(h.unlistened.filter((e) => e === 'lab-nav').length).toBe(1);
	});
});

describe('lab session ordering and catch-up', () => {
	it('registers both listeners before catch-up, and catch-up follows the layout fetch', async () => {
		const h = makeHarness();
		h.session.acquire({});
		await flush();
		const catchup = h.order.indexOf('get_lab_catchup');
		expect(h.order.indexOf('listen:lab-nav')).toBeLessThan(catchup);
		expect(h.order.indexOf('listen:lab-layout-updated')).toBeLessThan(catchup);
		expect(h.order.indexOf('fetchLayout')).toBeLessThan(catchup);
	});

	it('waits for the layout before reading the catch-up', async () => {
		const h = makeHarness();
		h.holdLayout();
		h.session.acquire({});
		await flush();
		expect(h.order).toContain('fetchLayout');
		expect(h.order).not.toContain('get_lab_catchup');
	});

	it('delivers catch-up events in order once the layout resolves', async () => {
		const h = makeHarness();
		h.state.catchupEvents = [PLAZA, ROOM1, ROOM2];
		const a = recorder();
		h.session.acquire(a.hooks);
		await flush();
		expect(a.events.map((e) => e.event)).toEqual([PLAZA, ROOM1, ROOM2]);
		expect(h.session.navState.currentRoom).toBe('2');
		expect(h.logs).toContain('[lab-session] replaying 3 catchup events');
	});

	it('delivers a live event during the layout wait and again from the catch-up', async () => {
		const h = makeHarness();
		h.state.catchupEvents = [ROOM1];
		const layout = h.holdLayout();
		const a = recorder();
		h.session.acquire(a.hooks);
		await flush();
		h.send('lab-nav', ROOM1);
		expect(a.events.map((e) => e.event)).toEqual([ROOM1]);
		layout.resolve(makeLayout());
		await flush();
		expect(a.events.map((e) => e.event)).toEqual([ROOM1, ROOM1]);
	});
});

describe('lab session late subscribers', () => {
	it('replays history to a late subscriber with the nav state after each event', async () => {
		const h = makeHarness();
		const a = recorder();
		h.session.acquire(a.hooks);
		await flush();
		h.send('lab-nav', PLAZA);
		h.send('lab-nav', ROOM1);
		h.send('lab-nav', ROOM2);
		const b = recorder();
		h.session.acquire(b.hooks);
		expect(b.events.map((e) => e.event)).toEqual([PLAZA, ROOM1, ROOM2]);
		expect(b.events[1].navAfter.currentRoom).toBe('1');
		expect(b.events[2].navAfter.currentRoom).toBe('2');
	});

	it('hands a late subscriber the last polled settings once', async () => {
		const h = makeHarness();
		h.state.settingsReply = { mode: 'arrow' };
		h.session.acquire({});
		await flush();
		h.state.settingsReply = { mode: 'minimap' };
		await vi.advanceTimersByTimeAsync(2000);
		const b = recorder();
		h.session.acquire(b.hooks);
		expect(b.settings).toEqual([{ mode: 'minimap' }]);
	});

	it('hands a late subscriber no settings before any read succeeded', async () => {
		const h = makeHarness();
		h.state.settingsReply = new Error('down');
		h.session.acquire({});
		await flush();
		const b = recorder();
		h.session.acquire(b.hooks);
		expect(b.settings).toEqual([]);
	});
});

describe('lab session non-hook events', () => {
	it('keeps LayoutChanged and StrategyChanged out of hooks and replay', async () => {
		const h = makeHarness();
		const a = recorder();
		h.session.acquire(a.hooks);
		await flush();
		h.send('lab-nav', { type: 'LayoutChanged', difficulty: 'Cruel' });
		h.send('lab-nav', { type: 'StrategyChanged', strategy: 'shortest' });
		h.send('lab-nav', PLAZA);
		await flush();
		const b = recorder();
		h.session.acquire(b.hooks);
		expect(a.types()).toEqual(['PlazaEntered']);
		expect(b.types()).toEqual(['PlazaEntered']);
	});

	it('loads the layout a live LayoutChanged names', async () => {
		const h = makeHarness();
		h.session.acquire({});
		await flush();
		const merciless = makeLayout('Merciless');
		h.state.layoutReply = merciless;
		h.send('lab-nav', { type: 'LayoutChanged', difficulty: 'Merciless' });
		await flush();
		expect(h.fetchLayout.mock.calls.at(-1)![0].preferredDifficulty).toBe('Merciless');
		expect(h.session.navState.layout).toBe(merciless);
		expect(h.session.layoutLoaded).toBe(true);
	});

	it('sets the strategy a StrategyChanged names', async () => {
		const h = makeHarness();
		h.session.acquire({});
		await flush();
		h.send('lab-nav', { type: 'StrategyChanged', strategy: 'everything' });
		expect(h.session.navState.strategy).toBe('everything');
	});
});

describe('lab session layout reset', () => {
	async function outsideLabReset() {
		const h = makeHarness();
		const a = recorder();
		h.session.acquire(a.hooks);
		await flush();
		h.state.catchupEvents = [PLAZA, ROOM1];
		const resetAt = h.order.length;
		h.send('lab-layout-updated', { action: 'reset' });
		return { h, a, resetAt };
	}

	it('resets every subscriber when not in the lab', async () => {
		const { h, a } = await outsideLabReset();
		expect(a.resets).toBe(1);
		expect(h.session.layoutLoaded).toBe(false);
	});

	it('runs init again after a reset', async () => {
		const { h, a, resetAt } = await outsideLabReset();
		await flush();
		expect(h.order.slice(resetAt)).toEqual(['get_compass_settings', 'fetchLayout', 'get_lab_catchup']);
		expect(a.types()).toEqual(['PlazaEntered', 'RoomChanged']);
		expect(h.session.layoutLoaded).toBe(true);
	});

	it('replays only post-reset events to a later subscriber', async () => {
		const h = makeHarness();
		h.state.catchupEvents = [PLAZA, EXIT];
		h.session.acquire({});
		await flush();
		h.state.catchupEvents = [];
		h.send('lab-layout-updated', { action: 'reset' });
		await flush();
		h.send('lab-nav', PLAZA);
		const b = recorder();
		h.session.acquire(b.hooks);
		expect(b.types()).toEqual(['PlazaEntered']);
	});

	it('defers a reset while in the lab', async () => {
		const h = makeHarness();
		const a = recorder();
		h.session.acquire(a.hooks);
		await flush();
		h.send('lab-nav', PLAZA);
		h.send('lab-layout-updated', { action: 'reset' });
		expect(a.resets).toBe(0);
		expect(h.session.pendingLayoutReset).toBe(true);
		expect(h.logs).toContain('[lab-session] layout reset deferred (in lab)');
	});

	it('applies a deferred reset after delivering the next LabExited', async () => {
		const h = makeHarness();
		const seen: string[] = [];
		h.session.acquire({
			onNavEvent: (e, nav) => seen.push(`${e.type}:${nav.inLab}`),
			onReset: () => seen.push('reset')
		});
		await flush();
		h.send('lab-nav', PLAZA);
		h.send('lab-layout-updated', { action: 'reset' });
		const resetAt = h.order.length;
		h.send('lab-nav', EXIT);
		await flush();
		expect(seen).toEqual(['PlazaEntered:true', 'LabExited:false', 'reset']);
		expect(h.order.slice(resetAt)).toEqual(['get_compass_settings', 'fetchLayout', 'get_lab_catchup']);
	});

	it('keeps the locked difficulty across a reset', async () => {
		const h = makeHarness();
		h.state.layoutReply = makeLayout('Merciless');
		h.session.acquire({});
		await flush();
		h.send('lab-layout-updated', { action: 'reset' });
		await flush();
		expect(h.fetchLayout).toHaveBeenCalledTimes(2);
		expect(h.fetchLayout.mock.calls[1][0].lockedDifficulty).toBe('Merciless');
	});
});

describe('lab session settings poll', () => {
	it('logs two failing ticks once', async () => {
		const h = makeHarness();
		h.state.settingsReply = new Error('down');
		h.session.acquire({});
		await flush();
		await vi.advanceTimersByTimeAsync(2000);
		expect(h.logs.filter((l) => l.startsWith('[lab-session] settings poll failed'))).toEqual([
			'[lab-session] settings poll failed: Error: down'
		]);
	});

	it('logs a failure again after a success in between', async () => {
		const h = makeHarness();
		h.state.settingsReply = new Error('down');
		h.session.acquire({});
		await flush();
		h.state.settingsReply = {};
		await vi.advanceTimersByTimeAsync(2000);
		h.state.settingsReply = new Error('down');
		await vi.advanceTimersByTimeAsync(2000);
		expect(h.logs.filter((l) => l.startsWith('[lab-session] settings poll failed')).length).toBe(2);
	});

	it('applies a polled strategy change', async () => {
		const h = makeHarness();
		h.session.acquire({});
		await flush();
		h.state.settingsReply = { strategy: 'darkshrines' };
		await vi.advanceTimersByTimeAsync(2000);
		expect(h.session.navState.strategy).toBe('darkshrines');
	});

	it('keeps an already locked difficulty when a tick names another', async () => {
		const h = makeHarness();
		h.state.settingsReply = { difficulty: 'Cruel' };
		h.session.acquire({});
		await flush();
		h.state.settingsReply = { difficulty: 'Uber' };
		await vi.advanceTimersByTimeAsync(2000);
		expect(h.session.lockedDifficulty).toBe('Cruel');
	});

	it('locks the difficulty a tick names while none is locked', async () => {
		const h = makeHarness();
		h.state.layoutReply = null;
		h.session.acquire({});
		await flush();
		expect(h.session.lockedDifficulty).toBeNull();
		h.state.settingsReply = { difficulty: 'Normal' };
		await vi.advanceTimersByTimeAsync(2000);
		expect(h.session.lockedDifficulty).toBe('Normal');
	});
});

describe('lab session init failures', () => {
	function initSettingsFails() {
		const h = makeHarness();
		h.state.catchupEvents = [PLAZA];
		h.hold<CompassSettings>('get_compass_settings').resolve({});
		h.hold<CompassSettings>('get_compass_settings').reject(new Error('busy'));
		const a = recorder();
		h.session.acquire(a.hooks);
		return { h, a };
	}

	it('logs a failing init settings read once', async () => {
		const { h } = initSettingsFails();
		await flush();
		expect(h.logs.filter((l) => l.includes('init settings failed'))).toEqual([
			'[lab-session] init settings failed: Error: busy'
		]);
	});

	it('continues to the layout and catch-up after the init settings read fails', async () => {
		const { h, a } = initSettingsFails();
		await flush();
		expect(h.count('fetchLayout')).toBe(1);
		expect(h.count('get_lab_catchup')).toBe(1);
		expect(a.types()).toEqual(['PlazaEntered']);
	});

	it('logs a failing catch-up and delivers no events', async () => {
		const h = makeHarness();
		h.state.catchupEvents = new Error('no client.txt');
		const a = recorder();
		h.session.acquire(a.hooks);
		await flush();
		expect(h.logs).toContain('[lab-session] catchup failed: Error: no client.txt');
		expect(a.events).toEqual([]);
	});

	it('keeps polling after a catch-up failure', async () => {
		const h = makeHarness();
		h.state.catchupEvents = new Error('no client.txt');
		h.session.acquire({});
		await flush();
		const before = h.count('get_compass_settings');
		await vi.advanceTimersByTimeAsync(2000);
		expect(h.count('get_compass_settings')).toBe(before + 1);
	});
});

describe('lab session stale layout and init results', () => {
	it('discards the layout of an init superseded by a reset', async () => {
		const h = makeHarness();
		const oldLayout = h.holdLayout();
		h.session.acquire({});
		await flush();
		h.send('lab-layout-updated', { action: 'reset' });
		await flush();
		const fresh = h.session.navState.layout;
		expect(h.count('get_lab_catchup')).toBe(1);
		oldLayout.resolve(makeLayout('Normal'));
		await flush();
		expect(h.count('get_lab_catchup')).toBe(1);
		expect(h.session.navState.layout).toBe(fresh);
		expect(h.session.navState.layout?.difficulty).toBe('Uber');
	});

	it('discards the layout of an init superseded by release-all', async () => {
		const h = makeHarness();
		const oldLayout = h.holdLayout();
		const release = h.session.acquire({});
		await flush();
		release();
		oldLayout.resolve(makeLayout());
		await flush();
		expect(h.session.layoutLoaded).toBe(false);
		expect(h.session.navState.layout).toBeNull();
	});
});

describe('lab session stale poll results', () => {
	const OLD: CompassSettings = { strategy: 'everything', difficulty: 'Cruel', mode: 'old' };

	it('discards a poll that resolves after release-all', async () => {
		const h = makeHarness();
		const poll = h.hold<CompassSettings>('get_compass_settings');
		const release = h.session.acquire({});
		release();
		const logsBefore = h.logs.length;
		poll.resolve(OLD);
		await flush();
		expect(h.session.settings).toBeNull();
		expect(h.session.navState.strategy).toBe('darkshrines-on-route');
		expect(h.session.lockedDifficulty).toBeNull();
		expect(h.logs.length).toBe(logsBefore);
	});

	it('keeps an old lifetime poll away from a new subscriber', async () => {
		const h = makeHarness();
		const poll = h.hold<CompassSettings>('get_compass_settings');
		const release = h.session.acquire({});
		release();
		h.state.settingsReply = { mode: 'new' };
		const b = recorder();
		h.session.acquire(b.hooks);
		await flush();
		poll.resolve(OLD);
		await flush();
		expect(b.settings).toEqual([{ mode: 'new' }, { mode: 'new' }]);
		expect(h.session.settings).toEqual({ mode: 'new' });
	});

	it('discards a poll that resolves after a reset', async () => {
		const h = makeHarness();
		const a = recorder();
		h.session.acquire(a.hooks);
		await flush();
		const poll = h.hold<CompassSettings>('get_compass_settings');
		await vi.advanceTimersByTimeAsync(2000);
		h.send('lab-layout-updated', { action: 'reset' });
		await flush();
		const delivered = a.settings.length;
		poll.resolve(OLD);
		await flush();
		expect(h.session.navState.strategy).toBe('darkshrines-on-route');
		expect(a.settings.length).toBe(delivered);
	});

	it('logs no failure for an old lifetime poll but logs the current one', async () => {
		const h = makeHarness();
		const poll = h.hold<CompassSettings>('get_compass_settings');
		const release = h.session.acquire({});
		release();
		h.session.acquire({});
		await flush();
		poll.reject(new Error('old'));
		await flush();
		expect(h.logs.some((l) => l.includes('settings poll failed'))).toBe(false);
		h.state.settingsReply = new Error('new');
		await vi.advanceTimersByTimeAsync(2000);
		expect(h.logs.filter((l) => l.includes('settings poll failed'))).toEqual([
			'[lab-session] settings poll failed: Error: new'
		]);
	});
});

describe('lab session stale catch-up batch', () => {
	it('drops the rest of a catch-up batch once a replayed LabExited applies a deferred reset', async () => {
		const h = makeHarness();
		const oldCatchup = h.hold<{ events: unknown[] }>('get_lab_catchup');
		const a = recorder();
		h.session.acquire(a.hooks);
		await flush();
		h.send('lab-nav', PLAZA);
		h.send('lab-layout-updated', { action: 'reset' });
		const replacementSettings = h.hold<CompassSettings>('get_compass_settings');
		oldCatchup.resolve({ events: [EXIT, PLAZA, ROOM1] });
		await flush();
		expect(h.session.navState.inLab).toBe(false);
		expect(a.types()).toEqual(['PlazaEntered', 'LabExited']);
		const late = recorder();
		h.session.acquire(late.hooks);
		expect(late.events).toEqual([]);
		h.state.catchupEvents = [ROOM2];
		replacementSettings.resolve({});
		await flush();
		const after = recorder();
		h.session.acquire(after.hooks);
		expect(after.events.map((e) => e.event)).toEqual([ROOM2]);
	});
});
