import { invoke } from '@tauri-apps/api/core';
import { PhysicalPosition, PhysicalSize } from '@tauri-apps/api/dpi';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { availableMonitors, currentMonitor, primaryMonitor } from '@tauri-apps/api/window';
import { chooseMonitor, type GameMonitorInfo } from './monitor-choice';
import { clickthroughReport } from './clickthrough-report';

export type OcrRect = [number, number, number, number];

export type OcrPreviewRequest = {
	key: string;
	label: string;
	rect: OcrRect | null;
	rows: OcrRect[];
};

type OcrPreviewMonitor = {
	position: { x: number; y: number };
	scaleFactor: number;
};

type PreviewWindow = {
	label: string;
	once: (event: string, handler: (event?: unknown) => void) => unknown;
	setPosition: (position: unknown) => Promise<void>;
	setSize: (size: unknown) => Promise<void>;
	outerSize: () => Promise<{ width: number; height: number }>;
	close: () => Promise<void>;
	destroy: () => Promise<void>;
};

type PreviewWindowOptions = {
	url: string;
	transparent: boolean;
	decorations: boolean;
	alwaysOnTop: boolean;
	resizable: boolean;
	shadow: boolean;
	skipTaskbar: boolean;
	focus: boolean;
	width: number;
	height: number;
};

type TimerHandle = ReturnType<typeof setTimeout>;

export type OcrPreviewAdapters = {
	primaryMonitor: () => Promise<OcrPreviewMonitor | null>;
	currentMonitor: () => Promise<OcrPreviewMonitor | null>;
	availableMonitors: () => Promise<readonly OcrPreviewMonitor[]>;
	getGameMonitor: () => Promise<GameMonitorInfo | null>;
	createWindow: (label: string, options: PreviewWindowOptions) => PreviewWindow;
	getWindowByLabel: (label: string) => Promise<PreviewWindow | null>;
	physicalPosition: (x: number, y: number) => unknown;
	physicalSize: (width: number, height: number) => unknown;
	setClickthrough: (label: string) => Promise<void>;
	appLog: (message: string) => Promise<void>;
	wait: (ms: number) => Promise<void>;
	setTimer: (callback: () => void, ms: number) => TimerHandle;
	clearTimer: (timer: TimerHandle) => void;
	warn: (...args: unknown[]) => void;
	info: (...args: unknown[]) => void;
	error: (...args: unknown[]) => void;
};

