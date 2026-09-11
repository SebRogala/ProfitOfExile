import { untrack } from 'svelte';
import { invoke } from '@tauri-apps/api/core';
import { chooseMonitor, type GameMonitorInfo } from '../monitor-choice';
import { visibilityRow, type WidgetGeometry } from './widget-geometry';
import type { WidgetSpec } from './widget-registry';

/** The main window's copy of the persisted widget placements, and the monitor scale Show places new rows with. A widget with no entry has never been placed and draws where the registry ships it. */
export const widgetPlacements = $state({
	rows: {} as Record<string, WidgetGeometry>,
	scaleFactor: 0
});

/**
 * The scale factor of the monitor the widget overlay lives on.
 *
 * The overlay's display, not the main window's: a widget overlay is built on
 * the GAME's monitor (`routes/(app)/+layout.svelte`, POE-237), so that is the
 * display a widget's physical coordinates are measured against. Reading the
 * main window's factor instead would be wrong by the ratio between the two
 * whenever the main window sits on a second display with different scaling —
 * and silently, since it agrees on a single-monitor machine.
 *
 * The CHOICE is `chooseMonitor`'s, shared with the layout rather than
 * re-spelled, because the two answers have to be the same display: the layout
 * sizes the canvas from it and this converts the shipped CSS defaults into
 * coordinates inside that canvas. The primary is the fallback on every
 * failing path, exactly as the layout falls back.
 *
 * Zero until it answers, and the Show toggle declines while it is: creating a
 * placement row means converting the registry's CSS defaults to the physical
 * pixels Rust stores, and doing that at zero would write the widget to the
 * origin.
 *
 * RE-RESOLVED on `game-monitor-changed`, not read once at mount: the answer
 * is a property of whichever display the game is on, and the player moving
 * PoE to a screen with different scaling changes it. Holding the mount-time
 * factor would place every widget Show creates from then on by the old
 * display's ratio, into a canvas the layout has already rebuilt on the new
 * one.
 */
export async function resolveWidgetScaleFactor(): Promise<void> {
	const { availableMonitors, currentMonitor, primaryMonitor } = await import(
		'@tauri-apps/api/window'
	);
	const primary =
		(await primaryMonitor().catch(() => null)) ?? (await currentMonitor().catch(() => null));
	const game = await invoke<GameMonitorInfo | null>('get_game_monitor').catch((e: any) => {
		console.warn('[widget-placements] get_game_monitor failed, using the primary monitor:', e);
		return null;
	});
	const listed = await availableMonitors().catch((e: any) => {
		console.warn('[widget-placements] availableMonitors failed, using the primary monitor:', e);
		return [];
	});
	const monitor = chooseMonitor(game, listed, primary);
	if (monitor && monitor.scaleFactor > 0) widgetPlacements.scaleFactor = monitor.scaleFactor;
	else console.warn('[widget-placements] no monitor scale factor — Show cannot place a widget yet');
}

/**
 * Re-read one or more modules' placements.
 *
 * Per module rather than wholesale, because `widget-config-end` names one:
 * the ids of the module being refreshed are dropped and replaced with what
 * Rust answers, and every other module's rows are left as they were. The
 * `"<module>."` prefix is the same rule Rust's `widgets_for_module` uses, and
 * `widget-registry.test.ts` pins that an id's halves agree with its module.
 *
 * The previous map is read through `untrack`: this runs from an effect, and a
 * tracked read of the state it writes would re-run itself forever.
 */
export async function loadWidgetGeometries(modules: string[]): Promise<void> {
	for (const module of modules) {
		try {
			const rows = await invoke<{ id: string; geometry: WidgetGeometry }[]>(
				'get_widget_geometries',
				{ module }
			);
			const next: Record<string, WidgetGeometry> = {};
			for (const [id, geometry] of Object.entries(untrack(() => widgetPlacements.rows))) {
				if (!id.startsWith(`${module}.`)) next[id] = geometry;
			}
			for (const row of rows) next[row.id] = row.geometry;
			widgetPlacements.rows = next;
		} catch (e) {
			// The rows fall back to "Not set", which is what a widget with no
			// placement genuinely shows — hence the log: otherwise a dead IPC
			// reads as a user who never configured anything.
			console.warn(`[widget-placements] could not read the ${module} widget placements:`, e);
		}
	}
}

/**
 * Show or hide one widget, preserving everything else about its placement.
 *
 * A widget with no stored row gets one written from the registry's shipped
 * defaults, converted to physical pixels — and with a ZERO size, because that
 * is what the host reads back as "let the content decide" (`placementFor`).
 * Writing a measured size here would pin a widget the user never resized.
 *
 * The rune is updated first so the checkbox does not lag the click, and a
 * rejected write is undone by re-reading rather than by guessing what Rust
 * kept.
 */
export async function setWidgetVisible(spec: WidgetSpec, visible: boolean): Promise<void> {
	const geometry = visibilityRow(
		spec,
		widgetPlacements.rows[spec.id],
		widgetPlacements.scaleFactor,
		visible
	);
	if (!geometry) {
		console.warn(`[widget-placements] no scale factor yet — not placing ${spec.id}`);
		return;
	}
	widgetPlacements.rows = { ...widgetPlacements.rows, [spec.id]: geometry };
	try {
		await invoke('set_widget_geometry', { id: spec.id, geometry });
	} catch (e) {
		console.warn(`[widget-placements] could not save the ${spec.id} visibility:`, e);
		await loadWidgetGeometries([spec.module]);
	}
}
