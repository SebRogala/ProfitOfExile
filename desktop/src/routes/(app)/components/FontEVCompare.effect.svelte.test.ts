// @vitest-environment happy-dom
/**
 * FontEVCompare's Facetor's Lens card mounted in the client runtime under
 * happy-dom, so the Font fetch settles and the real Settings › Display switch
 * is clicked. External services are stubbed: the server is `fetch` answering
 * one Font payload for every variant, Rust is `invoke`, and every call each
 * records is a request or IPC write the app sent.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ calls: [] as [string, unknown][] }));

/** What the stubbed Rust answers the pages' mount reads with; everything else answers null. */
const ANSWERS: Record<string, unknown> = {
	get_proxy_settings: { saved: '', inEffect: '', savedTrade: true, tradeInEffect: true, updaterProxy: '', blocked: '' },
	get_ocr_rects: [],
	get_widget_geometries: [],
	get_webview_gpu_disabled: false
};

vi.mock('@tauri-apps/api/core', () => ({
	invoke: vi.fn(async (cmd: string, args?: unknown) => {
		ipc.calls.push([cmd, args]);
		return Object.hasOwn(ANSWERS, cmd) ? ANSWERS[cmd] : null;
	})
}));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock('@tauri-apps/api/app', () => ({ getVersion: async () => '0.0.0-test' }));
vi.mock('@tauri-apps/api/window', () => ({
	availableMonitors: async () => [],
	currentMonitor: async () => null,
	primaryMonitor: async () => ({ scaleFactor: 1 })
}));
vi.mock('@tauri-apps/api/webviewWindow', () => ({
	getCurrentWebviewWindow: () => ({ listen: async () => () => {} })
}));
vi.mock('$lib/updater/settings-update.svelte', async () => {
	const actual = await vi.importActual<typeof import('$lib/updater/settings-update.svelte')>(
		'$lib/updater/settings-update.svelte'
	);
	return {
		...actual,
		createSettingsUpdateController: (snapshot: Parameters<typeof actual.createSettingsUpdateController>[0]) =>
			actual.createSettingsUpdateController(snapshot, {
				checkForUpdate: async () => null,
				relaunch: async () => {}
			})
	};
});

import { buyBaseUrl, cheapestCorruptedTradeUrl, exceptionalSupportTradeUrl } from '$lib/trade-utils';
import buyButtonSource from '$lib/components/BuyButton.svelte?raw';
import tokensCss from '../../../tokens.css?raw';
import type { FontColor, FontEVResponse } from '$lib/api';

const LEAGUE = 'Probe League';
const LENS_URL = 'https://www.pathofexile.com/trade/search/Probe%20League?q=lens-stored-experience';
const TRADE_SERVER_URL = 'https://www.pathofexile.com/trade/search/Probe%20League?q=server-trade-feeder';
const ENHANCE_URL = 'https://www.pathofexile.com/trade/search/Probe%20League?q=enhance-lv3';
const ENLIGHTEN_URL = 'https://www.pathofexile.com/trade/search/Probe%20League?q=enlighten-lv3';
const EMPOWER_URL = 'https://www.pathofexile.com/trade/search/Probe%20League?q=empower-lv3';
const TRADE_NAME = 'Exceptional Support Gem lv 3+';
const FIXED_NAMES = [TRADE_NAME, 'Enhance Support lv 3', 'Enlighten Support lv 3', 'Empower Support lv 3'];

type Prices = [number | null, number | null, number | null, number | null];

/** The server's `lens` object with the four feeder choices, as the Go handler serves it. */
function feederLens(lens: number | null, prices: Prices, cheapest: number | null, evCheapest: number | null) {
	const at = new Date(Date.now() - 4 * 60_000).toISOString();
	const [trade, enhance, enlighten, empower] = prices;
	// The poe.ninja rows share one snapshot time, null when the league has no snapshot.
	const ninjaAt = [enhance, enlighten, empower].some((chaos) => chaos !== null) ? at : null;
	const choice = (source: string, name: string, chaos: number | null, tradeUrl: string) => ({
		source,
		name,
		chaos,
		fetchedAt: source === 'trade' ? (chaos === null ? null : at) : ninjaAt,
		tradeUrl
	});
	return {
		outcome: { chaos: lens, listings: 3, fetchedAt: at, lowConfidence: false, tradeUrl: LENS_URL },
		feeder: { chaos: trade, listings: 3, fetchedAt: trade === null ? null : at, lowConfidence: false, tradeUrl: TRADE_SERVER_URL },
		ev: lens !== null && trade !== null ? lens - trade : null,
		feeders: [
			choice('trade', TRADE_NAME, trade, TRADE_SERVER_URL),
			choice('poe.ninja', 'Enhance Support', enhance, ENHANCE_URL),
			choice('poe.ninja', 'Enlighten Support', enlighten, ENLIGHTEN_URL),
			choice('poe.ninja', 'Empower Support', empower, EMPOWER_URL)
		],
		cheapest,
		evCheapest
	};
}

/** Scenario 1's payload: Lens 12400c, feeders 30/12/20/7c, Empower cheapest, EV +12393c. */
const SCENARIO_1 = () => feederLens(12400, [30, 12, 20, 7], 3, 12393);

/** An older server's `lens` object: no `feeders`, `cheapest`, `evCheapest` or Lens search. */
function olderLens() {
	const at = new Date(Date.now() - 4 * 60_000).toISOString();
	return {
		outcome: { chaos: 412, listings: 3, fetchedAt: at, lowConfidence: false },
		feeder: { chaos: 95, listings: 2, fetchedAt: at, lowConfidence: false, tradeUrl: TRADE_SERVER_URL },
		ev: 317
	};
}

