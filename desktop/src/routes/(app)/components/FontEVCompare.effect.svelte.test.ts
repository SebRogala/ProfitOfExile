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
				structuredClone({ safe: [], premium: [], jackpot: [], bestColorSafe: '', bestColorPremium: '', bestColorJackpot: '', lens })
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

afterEach(() => {
	while (mounted.length > 0) unmount(mounted.pop()!);
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

// Loaded lazily through a glob: until the shared component exists the expected
// markup reads as missing and the comparison fails, instead of the file failing
// to resolve. Lazy, so each case loads it against that case's fresh `svelte`.
const buyButtonModules = import.meta.glob<{ default: import('svelte').Component<Record<string, unknown>> }>(
	'../../../lib/components/BuyButton.svelte'
);

/** The markup BuyButton renders for `props`, mounted on its own. */
async function buyButtonHTML(props: Record<string, unknown>): Promise<string> {
	const load = buyButtonModules['../../../lib/components/BuyButton.svelte'];
	if (!load) return 'desktop/src/lib/components/BuyButton.svelte does not exist';
	const { default: BuyButton } = await load();
	const target = document.createElement('div');
	const component = mount(BuyButton, { target, props });
	flushSync();
	const html = target.firstElementChild?.outerHTML ?? '';
	unmount(component);
	return html;
}

const LEAGUE_MISSING = 'League not detected — trade link unavailable';
const COLOURS = ['RED', 'GREEN', 'BLUE'] as const;

interface ExpectedBuy {
	label: string;
	href: string | null;
	colour: 'red' | 'green' | 'blue';
	tooltip: string;
}

/** A link with its Tooltip when the helper built a search, else the disabled control with the league-missing title. */
function buy(label: string, href: string | null, colour: ExpectedBuy['colour'], tooltip: string): ExpectedBuy {
	return href ? { label, href, colour, tooltip } : { label, href: null, colour, tooltip: LEAGUE_MISSING };
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
	buy('BUY', LENS_URL, 'blue', "Buy Facetor's Lens · stored experience ≥ 999,627,000"),
	buy('BUY', exceptionalSupportTradeUrl(league), 'blue', 'Buy the cheapest Exceptional Support Gem lv 3+'),
	buy('BUY', ENHANCE_URL, 'blue', 'Buy Enhance Support lv 3 · any corruption'),
	buy('BUY', ENLIGHTEN_URL, 'blue', 'Buy Enlighten Support lv 3 · any corruption'),
	buy('BUY', EMPOWER_URL, 'blue', 'Buy Empower Support lv 3 · any corruption')
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
		'keep:critical: $name, every BUY is the BuyButton for its trade-helper href with its unchanged tooltip',
		async ({ labMode, league }) => {
			ssotStore.applySnapshot({ league: { name: league }, showAllLensFeeders: true });
			serveLens(SCENARIO_1());
			if (labMode === 'normal') await mountCard();
			else await mountDedication();

			const controls = labMode === 'normal' ? [...tableBuys(), lensBuy(), ...costBuys()] : tableBuys();
			const actual = controls.map((el) => {
				if (!el) return null;
				const shown = { label: el.textContent?.trim() ?? '', href: el.getAttribute('href'), html: el.outerHTML };
				return { ...shown, tooltip: tooltipOf(el) };
			});
			const wanted = labMode === 'normal' ? [...baseBuys(league), ...lensBuys(league)] : inputBuys(league);
			const expected = [];
			for (const { label, href, colour, tooltip } of wanted) {
				const props = href
					? { label, href, colour, width: 'fixed' }
					: { label, href: null, colour, width: 'fixed', title: LEAGUE_MISSING };
				expected.push({ label, href, html: await buyButtonHTML(props), tooltip });
			}

			expect(actual).toEqual(expected);
		}
	);
});
