// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { mercenarySliceDefault, type MercCapture } from '$lib/mercenaries/capture';
import { MERC_SOURCES, SOURCE_IDS, type MercSourceId } from '$lib/mercenaries/rulesets';
import { columnLabel } from '$lib/mercenaries/ladder-view';
import { evaluateCapture } from '$lib/mercenaries/verdict';

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
		async (sourceId, rulesetId) => {
			await mountSource(sourceId, 'Covens');
			expect.soft(ruleSite(sourceId, rulesetId).saved).toBeNull();
			derivedQuery(sourceId, rulesetId, 'Covens');
		}
	);
});
