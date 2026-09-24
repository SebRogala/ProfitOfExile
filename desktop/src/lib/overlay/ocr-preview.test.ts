import { describe, expect, it, vi } from 'vitest';
import {
	createOcrPreviewOwner,
	type OcrPreviewAdapters,
	type OcrPreviewRequest
} from './ocr-preview';

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
type PreviewMonitor = Awaited<ReturnType<OcrPreviewAdapters['primaryMonitor']>>;
type AvailableMonitors = Awaited<ReturnType<OcrPreviewAdapters['availableMonitors']>>;
type GameMonitor = Awaited<ReturnType<OcrPreviewAdapters['getGameMonitor']>>;
type WindowEvent = (event?: unknown) => void | Promise<void>;

type FakeWindow = {
	label: string;
	options: unknown;
	positionCalls: unknown[];
	sizeCalls: unknown[];
	close: ReturnType<typeof vi.fn>;
	destroy: ReturnType<typeof vi.fn>;
	emit: (event: string, payload?: unknown) => Promise<void>;
};

function makeWindow(
	label: string,
	options: unknown,
	onDestroy: () => void,
	onOuterSize: () => void
): FakeWindow & WindowLike {
	const handlers = new Map<string, WindowEvent>();
	const positionCalls: unknown[] = [];
	const sizeCalls: unknown[] = [];
	const fake = {
		label,
		options,
		positionCalls,
		sizeCalls,
		once(event: string, handler: WindowEvent) {
			handlers.set(event, handler);
		},
		setPosition: vi.fn(async (position: unknown) => {
			positionCalls.push(position);
		}),
		setSize: vi.fn(async (size: unknown) => {
			sizeCalls.push(size);
		}),
		outerSize: vi.fn(async () => {
			onOuterSize();
			return { width: 300, height: 400 };
		}),
		close: vi.fn(async () => {}),
		destroy: vi.fn(async () => {
			onDestroy();
		}),
		emit: async (event: string, payload?: unknown) => {
			await handlers.get(event)?.(payload);
		}
	};
	return fake as unknown as FakeWindow & WindowLike;
}

function makeHarness() {
	let resolveTeardown!: () => void;
	const teardownComplete = new Promise<void>((resolve) => {
		resolveTeardown = resolve;
	});
	const harness = {
		activeWindow: null as (FakeWindow & WindowLike) | null,
		windows: [] as (FakeWindow & WindowLike)[],
		timers: [] as { callback: () => void; ms: number }[],
		wait: vi.fn(async (_ms: number) => {}),
		logs: { warn: vi.fn(), info: vi.fn(), error: vi.fn(), app: vi.fn(async (_message: string) => {}) },
		onOuterSize: undefined as (() => void) | undefined,
		destroyRemovesActive: true,
		primaryMonitor: async (): Promise<PreviewMonitor> => ({ position: { x: 100, y: 200 }, scaleFactor: 1.5 }),
		currentMonitor: async (): Promise<PreviewMonitor> => null,
		availableMonitors: async (): Promise<AvailableMonitors> => [
			{ position: { x: 100, y: 200 }, scaleFactor: 1.5 }
		],
		gameMonitor: async (): Promise<GameMonitor> => ({ id: 1, x: 100, y: 200, width: 2560, height: 1440 }),
		clickthroughLabels: [] as string[],
		clickthrough: async (_label: string) => {}
	};

	const adapters: Partial<OcrPreviewAdapters> = {
		primaryMonitor: () => harness.primaryMonitor(),
		currentMonitor: () => harness.currentMonitor(),
		availableMonitors: () => harness.availableMonitors(),
		getGameMonitor: () => harness.gameMonitor(),
		createWindow: (label, options) => {
			const win = makeWindow(
				label,
				options,
				() => {
					if (harness.destroyRemovesActive) harness.activeWindow = null;
					resolveTeardown();
				},
				() => harness.onOuterSize?.()
			);
			harness.windows.push(win);
			harness.activeWindow = win;
			return win;
		},
		getWindowByLabel: async () => harness.activeWindow,
		physicalPosition: (x, y) => ({ x, y }),
		physicalSize: (width, height) => ({ width, height }),
		setClickthrough: async (label) => {
			harness.clickthroughLabels.push(label);
			await harness.clickthrough(label);
		},
		appLog: harness.logs.app,
		wait: harness.wait,
		setTimer: (callback, ms) => {
			harness.timers.push({ callback, ms });
			return harness.timers.length - 1 as ReturnType<typeof setTimeout>;
		},
		clearTimer: vi.fn(),
		warn: harness.logs.warn,
		info: harness.logs.info,
		error: harness.logs.error
	};

	return Object.assign(harness, {
		owner: createOcrPreviewOwner(adapters),
		teardownComplete
	});
}

