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
import type { ExchangePriceRead, HarvestFamilyData } from '$lib/harvest/seam';

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

	it('marks NO DATA on Astrolabes, Oils and Catalysts only', async () => {
		const html = await renderPricesPending();
		const marked = [
			...html.matchAll(/role="tab"[^>]*>([^<]*)(?:<!--[^>]*-->)?<span class="no-data-mark[^"]*">NO DATA</g)
		].map((m) => m[1].trim());
		expect(marked).toEqual(['Astrolabes', 'Oils', 'Catalysts']);
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
		expect(textOf(await renderPopulated(), 'per-divine')).toBe('1 div → 9,905 lifeforce → 330 rerolls');
	});

	it('shows the yield ~13.1 keepers', async () => {
		const html = await renderPopulated();
		expect(html).toMatch(/1 div of lifeforce yields<\/dt>\s*<dd class="mono[^"]*">~13\.1 keepers</);
	});
});
