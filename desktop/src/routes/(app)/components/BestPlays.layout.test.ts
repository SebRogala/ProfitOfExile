import { describe, expect, it } from 'vitest';

/**
 * jsdom has no layout engine, so these guards pin the CSS mechanism behind the
 * multi-badge Rankings cells (POE-482), not rendered layout: under the table's
 * fixed layout a nowrap Signal/Signals cell pushes its second badge into the
 * neighbouring column.
 */

// Literal glob strings: Vite resolves them at transform time. The `(app)`
// segment is escaped because picomatch reads `(…)` as a group.
const desktopSources = import.meta.glob<string>('./BestPlays.svelte', { query: '?raw', import: 'default', eager: true });
const webSources = import.meta.glob<string>('../../../../../frontend/src/routes/lab/components/BestPlays.svelte', {
	query: '?raw',
	import: 'default',
	eager: true
});

interface Client {
	name: string;
	path: string;
	source: string | undefined;
}

const DESKTOP: Client = { name: 'desktop', path: './BestPlays.svelte', source: desktopSources['./BestPlays.svelte'] };
const WEB_PATH = '../../../../../frontend/src/routes/lab/components/BestPlays.svelte';
const WEB: Client = { name: 'web', path: WEB_PATH, source: webSources[WEB_PATH] };

const MULTI_BADGE_CELLS = ['.col-signal', '.col-signals'];

function styleOf(client: Client): string {
	if (client.source === undefined) throw new Error(`${client.name} BestPlays.svelte not found at ${client.path}`);
	const match = client.source.match(/<style[^>]*>([\s\S]*?)<\/style>/);
	if (!match) throw new Error(`${client.name} BestPlays.svelte has no <style> block`);
	return match[1];
}

/** Bodies of the rules whose whole selector is exactly `selector`. */
function ruleBodies(css: string, selector: string): string[] {
	const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
	const pattern = new RegExp(`(?:^|[}\\n])\\s*${escaped}\\s*\\{([^}]*)\\}`, 'g');
	return [...css.matchAll(pattern)].map((m) => m[1]);
}

function guardMultiBadgeCells(client: Client) {
	it.each(MULTI_BADGE_CELLS)(`${client.name} %s wraps its badges inside its 180px column`, (selector) => {
		const bodies = ruleBodies(styleOf(client), selector);
		expect(bodies, `${client.name} ${selector}: expected exactly one rule with this exact selector`).toHaveLength(1);
		const [body] = bodies;
		expect(body, `${client.name} ${selector}: column width changed`).toMatch(/(?:^|[;\s])width\s*:\s*180px\s*;/);
		expect(body, `${client.name} ${selector}: cell-level nowrap pushes the second badge into the next column`).not.toMatch(
			/white-space\s*:\s*nowrap/
		);
	});

	it(`${client.name} .plays-table keeps its fixed table layout`, () => {
		const bodies = ruleBodies(styleOf(client), '.plays-table');
		expect(bodies, `${client.name} .plays-table: expected exactly one rule with this exact selector`).toHaveLength(1);
		expect(bodies[0], `${client.name} .plays-table: table-layout changed`).toMatch(/table-layout\s*:\s*fixed/);
	});
}

describe('keep:regression BestPlays multi-badge cells (desktop overlap escaped, POE-482)', () => {
	guardMultiBadgeCells(DESKTOP);
});

// Silent consequence: a fix applied to desktop only leaves the web Rankings
// plausible while its badges cover the Sell and Listings values beside them.
describe('keep:critical BestPlays multi-badge cells (web mirror of the fix)', () => {
	guardMultiBadgeCells(WEB);
});
