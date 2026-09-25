import { describe, expect, it, vi } from 'vitest';
import {
	createOcrPreviewOwner,
	type OcrPreviewAdapters
} from '$lib/overlay/ocr-preview';
import { requestOcrPreview, type OcrRectView } from './ocr-preview-request';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/dpi', () => ({
	PhysicalPosition: class {
		constructor(public x: number, public y: number) {}
	},
	PhysicalSize: class {
		constructor(public width: number, public height: number) {}
	}
}));
vi.mock('@tauri-apps/api/webviewWindow', () => ({
	WebviewWindow: class {
		static getByLabel = vi.fn();
	}
}));
vi.mock('@tauri-apps/api/window', () => ({
	availableMonitors: vi.fn(),
	currentMonitor: vi.fn(),
	primaryMonitor: vi.fn()
}));

type WindowLike = ReturnType<OcrPreviewAdapters['createWindow']>;
type WindowEvent = (event?: unknown) => void | Promise<void>;

type FakeWindow = {
	label: string;
	options: unknown;
	close: ReturnType<typeof vi.fn>;
	destroy: ReturnType<typeof vi.fn>;
	emit: (event: string, payload?: unknown) => Promise<void>;
};

type Timer = {
	id: number;
	callback: () => void;
	canceled: boolean;
};

function deferred<T = void>() {
	let resolve!: (value: T | PromiseLike<T>) => void;
	const promise = new Promise<T>((accept) => { resolve = accept; });
	return { promise, resolve };
}

function makeWindow(
	label: string,
	options: unknown,
	onClose: () => void,
	onDestroy: () => Promise<void>
): FakeWindow & WindowLike {
	const handlers = new Map<string, WindowEvent>();
	return {
		label,
		options,
		once(event: string, handler: WindowEvent) {
			handlers.set(event, handler);
		},
		setPosition: vi.fn(async () => {}),
		setSize: vi.fn(async () => {}),
		outerSize: vi.fn(async () => ({ width: 300, height: 400 })),
		close: vi.fn(async () => { onClose(); }),
		destroy: vi.fn(onDestroy),
		emit: async (event: string, payload?: unknown) => {
			await handlers.get(event)?.(payload);
		}
	} as unknown as FakeWindow & WindowLike;
}

function makeHarness(deferDestroy: boolean) {
	const events: string[] = [];
	const windows: (FakeWindow & WindowLike)[] = [];
	const timers: Timer[] = [];
	const destroyGate = deferred();
	let activeWindow: (FakeWindow & WindowLike) | null = null;

	const adapters: Partial<OcrPreviewAdapters> = {
		primaryMonitor: async () => ({ position: { x: 100, y: 200 }, scaleFactor: 1 }),
		currentMonitor: async () => null,
		availableMonitors: async () => [
			{ position: { x: 100, y: 200 }, scaleFactor: 1 }
		],
		getGameMonitor: async () => ({ id: 1, x: 100, y: 200, width: 1920, height: 1200 }),
		createWindow: (label, options) => {
			const window = makeWindow(
				label,
				options,
				() => { events.push(`close:${windows.indexOf(window) + 1}`); },
				async () => {
					const number = windows.indexOf(window) + 1;
					events.push(`destroy-start:${number}`);
					if (deferDestroy) await destroyGate.promise;
					if (activeWindow === window) activeWindow = null;
					events.push(`destroy-end:${number}`);
				}
			);
			windows.push(window);
			activeWindow = window;
			events.push(`create:${windows.length}`);
			return window;
		},
		getWindowByLabel: async () => activeWindow,
		physicalPosition: (x, y) => ({ x, y }),
		physicalSize: (width, height) => ({ width, height }),
		setClickthrough: async () => {},
		appLog: async () => {},
		wait: async () => {},
		setTimer: (callback) => {
			const timer = { id: 100 + timers.length, callback, canceled: false };
			timers.push(timer);
			return timer.id as ReturnType<typeof setTimeout>;
		},
		clearTimer: vi.fn((timer: ReturnType<typeof setTimeout>) => {
			const entry = timers.find((candidate) => candidate.id === timer);
			if (entry) entry.canceled = true;
		}),
		warn: vi.fn(),
		info: vi.fn(),
		error: vi.fn()
	};

	return {
		events,
		windows,
		timers,
		destroyGate,
		owner: createOcrPreviewOwner(adapters)
	};
}

