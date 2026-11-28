// @vitest-environment happy-dom
/**
 * The Rankings table's expanded gem row (POE-480), mounted through BestPlays'
 * public `plays` prop in the client runtime. A test clicks the real row and
 * reads what the player reads: the Buy, Sell and Market cards and the Signal
 * history beside them. History is controlled at the `fetchSignalHistory` API
 * boundary; the league is the ssot store's, the server url the server-base
 * store's. Numbers follow the approved design PNGs.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';
import type { GemPlay, SignalTransition } from '$lib/api';
import type { TradeListingDetail, TradeLookupResult } from '$lib/trade-types';
import { invoke } from '@tauri-apps/api/core';

const history = vi.hoisted(() => ({ fetch: vi.fn<(name: string, variant: string) => Promise<SignalTransition[]>>() }));

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock('$lib/api', async (importOriginal) => ({
	...(await importOriginal<typeof import('$lib/api')>()),
	fetchSignalHistory: history.fetch
}));

import { getApiBase } from '$lib/api';
import { resetServerBaseForTests, applyServerSlice } from '$lib/stores/server-base.svelte';
import { applySnapshot } from '$lib/stores/ssot.svelte';
import { setDivineRate } from '$lib/price.svelte';
import { baseGemTradeUrl } from '$lib/trade-utils';
import { CHAOS_ID, DIVINE_ID, currencyIconPath, iconSrc } from '$lib/exchange/view';
import BestPlays from './BestPlays.svelte';

const LEAGUE = 'Probe League';
/** One rate for every example: 412c stays chaos, 5.1d and 3.4d read in divines. */
const DIVINE_RATE = 230;
const GCP_ID = 'Metadata/Items/Currency/CurrencyGemQuality';

function play(overrides: Partial<GemPlay>): GemPlay {
	return {
		name: 'Cleave of Rage',
		baseName: 'Cleave',
		variant: '20/20',
		color: 'RED',
		confidence: 'OK',
		roi: 318,
		roiPercent: 338,
		weightedRoi: 300,
		signal: 'HERD',
		cv: 0,
		windowSignal: 'CLOSED',
		advancedSignal: '',
		liquidityTier: 'MED',
		transListings: 57,
		transVelocity: 0,
		baseListings: 20,
		baseVelocity: 9,
		basePrice: 94,
		transPrice: 412,
		sparkline: [],
		sparklineListings: [],
		signalHistory: [],
		priceTier: 'HIGH',
		lowConfidence: false,
		tierAction: '',
		sellUrgency: '',
		sellReason: '',
		sellability: 48,
		sellabilityLabel: 'MODERATE',
		low7d: 0,
		high7d: 0,
		histPosition: 0,
		sellConfidence: 'FAIR',
		tradeConfidenceNote: '',
		gcpRecipeCost: 0,
		gcpRecipeBase: 0,
		gcpRecipeSaves: 0,
		...overrides
	};
}

function hist(time: string, from: string, to: string, reason: string, listings: number): SignalTransition {
	return { time, from, to, reason, listings };
}

// --- Design fixtures (typical, hold, sell-now, gcp-costs-more, no-verdict, no-buy-links, sparse) ---

const TYPICAL = play({
	gcpRecipeCost: 72,
	gcpRecipeBase: 28,
	gcpRecipeSaves: 22,
	sellUrgency: 'UNDERCUT',
	sellReason: 'HERD at historical peak — undercut 10-15% NOW before crash',
	tierAction: 'UNDERCUT — herd arrived, sell into pressure'
});
const TYPICAL_HISTORY = [
	hist('15:35', 'UNCERTAIN', 'STABLE', '-6c', 48),
	hist('16:05', 'STABLE', 'HERD', '+38c/h', 52),
	hist('16:35', 'HERD', 'HERD', '+9 listings/6h', 57)
];

