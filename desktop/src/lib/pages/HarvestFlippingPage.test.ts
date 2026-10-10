/**
 * HarvestFlippingPage server-rendered from a captured real controller, as
 * `SettingsPage.test.ts` captures its update controller: the factory mock
 * hands the page a controller this test built and loaded first, with the
 * fixture adapters behind its ports. Tauri's prefs load and `listen` are
 * mocked; `render()` runs no `$effect`, so the page's own `start()` stays off.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from 'svelte/server';
import type { HarvestController } from '$lib/harvest/controller.svelte';
import type { ExchangePriceRead, HarvestFamily, HarvestFamilyData } from '$lib/harvest/seam';
import { loadExchangePrices } from '$lib/harvest/sources/exchange-fixture';
import { loadHarvestFamilies } from '$lib/harvest/sources/harvest-fixture';

const pageHarness = vi.hoisted(() => ({
	controller: null as HarvestController | null
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => ({})) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

vi.mock('$lib/harvest/controller.svelte', async () => {
	const actual = await vi.importActual<typeof import('$lib/harvest/controller.svelte')>(
		'$lib/harvest/controller.svelte'
	);
	return {
		...actual,
		createHarvestController: () => {
			if (!pageHarness.controller) throw new Error('the test builds the controller before render');
			return pageHarness.controller;
		}
	};
});

const { createHarvestController, productionDependencies } = await vi.importActual<
	typeof import('$lib/harvest/controller.svelte')
>('$lib/harvest/controller.svelte');

import HarvestFlippingPage from './HarvestFlippingPage.svelte';

const KEEPERS_REGEX = '"shu|san|gil|fac|fra|hol|gly"';
const DENSE = 'Metadata/Items/Currency/CurrencyDelveCraftingDefences';
const HOLLOW = 'Metadata/Items/Currency/CurrencyDelveCraftingAbyss';
const FAMILY_LABELS = [
	'Delirium Orbs',
	'Deafening Essences',
	'Corrupted Essences',
	'Fossils',
	'Astrolabes',
	'Oils',
	'Catalysts'
];

function never<T>(): Promise<T> {
	return new Promise<T>(() => {});
}

function renderPage(): string {
	return render(HarvestFlippingPage).body;
}

/** Let the ports' promises and the controller's assignments settle. */
async function settle(): Promise<void> {
	for (let i = 0; i < 5; i++) await Promise.resolve();
}

/** Neither port has answered. */
function renderBeforeAnyPort(): string {
	const controller = createHarvestController({
		...productionDependencies(),
		loadExchangePrices: () => never<ExchangePriceRead>(),
		loadHarvestFamilies: () => never<HarvestFamilyData>()
	});
	void controller.load();
	pageHarness.controller = controller;
	return renderPage();
}

/** Harvest has answered (tabs and weights ship with the app); prices have not. */
async function renderPricesPending(): Promise<string> {
	const controller = createHarvestController({
		...productionDependencies(),
		loadExchangePrices: () => never<ExchangePriceRead>()
	});
	void controller.load();
	await settle();
	pageHarness.controller = controller;
	return renderPage();
}

/** Both fixture adapters answered: Fossils, reference 01. */
async function renderPopulated(): Promise<string> {
	const controller = createHarvestController(productionDependencies());
	await controller.load();
	pageHarness.controller = controller;
	return renderPage();
}

/**
 * Both fixture adapters answered over restored prefs (a family, picks). The
 * prefs are plain values: a write through `persisted()` would land in its
 * module cache and seed every later controller in this file.
 */
async function renderWithPrefs(restored: { family?: string; picks?: object }): Promise<string> {
	const controller = createHarvestController({
		...productionDependencies(),
		prefs: {
			picks: { value: JSON.stringify(restored.picks ?? {}) },
			horizon: { value: 'day' },
			family: { value: restored.family ?? 'fossil' }
		}
	});
	await controller.load();
	pageHarness.controller = controller;
	return renderPage();
}

/** A test-local family with no HarvestForge log: every fixture family now has weights. */
const UNLOGGED: HarvestFamily = {
	id: 'unlogged',
	label: 'Unlogged Orbs',
	lifeforce: null,
	rerollCost: null,
	weights: null,
	tiers: null,
	note: null
};

