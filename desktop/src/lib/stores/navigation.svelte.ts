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

export type View = 'lab' | 'settings' | 'planner';

export const nav = $state({
	view: 'lab' as View,
	go(path: string) {
		if (path === '/settings') nav.view = 'settings';
		else if (path === '/planner') nav.view = 'planner';
		else nav.view = 'lab';
	},
});