const HOLD = play({
	name: 'Ethereal Knives of the Massacre',
	baseName: 'Ethereal Knives',
	color: 'GREEN',
	priceTier: 'TOP',
	liquidityTier: 'HIGH',
	basePrice: 110,
	transPrice: 5.1 * DIVINE_RATE,
	gcpRecipeCost: 79,
	gcpRecipeBase: 35,
	gcpRecipeSaves: 31,
	sellUrgency: 'HOLD',
	sellReason: 'No strong sell signal — hold and monitor',
	sellability: 65,
	sellabilityLabel: 'GOOD'
});
const HOLD_HISTORY = [
	hist('15:35', 'UNCERTAIN', 'UNCERTAIN', '-6c', 231),
	hist('16:05', 'UNCERTAIN', 'UNCERTAIN', '-266c/h', 235),
	hist('16:35', 'UNCERTAIN', 'UNCERTAIN', '-6c', 233)
];

const SELL_NOW = play({
	name: 'Lightning Arrow of Electrocution',
	baseName: 'Lightning Arrow',
	color: 'GREEN',
	priceTier: 'TOP',
	liquidityTier: 'MED',
	basePrice: 96,
	transPrice: 3.4 * DIVINE_RATE,
	gcpRecipeCost: 74,
	gcpRecipeBase: 30,
	gcpRecipeSaves: 22,
	sellUrgency: 'SELL_NOW',
	sellReason: 'Price dropping with rising supply — undercut hard',
	tierAction: 'SELL IMMEDIATELY',
	sellability: 31,
	sellabilityLabel: 'SLOW'
});
const SELL_NOW_HISTORY = [
	hist('16:05', 'STABLE', 'DUMPING', '-180c/h', 74),
	hist('16:35', 'DUMPING', 'DUMPING', '+14 listings/6h', 88)
];

const COSTS_MORE = { ...TYPICAL, basePrice: 60, gcpRecipeSaves: -12 };

const NO_VERDICT = play({
	name: 'Frost Blades of Katabasis',
	baseName: 'Frost Blades',
	color: 'GREEN',
	priceTier: 'MID',
	liquidityTier: 'HIGH',
	basePrice: 42,
	transPrice: 148,
	sellability: 72,
	sellabilityLabel: 'GOOD'
});

const NO_BUY_LINKS = play({
	name: 'Divine Ire',
	baseName: 'Divine Ire',
	color: 'BLUE',
	basePrice: 85,
	transPrice: 286,
	gcpRecipeCost: 74,
	gcpRecipeBase: 30,
	gcpRecipeSaves: 11,
	sellUrgency: 'WAIT',
	sellReason: 'Price stable — list at market price, will sell eventually',
	sellability: 55,
	sellabilityLabel: 'MODERATE'
});

const SPARSE = play({
	name: 'Arc of Oscillating',
	baseName: 'Arc',
	color: 'BLUE',
	confidence: 'NO_BASE',
	priceTier: 'MID',
	liquidityTier: 'LOW',
	basePrice: 0,
	transPrice: 61,
	sellUrgency: 'WAIT',
	sellReason: 'Price stable — list at market price, will sell eventually',
	sellability: 24,
	sellabilityLabel: 'SLOW'
});

// --- Mounting and reading the expanded row ---

const mounted: ReturnType<typeof mount>[] = [];
let releaseHistory: (() => void) | null = null;

const pendingResponses: (() => void)[] = [];
const NOW = new Date('2026-11-28T12:00:00Z');
let lookup: ReturnType<typeof vi.fn<(body: Record<string, unknown>) => Promise<Response>>>;
let requests: Record<string, unknown>[];
let requestUrls: string[];

beforeEach(() => {
	applyServerSlice({ url: 'https://server.test', defaultUrl: 'https://server.test' });
	applySnapshot({ league: { name: LEAGUE } });
	setDivineRate(DIVINE_RATE);
	vi.useFakeTimers({ toFake: ['Date'] });
	vi.setSystemTime(NOW);
	requests = [];
	requestUrls = [];
	vi.mocked(invoke).mockClear();
	lookup = vi.fn(async () => new Response(null, { status: 204 }));
	vi.stubGlobal('fetch', vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
		const url = new URL(String(input), window.location.origin);
		if (url.href !== `${getApiBase()}/trade/lookup` || init?.method !== 'POST') {
			throw new Error(`Unexpected request: ${init?.method} ${url.pathname}`);
		}
		const body = JSON.parse(String(init.body));
		requests.push(body);
		requestUrls.push(url.href);
		return lookup(body);
	}));
});

