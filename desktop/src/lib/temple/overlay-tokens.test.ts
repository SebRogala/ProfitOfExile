/**
 * The temple overlay's palette must actually resolve in the overlay window.
 *
 * This is a static check, not a behaviour test, and it exists because the bug
 * it pins is invisible in every other gate: `--color-lab-*` used to be declared
 * only in `app.css`, which only `routes/(app)/+layout.svelte` imports. The
 * overlay windows load `routes/overlay/+layout.svelte` instead, so the same
 * `TempleLattice.svelte` that renders correctly on the page rendered with every
 * custom property unset over the game — no build error, no type error, no test
 * failure, just an unreadable board. Svelte compiles `var(--nope)` happily and
 * there is no DOM harness here to catch it at runtime.
 *
 * So the assertion follows the real chain: which stylesheets does the OVERLAY
 * layout import, which custom properties do they declare, and does every
 * property the temple's overlay surfaces reference appear among them.
 *
 * Sources are read through Vite's `?raw` rather than through `node:fs` — this
 * app has no `@types/node`, and the bundler already has every one of these
 * files.
 */
import { describe, expect, it } from 'vitest';
import overlayLayoutSource from '../../routes/overlay/+layout.svelte?raw';
import overlayPageSource from '../../routes/overlay/temple/+page.svelte?raw';

/** The directory the overlay layout's own imports resolve against. */
const OVERLAY_LAYOUT_DIR = '/src/routes/overlay';

/** Every component in this directory — all of them draw inside the overlay. */
const templeComponents = import.meta.glob('./*.svelte', {
	query: '?raw',
	import: 'default',
	eager: true
}) as Record<string, string>;

/** Every stylesheet in the app, by root-relative path. */
const stylesheets = import.meta.glob('/src/**/*.css', {
	query: '?raw',
	import: 'default',
	eager: true
}) as Record<string, string>;

/** The surfaces drawn INSIDE the overlay window, and nothing else. */
const overlaySurfaces: [string, string][] = [
	['routes/overlay/temple/+page.svelte', overlayPageSource],
	...Object.entries(templeComponents)
];

/** Resolve a relative import specifier against a root-relative directory. */
function resolvePath(fromDir: string, spec: string): string {
	const out = fromDir.split('/').filter(Boolean);
	for (const part of spec.split('/')) {
		if (part === '' || part === '.') continue;
		if (part === '..') out.pop();
		else out.push(part);
	}
	return `/${out.join('/')}`;
}

/** The `.css` files a source file imports, as root-relative paths. */
function cssImportsOf(source: string, fromDir: string): string[] {
	return [...source.matchAll(/import\s+['"]([^'"]+\.css)['"]/g)].map((m) =>
		resolvePath(fromDir, m[1])
	);
}

/** Every custom property a stylesheet declares, following its `@import`s. */
function declaredIn(path: string, seen = new Set<string>()): Set<string> {
	const declared = new Set<string>();
	const source = stylesheets[path];
	if (source === undefined || seen.has(path)) return declared;
	seen.add(path);
	for (const m of source.matchAll(/(--[\w-]+)\s*:/g)) declared.add(m[1]);
	const dir = path.slice(0, path.lastIndexOf('/'));
	for (const m of source.matchAll(/@import\s+['"]([^'"]+)['"]/g)) {
		for (const token of declaredIn(resolvePath(dir, m[1]), seen)) declared.add(token);
	}
	return declared;
}

/** Every custom property a source file reads through `var(…)`. */
function referencedIn(source: string): string[] {
	return [...new Set([...source.matchAll(/var\(\s*(--[\w-]+)/g)].map((m) => m[1]))];
}

describe('the temple overlay palette', () => {
	const imported = cssImportsOf(overlayLayoutSource, OVERLAY_LAYOUT_DIR);
	const declared = new Set<string>();
	for (const sheet of imported) {
		for (const token of declaredIn(sheet)) declared.add(token);
	}

	it('pulls a stylesheet that exists into the overlay layout', () => {
		// Named separately so removing the import fails as "the overlay has no
		// stylesheet" rather than as thirteen missing colours, and so a rename
		// of the file it points at cannot pass as an empty declaration set.
		expect(imported.length).toBeGreaterThan(0);
		expect(imported.filter((path) => stylesheets[path] === undefined)).toEqual([]);
	});

	it.each(overlaySurfaces)('declares every custom property %s draws with', (_label, source) => {
		const missing = referencedIn(source).filter((token) => !declared.has(token));
		expect(missing).toEqual([]);
	});

	it('reads a palette from the temple surfaces rather than passing on an empty set', () => {
		// The check above is vacuously true if the sources ever stop matching
		// (a renamed route, a moved component), so the reference set being
		// non-empty is asserted rather than assumed.
		const referenced = overlaySurfaces.flatMap(([, source]) => referencedIn(source));
		expect(referenced.length).toBeGreaterThan(0);
	});
});
