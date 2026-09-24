import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
	createWidgetConfigRequestController,
	type WidgetConfigPayload,
	type WidgetConfigRequestDependencies
} from './widget-config-request.svelte';

vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));
vi.mock('@tauri-apps/api/webviewWindow', () => ({
	getCurrentWebviewWindow: vi.fn(() => ({ emit: vi.fn(async () => {}) }))
}));

type WidgetConfigEvent = { payload?: WidgetConfigPayload };
type WidgetConfigHandler = (event: WidgetConfigEvent) => void;

function makeHarness(
	listenOverride?: WidgetConfigRequestDependencies['listen']
) {
	const handlers = new Map<string, Set<WidgetConfigHandler>>();
	const emitted: { event: string; payload: { module: string } }[] = [];
	const listen = listenOverride ?? vi.fn(async (event: string, handler: WidgetConfigHandler) => {
		const eventHandlers = handlers.get(event) ?? new Set<WidgetConfigHandler>();
		eventHandlers.add(handler);
		handlers.set(event, eventHandlers);
		return () => eventHandlers.delete(handler);
	});
	const emit = vi.fn(async (event: string, payload: { module: string }) => {
		emitted.push({ event, payload });
	});
	const reloadGeometry = vi.fn();
	const warn = vi.fn();
	const controller = createWidgetConfigRequestController({
		listen,
		emit,
		reloadGeometry,
		setTimer: (callback, delay) => setTimeout(callback, delay),
		clearTimer: (timer) => clearTimeout(timer),
		warn
	});

	return {
		controller,
		listen,
		emit,
		emitted,
		reloadGeometry,
		warn,
		send(event: string, module?: string) {
			for (const handler of handlers.get(event) ?? []) {
				handler({ payload: module === undefined ? {} : { module } });
			}
		}
	};
}

beforeEach(() => {
	vi.useFakeTimers();
});

afterEach(() => {
	vi.useRealTimers();
});