/** The fixtures plus the test-local unlogged family, opened on `family`; prices pending unless `priced`. */
async function renderWithUnlogged(opts: { family: string; priced: boolean }): Promise<string> {
	const controller = createHarvestController({
		...productionDependencies(),
		...(opts.priced ? {} : { loadExchangePrices: () => never<ExchangePriceRead>() }),
		loadHarvestFamilies: async (horizon) => {
			const data = await loadHarvestFamilies(horizon);
			return { ...data, families: [...data.families, UNLOGGED] };
		},
		prefs: {
			picks: { value: '{}' },
			horizon: { value: 'day' },
			family: { value: opts.family }
		}
	});
	if (opts.priced) await controller.load();
	else {
		void controller.load();
		await settle();
	}
	pageHarness.controller = controller;
	return renderPage();
}

/** The fixtures, with the exchange read changed by `edit`. */
async function renderWithExchange(edit: (read: ExchangePriceRead) => ExchangePriceRead): Promise<string> {
	const controller = createHarvestController({
		...productionDependencies(),
		loadExchangePrices: async (horizon) => edit(await loadExchangePrices(horizon))
	});
	await controller.load();
	pageHarness.controller = controller;
	return renderPage();
}

/** Harvest answered, the exchange rejected: never reached the server (reference 08 §4). */
async function renderUnreachable(): Promise<string> {
	const controller = createHarvestController({
		...productionDependencies(),
		loadExchangePrices: () => Promise.reject(new Error('server down'))
	});
	await controller.load();
	pageHarness.controller = controller;
	return renderPage();
}

/** Loaded once, then a refresh the server rejects: stale (reference 08 §3). */
async function renderStale(): Promise<string> {
	let calls = 0;
	const controller = createHarvestController({
		...productionDependencies(),
		loadExchangePrices: (horizon) =>
			calls++ === 0 ? loadExchangePrices(horizon) : Promise.reject(new Error('server down'))
	});
	await controller.load();
	await controller.load();
	pageHarness.controller = controller;
	return renderPage();
}

/** The tab labels that carry the NO DATA mark, in tab order. */
function noDataMarked(html: string): string[] {
	return [
		...html.matchAll(/role="tab"[^>]*>([^<]*)(?:<!--[^>]*-->)?<span class="no-data-mark[^"]*">NO DATA</g)
	].map((m) => m[1].trim());
}

/** The opening tag of the first element whose class list holds `cls`, or ''. */
function tagOf(html: string, cls: string): string {
	return html.match(new RegExp(`<[a-z]+\\b[^>]*class="(?:[^"]* )?${cls}(?: [^"]*)?"[^>]*>`))?.[0] ?? '';
}

/** The text of the first element whose class list holds `cls`, or null. */
function textOf(html: string, cls: string): string | null {
	const m = html.match(new RegExp(`class="(?:[^"]* )?${cls}(?: [^"]*)?"[^>]*>([^<]*)<`));
	return m ? m[1] : null;
}

beforeEach(() => {
	pageHarness.controller = null;
});

describe('HarvestFlippingPage before any port answers', () => {
	it('shows the drawn Loading… line', () => {
		expect(textOf(renderBeforeAnyPort(), 'state-line')).toBe('Loading…');
	});

	it('draws no tabs', () => {
		expect(renderBeforeAnyPort()).not.toContain('role="tablist"');
	});

	it('draws no verdict', () => {
		expect(renderBeforeAnyPort()).not.toContain('Is it worth it?');
	});
});

describe('HarvestFlippingPage with prices pending', () => {
	it('shows the page title Harvest Flipping', async () => {
		expect(await renderPricesPending()).toMatch(/<h1[^>]*>Harvest Flipping<\/h1>/);
	});

	it('lists every family tab label in order', async () => {
		const html = await renderPricesPending();
		const tabs = [...html.matchAll(/role="tab"[^>]*>([^<]*)</g)].map((m) => m[1].trim());
		expect(tabs).toEqual(FAMILY_LABELS);
	});

	it('marks NO DATA on no fixture family: all seven carry weights', async () => {
		expect(noDataMarked(await renderPricesPending())).toEqual([]);
	});

	it('marks NO DATA on a family without weights only', async () => {
		const html = await renderWithUnlogged({ family: 'fossil', priced: false });
		expect(noDataMarked(html)).toEqual(['Unlogged Orbs']);
	});

	it('shows the drawn Loading… line', async () => {
		expect(textOf(await renderPricesPending(), 'state-line')).toBe('Loading…');
	});

	it('draws no verdict while prices load', async () => {
		expect(await renderPricesPending()).not.toContain('Is it worth it?');
	});
});