const server = { fontRequests: 0 };
const realFetch = globalThis.fetch;

/** The server answers every Font request with `lens`. */
function serveLens(lens: unknown): void {
	globalThis.fetch = vi.fn(async (url: string | URL) => {
		if (String(url).includes('/api/analysis/font')) server.fontRequests++;
		return {
			ok: true,
			status: 200,
			statusText: 'OK',
			json: async () =>
				structuredClone({ safe: [], premium: [], jackpot: [], best: null, lens })
		};
	}) as unknown as typeof fetch;
}

let { flushSync, mount, unmount } = await import('svelte');
let FontEVCompare: typeof import('./FontEVCompare.svelte').default;
let SettingsPage: typeof import('$lib/pages/SettingsPage.svelte').default;
let ssotStore: typeof import('$lib/stores/ssot.svelte');

// ssot.svelte.ts holds the all-feeders choice and the league at module level;
// fresh modules per case keep one case's click or snapshot out of the next.
beforeEach(async () => {
	vi.resetModules();
	({ flushSync, mount, unmount } = await import('svelte'));
	(await import('$lib/stores/server-base.svelte')).applyServerSlice({ url: 'https://server.test', defaultUrl: 'https://server.test' });
	ssotStore = await import('$lib/stores/ssot.svelte');
	ssotStore.applySnapshot({ league: { name: LEAGUE } });
	FontEVCompare = (await import('./FontEVCompare.svelte')).default;
	SettingsPage = (await import('$lib/pages/SettingsPage.svelte')).default;
});

const mounted: ReturnType<typeof mount>[] = [];

it.each([
	{ prefix: 'keep:critical WI-5 withheld EV', name: 'no covered cell', best: null, expected: [] },
	{ prefix: 'keep:happy WI-5 covered', name: 'served BLUE 1/20 despite larger RED 20/20', best: { variant: '1/20', color: 'BLUE' }, expected: ['1/20:BLUE'] }
])('$prefix: the BEST ring sits on exactly the served best cell ($name)', async ({ best, expected }) => {
	globalThis.fetch = vi.fn(async (input: string | URL | Request) => {
		const url = new URL(String(input));
		if (url.pathname !== '/api/analysis/font') throw new Error(`Unexpected request ${url.pathname}`);
		const variant = url.searchParams.get('variant');
		const rows = ['RED', 'GREEN', 'BLUE'].map((color) => {
			const ev = best === null ? null : variant === '20/20' && color === 'RED' ? 90 : 14;
			return { color, ev, evRaw: ev, profit: ev, pool: 88, pricedGems: best === null ? 0 : 88, coverage: best === null ? 0 : 1, winners: 0 };
		});
		return Response.json({ safe: rows, premium: rows, jackpot: rows, best });
	}) as typeof fetch;
	const target = document.body.appendChild(document.createElement('div'));
	flushSync(() => mounted.push(mount(FontEVCompare, { target, props: { labMode: 'normal' } })));
	await vi.waitFor(() => {
		flushSync();
		expect(target.querySelectorAll('table.ft tbody tr')).toHaveLength(4);
	});
	const cells = [...target.querySelectorAll('table.ft tbody tr')].flatMap((row) =>
		['RED', 'GREEN', 'BLUE'].map((color, index) => ({
			key: `${row.firstElementChild?.textContent?.trim()}:${color}`,
			cell: row.children[index + 1]
		}))
	);
	expect(cells.filter(({ cell }) => cell.matches('.w-red, .w-green, .w-blue')).map(({ key }) => key)).toEqual(expected);
	expect(cells.filter(({ cell }) => cell.querySelector('.best-red, .best-green, .best-blue')).map(({ key }) => key)).toEqual(expected);
	for (const { cell } of cells) {
		if (best === null) {
			// A plausible unsupported EV silently guides the player's choice.
			expect(cell.querySelector('.ev, .tier-lines')).toBeNull();
			expect(cell.textContent).not.toContain('/font');
			expect(cell.textContent).toContain('0 of 88 priced');
			expect(cell.textContent).toContain('too few for an EV');
		} else {
			expect(cell.textContent).toContain('88 of 88 priced');
			expect(cell.querySelector('.ev')?.textContent).toMatch(/(?:14|90)c\/font/);
			expect(cell.querySelector('.tier-lines')?.textContent).toContain('Safe');
			expect(cell.querySelector('.tier-lines')?.textContent).toContain('Premium');
		}
	}
});

afterEach(() => {
	while (mounted.length > 0) unmount(mounted.pop()!);
	document.head.querySelectorAll('style').forEach((style) => style.remove());
	document.body.innerHTML = '';
	globalThis.fetch = realFetch;
	server.fontRequests = 0;
	ipc.calls.length = 0;
});

/** Mounts the Font EV view, and Settings beside it when `withSettings`, then waits for the Font fetch to settle. */
async function mountCard(withSettings = false): Promise<void> {
	const fontTarget = document.body.appendChild(document.createElement('div'));
	mounted.push(mount(FontEVCompare, { target: fontTarget, props: { labMode: 'normal' } }));
	if (withSettings) {
		const settingsTarget = document.body.appendChild(document.createElement('div'));
		mounted.push(mount(SettingsPage, { target: settingsTarget }));
	}
	await vi.waitFor(() => {
		flushSync();
		if (!card().textContent?.includes('Costs') || card().textContent?.includes('pricing…')) {
			throw new Error('the Font response has not settled into the Lens card');
		}
	});
}