describe('widget config request controller', () => {
	it('does not register listeners before connect', () => {
		const harness = makeHarness();

		expect(harness.listen).not.toHaveBeenCalled();
		harness.controller.connect();

		expect(harness.listen).toHaveBeenCalledTimes(3);
	});

	it('ends an unanswered request after the opening deadline', async () => {
		const harness = makeHarness();
		harness.controller.connect();

		await harness.controller.configureWidgets('temple');
		await vi.advanceTimersByTimeAsync(20_000);

		expect(harness.controller.configuring).toBeNull();
		expect(harness.emitted).toEqual([
			{ event: 'widget-config-start', payload: { module: 'temple' } },
			{ event: 'widget-config-end', payload: { module: 'temple' } }
		]);
		expect(harness.warn).toHaveBeenCalledWith(
			'[settings] no answer to widget config mode for temple — giving up'
		);
	});

	it('extends the opening deadline once after pickup', async () => {
		const harness = makeHarness();
		harness.controller.connect();

		await harness.controller.configureWidgets('temple');
		harness.send('widget-config-opening', 'temple');
		await vi.advanceTimersByTimeAsync(19_999);
		expect(harness.controller.configuring).toBe('temple');

		await vi.advanceTimersByTimeAsync(1);
		expect(harness.controller.configuring).toBe('temple');
		expect(harness.emitted).toHaveLength(1);
		expect(harness.warn).toHaveBeenCalledWith(
			'[settings] temple config mode is slow to open — waiting once more'
		);

		await vi.advanceTimersByTimeAsync(19_999);
		expect(harness.controller.configuring).toBe('temple');
		await vi.advanceTimersByTimeAsync(1);

		expect(harness.controller.configuring).toBeNull();
		expect(harness.emitted).toHaveLength(2);
	});

	it('keeps an opened session past the opening deadline', async () => {
		const harness = makeHarness();
		harness.controller.connect();

		await harness.controller.configureWidgets('temple');
		harness.send('widget-config-opening', 'temple');
		harness.send('widget-config-open', 'temple');
		await vi.advanceTimersByTimeAsync(40_000);

		expect(harness.controller.configuring).toBe('temple');
		expect(harness.emitted).toHaveLength(1);
	});

	it('ignores another module acknowledgement', async () => {
		const harness = makeHarness();
		harness.controller.connect();

		await harness.controller.configureWidgets('temple');
		harness.send('widget-config-opening', 'mercenary');
		harness.send('widget-config-open', 'mercenary');
		harness.send('widget-config-end', 'mercenary');
		await vi.advanceTimersByTimeAsync(19_999);
		expect(harness.controller.configuring).toBe('temple');
		await vi.advanceTimersByTimeAsync(1);

		expect(harness.controller.configuring).toBeNull();
		expect(harness.reloadGeometry).toHaveBeenCalledWith('mercenary');
	});

	it('ends the matching module and reloads its geometry', async () => {
		const harness = makeHarness();
		harness.controller.connect();

		await harness.controller.configureWidgets('temple');
		harness.send('widget-config-end', 'temple');

		expect(harness.controller.configuring).toBeNull();
		expect(harness.reloadGeometry).toHaveBeenCalledWith('temple');
	});

	it('restarts the deadline for a repeated request', async () => {
		const harness = makeHarness();
		harness.controller.connect();

		await harness.controller.configureWidgets('temple');
		await vi.advanceTimersByTimeAsync(10_000);
		await harness.controller.configureWidgets('temple');
		await vi.advanceTimersByTimeAsync(9_999);
		expect(harness.controller.configuring).toBe('temple');
		await vi.advanceTimersByTimeAsync(1);
		expect(harness.controller.configuring).toBe('temple');
		await vi.advanceTimersByTimeAsync(9_999);
		expect(harness.controller.configuring).toBe('temple');
		await vi.advanceTimersByTimeAsync(1);

		expect(harness.controller.configuring).toBeNull();
		expect(harness.emitted.filter(({ event }) => event === 'widget-config-start')).toHaveLength(2);
		expect(harness.emitted.filter(({ event }) => event === 'widget-config-end')).toHaveLength(1);
	});

	it('logs a failed start request and clears its state', async () => {
		const harness = makeHarness();
		const failure = new Error('event bridge unavailable');
		harness.emit.mockImplementation(async (event, payload) => {
			if (event === 'widget-config-start') throw failure;
			harness.emitted.push({ event, payload });
		});
		harness.controller.connect();

		await harness.controller.configureWidgets('temple');

		expect(harness.controller.configuring).toBeNull();
		expect(harness.warn).toHaveBeenCalledWith(
			'[settings] could not ask for widget config mode:',
			failure
		);
	});

	it('disposes the timer and listeners registered asynchronously', async () => {
		const pending: ((unlisten: () => void) => void)[] = [];
		const listen = vi.fn(
			(_event: string, _handler: WidgetConfigHandler) =>
				new Promise<() => void>((resolve) => pending.push(resolve))
		);
		const harness = makeHarness(listen);
		harness.controller.connect();
		await harness.controller.configureWidgets('temple');
		harness.controller.dispose();

		expect(harness.controller.configuring).toBeNull();
		expect(vi.getTimerCount()).toBe(0);
		expect(harness.listen).toHaveBeenCalledTimes(3);
		expect(harness.emitted).toHaveLength(1);
		await vi.advanceTimersByTimeAsync(20_000);
		expect(harness.emitted).toHaveLength(1);

		const unlisteners = pending.map(() => vi.fn());
		pending.forEach((resolve, index) => resolve(unlisteners[index]));
		await Promise.resolve();
		await Promise.resolve();

		expect(harness.warn).not.toHaveBeenCalled();
		for (const unlisten of unlisteners) expect(unlisten).toHaveBeenCalledTimes(1);
	});
});
