/**
 * Every monitor-sized module window, built from one row table, and the one
 * desired-state rule that raises and lowers them (POE-346).
 *
 * Call once at layout init: the per-row `$effect`s need a component context.
 */
import { moduleOverlayWanted, moduleWindowStep } from './module-lifecycle';
import { createWidgetWindow, type WidgetWindow } from './widget-window';
import {
	widgetConfigLive,
	widgetConfigSessionsInit,
	type WidgetConfigSessions
} from './widgets/widget-config-session';

export interface ModuleWindowRow {
	label: string;
	moduleId: string;
	granted(): boolean;
	/** `undefined` while the flag is not yet known. */
	enabled(): boolean | undefined;
	debug(): boolean;
}

export type ModuleWindowConfigTarget = WidgetWindow['configTarget'];

export interface ModuleWindows {
	/** In row order. */
	windows: WidgetWindow[];
	/** Keyed by WINDOW LABEL — what a `WidgetSpec.module` is, and so what Settings sends. */
	configTargets: Record<string, ModuleWindowConfigTarget>;
	getSessions(): WidgetConfigSessions;
	setSessions(next: WidgetConfigSessions): void;
}

export function createModuleWindows(rows: ModuleWindowRow[]): ModuleWindows {
	/**
	 * What each live config session forced — and, since POE-241, the second term
	 * of every module overlay's desired state, which is why it is a rune.
	 *
	 * `$state.raw` and not deep `$state`: the map is only ever REPLACED (both
	 * reducers return a new object), so a proxy would buy nothing and would put
	 * one between `widgetConfigLive` and a plain record read. Nothing renders
	 * from it; its one reactive consumer is the overlay effects below, which
	 * write nothing, so a session recorded here asks for the window the same
	 * way the module flag does.
	 */
	let widgetConfigSessions: WidgetConfigSessions = $state.raw(widgetConfigSessionsInit());

	const windows = rows.map((row) => {
		const configLive = () => widgetConfigLive(widgetConfigSessions, row.label);
		// Read by the two rebuild sites in widget-window.ts: a rebuild is an
		// unconditional off/on, so without this a module switched off while the
		// window was being built would come straight back.
		const wanted = () => moduleOverlayWanted(row.enabled(), configLive(), row.granted());
		const window = createWidgetWindow({
			label: row.label,
			moduleId: row.moduleId,
			wanted,
			configLive,
			debug: () => row.debug()
		});

		// The SESSION term is what makes arranging widgets cost no module work
		// (POE-241): the window is raised here, for the length of the session,
		// while the Rust module loop stays spawned by the module flag alone
		// (`modules.rs` reconcile — untouched by this flow). Dropping the record in
		// the orchestrator's `end` is therefore what tears the window down again
		// when the flag is off, and it is the LAST thing that function does for
		// exactly that reason. Re-reporting the same value is a no-op inside the
		// driver.
		$effect(() => {
			const step = moduleWindowStep({
				enabled: row.enabled(),
				configuring: configLive(),
				built: window.driver.built(),
				granted: row.granted()
			});
			if (!step.skip) window.driver.setDesired(step.desired);
		});

		return window;
	});

	return {
		windows,
		configTargets: Object.fromEntries(windows.map((w) => [w.label, w.configTarget])),
		getSessions: () => widgetConfigSessions,
		setSessions: (next) => {
			widgetConfigSessions = next;
		}
	};
}
