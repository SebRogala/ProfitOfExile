/**
 * Source guard for the desktop server base (POE-383): no file under src/
 * builds a server URL from the build-time VITE_SERVER_URL. The base comes from
 * Rust through the SSOT snapshot (lib/stores/server-base.svelte.ts).
 */
import { describe, expect, it } from 'vitest';

const sources = import.meta.glob(['/src/**/*.{ts,svelte}', '!/src/**/*.test.ts'], {
	query: '?raw',
	import: 'default',
	eager: true
}) as Record<string, string>;

/** The source with `//`, block and HTML comments removed, so prose naming a variable does not count. */
function code(src: string): string {
	return src
		.replace(/<!--[\s\S]*?-->/g, '')
		.replace(/\/\*[\s\S]*?\*\//g, '')
		.replace(/(^|[^:'"`\\])\/\/.*$/gm, '$1');
}

describe('the desktop server base', () => {
	it('is read from no build-time VITE_SERVER_URL', () => {
		const offenders = Object.entries(sources)
			.filter(([, src]) => code(src).includes('VITE_SERVER_URL'))
			.map(([path]) => path);

		expect(Object.keys(sources).length).toBeGreaterThan(50);
		expect(offenders).toEqual([]);
	});
});

describe('the saved server_url', () => {
	it('only the settings, status and top-bar files read server_url', () => {
		// Server calls build on getServerUrl() (stores/server-base.svelte.ts). The
		// status store keeps the AppStatus field; Settings and TopBar show/edit it.
		const allowed = [
			'/src/lib/stores/status.svelte.ts',
			'/src/lib/pages/SettingsPage.svelte',
			'/src/lib/components/TopBar.svelte'
		];
		const readers = Object.entries(sources)
			.filter(([, src]) => /\.server_url\b|\bserver_url\s*:/.test(code(src)))
			.map(([path]) => path)
			.sort();

		expect(readers).toEqual([...allowed].sort());
	});
});
