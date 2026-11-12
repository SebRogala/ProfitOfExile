// @vitest-environment node
// @ts-nocheck -- the frontend has no @types/node; this file runs only under vitest in Node.
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';
import { beforeAll, describe, expect, it } from 'vitest';

const FRONTEND = resolve(__dirname, '..');
const ROUTES = join(FRONTEND, 'src/routes');
const STATIC = join(FRONTEND, 'static');
const BUILD = join(FRONTEND, 'build');
const SITE = 'https://profitofexile.top/';

type OgCard = { page: string; url: string; sha256: string };

function sha256(path: string): string {
	return createHash('sha256').update(readFileSync(path)).digest('hex');
}

/** Route paths (e.g. "/", "/lab") whose +page.svelte sets og:image. */
function routesSettingOgImage(dir = ROUTES): string[] {
	const found: string[] = [];
	for (const entry of readdirSync(dir)) {
		const path = join(dir, entry);
		if (statSync(path).isDirectory()) found.push(...routesSettingOgImage(path));
		else if (entry === '+page.svelte' && readFileSync(path, 'utf8').includes('og:image"'))
			found.push('/' + relative(ROUTES, dir).split('/').filter(Boolean).join('/'));
	}
	return found.sort();
}

function prerenderedHtml(route: string): string {
	return join(BUILD, route === '/' ? 'index.html' : `${route.slice(1)}.html`);
}

function schemaImageHints(html: string): string[] {
	const graphs = [...html.matchAll(/<script type="application\/ld\+json">([\s\S]*?)<\/script>/g)].map((m) =>
		JSON.parse(m[1]),
	);
	return graphs.flatMap((g) =>
		g['@type'] === 'WebPage' ? [g.primaryImageOfPage?.url] : g['@type'] === 'SoftwareApplication' ? [g.image, g.screenshot] : [],
	);
}

function ogImageOf(html: string): string[] {
	return [...html.matchAll(/<meta[^>]*property="og:image"[^>]*content="([^"]*)"/g)].map((m) => m[1]);
}

/** Runs a fresh production build and reads every og:image card it prerendered. */
function buildAndReadCards(routes: string[]): OgCard[] {
	// VITEST in the child would switch vite.config's resolve conditions to the test ones.
	const env = { ...process.env };
	delete env.VITEST;
	execFileSync('npx', ['vite', 'build'], { cwd: FRONTEND, env, stdio: 'pipe' });
	return routes.map((page) => {
		const htmlPath = prerenderedHtml(page);
		if (!existsSync(htmlPath)) throw new Error(`build did not prerender ${page} at ${htmlPath}`);
		const urls = ogImageOf(readFileSync(htmlPath, 'utf8'));
		if (urls.length !== 1) throw new Error(`${page}: expected one og:image, found ${urls.length}`);
		const servedFile = join(BUILD, decodeURIComponent(new URL(urls[0], SITE).pathname));
		if (!existsSync(servedFile)) throw new Error(`${page}: ${urls[0]} is not in build output`);
		return { page, url: urls[0], sha256: sha256(servedFile) };
	});
}

describe('og:image across builds', () => {
	let routes: string[];
	let first: OgCard[];
	let second: OgCard[];

	beforeAll(() => {
		routes = routesSettingOgImage();
		first = buildAndReadCards(routes);
		second = buildAndReadCards(routes);
	}, 120_000);

	it('finds the home and lab pages as the pages that set og:image', () => {
		expect(routes).toEqual(expect.arrayContaining(['/', '/lab']));
	});

	it('points each og:image at an absolute site URL naming a file in static/', () => {
		for (const card of first) {
			expect(card.url, card.page).toMatch(/^https:\/\/profitofexile\.top\/[^/]+$/);
			const name = card.url.slice(SITE.length);
			expect(existsSync(join(STATIC, name)), `${card.page}: static/${name}`).toBe(true);
		}
	});

	it('serves the static/ bytes unchanged at each og:image URL', () => {
		for (const card of first) {
			const name = card.url.slice(SITE.length);
			expect(card.sha256, card.page).toBe(sha256(join(STATIC, name)));
		}
	});

	it('uses the logo card as the home page og:image', () => {
		expect(first.find((c) => c.page === '/')?.url).toBe(`${SITE}og-card.jpg`);
	});

	it('keeps the home page schema.org image hints on og-home.jpg', () => {
		const hints = schemaImageHints(readFileSync(prerenderedHtml('/'), 'utf8'));
		expect(hints).toEqual(Array(3).fill(`${SITE}og-home.jpg`));
	});

	it('gives the same og:image URL and bytes on a second build', () => {
		expect(second).toEqual(first);
	});
});