const productionAdapters: OcrPreviewAdapters = {
	primaryMonitor: () => primaryMonitor() as Promise<OcrPreviewMonitor | null>,
	currentMonitor: () => currentMonitor() as Promise<OcrPreviewMonitor | null>,
	availableMonitors: () => availableMonitors() as Promise<readonly OcrPreviewMonitor[]>,
	getGameMonitor: () => invoke<GameMonitorInfo | null>('get_game_monitor'),
	createWindow: (label, options) =>
		new WebviewWindow(label, options) as unknown as PreviewWindow,
	getWindowByLabel: async (label) =>
		(await WebviewWindow.getByLabel(label)) as PreviewWindow | null,
	physicalPosition: (x, y) => new PhysicalPosition(x, y),
	physicalSize: (width, height) => new PhysicalSize(width, height),
	setClickthrough: async (label) => {
		await invoke('set_overlay_clickthrough', { label });
	},
	appLog: async (message) => {
		await invoke('app_log_from_frontend', { msg: message });
	},
	wait: (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
	setTimer: (callback, ms) => setTimeout(callback, ms),
	clearTimer: (timer) => clearTimeout(timer),
	warn: (...args) => console.warn(...args),
	info: (...args) => console.info(...args),
	error: (...args) => console.error(...args)
};

export function createOcrPreviewOwner(
	overrides: Partial<OcrPreviewAdapters> = {}
) {
	const adapters: OcrPreviewAdapters = { ...productionAdapters, ...overrides };
	let previewWindow: PreviewWindow | null = null;
	let previewTimer: TimerHandle | undefined;

	async function resolveOverlayMonitor(context: string): Promise<OcrPreviewMonitor | null> {
		const primary =
			(await adapters.primaryMonitor().catch((error) => {
				adapters.warn(`[settings] primaryMonitor failed for ${context}:`, error);
				return null;
			})) ?? (await adapters.currentMonitor().catch((error) => {
				adapters.warn(`[settings] currentMonitor failed for ${context}:`, error);
				return null;
			}));
		const game = await adapters.getGameMonitor().catch((error) => {
			adapters.warn(`[settings] get_game_monitor failed for ${context}:`, error);
			return null;
		});
		const listed = await adapters.availableMonitors().catch((error) => {
			adapters.warn(`[settings] availableMonitors failed for ${context}:`, error);
			return [];
		});
		const monitor = chooseMonitor(game, listed, primary);
		if (!monitor) {
			adapters.error(`[settings] ${context} has no monitor to build on`);
			return null;
		}
		if (game && (monitor.position.x !== game.x || monitor.position.y !== game.y)) {
			adapters.warn(
				`[settings] ${context} monitor disagreement: game at (${game.x}, ${game.y}), ` +
				`using primary at (${monitor.position.x}, ${monitor.position.y})`
			);
		}

		return {
			position: monitor.position,
			scaleFactor: monitor.scaleFactor > 0 ? monitor.scaleFactor : 1
		};
	}

	function reportClickthroughFailure(label: string, reason: unknown): void {
		const report = clickthroughReport(label, reason);
		if (report.level === 'error') adapters.error(`[overlay] ${report.message}`);
		else adapters.info(`[overlay] ${report.message}`);
		adapters.appLog(report.message).catch((error) => {
			adapters.error('[overlay] app log unreachable:', error);
		});
	}

	async function destroy(): Promise<void> {
		previewWindow = null;
		if (previewTimer) {
			adapters.clearTimer(previewTimer);
			previewTimer = undefined;
		}

		for (let i = 0; i < 5; i++) {
			const existing = await adapters.getWindowByLabel('overlay-preview').catch(() => null);
			if (!existing) return;
			try { await existing.close(); } catch (_) {}
			try { await existing.destroy(); } catch (_) {}
			if (i < 4) await adapters.wait(100);
		}
		const remaining = await adapters.getWindowByLabel('overlay-preview').catch(() => null);
		if (remaining) adapters.error('[settings] destroying OCR preview failed after 5 attempts');
	}

	async function preview(request: OcrPreviewRequest): Promise<void> {
		if (!request.rect) return;
		const monitor = await resolveOverlayMonitor('OCR preview');
		if (!monitor) return;

		const [x, y, width, height] = request.rect;
		const sf = monitor.scaleFactor;
		const win = adapters.createWindow('overlay-preview', {
			url: `/overlay?preview=${encodeURIComponent(request.key)}&label=${encodeURIComponent(request.label)}&rect=${request.rect.join(',')}&rows=${encodeURIComponent(JSON.stringify(request.rows))}&dpr=${sf}`,
			transparent: true,
			decorations: false,
			alwaysOnTop: true,
			resizable: false,
			shadow: false,
			skipTaskbar: true,
			focus: false,
			width: Math.max(1, Math.round(width / sf)),
			height: Math.max(1, Math.round(height / sf))
		});
		previewWindow = win;
		win.once('tauri://created', async () => {
			// Checkpoint 1: a replacement or disposal before callback entry owns
			// the label now, so this callback must do no placement work.
			if (previewWindow !== win) return;
			try {
				const position = adapters.physicalPosition(monitor.position.x + x, monitor.position.y + y);
				const exactSize = adapters.physicalSize(width, height);
				await win.setPosition(position);
				await win.setSize(exactSize);
				const size = await win.outerSize();
				await win.setSize(adapters.physicalSize(size.width + 1, size.height + 1));
				await win.setSize(adapters.physicalSize(size.width, size.height));
				// Checkpoint 2: preserve the existing resize workaround, then stop
				// an old callback before the second exact placement pass.
				if (previewWindow !== win) return;
				await win.setPosition(position);
				await win.setSize(exactSize);
				try {
					await adapters.setClickthrough(win.label);
				} catch (error) {
					reportClickthroughFailure(win.label, error);
					await destroy();
					return;
				}
				// Checkpoint 3: click-through setup is awaited, but timer ownership
				// still belongs only to the current window.
				if (previewWindow !== win) return;
				previewTimer = adapters.setTimer(() => { void destroy(); }, 10_000);
			} catch (error) {
				adapters.error('[settings] positioning OCR preview failed:', error);
				await destroy();
			}
		});
		win.once('tauri://error', (event) => {
			adapters.error('[settings] OCR preview creation failed:', event);
			void destroy();
		});
	}

	return {
		get hasPreview() {
			return previewWindow !== null;
		},
		preview,
		destroy
	};
}