afterEach(async () => {
	while (mounted.length > 0) await unmount(mounted.pop()!);
	document.body.innerHTML = '';
	releaseHistory?.();
	releaseHistory = null;
	for (const release of pendingResponses.splice(0)) release();
	await settle();
	history.fetch.mockReset();
	setDivineRate(0);
	resetServerBaseForTests();
	vi.unstubAllGlobals();
	vi.useRealTimers();
});

async function settle(): Promise<void> {
	await new Promise((resolve) => setTimeout(resolve, 0));
	await tick();
	flushSync();
}

/** Mounts BestPlays with one gem, clicks its row, and returns the expanded row beneath it. */
async function expand(gem: GemPlay): Promise<HTMLElement> {
	const target = document.body.appendChild(document.createElement('div'));
	flushSync(() => {
		mounted.push(mount(BestPlays, { target, props: { plays: [gem] } }));
	});
	const row = target.querySelector<HTMLElement>('tbody tr');
	if (!row) throw new Error('BestPlays rendered no gem row');
	row.click();
	flushSync();
	await settle();
	const expanded = row.nextElementSibling as HTMLElement | null;
	if (!expanded) throw new Error('clicking the gem row opened no expanded row');
	return expanded;
}

/** Text with all whitespace removed, so markup spacing does not decide a match. */
const flat = (text: string | null | undefined) => (text ?? '').replace(/\s+/g, '');
/** Expected card text: the design's facts in display order. */
const seq = (...parts: string[]) => flat(parts.join(''));

/**
 * The expanded row cut into its cards by their eyebrows (`1 Buy`, `2 Sell`,
 * `Market`, `Signal history`); null when the row is not laid out as cards.
 */
function cardsOf(expanded: HTMLElement) {
	const m = flat(expanded.textContent).match(/^1?Buy(.*?)2?Sell(Transfigured.*?)Market(Liquidity.*?)Signalhistory(.*)$/);
	const historyText = expanded.querySelector('section.history')?.textContent;
	return m ? { buy: m[1], sell: m[2], market: m[3], history: flat(historyText).replace(/^Signalhistory/, '') } : null;
}

/** True when the history card shows `rows` in order, each as time, from → to, reason, N listings. */
function historyShows(historyText: string, rows: SignalTransition[]): boolean {
	const esc = (s: string) => flat(s).replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
	const pattern = rows
		.map((r) => [r.time, r.from, '→', r.to, r.reason, `${r.listings} listings`].map(esc).join('.*?'))
		.join('.*?');
	return new RegExp(pattern).test(historyText);
}

/** The text a player reads when pointing at `el`: its Tooltip popup, else its title. */
function tooltipOf(el: HTMLElement): string {
	el.dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
	flushSync();
	const tip = document.getElementById(el.getAttribute('aria-describedby') ?? '');
	el.dispatchEvent(new FocusEvent('focusout', { bubbles: true }));
	return tip?.textContent ?? el.getAttribute('title') ?? '';
}

/**
 * The row's BUY controls in display order, links and controls announced as
 * disabled: the label shown, the accessible name a screen reader announces, the
 * destination and the tooltip.
 */
function buyControls(expanded: HTMLElement) {
	return [...expanded.querySelectorAll<HTMLElement>('a[href], [aria-disabled="true"]')].map((el) => ({
		label: el.textContent?.trim(),
		name: el.getAttribute('aria-label'),
		href: el.getAttribute('href'),
		disabled: el.getAttribute('aria-disabled') === 'true',
		tooltip: tooltipOf(el)
	}));
}