function request(rect: OcrPreviewRequest['rect'] = [10, 20, 300, 400]): OcrPreviewRequest {
	return {
		key: 'lab.gem',
		label: 'Gem Tooltip Region',
		rect,
		rows: [[12, 22, 30, 40]]
	};
}

describe('OCR preview owner', () => {
	it('does not create a window for an absent rectangle', async () => {
		const harness = makeHarness();

		await harness.owner.preview(request(null));

		expect(harness.windows).toHaveLength(0);
	});

	it('falls back to the current monitor when primary monitor lookup fails', async () => {
		const harness = makeHarness();
		const primaryFailure = new Error('primary unavailable');
		harness.primaryMonitor = async () => {
			throw primaryFailure;
		};
		harness.currentMonitor = async () => ({ position: { x: 400, y: 500 }, scaleFactor: 2 });
		harness.gameMonitor = async () => null;

		await harness.owner.preview(request());

		expect(harness.windows).toHaveLength(1);
		expect(harness.windows[0].options).toMatchObject({ width: 150, height: 200 });
		expect(harness.logs.warn).toHaveBeenCalledWith(
			'[settings] primaryMonitor failed for OCR preview:',
			primaryFailure
		);
	});

	it('logs disagreement when the game monitor is not listed', async () => {
		const harness = makeHarness();
		harness.gameMonitor = async () => ({ id: 2, x: 900, y: 100, width: 1920, height: 1080 });

		await harness.owner.preview(request());

		expect(harness.windows).toHaveLength(1);
		expect(harness.logs.warn).toHaveBeenCalledWith(
			'[settings] OCR preview monitor disagreement: game at (900, 100), using primary at (100, 200)'
		);
	});

	it('refuses to create a window when all monitor sources are unavailable', async () => {
		const harness = makeHarness();
		const primaryFailure = new Error('primary unavailable');
		const currentFailure = new Error('current unavailable');
		const availableFailure = new Error('enumeration unavailable');
		harness.primaryMonitor = async () => {
			throw primaryFailure;
		};
		harness.currentMonitor = async () => {
			throw currentFailure;
		};
		harness.availableMonitors = async () => {
			throw availableFailure;
		};

		await harness.owner.preview(request());

		expect(harness.windows).toHaveLength(0);
		expect(harness.logs.warn).toHaveBeenCalledWith(
			'[settings] primaryMonitor failed for OCR preview:',
			primaryFailure
		);
		expect(harness.logs.warn).toHaveBeenCalledWith(
			'[settings] currentMonitor failed for OCR preview:',
			currentFailure
		);
		expect(harness.logs.warn).toHaveBeenCalledWith(
			'[settings] availableMonitors failed for OCR preview:',
			availableFailure
		);
		expect(harness.logs.error).toHaveBeenCalledWith(
			'[settings] OCR preview has no monitor to build on'
		);
	});

	it('tears down the existing preview before a replacement', async () => {
		const harness = makeHarness();

		await harness.owner.preview(request());
		const first = harness.windows[0];
		await harness.owner.destroy();
		await harness.owner.preview(request([30, 40, 500, 600]));

		expect(first.close).toHaveBeenCalledTimes(1);
		expect(first.destroy).toHaveBeenCalledTimes(1);
		expect(harness.windows).toHaveLength(2);
		expect(harness.owner.hasPreview).toBe(true);
	});

	it('does no placement work when the creation callback is stale at entry', async () => {
		const harness = makeHarness();

		await harness.owner.preview(request());
		const first = harness.windows[0];
		await harness.owner.destroy();
		await first.emit('tauri://created');

		expect(first.positionCalls).toHaveLength(0);
		expect(first.sizeCalls).toHaveLength(0);
	});

	it('places the rectangle at the monitor origin using the monitor scale', async () => {
		const harness = makeHarness();

		await harness.owner.preview(request());
		const preview = harness.windows[0];
		await preview.emit('tauri://created');

		expect(preview.label).toBe('overlay-preview');
		expect(preview.options).toEqual({
			url: '/overlay?preview=lab.gem&label=Gem%20Tooltip%20Region&rect=10,20,300,400&rows=%5B%5B12%2C22%2C30%2C40%5D%5D&dpr=1.5',
			transparent: true,
			decorations: false,
			alwaysOnTop: true,
			resizable: false,
			shadow: false,
			skipTaskbar: true,
			focus: false,
			width: 200,
			height: 267
		});
		expect(preview.options).toMatchObject({ width: 200, height: 267 });
		expect(preview.positionCalls).toEqual([{ x: 110, y: 220 }, { x: 110, y: 220 }]);
		expect(preview.sizeCalls).toEqual([
			{ width: 300, height: 400 },
			{ width: 301, height: 401 },
			{ width: 300, height: 400 },
			{ width: 300, height: 400 }
		]);
		expect(harness.clickthroughLabels).toEqual(['overlay-preview']);
		expect(harness.timers).toEqual([{ callback: expect.any(Function), ms: 10_000 }]);
	});

	it('stops after the resize checkpoint when replacement happens during the workaround', async () => {
		const harness = makeHarness();

		await harness.owner.preview(request());
		harness.onOuterSize = () => {
			harness.activeWindow = null;
			void harness.owner.destroy();
		};
		const preview = harness.windows[0];
		await preview.emit('tauri://created');

		expect(preview.positionCalls).toHaveLength(1);
		expect(preview.sizeCalls).toHaveLength(3);
		expect(harness.timers).toHaveLength(0);
	});

	it('stops before arming expiry when replacement happens during click-through setup', async () => {
		const harness = makeHarness();

		await harness.owner.preview(request());
		harness.clickthrough = async () => {
			harness.activeWindow = null;
			void harness.owner.destroy();
		};
		const preview = harness.windows[0];
		await preview.emit('tauri://created');

		expect(preview.positionCalls).toHaveLength(2);
		expect(preview.sizeCalls).toHaveLength(4);
		expect(harness.timers).toHaveLength(0);
	});

	it('destroys and logs when click-through setup fails', async () => {
		const harness = makeHarness();

		await harness.owner.preview(request());
		harness.clickthrough = async () => {
			throw new Error('WS_EX_TRANSPARENT did not read back');
		};
		const preview = harness.windows[0];
		await preview.emit('tauri://created');

		expect(preview.close).toHaveBeenCalledTimes(1);
		expect(preview.destroy).toHaveBeenCalledTimes(1);
		expect(harness.logs.error).toHaveBeenCalledWith(
		'[overlay] [overlay-preview-overlay] click-through setup failed — the window may be catching clicks meant for the game: Error: WS_EX_TRANSPARENT did not read back'
		);
		expect(harness.logs.app).toHaveBeenCalledWith(
		'[overlay-preview-overlay] click-through setup failed — the window may be catching clicks meant for the game: Error: WS_EX_TRANSPARENT did not read back'
		);
		expect(harness.timers).toHaveLength(0);
	});

	it('retries close and destroy five times with the existing backoff', async () => {
		const harness = makeHarness();
		harness.destroyRemovesActive = false;

		await harness.owner.preview(request());
		await harness.owner.destroy();
		const preview = harness.windows[0];

		expect(preview.close).toHaveBeenCalledTimes(5);
		expect(preview.destroy).toHaveBeenCalledTimes(5);
		expect(harness.wait).toHaveBeenCalledTimes(4);
		expect(harness.wait).toHaveBeenNthCalledWith(1, 100);
		expect(harness.logs.error).toHaveBeenCalledWith(
		'[settings] destroying OCR preview failed after 5 attempts'
		);
	});

	it('destroys the preview when its ten-second timer expires', async () => {
		const harness = makeHarness();

		await harness.owner.preview(request());
		const preview = harness.windows[0];
		await preview.emit('tauri://created');
		harness.timers[0].callback();
		await harness.teardownComplete;

		expect(preview.close).toHaveBeenCalledTimes(1);
		expect(preview.destroy).toHaveBeenCalledTimes(1);
		expect(harness.owner.hasPreview).toBe(false);
	});
});
