// @vitest-environment happy-dom
/**
 * The overlay route mounted in the client runtime with the exact URL the OCR
 * preview owner hands its window, so the rendered frame is what a windowed
 * client's preview shows.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import { createOcrPreviewOwner, type OcrPreviewAdapters, type OcrRect } from '$lib/overlay/ocr-preview';
import OverlayPage from './+page.svelte';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/dpi', () => ({ PhysicalPosition: class {}, PhysicalSize: class {} }));
vi.mock('@tauri-apps/api/webviewWindow', () => ({ WebviewWindow: class {} }));
vi.mock('@tauri-apps/api/window', () => ({
	availableMonitors: vi.fn(),
	currentMonitor: vi.fn(),
	primaryMonitor: vi.fn()
}));

const monitor = { position: { x: 1920, y: 0 }, scaleFactor: 1.25 };

async function previewUrl(rect: OcrRect, rows: OcrRect[]): Promise<string> {
	const urls: string[] = [];
	const adapters: Partial<OcrPreviewAdapters> = {
		primaryMonitor: async () => monitor,
		currentMonitor: async () => null,
		availableMonitors: async () => [monitor],
		getGameMonitor: async () => ({ id: 2, x: 1920, y: 0, width: 2560, height: 1440 }),
		createWindow: (label, options) => {
			urls.push(options.url);
			return { label, once: () => {} } as unknown as ReturnType<OcrPreviewAdapters['createWindow']>;
		}
	};
	await createOcrPreviewOwner(adapters).preview({ key: 'lab.gem', label: 'Gem Tooltip Region', rect, rows });
	expect(urls).toHaveLength(1);
	return urls[0];
}

let mounted: ReturnType<typeof mount> | null = null;
afterEach(() => {
	if (mounted) unmount(mounted);
	mounted = null;
	document.body.innerHTML = '';
});

async function render(rect: OcrRect, rows: OcrRect[]): Promise<HTMLElement> {
	window.history.replaceState(null, '', await previewUrl(rect, rows));
	const target = document.createElement('div');
	document.body.appendChild(target);
	mounted = mount(OverlayPage, { target });
	flushSync();
	return target;
}

function expectRowAt(row: HTMLElement): void {
	const { left, top, width, height } = row.style;
	expect({ left, top, width, height }).toEqual({ left: '8px', top: '4px', width: '224px', height: '16px' });
}

const WINDOWED = { rect: [200, 120, 300, 60] as OcrRect, row: [210, 125, 280, 20] as OcrRect };
const MOVED = { rect: [400, 220, 300, 60] as OcrRect, row: [410, 225, 280, 20] as OcrRect };

function onlyRow(target: HTMLElement): HTMLElement {
	const rows = target.querySelectorAll<HTMLElement>('.preview-row');
	expect(rows).toHaveLength(1);
	return rows[0];
}

describe('keep_happy: overlay preview page for a windowed client', () => {
	it('labels the preview frame for assistive tech', async () => {
		const target = await render(WINDOWED.rect, [WINDOWED.row]);

		const frame = target.querySelector('[role="img"]');
		expect(frame?.getAttribute('aria-label')).toBe('Gem Tooltip Region OCR region preview');
	});

	it('shows the rect numbers', async () => {
		const target = await render(WINDOWED.rect, [WINDOWED.row]);

		expect(target.querySelector('.preview-label')?.textContent).toContain('[200, 120, 300, 60]');
	});

	it('draws the row at its offset inside the rect', async () => {
		const target = await render(WINDOWED.rect, [WINDOWED.row]);

		expectRowAt(onlyRow(target));
	});

	it('shows the moved rect numbers', async () => {
		const target = await render(MOVED.rect, [MOVED.row]);

		expect(target.querySelector('.preview-label')?.textContent).toContain('[400, 220, 300, 60]');
	});

	it('draws the moved row at the same offset', async () => {
		const target = await render(MOVED.rect, [MOVED.row]);

		expectRowAt(onlyRow(target));
	});
});
