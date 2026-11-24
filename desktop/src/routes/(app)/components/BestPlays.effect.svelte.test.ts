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

const history = vi.hoisted(() => ({ fetch: vi.fn<(name: string, variant: string) => Promise<SignalTransition[]>>() }));

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock('$lib/api', async (importOriginal) => ({
	...(await importOriginal<typeof import('$lib/api')>()),
	fetchSignalHistory: history.fetch
}));

import { getApiBase } from '$lib/api';
import { applyServerSlice } from '$lib/stores/server-base.svelte';
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

beforeEach(() => {
	applyServerSlice({ url: 'https://server.test', defaultUrl: 'https://server.test' });
	applySnapshot({ league: { name: LEAGUE } });
	setDivineRate(DIVINE_RATE);
});

afterEach(() => {
	while (mounted.length > 0) unmount(mounted.pop()!);
	document.body.innerHTML = '';
	releaseHistory?.();
	releaseHistory = null;
	history.fetch.mockReset();
	setDivineRate(0);
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
	return m ? { buy: m[1], sell: m[2], market: m[3], history: m[4] } : null;
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

/** The row's BUY controls in display order: links, and controls announced as disabled. */
function buyControls(expanded: HTMLElement) {
	return [...expanded.querySelectorAll<HTMLElement>('a[href], [aria-disabled="true"]')].map((el) => ({
		label: el.textContent?.trim(),
		href: el.getAttribute('href'),
		disabled: el.getAttribute('aria-disabled') === 'true',
		tooltip: el.getAttribute('aria-disabled') === 'true' ? tooltipOf(el) : undefined
	}));
}

function buyControl(expanded: HTMLElement, label: string): HTMLElement | undefined {
	return [...expanded.querySelectorAll<HTMLElement>('a, [aria-disabled="true"]')].find((el) => el.textContent?.trim() === label);
}

// Desktop prices carry their unit as a currency icon, so the visible amount is the bare number.
const c = (amount: string | number) => String(amount);

describe('BestPlays expanded row (desktop)', () => {
	// keep:happy
	it.each([
		{
			example: 'typical UNDERCUT',
			gem: TYPICAL,
			rows: TYPICAL_HISTORY,
			buy: seq('Base', '20/20', c(94), 'BUY BASE', 'GCP recipe', c(72), 'BUY 20/0', '20/0 base', c(28), '+', 'GCPs', c(44), 'saves 22'),
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
			buy: seq('Base', '20/20', c(110), 'BUY BASE', 'GCP recipe', c(79), 'BUY 20/0', '20/0 base', c(35), '+', 'GCPs', c(44), 'saves 31'),
			sell: seq('Transfigured', '20/20', c('5.1'), 'HOLD', 'No strong sell signal — hold and monitor'),
			market: seq('Liquidity', 'HIGH', 'Color', 'GREEN', 'Sellability', 'GOOD · 65')
		},
		{
			example: 'SELL NOW',
			gem: SELL_NOW,
			rows: SELL_NOW_HISTORY,
			buy: seq('Base', '20/20', c(96), 'BUY BASE', 'GCP recipe', c(74), 'BUY 20/0', '20/0 base', c(30), '+', 'GCPs', c(44), 'saves 22'),
			sell: seq(
				'Transfigured', '20/20', c('3.4'), 'SELL NOW', 'Price dropping with rising supply — undercut hard',
				'Tier', 'TOP', 'SELL IMMEDIATELY'
			),
			market: seq('Liquidity', 'MED', 'Color', 'GREEN', 'Sellability', 'SLOW · 31')
		}
	])('$example: Buy, Sell and Market cards show the gem facts and the history lists each signal change', async ({ gem, rows, buy, sell, market }) => {
		history.fetch.mockResolvedValue(rows);
		const expanded = await expand(gem);

		const cards = cardsOf(expanded);
		expect(
			cards && { buy: cards.buy, sell: cards.sell, market: cards.market, historyListed: historyShows(cards.history, rows) },
			flat(expanded.textContent)
		).toEqual({ buy, sell, market, historyListed: true });
	});

	// keep:happy
	it('a GCP recipe dearer than the base reads costs N more', async () => {
		history.fetch.mockResolvedValue(TYPICAL_HISTORY);
		const expanded = await expand(COSTS_MORE);

		expect(cardsOf(expanded)?.buy, flat(expanded.textContent)).toBe(
			seq('Base', '20/20', c(60), 'BUY BASE', 'GCP recipe', c(72), 'BUY 20/0', '20/0 base', c(28), '+', 'GCPs', c(44), 'costs 12', 'more')
		);
	});

	// keep:happy
	it('with no urgency, tier action or recipe the Sell card shows the Transfigured price alone', async () => {
		history.fetch.mockResolvedValue([hist('14:05', 'UNCERTAIN', 'STABLE', '+4c', 61)]);
		const expanded = await expand(NO_VERDICT);

		const cards = cardsOf(expanded);
		expect(cards && { buy: cards.buy, sell: cards.sell }, flat(expanded.textContent)).toEqual({
			buy: seq('Base', '20/20', c(42), 'BUY BASE'),
			sell: seq('Transfigured', '20/20', c(148))
		});
	});

	// keep:happy
	it('a gem name without " of " shows no BUY controls and keeps its prices', async () => {
		history.fetch.mockResolvedValue([hist('13:35', 'UNCERTAIN', 'STABLE', '-3c', 40)]);
		const expanded = await expand(NO_BUY_LINKS);

		expect({ buy: cardsOf(expanded)?.buy, controls: buyControls(expanded) }, flat(expanded.textContent)).toEqual({
			buy: seq('Base', '20/20', c(85), 'GCP recipe', c(74), '20/0 base', c(30), '+', 'GCPs', c(44), 'saves 11'),
			controls: []
		});
	});

	// keep:happy
	it('an unlisted base with no recipe and no history reads not listed and No signal changes', async () => {
		history.fetch.mockResolvedValue([]);
		const expanded = await expand(SPARSE);

		const cards = cardsOf(expanded);
		expect(cards && { buy: cards.buy, history: cards.history }, flat(expanded.textContent)).toEqual({
			buy: seq('Base', '20/20', 'not listed', 'BUY BASE'),
			history: seq('No signal changes')
		});
	});

	// keep:happy
	it('with no league detected both BUY controls are disabled with League not detected, and a failed history reads History unavailable', async () => {
		applySnapshot({ league: { name: null } });
		history.fetch.mockRejectedValue(new Error('API /analysis/history: 502 Bad Gateway'));
		const expanded = await expand(HOLD);

		expect({ controls: buyControls(expanded), history: cardsOf(expanded)?.history }, flat(expanded.textContent)).toEqual({
			controls: [
				{ label: 'BUY BASE', href: null, disabled: true, tooltip: 'League not detected' },
				{ label: 'BUY 20/0', href: null, disabled: true, tooltip: 'League not detected' }
			],
			history: seq('History unavailable')
		});
	});

	// keep:happy
	it('while the history request is pending the history card reads Loading…', async () => {
		history.fetch.mockReturnValue(new Promise((resolve) => (releaseHistory = () => resolve([]))));
		const expanded = await expand(HOLD);

		expect(cardsOf(expanded)?.history, flat(expanded.textContent)).toBe(seq('Loading…'));
	});

	// keep:critical — silent: a plausible wrong variant in either trade URL sends the
	// player to buy the wrong gem (a 20/20 base for the recipe, or a 20/0 for the base).
	it('BUY BASE opens the trade search for the gem variant and BUY 20/0 the search for the 20/0 base', async () => {
		history.fetch.mockResolvedValue(TYPICAL_HISTORY);
		const expanded = await expand(TYPICAL);

		expect({
			base: buyControl(expanded, 'BUY BASE')?.getAttribute('href'),
			recipe: buyControl(expanded, 'BUY 20/0')?.getAttribute('href')
		}).toEqual({
			base: baseGemTradeUrl('Cleave of Rage', '20/20', LEAGUE),
			recipe: baseGemTradeUrl('Cleave of Rage', '20/0', LEAGUE)
		});
	});

	// keep:happy
	it('prices show chaos and divine icons sized 18 for prices, 20 for Transfigured, 16 in the saves chip, and an 18px GCP icon', async () => {
		history.fetch.mockResolvedValue(HOLD_HISTORY);
		const expanded = await expand(HOLD);

		const apiBase = getApiBase();
		const chaos = iconSrc(apiBase, currencyIconPath(CHAOS_ID));
		const divine = iconSrc(apiBase, currencyIconPath(DIVINE_ID));
		const gcp = iconSrc(apiBase, currencyIconPath(GCP_ID));
		const icons = [...expanded.querySelectorAll('img')].map((img) => ({ src: img.getAttribute('src'), size: img.getAttribute('width') }));
		expect(icons).toEqual([
			{ src: chaos, size: '18' }, // Base 110
			{ src: chaos, size: '18' }, // GCP recipe 79
			{ src: chaos, size: '18' }, // 20/0 base 35
			{ src: gcp, size: '18' }, // before GCPs
			{ src: chaos, size: '18' }, // GCPs 44
			{ src: chaos, size: '16' }, // saves 31 chip
			{ src: divine, size: '20' } // Transfigured 5.1
		]);
	});
});
