import { listen as tauriListen } from '@tauri-apps/api/event';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { loadWidgetGeometries } from './widget-placements.svelte';

export type WidgetConfigPayload = {
	module?: string;
};

type WidgetConfigEvent = {
	payload?: WidgetConfigPayload;
};

type WidgetConfigUnlisten = () => void;

export type WidgetConfigRequestDependencies = {
	listen: (
		event: string,
		handler: (event: WidgetConfigEvent) => void
	) => Promise<WidgetConfigUnlisten>;
	emit: (event: string, payload: { module: string }) => Promise<void>;
	reloadGeometry: (module: string) => void;
	setTimer: (callback: () => void, delay: number) => ReturnType<typeof setTimeout>;
	clearTimer: (timer: ReturnType<typeof setTimeout>) => void;
	warn: (...args: unknown[]) => void;
};

const WIDGET_CONFIG_ACK_MS = 20_000;

/**
 * Settings owns the REQUEST, not config mode itself. It emits
 * `widget-config-start` for the request and `widget-config-end` only for an
 * opening timeout; the layout owns the ordered window creation, Rust
 * config-mode flag, and webview-scoped host event because it owns every
 * overlay window. This controller only waits for the layout's acknowledgements
 * and reports the request state back to Settings.
 */
const productionDependencies: WidgetConfigRequestDependencies = {
	listen: (event, handler) => tauriListen<WidgetConfigPayload>(event, handler),
	emit: (event, payload) => getCurrentWebviewWindow().emit(event, payload),
	reloadGeometry: (module) => { void loadWidgetGeometries([module]); },
	setTimer: (callback, delay) => setTimeout(callback, delay),
	clearTimer: (timer) => clearTimeout(timer),
	warn: (...args) => console.warn(...args)
};

export function createWidgetConfigRequestController(
	overrides: Partial<WidgetConfigRequestDependencies> = {}
) {
	const dependencies: WidgetConfigRequestDependencies = {
		...productionDependencies,
		...overrides
	};
	let configuring = $state<string | null>(null);
	let widgetConfigAckTimer: ReturnType<typeof setTimeout> | null = null;
	let widgetConfigPickedUp = false;
	let connected = false;
	let disposed = false;
	const unlisteners = new Set<WidgetConfigUnlisten>();

	function clearWidgetConfigAck(): void {
		if (widgetConfigAckTimer === null) return;
		dependencies.clearTimer(widgetConfigAckTimer);
		widgetConfigAckTimer = null;
	}

	function release(unlisten: WidgetConfigUnlisten): void {
		try {
			unlisten();
		} catch (_) {}
	}

	function register(
		event: string,
		handler: (event: WidgetConfigEvent) => void
	): void {
		let registration: Promise<WidgetConfigUnlisten>;
		try {
			registration = dependencies.listen(event, handler);
		} catch (_) { return; }
		registration.then((unlisten) => {
			if (disposed) {
				release(unlisten);
				return;
			}
			unlisteners.add(unlisten);
		}).catch(() => {});
	}

	/** Called by the page's mount effect so Tauri listeners never register during
	 * component construction or SSR. */
	function connect(): void {
		if (connected || disposed) return;
		connected = true;
		register('widget-config-opening', (event) => {
			if (event.payload?.module === configuring) widgetConfigPickedUp = true;
		});
		register('widget-config-open', (event) => {
			if (event.payload?.module !== configuring) return;
			clearWidgetConfigAck();
			widgetConfigPickedUp = false;
		});
		register('widget-config-end', (event) => {
			const module = event.payload?.module;
			if (!module) return;
			if (configuring === module) {
				configuring = null;
				clearWidgetConfigAck();
				widgetConfigPickedUp = false;
			}
			// Save wrote through `set_widget_geometry` in the overlay window, and
			// Cancel may have restored and re-read the persisted map there. Either
			// path can leave this page's copy stale, including for another module.
			dependencies.reloadGeometry(module);
		});
	}

	/**
	 * How long the button waits for the layout to say config mode is OPEN.
	 *
	 * Not a limit on the arranging session — that ends when the user presses Save
	 * or Cancel, however long they take. This bounds the OPENING only, and it has
	 * to sit above the layout's own bound on the same work: it waits up to 10 s
	 * for the window to be built and then makes three more IPC calls, and every
	 * failure it can see it already reports as `widget-config-end`. What is left
	 * for this timer is the failure the layout cannot report — its listener never
	 * registered, or the emit never arrived — which is exactly the case where
	 * nothing else will ever clear the button.
	 *
	 * A pickup buys one more period because the layout may still be waiting for
	 * its window or IPC while it has already accepted the request, and abandoning
	 * that in-flight start could tear down the window it is about to put into
	 * config mode. See the Config-mode ordering contract in
	 * `docs/OVERLAY-GUIDE.md`.
	 */
	function armWidgetConfigAck(module: string): void {
		clearWidgetConfigAck();
		widgetConfigAckTimer = dependencies.setTimer(() => {
			widgetConfigAckTimer = null;
			if (disposed || configuring !== module) return;
			if (widgetConfigPickedUp) {
				widgetConfigPickedUp = false;
				dependencies.warn(`[settings] ${module} config mode is slow to open — waiting once more`);
				armWidgetConfigAck(module);
				return;
			}
			dependencies.warn(`[settings] no answer to widget config mode for ${module} — giving up`);
			configuring = null;
			// Told, not just forgotten: if the layout DID force the module window on
			// and then went quiet, this lets the layout end the request and undo that
			// force-shown window. It never toggles the module's work flag.
			void dependencies.emit('widget-config-end', { module }).catch(() => {});
		}, WIDGET_CONFIG_ACK_MS);
	}

	/**
	 * Ask the layout to open config mode on a module's overlay window.
	 *
	 * Settings owns none of the three ordering steps itself
	 * (`docs/OVERLAY-GUIDE.md`, "Config-mode ordering contract") — the layout
	 * does, because it is the file that builds and owns that window. This emits
	 * the request and then waits for `widget-config-end`, which the HOST sends
	 * after Save or Cancel and the layout sends if it could not open the window
	 * at all.
	 *
	 * Pressing it again while a session is live is deliberate, not a bug to
	 * guard: it is the user's way out of a window that somehow missed the event,
	 * and both entering config mode and this request are idempotent.
	 */

	async function configureWidgets(module: string): Promise<void> {
		if (disposed) return;
		configuring = module;
		widgetConfigPickedUp = false;
		armWidgetConfigAck(module);
		try {
			await dependencies.emit('widget-config-start', { module });
		} catch (error) {
			dependencies.warn('[settings] could not ask for widget config mode:', error);
			clearWidgetConfigAck();
			configuring = null;
		}
	}

	/** Page disposal cancels the opening deadline and releases registrations that
	 * resolved before or after disposal. */
	function dispose(): void {
		if (disposed) return;
		disposed = true;
		clearWidgetConfigAck();
		configuring = null;
		widgetConfigPickedUp = false;
		for (const unlisten of unlisteners) release(unlisten);
		unlisteners.clear();
	}

	return {
		get configuring() {
			return configuring;
		},
		connect,
		configureWidgets,
		dispose
	};
}