/** The control a screen reader announces as `name`; every BUY now reads the same label. */
function buyNamed(expanded: HTMLElement, name: string): HTMLElement | undefined {
	return [...expanded.querySelectorAll<HTMLElement>('a, [aria-disabled="true"]')].find((el) => el.getAttribute('aria-label') === name);
}

// Desktop prices carry their unit as a currency icon, so the visible amount is the bare number.
const c = (amount: string | number) => String(amount);

describe('BestPlays expanded row (desktop)', () => {
	it.each([
		{
			example: 'typical UNDERCUT',
			gem: TYPICAL,
			rows: TYPICAL_HISTORY,
			buy: seq('Base', '20/20', c(94), 'BUY', 'GCP recipe', 'saves 22', c(72), '20/0 base', c(28), 'BUY', '+', 'GCPs', c(44)),
			sell: seq(
				'Transfigured', '20/20', c(412), 'UNDERCUT', 'HERD at historical peak — undercut 10-15% NOW before crash',
				'Tier', 'HIGH', 'UNDERCUT — herd arrived, sell into pressure'
			),
			market: seq('Liquidity', 'MED', 'Color', 'RED', 'Sellability', 'MODERATE · 48')
		},
		{
			example: 'HOLD',
			gem: HOLD,
			rows: HOLD_HISTORY,
			buy: seq('Base', '20/20', c(110), 'BUY', 'GCP recipe', 'saves 31', c(79), '20/0 base', c(35), 'BUY', '+', 'GCPs', c(44)),
			sell: seq('Transfigured', '20/20', c('5.1'), '1173c', 'HOLD', 'No strong sell signal — hold and monitor'),
			market: seq('Liquidity', 'HIGH', 'Color', 'GREEN', 'Sellability', 'GOOD · 65')
		},
		{
			example: 'SELL NOW',
			gem: SELL_NOW,
			rows: SELL_NOW_HISTORY,
			buy: seq('Base', '20/20', c(96), 'BUY', 'GCP recipe', 'saves 22', c(74), '20/0 base', c(30), 'BUY', '+', 'GCPs', c(44)),
			sell: seq(
				'Transfigured', '20/20', c('3.4'), '782c', 'SELL NOW', 'Price dropping with rising supply — undercut hard',
				'Tier', 'TOP', 'SELL IMMEDIATELY'
			),
			market: seq('Liquidity', 'MED', 'Color', 'GREEN', 'Sellability', 'SLOW · 31')
		}
	])('keep:happy: $example: Buy, Sell and Market cards show the gem facts and the history lists each signal change', async ({ gem, rows, buy, sell, market }) => {
		history.fetch.mockResolvedValue(rows);
		const expanded = await expand(gem);

		const cards = cardsOf(expanded);
		expect(
			cards && { buy: cards.buy, sell: cards.sell, market: cards.market, historyListed: historyShows(cards.history, rows) },
			flat(expanded.textContent)
		).toEqual({ buy, sell, market, historyListed: true });
	});

	it('keep:happy: a GCP recipe dearer than the base reads costs N more', async () => {
		history.fetch.mockResolvedValue(TYPICAL_HISTORY);
		const expanded = await expand(COSTS_MORE);

		expect(cardsOf(expanded)?.buy, flat(expanded.textContent)).toBe(
			seq('Base', '20/20', c(60), 'BUY', 'GCP recipe', 'costs 12', 'more', c(72), '20/0 base', c(28), 'BUY', '+', 'GCPs', c(44))
		);
	});

	it('keep:happy: with no urgency, tier action or recipe the Sell card shows the Transfigured price alone', async () => {
		history.fetch.mockResolvedValue([hist('14:05', 'UNCERTAIN', 'STABLE', '+4c', 61)]);
		const expanded = await expand(NO_VERDICT);

		const cards = cardsOf(expanded);
		expect(cards && { buy: cards.buy, sell: cards.sell }, flat(expanded.textContent)).toEqual({
			buy: seq('Base', '20/20', c(42), 'BUY'),
			sell: seq('Transfigured', '20/20', c(148))
		});
	});

	it('keep:happy: a gem name without " of " shows no BUY controls and keeps its prices', async () => {
		history.fetch.mockResolvedValue([hist('13:35', 'UNCERTAIN', 'STABLE', '-3c', 40)]);
		const expanded = await expand(NO_BUY_LINKS);

		expect({ buy: cardsOf(expanded)?.buy, controls: buyControls(expanded) }, flat(expanded.textContent)).toEqual({
			buy: seq('Base', '20/20', c(85), 'GCP recipe', 'saves 11', c(74), '20/0 base', c(30), '+', 'GCPs', c(44)),
			controls: []
		});
	});

	it('keep:happy: an unlisted base with no recipe and no history reads not listed and No signal changes', async () => {
		history.fetch.mockResolvedValue([]);
		const expanded = await expand(SPARSE);

		const cards = cardsOf(expanded);
		expect(cards && { buy: cards.buy, history: cards.history }, flat(expanded.textContent)).toEqual({
			buy: seq('Base', '20/20', 'not listed', 'BUY'),
			history: seq('No signal changes')
		});
	});

	it('keep:happy: with no league detected both BUY controls are disabled and named League not detected, and a failed history reads History unavailable', async () => {
		applySnapshot({ league: { name: null } });
		history.fetch.mockRejectedValue(new Error('API /analysis/history: 502 Bad Gateway'));
		const expanded = await expand(HOLD);

		expect({ controls: buyControls(expanded), history: cardsOf(expanded)?.history }, flat(expanded.textContent)).toEqual({
			controls: [
				{ label: 'BUY', name: 'League not detected', href: null, disabled: true, tooltip: 'League not detected' },
				{ label: 'BUY', name: 'League not detected', href: null, disabled: true, tooltip: 'League not detected' }
			],
			history: seq('History unavailable')
		});
	});

	it('keep:happy: while the history request is pending the history card reads Loading…', async () => {
		history.fetch.mockReturnValue(new Promise((resolve) => (releaseHistory = () => resolve([]))));
		const expanded = await expand(HOLD);

		expect(cardsOf(expanded)?.history, flat(expanded.textContent)).toBe(seq('Loading…'));
	});

	// keep:critical — silent: both controls read BUY, so a name or variant crossed
	// between them sends the player to buy the wrong gem (a 20/20 base for the
	// recipe, or a 20/0 for the base) with nothing on screen to tell them apart.
	it('keep:critical: the BUY named for the gem variant opens its trade search and the one named for the 20/0 base the 20/0 search', async () => {
		history.fetch.mockResolvedValue(TYPICAL_HISTORY);
		const expanded = await expand(TYPICAL);

		expect({
			controls: buyControls(expanded).map(({ label, name, tooltip }) => ({ label, name, tooltip })),
			base: buyNamed(expanded, 'Buy Cleave (20/20)')?.getAttribute('href'),
			recipe: buyNamed(expanded, 'Buy Cleave (20/0)')?.getAttribute('href')
		}).toEqual({
			controls: [
				{ label: 'BUY', name: 'Buy Cleave (20/20)', tooltip: 'Buy Cleave (20/20)' },
				{ label: 'BUY', name: 'Buy Cleave (20/0)', tooltip: 'Buy Cleave (20/0)' }
			],
			base: baseGemTradeUrl('Cleave of Rage', '20/20', LEAGUE),
			recipe: baseGemTradeUrl('Cleave of Rage', '20/0', LEAGUE)
		});
	});

	it('keep:happy: prices show chaos and divine icons sized 18 for prices, 20 for Transfigured, 16 in the saves chip, and an 18px GCP icon', async () => {
		history.fetch.mockResolvedValue(HOLD_HISTORY);
		const expanded = await expand(HOLD);

		const apiBase = getApiBase();
		const chaos = iconSrc(apiBase, currencyIconPath(CHAOS_ID));
		const divine = iconSrc(apiBase, currencyIconPath(DIVINE_ID));
		const gcp = iconSrc(apiBase, currencyIconPath(GCP_ID));
		const icons = [...expanded.querySelectorAll('img')].map((img) => ({ src: img.getAttribute('src'), size: img.getAttribute('width') }));
		expect(icons).toEqual([
			{ src: chaos, size: '18' }, // Base 110
			{ src: chaos, size: '16' }, // saves 31 chip beside GCP recipe
			{ src: chaos, size: '18' }, // GCP recipe 79
			{ src: chaos, size: '18' }, // 20/0 base 35
			{ src: gcp, size: '18' }, // before GCPs
			{ src: chaos, size: '18' }, // GCPs 44
			{ src: divine, size: '20' } // Transfigured 5.1
		]);
	});
});