function card(): HTMLElement {
	const el = document.querySelector<HTMLElement>('article[aria-label="Facetor’s Lens · 60%"]');
	if (!el) throw new Error('the Facetor’s Lens card did not render');
	return el;
}

/** The BUY controls inside `root`, in display order: links, and disabled controls announced as such. */
function buyIn(root: Element): HTMLElement[] {
	return [...root.querySelectorAll<HTMLElement>('a[href], [aria-disabled="true"]')];
}

interface ShownRow {
	name: string;
	sub: string | null;
	price: string;
	buy: HTMLElement | null;
}

/** The card's rows under the group heading `group` ("Gives" or "Costs"), in display order. */
function rowsUnder(group: 'Gives' | 'Costs'): ShownRow[] {
	const rows: ShownRow[] = [];
	let current = '';
	for (const el of card().children) {
		if (el.classList.contains('rc-group')) current = el.textContent?.trim() ?? '';
		else if (el.classList.contains('rc-row') && current === group) {
			rows.push({
				name: el.querySelector('.rc-name > span')?.textContent?.trim() ?? '',
				sub: el.querySelector('.rc-name > small')?.textContent?.trim() ?? null,
				price: el.querySelector('.rc-num, .rc-nil')?.textContent?.trim() ?? '',
				buy: buyIn(el)[0] ?? null
			});
		}
	}
	return rows;
}

const costs = () => rowsUnder('Costs').map(({ name, sub, price }) => ({ name, sub, price }));

function fontRow(overrides: Partial<FontColor> = {}): FontColor {
	return {
		color: 'BLUE', ev: null, evRaw: null, profit: null, pool: 88,
		pricedGems: 0, coverage: 0, winners: 0, pWin: 0, evDelta2h: 0,
		poolBreakdown: [], lowConfidenceGems: [], ...overrides
	};
}

const cellText = (node: Element | null) => (node?.textContent ?? '').replace(/\s+/g, ' ').trim();

function fontCell(target: Element, variant: string, color: FontColor['color']): HTMLElement {
	const row = [...target.querySelectorAll('table.ft tbody tr')].find(
		(row) => row.firstElementChild?.textContent?.trim() === variant
	);
	const cell = row?.children[['RED', 'GREEN', 'BLUE'].indexOf(color) + 1];
	if (!cell) throw new Error('Font cell did not render: ' + variant + ' ' + color);
	return cell as HTMLElement;
}

const solidRows = [
	{ tier: 'LOW', count: 3, minPrice: 6, maxPrice: 6 },
	{ tier: 'MID', count: 2, minPrice: 11, maxPrice: 18 }
];
const riskyRows = [
	{ name: 'Fireball of Radiation', chaos: 35, listings: 1 },
	{ name: 'Storm Burst of Repulsion', chaos: 527, listings: 1 },
	{ name: 'Ice Spear of Splitting', chaos: 170, listings: 2 }
];

it.each([
	{
		name: 'SOLID-only', variant: '20/0', row: fontRow({ pricedGems: 12, poolBreakdown: solidRows }),
		expected: ['MID 2 · 11c — 18c', 'LOW 3 · 6c'], groups: ['SOLID'], hidden: 0
	},
	{
		name: 'RISKY-only', variant: '20/20', row: fontRow({ lowConfidenceGems: riskyRows }),
		expected: ['2.1d Storm Burst of Repulsion', '170c Ice Spear of Splitting', '35c Fireball of Radiation'],
		groups: ['RISKY'], hidden: 0
	},
	{
		name: 'mixed', variant: '20/0',
		row: fontRow({
			color: 'GREEN', pool: 94, pricedGems: 19,
			poolBreakdown: [
				{ tier: 'LOW', count: 4, minPrice: 6, maxPrice: 9 },
				{ tier: 'MID', count: 3, minPrice: 12, maxPrice: 30 }
			],
			lowConfidenceGems: [
				{ name: 'Sunder of Earthbreaking', chaos: 25, listings: 1 },
				{ name: 'Lacerate of Butchering', chaos: 49, listings: 1 },
				{ name: 'Molten Strike of the Zenith', chaos: 50, listings: 1 }
			]
		}),
		expected: ['MID 3 · 12c — 30c', 'LOW 4 · 6c — 9c', '50c Molten Strike of the Zenith', '49c Lacerate of Butchering'],
		groups: ['SOLID', 'RISKY'], hidden: 1
	}
])('keep:happy WI-5 raw prices: $name displays ordered rows with a complete keyboard-accessible list', async ({ variant, row, expected, groups, hidden }) => {
	const target = await mountFontFixture(row, variant);
	const cell = fontCell(target, variant, row.color);
	expect(cellText(cell)).toContain(row.pricedGems + ' of ' + row.pool + ' priced');
	expect(cellText(cell)).toContain('too few for an EV');
	expect(cell.querySelector('.ev, .tier-lines')).toBeNull();
	expect(cell.matches('.w-red, .w-green, .w-blue')).toBe(false);
	expect([...cell.querySelectorAll('.uf-row')].map((node) => [...node.children].map(cellText).join(' '))).toEqual(expected);
	expect([...cell.querySelectorAll('.uf-group')].map((node) => cellText(node).toUpperCase())).toEqual(groups);
	expect(cellText(cell.querySelector('.uf-more'))).toBe(hidden ? '+' + hidden + ' more' : '');
	const tip = await openFontInfo(cell);
	expect(cellText(tip)).toContain('Listed prices, ' + row.color + ' ' + variant);
	expect(cellText(tip)).toContain(groups[0]);
	for (const tier of row.poolBreakdown ?? []) {
		expect(cellText(tip)).toContain(tier.tier + ': ' + tier.count + ' gems');
	}
	for (const gem of row.lowConfidenceGems ?? []) {
		expect(cellText(tip)).toContain(gem.name + ': ');
		expect(cellText(tip)).toContain('(' + gem.listings + (gem.listings === 1 ? ' listing)' : ' listings)'));
	}
	if (groups.includes('SOLID')) expect(cellText(tip)).toContain('enough listings to trust');
	if (groups.includes('RISKY')) expect(cellText(tip)).toContain('too few listings to trust');
	if (row.lowConfidenceGems?.some((gem) => gem.chaos === 527)) {
		expect(cellText(tip)).toContain('Storm Burst of Repulsion: 2.1d (1 listing)');
		expect(cellText(tip)).toContain('Ice Spear of Splitting: 170c (2 listings)');
	}
	if (hidden) expect(cellText(tip)).toContain('Sunder of Earthbreaking: 25c (1 listing)');
});

