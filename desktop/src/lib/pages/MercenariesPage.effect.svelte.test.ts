// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { mercenarySliceDefault, type MercCapture } from '$lib/mercenaries/capture';
import { MERC_SOURCES, SOURCE_IDS, type MercSourceId } from '$lib/mercenaries/rulesets';
import { columnLabel } from '$lib/mercenaries/ladder-view';
import { evaluateCapture } from '$lib/mercenaries/verdict';
import guideGFrostBlades from '$lib/mercenaries/__fixtures__/guide-g-frost-blades.json';
import mercenaryStats from '$lib/mercenaries/__fixtures__/mercenary-stats.json';

const rust = vi.hoisted(() => ({ snapshot: {} as Record<string, unknown> }));
vi.mock('@tauri-apps/api/core', () => ({
	invoke: vi.fn(async (command: string) => {
		if (command === 'get_ssot') return rust.snapshot;
		throw new Error(`unexpected Tauri command: ${command}`);
	})
}));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

const CAPTURE: MercCapture = {
	capturedAtMs: 1_755_000_000_000,
	live: true,
	scale: 1,
	screen: [2560, 1440],
	header: { name: 'Fixture recruit', class: null, level: 70, wager: null },
	rows: [],
	rowsOnScreen: 0,
	rowsRead: 0
};

let { flushSync, mount, unmount } = await import('svelte');
let MercenariesPage: typeof import('./MercenariesPage.svelte').default;
let fetchSsot: typeof import('$lib/stores/ssot.svelte').fetchSsot;
let app: ReturnType<typeof mount> | null = null;

beforeEach(async () => {
	vi.resetModules();
	({ flushSync, mount, unmount } = await import('svelte'));
	MercenariesPage = (await import('./MercenariesPage.svelte')).default;
	({ fetchSsot } = await import('$lib/stores/ssot.svelte'));
});

afterEach(async () => {
	if (app) await unmount(app);
	app = null;
	document.body.innerHTML = '';
	rust.snapshot = {};
	vi.clearAllMocks();
	vi.useRealTimers();
});

async function settle(): Promise<void> {
	for (let i = 0; i < 10; i++) await Promise.resolve();
	flushSync();
}

async function mountSource(sourceId: MercSourceId, league: string): Promise<void> {
	rust.snapshot = {
		league: { name: league, generation: 1 },
		mercenary: { ...mercenarySliceDefault(), status: 'done', capture: CAPTURE }
	};
	app = mount(MercenariesPage, { target: document.body });
	await fetchSsot('initial');
	await settle();
	const label = MERC_SOURCES.find((source) => source.id === sourceId)!.label;
	const button = [...document.querySelectorAll<HTMLButtonElement>('.page-head button')]
		.find((candidate) => candidate.textContent?.trim() === label);
	if (!button) throw new Error(`missing source control: ${label}`);
	button.click();
	await settle();
}

function ruleSite(sourceId: MercSourceId, rulesetId: string) {
	const source = MERC_SOURCES.find((candidate) => candidate.id === sourceId)!;
	const rule = source.rulesets.find((candidate) => candidate.id === rulesetId)!;
	const article = [...document.querySelectorAll<HTMLElement>('article.card')].find((candidate) =>
		candidate.querySelector('h3')?.textContent?.trim() ===
			(rule.ladder ? `${rule.label} — tier ladder` : rule.label)
	);
	if (!article) throw new Error(`missing rendered rule: ${rulesetId}`);
	if (!rule.ladder) return { saved: article.querySelector<HTMLAnchorElement>('a[aria-label^="open saved search"]'), derived: article.querySelector<HTMLAnchorElement>('a[aria-label^="open derived search"]') };
	const columns = [...article.querySelectorAll<HTMLElement>('th.tier-col')];
	const index = columns.findIndex((column) => column.querySelector('.tier-name')?.textContent?.trim() === columnLabel(rule));
	if (index < 0) throw new Error(`missing rendered rung: ${rulesetId}`);
	return {
		saved: columns[index].querySelector<HTMLAnchorElement>('a[aria-label^="open saved search"]'),
		derived: article.querySelectorAll<HTMLElement>('.verdict-row td')[index]
			.querySelector<HTMLAnchorElement>('a[aria-label^="open derived search"]')
	};
}

function derivedQuery(sourceId: MercSourceId, rulesetId: string, league: string) {
	const href = ruleSite(sourceId, rulesetId).derived?.href ?? '';
	expect(new URL(href).pathname).toBe(`/trade/search/${encodeURIComponent(league)}`);
	const expected = evaluateCapture(CAPTURE, MERC_SOURCES, new Set(SOURCE_IDS), league)
		.sources.find((source) => source.id === sourceId)!
		.rulesets.find((rule) => rule.id === rulesetId)!.derivedUrl!;
	expect(JSON.parse(new URL(href).searchParams.get('q') ?? '')).toEqual(
		JSON.parse(new URL(expected).searchParams.get('q') ?? '')
	);
}

const SURVIVING = [
	['guide-a', 'guide-a-manyshot', 'WvKGjV8Kfm'],
	['guide-b', 'guide-b-frost-blades-mv', 'Kld4gv0Pi5']
] as const;