function listing(chaosPrice: number, minutes: number): TradeListingDetail {
	return {
		price: chaosPrice, currency: 'chaos', chaosPrice,
		indexedAt: new Date(NOW.getTime() - minutes * 60_000).toISOString(),
		gemLevel: 20, gemQuality: 20, corrupted: false
	};
}

const FOUR_LISTINGS = [listing(90, 3), listing(30, 12), listing(60, 25), listing(45, 40)];

function lookupResult(listings: TradeListingDetail[] = FOUR_LISTINGS): TradeLookupResult {
	return {
		gem: HOLD.name, variant: HOLD.variant, league: LEAGUE, total: listings.length,
		priceFloor: 30, priceCeiling: 90, priceSpread: 60, medianTop10: 52.5, medianTop3: 45,
		listings, divinePrice: DIVINE_RATE, tradeUrl: '', fetchedAt: NOW.toISOString(),
		signals: { sellerConcentration: 'NORMAL', cheapestStaleness: 'FRESH', priceOutlier: false, uniqueAccounts: 4 }
	};
}

function listingsOf(expanded: Element): HTMLElement | undefined {
	return [...expanded.querySelectorAll<HTMLElement>('section')].find((section) =>
		/listings/i.test(section.querySelector('.eyebrow, h3, h4')?.textContent ?? '')
	);
}