it('keep:happy WI-5 cold: absent lists explain the next update without a cell info control', async () => {
	const row = fontRow({ poolBreakdown: undefined, lowConfidenceGems: undefined });
	const target = await mountFontFixture(row);
	const cell = fontCell(target, '20/20', 'BLUE');
	expect(cellText(cell)).toContain('0 of 88 priced');
	expect(cellText(cell)).toContain('too few for an EV');
	expect(cellText(cell)).toContain('listed prices load on the next update');
	expect(cell.querySelector('[role="button"], .uf-list')).toBeNull();
	expect(cellText(cell)).not.toContain('no listed prices');
});

it('keep:happy WI-5 empty lists: a nonzero solid numerator can have no listed prices', async () => {
	const target = await mountFontFixture(fontRow({ color: 'RED', pool: 96, pricedGems: 9 }));
	const cell = fontCell(target, '20/20', 'RED');
	expect(cellText(cell)).toContain('9 of 96 priced');
	expect(cellText(cell)).toContain('too few for an EV');
	expect(cellText(cell)).toContain('no listed prices');
	expect(cell.querySelector('[role="button"], .uf-list')).toBeNull();
	expect(cellText(cell)).not.toContain('listed prices load on the next update');
});

it('keep:regression WI-5 zero EV: a covered zero is 0c/font without a BEST ring', async () => {
	const target = await mountFontFixture(fontRow({
		color: 'GREEN', pool: 94, pricedGems: 58, coverage: 58 / 94,
		ev: 0, evRaw: 0, profit: 0, topEvaluated: true
	}), '1/0');
	const cell = fontCell(target, '1/0', 'GREEN');
	expect(cellText(cell.querySelector('.ev'))).toBe('0c/font');
	expect(cellText(cell)).toContain('58 of 94 priced');
	expect(cell.matches('.w-red, .w-green, .w-blue')).toBe(false);
	expect(cell.querySelector('.best-red, .best-green, .best-blue')).toBeNull();
	expect(cellText(cell)).not.toContain('too few for an EV');
	expect([...cell.querySelectorAll('.tier-row')].map((node) => [...node.children].map(cellText).join(' '))).toEqual(['Safe — none', 'Premium — none', 'Jackpot — none']);
	expect(cellText(fontCell(target, '1/0', 'RED'))).toBe('—');
});

it.each([
	{ name: 'winners', winners: 1, topEvaluated: true, expected: '1 in 40 (2%) ~310c', tip: 'Smite of Divine Judgement' },
	{ name: 'evaluated empty', winners: 0, topEvaluated: true, expected: '— none', tip: null },
	{ name: 'insufficient', winners: 0, topEvaluated: false, expected: '— not enough data', tip: 'Too few solid gems at 1/20 to look for TOP-tier outliers.' },
	{ name: 'cold empty', winners: 0, topEvaluated: undefined, expected: null, tip: null },
	{ name: 'cold winners', winners: 1, topEvaluated: undefined, expected: '1 in 40 (2%) ~310c', tip: 'Smite of Divine Judgement' }
])('keep:happy WI-5 Jackpot: $name shows its evaluation state', async ({ winners, topEvaluated, expected, tip }) => {
	const safe = fontRow({ color: 'RED', pool: 96, pricedGems: 44, ev: 13, evRaw: 13, profit: 13, coverage: 44 / 96 });
	const jackpot = { ...safe, winners, topEvaluated, pWin: 0.02, fontsToHit: 40, avgWinRaw: 310,
		jackpotGems: [{ name: 'Smite of Divine Judgement', chaos: 310 }] };
	const target = await mountFontFixture(safe, '1/20', jackpot);
	const cell = fontCell(target, '1/20', 'RED');
	const line = [...cell.querySelectorAll<HTMLElement>('.tier-row')].find((node) => node.querySelector('.tier-label')?.textContent === 'Jackpot');
	if (expected === null) {
		expect(line).toBeUndefined();
	} else {
		expect(cellText(line?.querySelector('.tier-val') ?? null)).toBe(expected);
		if (tip) expect(cellText(await openFontInfo(line!))).toContain(tip);
		else expect(line?.querySelector('[role="button"]')).toBeNull();
	}
});

