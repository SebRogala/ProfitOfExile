/**
 * Global navigation store — replaces SvelteKit routing for the desktop app.
 *
 * All pages are always mounted (hidden via CSS). Navigation toggles visibility.
 * This keeps event listeners (Comparator, overlay events) alive across views.
 *
 * The selected view is a persisted preference (`navView`, ADR-013 `ui_prefs`),
 * so the app reopens on the tool that was last open. The pref IS the state —
 * `nav.view` reads through it instead of mirroring it into a rune — because the
 * prefs map loads asynchronously and lands after the first paint. A mirror would
 * need a restore step, and that step would have to re-derive "has the user
 * picked since launch?" to avoid overwriting a pick with the stored value.
 * Reading through the pref gets both for free: the late load flips the getter,
 * and a pick made before the load lands marks the key dirty, which drops the
 * loaded value (see `prefs.svelte.ts`) — a click is never undone a beat later by
 * the previous session's view.
 *
 * Usage:
 *   import { nav } from '$lib/stores/navigation.svelte';
 *   // Read: nav.view
 *   // Navigate: nav.go('/settings')
 */
import { persisted } from '$lib/prefs.svelte';

export type View = 'lab' | 'settings' | 'dev' | 'mercenaries' | 'temple' | 'currency-exchange';

/**
 * The path each view answers to. These strings are the Sidebar's keys — it
 * highlights on `currentPath === '/settings'` and navigates with `nav.go('/settings')`
 * — so 'lab' maps to '/', not '/lab'.
 *
 * Exported through `viewToPath` so the layout can hand the Sidebar the current
 * path without re-deriving the mapping in a ternary that silently falls through
 * to '/' for every view added later.
 */
export const VIEW_PATHS: Record<View, string> = {
	lab: '/',
	settings: '/settings',
	dev: '/dev',
	mercenaries: '/mercenaries',
	temple: '/temple',
	'currency-exchange': '/currency-exchange',
};

/** The path for a view — the inverse of `go` for every path `go` recognises. */
export function viewToPath(view: View): string {
	return VIEW_PATHS[view];
}

/**
 * Narrow a stored string to a view.
 *
 * Anything unrecognised — an empty pref, a view a newer or older build wrote
 * that this one does not have — becomes 'lab'. The layout reveals a page with
 * `nav.view !== '<name>'`, so an unvalidated value hides every page at once and
 * the app opens blank with nothing to explain it. Matching against the key list
 * rather than `raw in VIEW_PATHS` keeps `'constructor'` and the rest of
 * `Object.prototype` out.
 *
 * 'dev' only parses in a dev build: the layout renders the Dev page under
 * `import.meta.env.DEV`, so a release build restoring a dev-session 'dev' pref
 * would open on a blank content pane — the exact state this validation exists
 * to prevent.
 */
export function parseView(raw: string): View {
	if (raw === 'dev' && !import.meta.env.DEV) return 'lab';
	return Object.keys(VIEW_PATHS).includes(raw) ? (raw as View) : 'lab';
}

/**
 * The view each path selects — `VIEW_PATHS` inverted rather than a second
 * hand-written table, so `go` cannot drift from `viewToPath`. A Map, not an
 * object, so an unknown path can never resolve to an inherited property.
 */
const VIEW_BY_PATH = new Map<string, View>(
	(Object.entries(VIEW_PATHS) as [View, string][]).map(([view, path]) => [path, view]),
);

const navPref = persisted('navView', 'lab');

export const nav = {
	/** The view on screen. Validated on read — the pref is user-writable state. */
	get view(): View {
		return parseView(navPref.value);
	},
	/** Show the view a path names; a path no view claims falls back to lab. */
	go(path: string): void {
		navPref.value = VIEW_BY_PATH.get(path) ?? 'lab';
	},
};
