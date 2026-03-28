/**
 * Overlay window manager — spawn, destroy, and track overlay windows.
 *
 * Each overlay is a transparent, always-on-top Tauri WebviewWindow.
 * Routes to /overlay/{name} in SvelteKit.
 */

/** Active overlay window references, keyed by overlay name. */
const activeOverlays = new Map<string, any>();

export interface OverlayOptions {
	/** URL path for the overlay (e.g., '/overlay/region'). */
	url: string;
	/** Initial width in pixels. */
	width?: number;
	/** Initial height in pixels. */
	height?: number;
	/** Initial x position in pixels. */
	x?: number;
	/** Initial y position in pixels. */
	y?: number;
}

/**
 * Create and show an overlay window. Destroys any existing overlay with the same name first.
 * Returns the WebviewWindow instance, or null on failure.
 */
export async function showOverlay(name: string, options: OverlayOptions): Promise<any> {
	const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');

	// Destroy existing overlay with this name
	await destroyOverlay(name);

	return new Promise((resolve) => {
		const win = new WebviewWindow(name, {
			url: options.url,
			transparent: true,
			decorations: false,
			alwaysOnTop: true,
			resizable: true,
			shadow: false,
			skipTaskbar: true,
			width: options.width || 400,
			height: options.height || 200,
			x: options.x ?? 100,
			y: options.y ?? 100,
		});

		win.once('tauri://created', () => {
			activeOverlays.set(name, win);
			resolve(win);
		});

		win.once('tauri://error', (e: any) => {
			console.error(`Overlay '${name}' creation failed:`, e);
			resolve(null);
		});
	});
}

/**
 * Destroy an overlay window by name. No-op if it doesn't exist.
 */
export async function destroyOverlay(name: string): Promise<void> {
	const win = activeOverlays.get(name);
	if (win) {
		try {
			await win.destroy();
		} catch (e) {
			console.error(`Failed to destroy overlay '${name}':`, e);
		}
		activeOverlays.delete(name);
	}
}

/**
 * Get the active overlay window by name, or null if not active.
 */
export function getOverlay(name: string): any | null {
	return activeOverlays.get(name) ?? null;
}

/**
 * Check if an overlay is currently active.
 */
export function isOverlayActive(name: string): boolean {
	return activeOverlays.has(name);
}

/**
 * Read the physical screen position and size of an overlay window.
 * Returns { x, y, w, h } in physical pixels, or null on failure.
 */
export async function readOverlayRegion(name: string): Promise<{ x: number; y: number; w: number; h: number } | null> {
	const win = activeOverlays.get(name);
	if (!win) return null;

	try {
		const w = win.window ?? win;
		const pos = await w.outerPosition();
		const size = await w.outerSize();
		return { x: pos.x, y: pos.y, w: size.width, h: size.height };
	} catch (e) {
		console.error(`Failed to read overlay '${name}' region:`, e);
		return null;
	}
}