it.each([
	{ name: 'covered partial', pricedGems: 27, evRaw: 11, expected: '27 of 96 priced',
		tip: 'Only 27 of 96 RED gems have solid price evidence at 20/20; the rest count as 0c, so the EV is a floor.', under: false },
	{ name: 'under-floor nonzero', pricedGems: 9, evRaw: null, expected: '9 of 96 priced',
		tip: 'Only 9 of 96 RED gems have solid price evidence at 20/20: too few for an EV.', under: true },
	{ name: 'under-floor zero', pricedGems: 0, evRaw: null, expected: '0 of 96 priced',
		tip: 'None of the 96 RED gems have solid price evidence at 20/20: too few for an EV.', under: true },
	{ name: 'full pool', pricedGems: 96, evRaw: 11, expected: '96 gems', tip: null, under: false }
])('keep:happy WI-5 Pool badge: $name displays its count with the matching explanation', async ({ pricedGems, evRaw, expected, tip, under }) => {
	const target = await mountFontFixture(fontRow({ color: 'RED', pool: 96, pricedGems, evRaw, ev: evRaw, coverage: pricedGems / 96 }));
	const toggle = [...target.querySelectorAll<HTMLButtonElement>('button')].find((button) => button.textContent?.includes('Pool Overview'))!;
	if (target.querySelector('.pool-section')) { toggle.click(); flushSync(); }
	toggle.click();
	flushSync();
	const badge = target.querySelector<HTMLElement>('.pool-color-card .pool-count')!;
	expect(cellText(badge)).toBe(expected);
	expect(badge.classList.contains('pool-count-under')).toBe(under);
	expect(badge.classList.contains('pool-count-partial')).toBe(pricedGems < 96);
	if (tip) expect(await poolBadgeExplanation(badge)).toBe(tip);
	else expect(badge.getAttribute('title')).toBeNull();
});

async function mountFontFixture(safe: FontColor, variant = '20/20', jackpot: FontColor = { ...safe, winners: 0, topEvaluated: true }): Promise<HTMLElement> {
	(await import('$lib/price.svelte')).setDivineRate(250);
	ssotStore.applySnapshot({ league: { name: LEAGUE }, normalVariant: '20/20' });
	globalThis.fetch = vi.fn(async (input: string | URL | Request) => {
		const url = new URL(String(input));
		if (url.pathname !== '/api/analysis/font') throw new Error('Unexpected request ' + url.pathname);
		const selected = url.searchParams.get('variant') === variant;
		const payload: FontEVResponse = { safe: selected ? [safe] : [], premium: selected ? [safe] : [], jackpot: selected ? [jackpot] : [], best: null, lens: null };
		return Response.json(payload);
	}) as typeof fetch;
	const target = document.body.appendChild(document.createElement('div'));
	flushSync(() => mounted.push(mount(FontEVCompare, { target, props: { labMode: 'normal' } })));
	await vi.waitFor(() => {
		flushSync();
		expect(target.querySelectorAll('table.ft tbody tr')).toHaveLength(4);
	});
	return target;
}

async function openFontInfo(root: Element): Promise<HTMLElement> {
	const trigger = root.querySelector<HTMLElement>('.icon-circle[role="button"]');
	expect(trigger, 'the cell info control is keyboard reachable').not.toBeNull();
	expect(trigger?.tabIndex).toBe(0);
	trigger!.focus();
	flushSync();
	const panel = document.getElementById(trigger!.getAttribute('aria-describedby') ?? '');
	expect(panel?.getAttribute('role')).toBe('tooltip');
	expect(panel?.style.visibility).toBe('visible');
	return panel!;
}

async function poolBadgeExplanation(badge: HTMLElement): Promise<string> {
	badge.closest('.tooltip-wrap')!.dispatchEvent(new MouseEvent('mouseenter'));
	flushSync();
	return cellText([...document.querySelectorAll<HTMLElement>('[role="tooltip"]')].find((tip) => tip.style.visibility === 'visible') ?? null);
}

const costBuys = () => rowsUnder('Costs').map((r) => r.buy);
const lensBuy = () => rowsUnder('Gives')[0]?.buy ?? null;

function ev(): { value: string; sub: string; negative: boolean } {
	const block = card().querySelector('.rc-ev');
	const value = block?.querySelector('.rc-ev-value, .rc-nil');
	return {
		value: value?.textContent?.trim() ?? '',
		sub: block?.querySelector('small')?.textContent?.trim() ?? '',
		negative: value?.classList.contains('negative') ?? false
	};
}

const footerSource = () => card().querySelector('.rc-source')?.textContent?.trim() ?? '';

/** The text a player reads when pointing at `buy`: its Tooltip popup, else its title. */
function tooltipOf(buy: HTMLElement | null): string {
	if (!buy) throw new Error('no BUY to point at');
	if (!buy.closest('.tooltip-wrap')) return buy.getAttribute('title') ?? '';
	buy.dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
	flushSync();
	const tip = document.getElementById(buy.getAttribute('aria-describedby') ?? '');
	buy.dispatchEvent(new FocusEvent('focusout', { bubbles: true }));
	return (tip?.textContent ?? '').replaceAll('’', "'");
}

function allFeedersSwitch(): HTMLButtonElement {
	const row = [...document.querySelectorAll<HTMLElement>('.setting-row')].find(
		(r) => r.querySelector('.setting-label')?.textContent?.replaceAll('’', "'").trim() === "Facetor's Lens: all feeders"
	);
	const toggle = row?.querySelector<HTMLButtonElement>('button[role="switch"]');
	if (!toggle) throw new Error('the "Facetor\'s Lens: all feeders" switch did not render');
	return toggle;
}