function row(key: string, rect: [number, number, number, number] | null): OcrRectView {
	return { key, label: `${key} label`, rect, rows: [[1, 2, 3, 4]], source: 'seed' };
}

function advance(timer: Timer): void {
	if (!timer.canceled) timer.callback();
}

describe('OCR preview request seam', () => {
	it('canceled expiry cannot destroy a replacement preview', async () => {
		const harness = makeHarness(false);
		const firstRow = row('lab.gem', [10, 20, 300, 400]);
		const secondRow = row('lab.font', [30, 40, 500, 600]);
		const fetchRects = vi.fn()
			.mockResolvedValueOnce([firstRow])
			.mockResolvedValueOnce([secondRow]);
		const dependencies = {
			owner: harness.owner,
			fetchRects,
			setRects: vi.fn(),
			warn: vi.fn(),
			error: vi.fn()
		};

		await requestOcrPreview('lab.gem', dependencies);
		await harness.windows[0].emit('tauri://created');
		const firstTimer = harness.timers[0];

		await requestOcrPreview('lab.font', dependencies);
		await harness.windows[1].emit('tauri://created');
		advance(firstTimer);
		await new Promise((resolve) => setTimeout(resolve, 0));

		expect(harness.owner.hasPreview).toBe(true);
		expect(harness.windows[1].close).not.toHaveBeenCalled();
		expect(harness.windows[1].destroy).not.toHaveBeenCalled();
	});

	it('tears down the live preview before the next fetch and window creation', async () => {
		const harness = makeHarness(true);
		const firstRow = row('lab.gem', [10, 20, 300, 400]);
		const secondRow = row('lab.font', [30, 40, 500, 600]);
		const fetchRects = vi.fn()
			.mockImplementationOnce(async () => {
				harness.events.push('fetch:first');
				return [firstRow];
			})
			.mockImplementationOnce(async () => {
				harness.events.push('fetch:second');
				return [firstRow, secondRow];
			});
		const setRects = vi.fn();
		const dependencies = {
			owner: harness.owner,
			fetchRects,
			setRects,
			warn: vi.fn(),
			error: vi.fn()
		};

		await requestOcrPreview('lab.gem', dependencies);
		await harness.windows[0].emit('tauri://created');
		const first = harness.windows[0];
		const replacement = requestOcrPreview('lab.font', dependencies);
		await new Promise((resolve) => setTimeout(resolve, 0));

		expect(harness.events).toContain('destroy-start:1');
		expect(fetchRects).toHaveBeenCalledTimes(1);
		expect(harness.windows).toHaveLength(1);
		expect(harness.events).not.toContain('fetch:second');

		harness.destroyGate.resolve(undefined);
		await replacement;

		expect(first.close).toHaveBeenCalledTimes(1);
		expect(first.destroy).toHaveBeenCalledTimes(1);
		expect(harness.events.indexOf('destroy-end:1')).toBeLessThan(harness.events.indexOf('fetch:second'));
		expect(harness.events.indexOf('fetch:second')).toBeLessThan(harness.events.indexOf('create:2'));
		expect(setRects).toHaveBeenNthCalledWith(2, [firstRow, secondRow]);
		expect(harness.windows[1].options).toMatchObject({
			url: '/overlay?preview=lab.font&label=lab.font%20label&rect=30,40,500,600&rows=%5B%5B1%2C2%2C3%2C4%5D%5D&dpr=1'
		});
	});

	it('reports an unlocated requested row without creating a preview', async () => {
		const harness = makeHarness(false);
		const warn = vi.fn();

		await requestOcrPreview('lab.gem', {
			owner: harness.owner,
			fetchRects: async () => [row('lab.gem', null)],
			setRects: vi.fn(),
			warn,
			error: vi.fn()
		});

		expect(harness.windows).toHaveLength(0);
		expect(warn).toHaveBeenCalledWith("[settings] OCR preview 'lab.gem' is unlocated");
	});

	it('reports a rectangle fetch failure through the page error boundary', async () => {
		const harness = makeHarness(false);
		const failure = new Error('OCR data unavailable');
		const error = vi.fn();

		await requestOcrPreview('lab.gem', {
			owner: harness.owner,
			fetchRects: async () => { throw failure; },
			setRects: vi.fn(),
			warn: vi.fn(),
			error
		});

		expect(harness.windows).toHaveLength(0);
		expect(error).toHaveBeenCalledWith('[settings] OCR preview failed:', failure);
	});
});
