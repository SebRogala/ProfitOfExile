<script lang="ts">
	/**
	 * The fullscreen host every module's overlay window renders (POE-225).
	 *
	 * One window per module, the size of the primary monitor, click-through
	 * everywhere; inside it, one absolutely-positioned container per widget the
	 * registry declares for that module. This component owns the placement, the
	 * in-window config mode, the hot-rect declaration and the click routing —
	 * the module's route owns only what each widget DRAWS, through the `content`
	 * snippet.
	 *
	 * Read `docs/OVERLAY-GUIDE.md` first. The guards, and where each is met:
	 *
	 * 1. **Capabilities** — the module id is the window label, and every label
	 *    is in `src-tauri/capabilities/default.json`.
	 * 2/3. **Physical persistence / logical construction** — the WINDOW's
	 *    geometry belongs to `routes/(app)/+layout.svelte`; what this file
	 *    converts is the WIDGETS', through `widget-geometry.ts`, which persists
	 *    physical and lays out in CSS. `window.devicePixelRatio` is not used;
	 *    the window's own `scaleFactor()` is, and until it resolves nothing is
	 *    saved and no rect is claimed.
	 * 4. **Move, not recreate** — a widget moving is a CSS offset changing.
	 *    Config mode never touches the window.
	 * 5. **Settings survival** — the placements live in `Settings.widgets`,
	 *    which is owned by an `AppState` mutex and travels through `from_state`;
	 *    the survival tests are in `settings.rs`.
	 * 6. **Error visibility** — every invoke and every event operation here
	 *    reports through `app_log_from_frontend` as well as the console, because
	 *    a shipped build has no devtools.
	 *
	 * # Config mode is in-window (D2)
	 *
	 * Settings does not open a draggable copy of this window the way it does for
	 * the five per-window overlays. It turns THIS window interactive
	 * (`set_overlay_config_mode`) and emits `widget-config`; the widgets grow a
	 * red frame and drag/resize handles over their real content, and Save writes
	 * every widget of the module at once. WYSIWYG, and one code path rather than
	 * a second window per widget.
	 *
	 * `onMount` is not reliable in an overlay window and cross-window JS state is
	 * not either, so everything below is rune effects and Tauri events.
	 */
	import type { Snippet } from 'svelte';
	import { invoke } from '@tauri-apps/api/core';
	import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
	import { placeableWidgetsFor, type WidgetSpec } from './widget-registry';
	import {
		EDGE_CURSORS,
		dragged,
		edgeFor,
		gestureResized,
		placementFor,
		resized,
		seedRect,
		sizeToPersist,
		widgetGeometry,
		type HostSize,
		type ResizeEdge,
		type WidgetGeometry,
		type WidgetPlacement,
		type WidgetRect
	} from './widget-geometry';
	import { useHotRects } from './use-hot-rects';

	let {
		module,
		content,
		onAction
	}: {
		/** The module whose widgets this host draws — also the window label. */
		module: string;
		/**
		 * What each widget draws — called once per placed widget, with the spec
		 * and whether config mode is on.
		 *
		 * The second argument exists because a module's own content rule (the
		 * temple's "is there a board worth drawing") must NOT gate this host:
		 * a host that is not mounted has no `widget-config` listener, so a window
		 * flipped into config mode while the module happens to be drawing nothing
		 * would be interactive with no Save and no Cancel — a permanently
		 * click-eating rectangle over the game. So the module applies its rule
		 * INSIDE the snippet, and uses this flag to draw a placeholder instead of
		 * nothing while the user is arranging widgets.
		 */
		content: Snippet<[WidgetSpec, boolean]>;
		/**
		 * A click the mouse hook forwarded, already resolved to the
		 * `[data-action]` element under it. Only elements inside a declared
		 * `[data-hot]` rect can ever produce one.
		 */
		onAction?: (action: string, element: HTMLElement) => void;
	} = $props();

	/** Guard 6's channel — the console alone is unreadable in a shipped build. */
	function log(msg: string): void {
		console.warn(`[widgets] ${module}: ${msg}`);
		invoke('app_log_from_frontend', { msg: `[${module}-widgets] ${msg}` }).catch((e) =>
			console.error('[widgets] app log unreachable:', e)
		);
	}

	const specs = $derived(placeableWidgetsFor(module));

	/**
	 * The window's scale factor, cached.
	 *
	 * Zero until it answers, and every conversion declines while it is: a
	 * placement saved at scale 0 would be the origin, and a hot rect measured at
	 * 0 would be empty.
	 */
	let scaleFactor = $state(0);
	$effect(() => {
		getCurrentWebviewWindow()
			.scaleFactor()
			.then((sf) => {
				scaleFactor = sf;
			})
			.catch((e) => {
				log(`scaleFactor failed, widgets stay at their defaults: ${e}`);
			});
	});

	/** The persisted placements, by widget id. Empty means "nothing configured",
	 *  which is what a fresh install has. */
	let stored = $state<Record<string, WidgetGeometry>>({});

	async function loadStored(): Promise<void> {
		try {
			const rows = await invoke<{ id: string; geometry: WidgetGeometry }[]>(
				'get_widget_geometries',
				{ module }
			);
			const next: Record<string, WidgetGeometry> = {};
			for (const row of rows) next[row.id] = row.geometry;
			stored = next;
		} catch (e) {
			log(`could not read the saved placements, using the defaults: ${e}`);
		}
	}

	$effect(() => {
		loadStored();
	});

	/** The host's own box in CSS px — the bounds a drag is clamped to. */
	let host = $state<HostSize>({ width: 0, height: 0 });
	$effect(() => {
		const measure = () => {
			host = { width: window.innerWidth, height: window.innerHeight };
		};
		measure();
		window.addEventListener('resize', measure);
		return () => window.removeEventListener('resize', measure);
	});

	// ---- config mode -------------------------------------------------------

	let configMode = $state(false);
	/** The live CSS rectangles while config mode is on, by widget id. */
	let draft = $state<Record<string, WidgetRect>>({});
	/** Which widgets the user actually RESIZED in this config session. Only
	 *  these get a size persisted — see `sizeToPersist`. */
	let resizedThisSession = new Set<string>();
	/** What the Save/Cancel bar says when a save did not fully land. Empty is
	 *  the ordinary state; anything else keeps config mode open. */
	let saveError = $state('');
	/** The rendered container per widget, so entering config mode can seed a
	 *  content-sized widget's FRAME from what it actually measures. Its
	 *  POSITION never comes from here — see `seedRect`. */
	let nodes: Record<string, HTMLElement | undefined> = {};
	/** The cursor each widget's frame is advertising, by id. Kept as state and
	 *  written into the same `style` attribute Svelte owns: an imperative
	 *  `node.style.cursor` is erased the next time that attribute re-renders,
	 *  which for a widget is every drag frame. */
	let cursors = $state<Record<string, string>>({});

	// Re-entrancy guards. Every entry point here is an event or a pointer — two
	// `widget-config {on:true}` events, or a double-clicked Save, are ordinary,
	// and without these the second one reseeds the draft over a drag in progress
	// or runs the exit path twice.
	let entering = false;
	let exiting = false;
	let saving = $state(false);

	async function enterConfig(): Promise<void> {
		if (configMode || entering) return;
		entering = true;
		try {
			// The persisted map, freshly, before anything is seeded from it: the
			// window may have been created for this config session, in which case
			// the first `loadStored` is still in flight and `stored` is empty.
			// Seeding from an empty map would place every widget at its shipped
			// default and Save would then write that.
			await loadStored();
			const seeded: Record<string, WidgetRect> = {};
			for (const spec of specs) {
				const box = nodes[spec.id]?.getBoundingClientRect();
				const measured = box ? { x: box.left, y: box.top, w: box.width, h: box.height } : null;
				seeded[spec.id] = seedRect(spec, stored[spec.id], measured, scaleFactor, host);
			}
			draft = seeded;
			resizedThisSession = new Set();
			cursors = {};
			saveError = '';
			configMode = true;
		} finally {
			entering = false;
		}
	}

	async function exitConfig(): Promise<void> {
		if (exiting) return;
		exiting = true;
		configMode = false;
		draft = {};
		resizedThisSession = new Set();
		saveError = '';
		try {
			await invoke('set_overlay_config_mode', { label: module, on: false });
		} catch (e) {
			// The window is still interactive: it will keep eating the player's
			// clicks until something re-asserts click-through. Loud, not silent.
			log(`could not return the window to click-through: ${e}`);
		}
		try {
			await getCurrentWebviewWindow().emit('widget-config-end', { module });
		} catch (e) {
			log(`could not report the end of config mode: ${e}`);
		}
		exiting = false;
	}

	async function saveConfig(): Promise<void> {
		if (!configMode || saving) return;
		if (scaleFactor === 0) {
			// Config mode STAYS OPEN. Every placement would be written at the
			// origin, so the save is refused — but leaving silently would look
			// exactly like a save that worked, and the frames the user arranged
			// would be gone with it. The scale factor usually resolves within a
			// frame or two, so Save is a retry.
			log('scale factor unresolved — refusing to save placements that would be wrong');
			saveError = 'The window has not reported its scale yet — nothing saved. Try again.';
			return;
		}
		saving = true;
		const next: Record<string, WidgetGeometry> = { ...stored };
		const failed: string[] = [];
		try {
			for (const spec of specs) {
				const rect = draft[spec.id];
				if (!rect) continue;
				// A widget the user has never shown stays hidden: config mode places
				// it, the Show checkbox in Settings decides whether it is drawn.
				const visible = stored[spec.id]?.visible ?? true;
				const geometry = widgetGeometry(
					// Content sizing is the widget contract; a size is written only
					// when the user made one (`sizeToPersist`).
					sizeToPersist(spec, rect, resizedThisSession.has(spec.id), stored[spec.id]),
					scaleFactor,
					visible
				);
				try {
					await invoke('set_widget_geometry', { id: spec.id, geometry });
					// Only a write that actually landed joins the map. Committing
					// the whole draft and then reporting success is how a rejected
					// save reads as a saved one until the next restart.
					next[spec.id] = geometry;
				} catch (e) {
					failed.push(spec.label);
					log(`could not save the placement of ${spec.id}: ${e}`);
				}
			}
			stored = next;
		} finally {
			saving = false;
		}
		if (failed.length > 0) {
			// Config mode stays open with the draft intact: the frames are still
			// where the user put them, so Save is a retry rather than a redo.
			saveError = `Could not save ${failed.join(', ')} — the placement is unchanged. Try again.`;
			return;
		}
		await exitConfig();
	}

	async function cancelConfig(): Promise<void> {
		if (!configMode || saving) return;
		// Re-read rather than restore a snapshot taken on the way in: config mode
		// can begin before the first `get_widget_geometries` has answered, and a
		// snapshot of the empty map would have Cancel WIPE every placement the
		// user had. What is on disk is the only trustworthy "before".
		await loadStored();
		await exitConfig();
	}

	$effect(() => {
		let dropped = false;
		// Targeted at this window rather than broadcast, so a labelled `emit_to`
		// from Rust and a global `emit` from the app both arrive — a window-scoped
		// listener matches both, a bare `listen` matches only the second.
		const pending = getCurrentWebviewWindow().listen<{ module: string; on: boolean }>(
			'widget-config',
			(event) => {
				if (event.payload.module !== module) return;
				if (event.payload.on) void enterConfig();
				else void exitConfig();
			}
		);
		// The catch-up half of the ordering contract (`docs/OVERLAY-GUIDE.md`,
		// "Widget overlays"): Settings sets the flag in Rust BEFORE it emits, so a
		// window created for this very config session — and which therefore had no
		// listener when the event went out — still finds out.
		//
		// Chained onto the listener rather than run beside it: `listen` is itself
		// async, so a query that started first leaves a window in which the flag
		// reads false, the event fires, and nothing is registered to hear it. This
		// order can only duplicate, and `enterConfig` is idempotent.
		pending
			.then(() => {
				if (dropped) return;
				return invoke<boolean>('get_overlay_config_mode', { label: module });
			})
			.then((on) => {
				if (on && !dropped) return enterConfig();
			})
			.catch((e) => log(`could not read whether this window is in config mode: ${e}`));
		return () => {
			dropped = true;
			pending.then((unlisten) => unlisten()).catch((e) => log(`unlisten failed: ${e}`));
		};
	});

	// ---- dragging ----------------------------------------------------------

	interface Gesture {
		id: string;
		edge: ResizeEdge | null;
		startRect: WidgetRect;
		pointerX: number;
		pointerY: number;
	}
	let gesture: Gesture | null = null;

	function handlePointerDown(spec: WidgetSpec, event: PointerEvent): void {
		if (!configMode) return;
		const rect = draft[spec.id];
		if (!rect) return;
		event.preventDefault();
		gesture = {
			id: spec.id,
			edge: edgeFor(spec, rect, event.clientX - rect.x, event.clientY - rect.y),
			startRect: rect,
			pointerX: event.clientX,
			pointerY: event.clientY
		};
		(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
	}

	function handlePointerMove(spec: WidgetSpec, event: PointerEvent): void {
		if (!configMode) return;
		const rect = draft[spec.id];
		if (!rect) return;
		if (!gesture || gesture.id !== spec.id) {
			// Not dragging: the cursor advertises what a press would do. A widget
			// with no resize handles advertises `move` everywhere, including on
			// its border, because that is all a press there can do.
			const hovered = edgeFor(spec, rect, event.clientX - rect.x, event.clientY - rect.y);
			const cursor = hovered ? EDGE_CURSORS[hovered] : 'move';
			if (cursors[spec.id] !== cursor) cursors = { ...cursors, [spec.id]: cursor };
			return;
		}
		const dx = event.clientX - gesture.pointerX;
		const dy = event.clientY - gesture.pointerY;
		// A border press with no movement is not a resize, and counting it would
		// end this widget's content sizing for good.
		if (gestureResized(gesture.edge, dx, dy)) resizedThisSession.add(spec.id);
		draft = {
			...draft,
			[spec.id]: gesture.edge
				? resized(gesture.startRect, gesture.edge, dx, dy, host)
				: dragged(gesture.startRect, dx, dy, host)
		};
	}

	function handlePointerUp(event: PointerEvent): void {
		if (!gesture) return;
		gesture = null;
		const target = event.currentTarget as HTMLElement;
		if (target.hasPointerCapture(event.pointerId)) target.releasePointerCapture(event.pointerId);
	}

	// ---- clicks the mouse hook forwarded -----------------------------------

	$effect(() => {
		// Window-scoped, never a bare `listen`: Rust emits with `emit_to(label)`
		// and a bare listener registers for the `Any` target, which a labelled
		// emit does not match (guide, "Overlay types").
		const pending = getCurrentWebviewWindow().listen<{ label: string; x: number; y: number }>(
			'overlay-click',
			(event) => {
				if (scaleFactor === 0) return;
				const el = document.elementFromPoint(
					event.payload.x / scaleFactor,
					event.payload.y / scaleFactor
				);
				if (!el) {
					log(`elementFromPoint found nothing at the forwarded click — DPI mismatch?`);
					return;
				}
				const target = el.closest('[data-action]') as HTMLElement | null;
				const action = target?.dataset.action;
				if (!target || !action) return;
				onAction?.(action, target);
			}
		);
		return () => {
			pending.then((unlisten) => unlisten()).catch((e) => log(`unlisten failed: ${e}`));
		};
	});

	// ---- placement ---------------------------------------------------------

	const placed = $derived(
		specs
			.map((spec) => {
				const rect = configMode ? draft[spec.id] : null;
				// In config mode the draft IS the rectangle, size included: the
				// frame the user is dragging has to be the box they see, not a
				// content measurement that changes under them.
				const placement: WidgetPlacement | null = rect
					? { x: rect.x, y: rect.y, width: rect.w, height: rect.h, maxWidth: null }
					: placementFor(spec, stored[spec.id], scaleFactor, host);
				return { spec, placement };
			})
			.filter((entry) => entry.placement !== null) as {
			spec: WidgetSpec;
			placement: WidgetPlacement;
		}[]
	);

	function boxStyle(placement: WidgetPlacement, cursor: string): string {
		const size = [
			placement.width === null ? '' : `width:${placement.width}px;`,
			placement.height === null ? '' : `height:${placement.height}px;`,
			// A ceiling, not a size: the widget still shrinks to its content, it
			// just stops growing at the width the registry ships. Without it a
			// content-sized widget inside a monitor-sized host is `max-content`,
			// and a one-line headline runs the width of the screen.
			placement.maxWidth === null ? '' : `max-width:${placement.maxWidth}px;`
		].join('');
		// The cursor rides the same attribute rather than being written onto the
		// node: Svelte owns this `style`, so an imperative write is erased on the
		// next drag frame.
		return `left:${placement.x}px;top:${placement.y}px;${size}--widget-cursor:${cursor};`;
	}
</script>

<div
	class="widget-host"
	class:config={configMode}
	use:useHotRects={{ module, scaleFactor }}
>
	{#each placed as entry (entry.spec.id)}
		<div
			class="widget"
			class:config={configMode}
			style={boxStyle(entry.placement, cursors[entry.spec.id] ?? 'move')}
			bind:this={nodes[entry.spec.id]}
			role={configMode ? 'presentation' : undefined}
			onpointerdown={configMode ? (e) => handlePointerDown(entry.spec, e) : undefined}
			onpointermove={configMode ? (e) => handlePointerMove(entry.spec, e) : undefined}
			onpointerup={configMode ? handlePointerUp : undefined}
			onpointercancel={configMode ? handlePointerUp : undefined}
		>
			{@render content(entry.spec, configMode)}
			{#if configMode}
				<span class="widget-name">{entry.spec.label}</span>
			{/if}
		</div>
	{/each}

	{#if configMode}
		<div class="config-bar">
			<!-- The bar is the ONLY report a failed save gets: an overlay window has
			     no devtools and no status line, and a Save that silently dropped a
			     widget reads as one the user never moved. -->
			<span class="config-hint" class:failed={saveError !== ''}>
				{saveError === '' ? 'Drag to move, edges to resize' : saveError}
			</span>
			<button class="config-btn save" disabled={saving} onpointerup={saveConfig}>
				{saving ? 'Saving…' : 'Save'}
			</button>
			<button class="config-btn cancel" disabled={saving} onpointerup={cancelConfig}>Cancel</button>
		</div>
	{/if}
</div>

<style>
	/* The window is the whole primary monitor. Outside config mode nothing here
	   takes the mouse: OS-level click-through is `WS_EX_TRANSPARENT`, and this
	   is the webview half of the same promise. The exception is `[data-hot]`
	   below — those must be hit-testable so `elementFromPoint` can resolve a
	   click the mouse hook forwarded. */
	.widget-host {
		position: fixed;
		inset: 0;
		width: 100vw;
		height: 100vh;
		pointer-events: none;
		background: transparent;
	}

	.widget-host.config {
		pointer-events: auto;
	}

	.widget {
		position: absolute;
		pointer-events: none;
	}

	/* A button a widget declares. `pointer-events: auto` is not what lets the
	   player click it — the window is click-through at the OS level — it is what
	   lets `document.elementFromPoint` find it when the hook forwards the click. */
	.widget :global([data-hot]) {
		pointer-events: auto;
	}

	.widget.config {
		pointer-events: auto;
		border: 3px solid var(--color-lab-red);
		border-radius: 4px;
		/* The frame is drawn OUTSIDE the widget's own box, so turning config mode
		   on does not move the content the user is placing. */
		box-sizing: content-box;
		margin: -3px;
		/* A widget whose module is currently drawing nothing still has to be
		   grabbable — the placeholder inside it is a line of text, and a frame
		   that collapsed to it would be smaller than its own grab zone. */
		min-width: 96px;
		min-height: 28px;
		/* Set per widget in `boxStyle`, because Svelte owns this attribute. */
		cursor: var(--widget-cursor, move);
	}

	.widget-name {
		position: absolute;
		top: -3px;
		left: -3px;
		padding: 1px 5px;
		font-size: 10px;
		font-weight: 700;
		color: var(--color-lab-bg);
		background: var(--color-lab-red);
		border-radius: 3px 0 3px 0;
		white-space: nowrap;
	}

	.config-bar {
		position: absolute;
		bottom: 24px;
		left: 50%;
		transform: translateX(-50%);
		display: flex;
		gap: 8px;
		align-items: center;
		padding: 6px 10px;
		background: rgb(15 17 23 / 92%);
		border: 1px solid var(--color-lab-border);
		border-radius: 6px;
		pointer-events: auto;
	}

	.config-hint {
		font-size: 11px;
		color: var(--color-lab-text-secondary);
	}

	.config-hint.failed {
		font-weight: 700;
		color: var(--color-lab-red);
	}

	.config-btn {
		padding: 3px 12px;
		font-size: 12px;
		color: var(--color-lab-text);
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		border-radius: 3px;
		cursor: pointer;
	}

	.config-btn.save {
		color: var(--color-lab-bg);
		background: var(--color-lab-green);
		border-color: var(--color-lab-green);
	}

	.config-btn:disabled {
		cursor: default;
		opacity: 0.6;
	}
</style>