async function clickAllFeeders(): Promise<void> {
	allFeedersSwitch().click();
	flushSync();
	for (let i = 0; i < 10; i++) await Promise.resolve();
	flushSync();
}

const writes = () => ipc.calls.filter(([cmd]) => cmd === 'set_show_all_lens_feeders').map(([, args]) => args);

const EXPANDED_SCENARIO_1 = [
	{ name: TRADE_NAME, sub: 'trade', price: '30c' },
	{ name: 'Enhance Support lv 3', sub: 'poe.ninja', price: '12c' },
	{ name: 'Enlighten Support lv 3', sub: 'poe.ninja', price: '20c' },
	{ name: 'Empower Support lv 3', sub: 'poe.ninja · cheapest', price: '7c' }
];

describe('FontEVCompare Lens card with the four feeder choices', () => {
	// keep:happy
	it('settles on the cheapest feeder alone, the lens − cheapest feeder EV and the trade · poe.ninja footer', async () => {
		serveLens(SCENARIO_1());
		await mountCard();

		expect({ costs: costs(), ev: ev(), source: footerSource() }).toEqual({
			costs: [{ name: 'Empower Support lv 3', sub: 'poe.ninja · cheapest of 4', price: '7c' }],
			ev: { value: '+12393c', sub: 'lens − cheapest feeder', negative: false },
			source: 'trade · poe.ninja'
		});
	});

	// keep:happy
	it('collapsed, the Lens BUY opens the server Lens search and the Empower BUY its server search', async () => {
		serveLens(SCENARIO_1());
		await mountCard();

		expect([lensBuy()?.getAttribute('href'), costBuys()[0]?.getAttribute('href')]).toEqual([LENS_URL, EMPOWER_URL]);
	});

	// keep:happy
	it('clicking the Display switch shows the four fixed-order feeders with an unchanged EV and writes the choice', async () => {
		serveLens(SCENARIO_1());
		await mountCard(true);

		await clickAllFeeders();

		expect({ costs: costs(), ev: ev().value, writes: writes() }).toEqual({
			costs: EXPANDED_SCENARIO_1,
			ev: '+12393c',
			writes: [{ show: true }]
		});
	});

	// keep:happy
	it('expanded, the trade BUY opens the client-built league search and each poe.ninja BUY its own server search', async () => {
		serveLens(SCENARIO_1());
		await mountCard(true);
		await clickAllFeeders();

		const [trade, ...ninja] = costBuys();
		const tradeHref = decodeURIComponent(trade?.getAttribute('href') ?? '');
		expect(tradeHref).toContain(`/trade/search/${LEAGUE}?q=`);
		expect(tradeHref).toContain('"gem.supportgem"');
		expect(ninja.map((b) => b?.getAttribute('href'))).toEqual([ENHANCE_URL, ENLIGHTEN_URL, EMPOWER_URL]);
		expect(lensBuy()?.getAttribute('href')).toBe(LENS_URL);
	});

	// keep:happy
	it('expanded, each BUY tooltip names what its search buys', async () => {
		serveLens(SCENARIO_1());
		await mountCard(true);
		await clickAllFeeders();

		expect([lensBuy(), ...costBuys()].map(tooltipOf)).toEqual([
			"Buy Facetor's Lens · stored experience ≥ 999,627,000",
			'Buy the cheapest Exceptional Support Gem lv 3+',
			'Buy Enhance Support lv 3 · any corruption',
			'Buy Enlighten Support lv 3 · any corruption',
			'Buy Empower Support lv 3 · any corruption'
		]);
	});

	// keep:happy
	it('a persisted on choice opens a fresh mount expanded', async () => {
		ssotStore.applySnapshot({ league: { name: LEAGUE }, showAllLensFeeders: true });
		serveLens(SCENARIO_1());
		await mountCard();

		expect(costs()).toEqual(EXPANDED_SCENARIO_1);
	});

	// keep:happy
	it('switching the choice off collapses the card to the cheapest row without refetching', async () => {
		ssotStore.applySnapshot({ league: { name: LEAGUE }, showAllLensFeeders: true });
		serveLens(SCENARIO_1());
		await mountCard(true);
		const requestsBefore = server.fontRequests;

		await clickAllFeeders();

		expect({ costs: costs(), requests: server.fontRequests, writes: writes() }).toEqual({
			costs: [{ name: 'Empower Support lv 3', sub: 'poe.ninja · cheapest of 4', price: '7c' }],
			requests: requestsBefore,
			writes: [{ show: false }]
		});
	});

	// keep:happy
	it('with no league detected only the trade BUY is disabled; the Lens and poe.ninja BUYs stay links', async () => {
		ssotStore.applySnapshot({ league: { name: null }, showAllLensFeeders: true });
		serveLens(SCENARIO_1());
		await mountCard();

		const [trade, ...ninja] = costBuys();
		expect({
			trade: [trade?.tagName, trade?.getAttribute('aria-disabled'), trade?.getAttribute('title')],
			ninja: ninja.map((b) => b?.getAttribute('href')),
			lens: lensBuy()?.getAttribute('href')
		}).toEqual({
			trade: ['SPAN', 'true', 'League not detected — trade link unavailable'],
			ninja: [ENHANCE_URL, ENLIGHTEN_URL, EMPOWER_URL],
			lens: LENS_URL
		});
	});

	// keep:happy
	it.each([
		{ showAll: false, rows: [{ name: TRADE_NAME, sub: 'cheapest of 4', price: 'no price' }] },
		{
			showAll: true,
			rows: [
				{ name: TRADE_NAME, sub: 'trade', price: 'no price' },
				{ name: 'Enhance Support lv 3', sub: 'poe.ninja', price: 'no price' },
				{ name: 'Enlighten Support lv 3', sub: 'poe.ninja', price: 'no price' },
				{ name: 'Empower Support lv 3', sub: 'poe.ninja', price: 'no price' }
			]
		}
	])('with every feeder unpriced (setting $showAll) the rows read no price and the EV asks for a feeder price', async ({ showAll, rows }) => {
		ssotStore.applySnapshot({ league: { name: LEAGUE }, showAllLensFeeders: showAll });
		serveLens(feederLens(12400, [null, null, null, null], null, null));
		await mountCard();

		expect({ costs: costs(), ev: ev().value }).toEqual({ costs: rows, ev: '— needs a feeder price' });
	});

	// keep:happy
	it('with only the trade feeder priced the trade row is cheapest and the poe.ninja rows read no price', async () => {
		ssotStore.applySnapshot({ league: { name: LEAGUE }, showAllLensFeeders: true });
		serveLens(feederLens(12400, [30, null, null, null], 0, 12370));
		await mountCard();

		expect({ costs: costs(), ev: ev().value }).toEqual({
			costs: [
				{ name: TRADE_NAME, sub: 'trade · cheapest', price: '30c' },
				{ name: 'Enhance Support lv 3', sub: 'poe.ninja', price: 'no price' },
				{ name: 'Enlighten Support lv 3', sub: 'poe.ninja', price: 'no price' },
				{ name: 'Empower Support lv 3', sub: 'poe.ninja', price: 'no price' }
			],
			ev: '+12370c'
		});
	});

	// keep:happy
	it('an unpriced Lens keeps its BUY to the server Lens search and the EV asks for the lens price', async () => {
		serveLens(feederLens(null, [30, 12, 20, 7], 3, null));
		await mountCard();

		expect({ lens: rowsUnder('Gives')[0]?.price, href: lensBuy()?.getAttribute('href'), ev: ev().value }).toEqual({
			lens: 'no price',
			href: LENS_URL,
			ev: '— needs the lens price'
		});
	});

	// keep:happy
	it('a negative cheapest EV renders signed in the negative style', async () => {
		serveLens(feederLens(5, [30, 12, 20, 7], 3, -2));
		await mountCard();

		expect(ev()).toEqual({ value: '−2c', sub: 'lens − cheapest feeder', negative: true });
	});

	// keep:happy
	it.each([
		{ name: 'all priced, expanded', showAll: true, lens: SCENARIO_1 },
		{ name: 'none priced, collapsed', showAll: false, lens: () => feederLens(12400, [null, null, null, null], null, null) }
	])('$name: the card shows no corrupted tag, extra cheapest badge or "nothing to buy" text', async ({ showAll, lens }) => {
		ssotStore.applySnapshot({ league: { name: LEAGUE }, showAllLensFeeders: showAll });
		serveLens(lens());
		await mountCard();

		const text = card().textContent?.toLowerCase() ?? '';
		const costsText = costs().map((r) => `${r.name} ${r.sub ?? ''} ${r.price}`).join(' ').toLowerCase();
		expect({
			corrupted: text.includes('corrupt'),
			nothingToBuy: text.includes('nothing to buy'),
			cheapestMarks: costsText.match(/cheapest/g)?.length ?? 0,
			rows: costs().length
		}).toEqual({ corrupted: false, nothingToBuy: false, cheapestMarks: 1, rows: showAll ? 4 : 1 });
	});
});

