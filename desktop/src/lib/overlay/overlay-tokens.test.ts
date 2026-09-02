/**
 * EVERY overlay route's palette must actually resolve in the overlay window.
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
 * property each overlay surface references appear among them.
 *
 * The route list is a GLOB, not a list of imports, so an overlay added later is
 * covered the day it is added rather than the day someone remembers this file —
 * which is how the merc verdict overlay (POE-199) reached review drawing four
 * tokens nothing here had ever checked. That is also why the file now lives
 * under `lib/overlay/` instead of `lib/temple/`: it is not the temple's test.
 *
 * Sources are read through Vite's `?raw` rather than through `node:fs` — this
 * app has no `@types/node`, and the bundler already has every one of these
 * files.
 */
import { describe, expect, it } from 'vitest';
import overlayLayoutSource from '../../routes/overlay/+layout.svelte?raw';

/** The directory the overlay layout's own imports resolve against. */
const OVERLAY_LAYOUT_DIR = '/src/routes/overlay';

/**
 * Every overlay ROUTE page, by root-relative path — the windows themselves.
 *
 * `/src/routes/overlay/*` + `+page.svelte`, so a new `/overlay/<name>` route is
 * picked up automatically. `routes/overlay/+page.svelte` (the capture/config
 * overlay) matches too, which is correct: it draws in the same window shell.
 */
const overlayPages = import.meta.glob('/src/routes/overlay/**/+page.svelte', {
	query: '?raw',
	import: 'default',
	eager: true
}) as Record<string, string>;

/**
 * Every component an overlay route draws WITH.
 *
 * Two directories: `lib/temple/` (`TempleLattice.svelte`, shared with the page)
 * and `lib/overlay/widgets/` (`WidgetHost.svelte` and anything the widget
 * engine grows — it renders in EVERY module's overlay window, so its palette
 * has to resolve out here for all of them). They are listed by directory rather
 * than globbed app-wide because most `$lib` components never enter an overlay
 * window and would fail this check for tokens only `app.css` declares —
 * correctly, since they are never drawn out here.
 */
const overlayComponents = {
	...(import.meta.glob('/src/lib/temple/*.svelte', {
		query: '?raw',
		import: 'default',
		eager: true
	}) as Record<string, string>),
	...(import.meta.glob('/src/lib/overlay/widgets/*.svelte', {
		query: '?raw',
		import: 'default',
		eager: true
	}) as Record<string, string>)
};

/** Every stylesheet in the app, by root-relative path. */
const stylesheets = import.meta.glob('/src/**/*.css', {
	query: '?raw',
	import: 'default',
	eager: true
}) as Record<string, string>;

/** The surfaces drawn INSIDE the overlay window, and nothing else. */
const overlaySurfaces: [string, string][] = [
	...Object.entries(overlayPages),
	...Object.entries(overlayComponents)
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

/**
 * Every custom property a source file declares FOR ITSELF.
 *
 * The rule below is about the PALETTE — a colour an overlay surface names and
 * no stylesheet in that window declares renders as nothing over the game. A
 * property the component both sets and reads is not a palette token and has no
 * stylesheet to be declared in: `WidgetHost.svelte` writes `--widget-cursor`
 * into the same `style` attribute Svelte owns, because an imperative
 * `node.style.cursor` is erased on the next re-render of that attribute. So a
 * locally declared property satisfies its own reference, and everything else
 * still has to come from the palette.
 */
function selfDeclaredIn(source: string): Set<string> {
	return new Set([...source.matchAll(/(--[\w-]+)\s*:/g)].map((m) => m[1]));
}

describe('the overlay palette', () => {
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
		const own = selfDeclaredIn(source);
		const missing = referencedIn(source).filter(
			(token) => !declared.has(token) && !own.has(token)
		);
		expect(missing).toEqual([]);
	});

	it('reads a palette from the overlay surfaces rather than passing on an empty set', () => {
		// The check above is vacuously true if the sources ever stop matching
		// (a renamed route, a moved component), so the reference set being
		// non-empty is asserted rather than assumed.
		const referenced = overlaySurfaces.flatMap(([, source]) => referencedIn(source));
		expect(referenced.length).toBeGreaterThan(0);
	});

	it('covers every overlay route, not a hand-kept list of them', () => {
		// The glob is what makes the check above true for an overlay added
		// later. Naming the two module-coupled routes explicitly is the part
		// that would have caught the merc window: it drew tokens nothing
		// checked, because the old version of this file imported one route by
		// path. A route renamed or removed fails here rather than silently
		// shrinking the coverage of every assertion above.
		const covered = Object.keys(overlayPages);
		expect(covered).toContain('/src/routes/overlay/temple/+page.svelte');
		expect(covered).toContain('/src/routes/overlay/mercenary/+page.svelte');
		expect(covered.length).toBeGreaterThanOrEqual(6);
	});

	it('covers the widget host, which draws inside every module overlay', () => {
		// The host is not a route, so the page glob above cannot see it, and it
		// draws the config frame and the Save/Cancel bar in whatever window a
		// module opens. Named explicitly because it is the component whose
		// absence from the component glob would silently narrow this suite.
		expect(Object.keys(overlayComponents)).toContain(
			'/src/lib/overlay/widgets/WidgetHost.svelte'
		);
	});
});
