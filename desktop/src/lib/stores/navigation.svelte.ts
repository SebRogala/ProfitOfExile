/**
 * Global navigation store — replaces SvelteKit routing for the desktop app.
 *
 * All pages are always mounted (hidden via CSS). Navigation toggles visibility.
 * This keeps event listeners (Comparator, overlay events) alive across views.
 *
 * Usage:
 *   import { nav } from '$lib/stores/navigation.svelte';
 *   // Read: nav.view
 *   // Navigate: nav.go('/settings')
 */

export type View = 'lab' | 'settings' | 'dev' | 'mercenaries' | 'temple';

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
};

/** The path for a view — the inverse of `go` for every path `go` recognises. */
export function viewToPath(view: View): string {
	return VIEW_PATHS[view];
}

export const nav = $state({
	view: 'lab' as View,
	go(path: string) {
		if (path === '/settings') nav.view = 'settings';
		else if (path === '/dev') nav.view = 'dev';
		else if (path === '/mercenaries') nav.view = 'mercenaries';
		else if (path === '/temple') nav.view = 'temple';
		else nav.view = 'lab';
	},
});