describe('FontEVCompare Lens card against an older server', () => {
	// keep:regression
	it('keeps the one trade row, server ev, median footer and no Lens BUY with the switch flipped both ways', async () => {
		serveLens(olderLens());
		await mountCard(true);
		const shown = () => ({
			costs: costs(),
			ev: ev(),
			source: footerSource(),
			lensBuy: lensBuy()
		});
		const legacy = {
			costs: [{ name: TRADE_NAME, sub: 'trade', price: '95c' }],
			ev: { value: '+317c', sub: 'lens − support', negative: false },
			source: 'trade · median of the 3 lowest',
			lensBuy: null
		};

		const off = shown();
		await clickAllFeeders();
		const on = shown();
		await clickAllFeeders();

		expect([off, on, shown()]).toEqual([legacy, legacy, legacy]);
	});
});

/**
 * The shared BuyButton's stylesheet and the desktop tokens, added to the
 * document so a control's colour and width read as computed style.
 */
function addBuyStyles(): void {
	const style = document.createElement('style');
	style.textContent = tokensCss + (buyButtonSource.match(/<style[^>]*>([\s\S]*?)<\/style>/)?.[1] ?? '');
	document.head.append(style);
}

const tokenColour = (name: string) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();

const LEAGUE_MISSING = 'League not detected — trade link unavailable';
const COLOURS = ['RED', 'GREEN', 'BLUE'] as const;

interface ExpectedBuy {
	label: string;
	href: string | null;
	/** The colour token a link reads in; `buy` is the neutral action ink. */
	colour: 'red' | 'green' | 'blue' | 'buy';
	/** The accessible name; gem buys carry none of their own. */
	name: string | null;
	tooltip: string;
}