function listingPairs(expanded: Element): string[] {
	return flat(listingsOf(expanded)?.textContent).match(/\d+(?:\.\d+)?c\d+mago/g) ?? [];
}

function deferredResponse() {
	let resolve!: (response: Response) => void;
	let reject!: (reason: Error) => void;
	const promise = new Promise<Response>((yes, no) => { resolve = yes; reject = no; });
	pendingResponses.push(() => resolve(new Response(null, { status: 204 })));
	return { promise, resolve, reject };
}

function mountRows(plays: GemPlay[]): HTMLElement {
	const target = document.body.appendChild(document.createElement('div'));
	flushSync(() => mounted.push(mount(BestPlays, { target, props: { plays } })));
	return target;
}

async function clickRow(target: HTMLElement, index: number): Promise<void> {
	target.querySelectorAll<HTMLElement>('tr.play-row')[index].click();
	flushSync();
	await settle();
}

describe('POE-519 desktop expanded cached listings', () => {
	it('keep:happy: POE-519 expanded divine sale shows its chaos equivalent beside the three cheapest indexed offers', async () => {
		history.fetch.mockResolvedValue(HOLD_HISTORY);
		lookup.mockResolvedValue(Response.json(lookupResult()));
		const expanded = await expand(HOLD);
		expect(listingPairs(expanded), listingsOf(expanded)?.textContent ?? 'Listings column missing').toEqual(['30c12mago', '45c40mago', '60c25mago']);
		expect(cardsOf(expanded)?.sell, expanded.textContent ?? '').toBe(
			seq('Transfigured', '20/20', '5.1', '1173c', 'HOLD', HOLD.sellReason)
		);
		expect(historyShows(cardsOf(expanded)!.history, HOLD_HISTORY)).toBe(true);
	});

	it.each([
		{ source: '204 cache miss', response: () => new Response(null, { status: 204 }) },
		{ source: '200 empty listings', response: () => Response.json(lookupResult([])) },
		{ source: '200 null listings', response: () => Response.json({ ...lookupResult([]), listings: null }) }
	])('keep:critical: POE-519 $source labels missing offers so a silent empty column cannot imply useful prices', async ({ response }) => {
		history.fetch.mockResolvedValue([]);
		lookup.mockImplementation(async () => response());
		const expanded = await expand(HOLD);
		expect(listingsOf(expanded)?.textContent ?? '').toContain('No recent listings');
		expect(listingPairs(expanded)).toEqual([]);
		expect(requests).toEqual([{ gem: HOLD.name, variant: HOLD.variant, force: false, cacheOnly: true }]);
	});

	it.each(['pending', 'error', 'unset server'] as const)('keep:happy: POE-519 %s lookup has an explicit state independent of signal history', async (state) => {
		history.fetch.mockResolvedValue(HOLD_HISTORY);
		const pending = deferredResponse();
		lookup.mockReturnValue(pending.promise);
		if (state === 'unset server') resetServerBaseForTests();
		const expanded = await expand(HOLD);
		if (state === 'error') {
			pending.resolve(new Response('Unavailable', { status: 503 }));
			await settle();
		}
		const text = listingsOf(expanded)?.textContent ?? '';
		if (state === 'pending') expect(text).toMatch(/Loading(?: listings)?…/i);
		else expect(text).toContain('Listings unavailable');
		if (state === 'unset server') {
			expect(requestUrls).toEqual([]);
			expect(vi.mocked(fetch)).not.toHaveBeenCalled();
		}
		expect(text).not.toContain('No recent listings');
		expect(historyShows(cardsOf(expanded)!.history, HOLD_HISTORY)).toBe(true);
	});

	it.each([
		['1/0', '1/0'], ['1/20', '1/20'], ['20/0', '20/0'], ['20/20', '20/20'],
		['1', '1'], ['20', '20'], ['21/20c', '21/20'], ['21/23c', '21/23']
	])('keep:happy: POE-519 expansion of %s sends trade variant %s in the cache-only POST', async (variant, tradeVariant) => {
		history.fetch.mockResolvedValue([]);
		await expand(play({ variant }));
		expect(requests).toEqual([{ gem: TYPICAL.name, variant: tradeVariant, force: false, cacheOnly: true }]);
		expect(requestUrls).toEqual(['https://server.test/api/trade/lookup']);
		expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === 'trade_lookup')).toEqual([]);
		expect(history.fetch).toHaveBeenCalledWith(TYPICAL.name, variant);
	});

	it.each([
		{ transition: 'A to B', late: 'success' },
		{ transition: 'A to B', late: 'rejection' },
		{ transition: 'collapse and reopen A', late: 'success' },
		{ transition: 'collapse and reopen A', late: 'rejection' }
	])('keep:critical: POE-519 $transition ignores late $late so plausible stale offers cannot guide a sale', async ({ transition, late }) => {
		history.fetch.mockResolvedValue(HOLD_HISTORY);
		const old = deferredResponse();
		const current = deferredResponse();
		lookup.mockReturnValueOnce(old.promise).mockReturnValueOnce(current.promise);
		const target = mountRows([HOLD, SELL_NOW]);
		await clickRow(target, 0);
		if (transition === 'A to B') await clickRow(target, 1);
		else {
			await clickRow(target, 0);
			expect(target.querySelector('.expanded-row')).toBeNull();
			await clickRow(target, 0);
		}
		expect(requests.map((body) => body.gem)).toEqual([HOLD.name, transition === 'A to B' ? SELL_NOW.name : HOLD.name]);
		const expanded = target.querySelector('.expanded-row')!;
		if (late === 'rejection') {
			old.reject(new Error('late lookup failure'));
			await settle();
			expect(listingsOf(expanded)?.textContent ?? '').toMatch(/Loading(?: listings)?…/i);
			expect(listingsOf(expanded)?.textContent ?? '').not.toMatch(/999|unavailable/i);
		}
		current.resolve(Response.json({
			...lookupResult([listing(44, 9)]), gem: transition === 'A to B' ? SELL_NOW.name : HOLD.name
		}));
		await settle();
		expect(listingPairs(expanded)).toEqual(['44c9mago']);
		if (late === 'success') {
			old.resolve(Response.json(lookupResult([listing(999, 7)])));
			await settle();
			expect(listingPairs(expanded)).toEqual(['44c9mago']);
		}
		expect(listingsOf(expanded)?.textContent ?? '').not.toMatch(/999|Loading|unavailable/i);
	});
});