describe('HarvestFlippingPage populated (Fossils, reference 01)', () => {
	it('headline reads Yes — feed the cheap tiers', async () => {
		expect(textOf(await renderPopulated(), 'headline')).toBe('Yes — feed the cheap tiers');
	});

	it('headline EV reads +25.7c', async () => {
		expect(textOf(await renderPopulated(), 'ev-big')).toBe('+25.7c');
	});

	it('names the feeder: per Lucent Fossil', async () => {
		expect(textOf(await renderPopulated(), 'per')).toBe('per Lucent Fossil');
	});

	it('shows the divine-scale line', async () => {
		expect(textOf(await renderPopulated(), 'divine-line')).toBe(
			'1 div profit ≈ 15 feeders · ~378 rerolls · ~11,349 Wild'
		);
	});

	it('draws the cards MID, LOW (feeders) then TOP, HIGH, MID (keepers)', async () => {
		const html = await renderPopulated();
		const tiers = [...html.matchAll(/class="tier[^"]*">([^<]*)</g)].map((m) => m[1]);
		expect(tiers).toEqual(['MID', 'LOW', 'TOP', 'HIGH', 'MID']);
	});

	it('puts the keepers regex in the Keepers regex box', async () => {
		const html = await renderPopulated();
		const value = html.match(/value="([^"]*)"[^>]*aria-label="Keepers regex"/)?.[1];
		expect(value).toBe(KEEPERS_REGEX.replaceAll('"', '&quot;'));
	});

	it('shows the cost per reroll 0.9c', async () => {
		expect(textOf(await renderPopulated(), 'cost')).toBe('0.9c');
	});

	it('shows 1 div → 9,905 lifeforce → 330 rerolls', async () => {
		const line = (await renderPopulated()).match(/<p class="per-divine[^"]*">([\s\S]*?)<\/p>/)?.[1] ?? '';
		expect(line.replace(/<[^>]*>/g, '').replace(/\s+/g, ' ').trim()).toBe('1 div → 9,905 lifeforce → 330 rerolls');
	});

	it('sets only 1 div and the two numbers in mono (prototype line 266, C9)', async () => {
		const line = (await renderPopulated()).match(/<p class="per-divine[^"]*">([\s\S]*?)<\/p>/)?.[1] ?? '';
		expect([...line.matchAll(/<span class="mono[^"]*">([^<]*)</g)].map((m) => m[1])).toEqual(['1 div', '9,905', '330']);
	});

	it('shows the league Allflame', async () => {
		expect(textOf(await renderPopulated(), 'league')).toBe('Allflame');
	});

	it('colours the +25.7c headline EV as a gain', async () => {
		expect(tagOf(await renderPopulated(), 'ev-big')).toMatch(/class="[^"]*\bgain\b/);
	});

	it('reads computed from prices', async () => {
		expect(textOf(await renderPopulated(), 'split-label')).toBe('computed from prices');
	});

	it('disables Reset when nothing was moved', async () => {
		expect(await renderPopulated()).toMatch(/<button class="btn[^"]*"[^>]*disabled/);
	});

	it('shows the yield ~13.1 keepers', async () => {
		const html = await renderPopulated();
		expect(html).toMatch(/1 div of lifeforce yields<\/dt>\s*<dd class="mono[^"]*">~13\.1 keepers</);
	});
});

describe('HarvestFlippingPage never reached the server (reference 08 §4)', () => {
	it('shows Couldn’t reach the server', async () => {
		expect(textOf(await renderUnreachable(), 'state-line')).toBe("Couldn't reach the server");
	});

	it('draws the state line amber', async () => {
		expect(tagOf(await renderUnreachable(), 'state-line')).toMatch(/class="[^"]*\bwarn\b/);
	});
});

describe('HarvestFlippingPage cold server (reference 08 §2)', () => {
	const cold = () => renderWithExchange((read) => ({ ...read, warm: false, lastUpdated: null }));

	it('draws the cold panel: No prices yet', async () => {
		expect(textOf(await cold(), 'headline')).toBe('No prices yet');
	});

	it('explains the cold panel', async () => {
		expect(textOf(await cold(), 'reason')).toBe(
			'The server has not read an hour of the exchange. This fills in on its own.'
		);
	});

	it('shows the waiting line', async () => {
		expect(textOf(await cold(), 'state-line')).toBe('Waiting for the first Currency Exchange hour…');
	});

	it('does not draw the waiting line amber', async () => {
		expect(tagOf(await cold(), 'state-line')).not.toMatch(/class="[^"]*\bwarn\b/);
	});
});