/** A link with its Tooltip when the helper built a search, else the disabled control with the league-missing title. */
function buy(label: string, href: string | null, colour: ExpectedBuy['colour'], tooltip: string, named = false): ExpectedBuy {
	return href
		? { label, href, colour, name: named ? tooltip : null, tooltip }
		: { label, href: null, colour, name: named ? LEAGUE_MISSING : null, tooltip: LEAGUE_MISSING };
}

const baseBuys = (league: string | null) =>
	['20/0', '20/20'].flatMap((variant) =>
		COLOURS.map((c) => buy(c, buyBaseUrl(variant, c, league), c.toLowerCase() as ExpectedBuy['colour'], `Buy cheapest ${c} base gem (${variant})`))
	);

const inputBuys = (league: string | null) =>
	['21/20', '21/23'].flatMap((variant) =>
		[false, true].flatMap((transfigured) =>
			COLOURS.map((c) =>
				buy(
					c,
					cheapestCorruptedTradeUrl(c, transfigured, league, variant),
					c.toLowerCase() as ExpectedBuy['colour'],
					`Buy cheapest corrupted ${variant} ${c} ${transfigured ? 'transfigured ' : ''}gems`
				)
			)
		)
	);

const lensBuys = (league: string | null) => [
	buy('BUY', LENS_URL, 'buy', "Buy Facetor's Lens · stored experience ≥ 999,627,000", true),
	buy('BUY', exceptionalSupportTradeUrl(league), 'buy', 'Buy the cheapest Exceptional Support Gem lv 3+', true),
	buy('BUY', ENHANCE_URL, 'buy', 'Buy Enhance Support lv 3 · any corruption', true),
	buy('BUY', ENLIGHTEN_URL, 'buy', 'Buy Enlighten Support lv 3 · any corruption', true),
	buy('BUY', EMPOWER_URL, 'buy', 'Buy Empower Support lv 3 · any corruption', true)
];

/** The BUY controls of the Font or Dedication table's Buy column, row by row. */
function tableBuys(): HTMLElement[] {
	const header = [...document.querySelectorAll('th')].find((th) => ['Buy Bases', 'Buy Input'].includes(th.textContent?.trim() ?? ''));
	if (!header) throw new Error('the EV table with its Buy column did not render');
	return [...header.closest('table')!.querySelectorAll('tbody tr')].flatMap((tr) => (tr.lastElementChild ? buyIn(tr.lastElementChild) : []));
}

async function mountDedication(): Promise<void> {
	const target = document.body.appendChild(document.createElement('div'));
	mounted.push(mount(FontEVCompare, { target, props: { labMode: 'dedication' } }));
	await vi.waitFor(() => {
		flushSync();
		tableBuys();
	});
}

describe('FontEVCompare BUY controls through the shared BuyButton', () => {
	// A wrong destination is silent: the player opens a plausible trade search
	// for the wrong colour, variant or feeder and buys the wrong input.
	it.each([
		{ name: 'Font with the league detected', labMode: 'normal', league: LEAGUE },
		{ name: 'Font with no league', labMode: 'normal', league: null },
		{ name: 'Dedication with the league detected', labMode: 'dedication', league: LEAGUE },
		{ name: 'Dedication with no league', labMode: 'dedication', league: null }
	] as const)(
		'keep:critical: $name, every BUY shows its label, trade-helper href, name and unchanged tooltip',
		async ({ labMode, league }) => {
			ssotStore.applySnapshot({ league: { name: league }, showAllLensFeeders: true });
			serveLens(SCENARIO_1());
			if (labMode === 'normal') await mountCard();
			else await mountDedication();

			const controls = labMode === 'normal' ? [...tableBuys(), lensBuy(), ...costBuys()] : tableBuys();
			const actual = controls.map((el) =>
				el ? { label: el.textContent?.trim() ?? '', href: el.getAttribute('href'), name: el.getAttribute('aria-label'), tooltip: tooltipOf(el) } : null
			);
			const wanted = labMode === 'normal' ? [...baseBuys(league), ...lensBuys(league)] : inputBuys(league);

			expect(actual).toEqual(wanted.map(({ label, href, name, tooltip }) => ({ label, href, name, tooltip })));
		}
	);

	// Gem buys keep their colour while the Lens and feeder buys turn neutral, so
	// the RED/GREEN/BLUE colour still tells the player which gem colour a search buys.
	it.each([
		{ name: 'Font', labMode: 'normal' },
		{ name: 'Dedication', labMode: 'dedication' }
	] as const)('keep:regression: $name gem buys keep their gem colour and the Lens and feeder buys read in the action ink', async ({ labMode }) => {
		ssotStore.applySnapshot({ league: { name: LEAGUE }, showAllLensFeeders: true });
		serveLens(SCENARIO_1());
		if (labMode === 'normal') await mountCard();
		else await mountDedication();
		addBuyStyles();

		const controls = labMode === 'normal' ? [...tableBuys(), lensBuy(), ...costBuys()] : tableBuys();
		const actual = controls.map((el) => (el ? { label: el.textContent?.trim() ?? '', colour: getComputedStyle(el).color, width: getComputedStyle(el).width } : null));
		const wanted = labMode === 'normal' ? [...baseBuys(LEAGUE), ...lensBuys(LEAGUE)] : inputBuys(LEAGUE);

		expect({ actionInk: tokenColour('--color-lab-buy'), controls: actual }).toEqual({
			actionInk: '#e4e4e7',
			controls: wanted.map(({ label, colour }) => ({ label, colour: tokenColour(`--color-lab-${colour}`), width: '56px' }))
		});
	});
});