import wi9BadgeSource from './BestPlays.svelte?raw';
import { TIER_RULES as wi9Rules } from '$lib/tooltips';

function wi9BadgeStyles(): HTMLStyleElement {
	const style = document.createElement('style');
	style.textContent = ':root { --color-lab-yellow: #eab308; }' + (wi9BadgeSource.match(/<style[^>]*>([\s\S]*?)<\/style>/)?.[1] ?? '');
	document.head.append(style);
	return style;
}

it.each([false, true])('keep:happy WI-9 Best Plays: lowConfidence=%s renders hollow yellow UNRATED in the row and expansion', async (lowConfidence) => {
	history.fetch.mockResolvedValue([]);
	const target = mountRows([
		play({ name: 'Ice Nova of Frostbolts', priceTier: 'UNRATED' as GemPlay['priceTier'], lowConfidence, tierAction: 'served action' }),
		play({ name: 'Smite of Divine Judgement', priceTier: 'TOP' }),
		play({ name: 'Lacerate of Butchering', priceTier: 'MID' })
	]);
	const style = wi9BadgeStyles();
	try {
		const rows = target.querySelectorAll<HTMLElement>('tr.play-row');
		expect([...rows].map((row) => row.querySelector('.tier-badge')?.textContent)).toEqual(['UNRATED', 'TOP', 'MID']);
		const badge = rows[0].querySelector<HTMLElement>('.tier-badge')!;
		expect(badge.classList.contains('tier-unrated')).toBe(true);
		expect(badge.matches('.tier-low, .tier-mid, .tier-top')).toBe(false);
		const css = getComputedStyle(badge);
		expect(css.color).toBe('rgb(234, 179, 8)');
		expect(css.backgroundColor).toMatch(/^(transparent|rgba\(0, 0, 0, 0\))$/);
		expect(css.borderTopWidth).toBe('1px');
		expect(css.borderTopStyle).toBe(lowConfidence ? 'dashed' : 'solid');
		expect(css.padding).toBe('1px 7px');
		rows[0].click();
		flushSync();
		await settle();
		const expanded = target.querySelector<HTMLElement>('.expanded-row .tier-badge')!;
		expect(expanded.textContent).toBe('UNRATED');
		expect(expanded.classList.contains('tier-unrated')).toBe(true);
		expect(expanded.classList.contains('low-conf')).toBe(lowConfidence);
		expect(getComputedStyle(expanded).color).toBe('rgb(234, 179, 8)');
	} finally { style.remove(); }
});

it('keep:happy WI-9 Best Plays Tier tooltip: UNRATED has the shared yellow explanation', () => {
	const target = mountRows([play({})]);
	const header = target.querySelector<HTMLElement>('th.col-tier')!;
	header.querySelector<HTMLElement>('.tooltip-wrap')!.dispatchEvent(new MouseEvent('mouseenter'));
	flushSync();
	const tip = [...document.querySelectorAll<HTMLElement>('[role="tooltip"]')].find((node) => node.style.visibility === 'visible');
	expect(tip?.textContent?.toLowerCase()).toContain('unrated');
	expect(tip?.textContent?.toLowerCase()).toContain('under 4 solid gems');
	expect(tip?.textContent?.toLowerCase()).toContain((wi9Rules as Record<string, string>).UNRATED);
	const label = [...(tip?.querySelectorAll<HTMLElement>('span') ?? [])].find((node) => node.textContent === 'UNRATED');
	expect(label?.style.color).toBe('rgb(234, 179, 8)');
});