describe('MercenariesPage Covens source', () => {
	it('keep:happy selecting the Covens source displays its supplied Frost Blades search', async () => {
		rust.snapshot = {
			league: { name: 'Covens', generation: 1 },
			mercenary: { ...mercenarySliceDefault(), status: 'done', capture: CAPTURE }
		};
		app = mount(MercenariesPage, { target: document.body });
		await fetchSsot('initial');
		await settle();
		const label = 'Covens trade search (2026-11-27)';
		const button = [...document.querySelectorAll<HTMLButtonElement>('.page-head button')]
			.find((candidate) => candidate.textContent?.trim() === label);
		expect(button?.textContent?.trim(), 'the supplied source must be selectable').toBe(label);
		button!.click();
		await settle();

		const heading = [...document.querySelectorAll<HTMLElement>('.panel-head')]
			.find((header) => header.querySelector('h2')?.textContent?.trim() === label);
		expect(heading?.querySelector<HTMLAnchorElement>('a.guide-link')?.href).toBe(
			'https://www.pathofexile.com/trade/search/Covens/H4sIAAAAAAAACrWS7QqCMBiF7-X8lnBfbu5WQmLVgtGaojMI2b2HJUGg1h9_P8_ec96XDeiiiX0HPaBuoqsDNMzdOG-O3iJlL95B7wfER2NHGs7IcHE-2vYN3BkaN9uebDDtY9ddnfcHRlUukbIFXAgu2TKmvGQEqRqFKfej_JNOKcnF7Pi-aeo2HhjJBf0OCHX8MXp6KwpazK82CVSWMt-yPedKFasNFCerAlNCkjVBMMIFUpXhbnxvxw9ycwGapc22KtXqVQkXRM40oilV6QlDYJemyQIAAA'
		);
		const cards = document.querySelectorAll<HTMLElement>('article.card');
		const rules = [...cards].filter((card) => card.querySelector('a[aria-label^="open derived search"]'));
		expect(rules).toHaveLength(1);
		const card = rules[0];
		expect(card.querySelector('a[aria-label^="open saved search"]')).toBeNull();
		const href = card.querySelector<HTMLAnchorElement>('a[aria-label^="open derived search"]')!.href;
		expect(new URL(href).pathname).toBe('/trade/search/Covens');
		expect(JSON.parse(new URL(href).searchParams.get('q') ?? '')).toEqual({ query: guideGFrostBlades.query });

		const speedNames = ['mercenary.support_987', 'mercenary.support_14517'].map((id) =>
			mercenaryStats.entries.find((entry) => entry.id === id)!.text
		);
		const speed = [...card.querySelectorAll<HTMLElement>('.group')].find((group) =>
			speedNames.every((name) => [...group.querySelectorAll('.entry-name')]
				.some((entry) => entry.textContent?.trim() === name))
		);
		expect(speed?.querySelector('.off-badge')).toBeNull();
		expect(speed?.querySelectorAll('.entry-required .entry-name').length).toBe(3);
	});
});

describe('MercenariesPage guide-a price floors', () => {
	it.each([
		['guide-a-manyshot', 'Allflame price: 5d+ with just Return on both'],
		['guide-a-kinetist-v1', 'Allflame price: 15d+ with just Return'],
		['guide-a-combatant', 'Allflame price: 3d+ with just Return, much more with Multistrike/GMP']
	])('keep:happy %s displays its unchanged floor as an Allflame price', async (id, floor) => {
		await mountSource('guide-a', 'Covens');
		const rule = MERC_SOURCES.find((source) => source.id === 'guide-a')!
			.rulesets.find((candidate) => candidate.id === id)!;
		const article = [...document.querySelectorAll<HTMLElement>('article.card')].find(
			(candidate) => candidate.querySelector('h3')?.textContent?.trim() === rule.label
		);
		expect(article?.querySelector('.floor')?.textContent?.trim()).toBe(`Floor: ${floor}`);
	});
});

describe('MercenariesPage saved search anchors', () => {
	for (const [sourceId, rulesetId, hash] of SURVIVING) {
		it.each(['Covens', 'Hardcore Covens'])(
			`keep:regression ${rulesetId} renders its saved anchor in live %s`,
			async (league) => {
				await mountSource(sourceId, league);
				expect.soft(ruleSite(sourceId, rulesetId).saved?.href).toBe(
					`https://www.pathofexile.com/trade/search/${encodeURIComponent(league)}/${hash}`
				);
				derivedQuery(sourceId, rulesetId, league);
				expect([...document.querySelectorAll('.league-badge')].map((badge) => badge.textContent?.trim()))
					.toContain('saved in Allflame');
			}
		);

		it(`keep:regression ${rulesetId} removes its saved anchor on a live-to-null SSOT update`, async () => {
			await mountSource(sourceId, 'Covens');
			expect(ruleSite(sourceId, rulesetId).saved?.href).toMatch(new RegExp(`/${hash}$`));
			rust.snapshot = { ...rust.snapshot, league: { name: null, generation: 2 } };
			await fetchSsot('poll');
			await settle();
			expect(ruleSite(sourceId, rulesetId).saved).toBeNull();
		});
	}

	it.each([
		['guide-b', 'guide-b-frost-blades-mid', 'Kld4gM7yi5'],
		['guide-b', 'guide-b-kinetist-mv', '7nRvBzl2S5'],
		['guide-d', 'guide-d-kinetist-20d', '7nRvBzl2S5']
	] as const)(
		'keep:critical %s %s hides dead %s so a player cannot trust an unusable comp link',
		async (sourceId, rulesetId, _hash) => {
			await mountSource(sourceId, 'Covens');
			expect.soft(ruleSite(sourceId, rulesetId).saved).toBeNull();
			derivedQuery(sourceId, rulesetId, 'Covens');
		}
	);
});