describe('HarvestFlippingPage stale (reference 08 §3)', () => {
	it('draws the stale since segment amber', async () => {
		const html = await renderStale();
		const first = html.match(/<div class="status-line[^"]*">\s*(?:<!--[^>]*-->\s*)*(<span[^>]*>)([^<]*)</);
		expect([first?.[2].startsWith('stale since '), /\bwarn\b/.test(first?.[1] ?? '')]).toEqual([true, true]);
	});

	it('marks the prices-from segment as the age', async () => {
		const html = await renderStale();
		expect(tagOf(html, 'age')).not.toBe('');
		expect(textOf(html, 'age')).toMatch(/^prices from /);
	});

	it('keeps every number: the headline EV still reads +25.7c', async () => {
		expect(textOf(await renderStale(), 'ev-big')).toBe('+25.7c');
	});
});

describe('HarvestFlippingPage family without weights (reference 06, 08 §9)', () => {
	const unlogged = () => renderWithUnlogged({ family: 'unlogged', priced: true });

	it('titles the no-data panel', async () => {
		expect(textOf(await unlogged(), 'no-data-title')).toBe('Unlogged Orbs: reroll weights not logged yet');
	});

	it('explains the no-data panel', async () => {
		expect(textOf(await unlogged(), 'no-data-text')).toMatch(/^The EV needs how often each type comes out of a reroll\./);
	});

	it('draws no verdict', async () => {
		expect(await unlogged()).not.toContain('Is it worth it?');
	});
});

describe('HarvestFlippingPage picks (reference 05)', () => {
	const densePicked = () => renderWithPrefs({ picks: { fossil: { [DENSE]: 'keep' } } });

	it('reads 1 type moved by you', async () => {
		expect(textOf(await densePicked(), 'split-label')).toBe('1 type moved by you');
	});

	it('draws the moved label amber', async () => {
		expect(tagOf(await densePicked(), 'split-label')).toMatch(/class="[^"]*\bmoved\b/);
	});

	it('enables Reset', async () => {
		expect(await densePicked()).not.toMatch(/<button class="btn[^"]*"[^>]*disabled/);
	});
});

describe('HarvestFlippingPage nothing to flip (reference 08 §7)', () => {
	const allFed = async () => {
		const fossil = (await loadHarvestFamilies('day')).families.find((f) => f.id === 'fossil')!;
		const picks = Object.fromEntries(fossil.weights!.types.map((t) => [t.itemId, 'reroll']));
		return renderWithPrefs({ picks: { fossil: picks } });
	};

	it('says No keepers: nothing to stop on.', async () => {
		expect(textOf(await allFed(), 'empty')).toBe('No keepers: nothing to stop on.');
	});

	it('prints the headline EV as —', async () => {
		expect(textOf(await allFed(), 'ev-big')).toBe('—');
	});

	it('mutes the — headline EV', async () => {
		expect(tagOf(await allFed(), 'ev-big')).toMatch(/class="[^"]*\bflat\b/);
	});

	it('prints no feeder under it', async () => {
		expect(textOf(await allFed(), 'per')).toBe('no feeder');
	});

	it('keeps the EV panel with — rows (prototype lines 451–456)', async () => {
		expect(await allFed()).toMatch(/Loop EV per feeder<\/dt>\s*<dd class="mono[^"]*">—</);
	});
});

describe('HarvestFlippingPage unpriced Hollow (reference 08 §5)', () => {
	const unpriced = () =>
		renderWithExchange((read) => ({
			...read,
			prices: { ...read.prices, [HOLLOW]: { chaos: null, divine: null, lastSeenChaos: 402 } }
		}));

	it('prints the floor note directly under Loop EV per feeder', async () => {
		expect(await unpriced()).toMatch(
			/Loop EV per feeder<\/dt>\s*<dd class="mono[^"]*">\+23\.7c<\/dd>\s*(?:<!--[^>]*-->\s*)*<dd class="small warn note[^"]*">1 unpriced type counted as 0c, so this EV is a floor\. At its last seen 402c it would read \+25\.7c\.</
		);
	});

	it('draws the floor note amber', async () => {
		expect(tagOf(await unpriced(), 'note')).toMatch(/class="[^"]*\bwarn\b/);
	});
});
