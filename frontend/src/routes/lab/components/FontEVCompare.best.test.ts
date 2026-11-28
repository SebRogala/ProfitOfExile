import { afterEach, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import FontEVCompare from './FontEVCompare.svelte';
import type { FontColor, FontEVResponse } from '$lib/api';
import { setDivineRate } from '$lib/price.svelte';

const mounted: ReturnType<typeof mount>[] = [];
afterEach(async () => {
	for (const component of mounted.splice(0)) await unmount(component);
	document.body.replaceChildren();
	vi.unstubAllGlobals();
	setDivineRate(0);
});

it.each([
	{ prefix: 'keep:critical WI-5 withheld EV', name: 'no covered cell', best: null, expected: [] },
	{ prefix: 'keep:happy WI-5 covered', name: 'served BLUE 1/20 despite larger RED 20/20', best: { variant: '1/20', color: 'BLUE' }, expected: ['1/20:BLUE'] }
])('$prefix: the BEST ring sits on exactly the served best cell ($name)', async ({ best, expected }) => {
	vi.stubGlobal('fetch', vi.fn(async (input: string | URL | Request) => {
		const url = new URL(String(input), window.location.origin);
		if (url.pathname !== '/api/analysis/font') throw new Error(`Unexpected request ${url.pathname}`);
		const variant = url.searchParams.get('variant');
		const rows = ['RED', 'GREEN', 'BLUE'].map((color) => {
			const ev = best === null ? null : variant === '20/20' && color === 'RED' ? 90 : 14;
			return { color, ev, evRaw: ev, profit: ev, pool: 88, pricedGems: best === null ? 0 : 88, coverage: best === null ? 0 : 1, winners: 0 };
		});
		return Response.json({ safe: rows, premium: rows, jackpot: rows, best });
	}));
	const target = document.body.appendChild(document.createElement('div'));
	flushSync(() => mounted.push(mount(FontEVCompare, { target, props: { league: 'Probe League' } })));
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
	setDivineRate(250);
	vi.stubGlobal('fetch', vi.fn(async (input: string | URL | Request) => {
		const url = new URL(String(input), window.location.origin);
		if (url.pathname !== '/api/analysis/font') throw new Error('Unexpected request ' + url.pathname);
		const selected = url.searchParams.get('variant') === variant;
		const payload: FontEVResponse = { safe: selected ? [safe] : [], premium: selected ? [safe] : [], jackpot: selected ? [jackpot] : [], best: null, lens: null };
		return Response.json(payload);
	}));
	const target = document.body.appendChild(document.createElement('div'));
	flushSync(() => mounted.push(mount(FontEVCompare, { target, props: { league: 'Probe League' } })));
	await vi.waitFor(() => {
		flushSync();
		expect(target.querySelectorAll('table.ft tbody tr')).toHaveLength(4);
	});
	return target;
}

async function openFontInfo(root: Element): Promise<HTMLElement> {
	const trigger = root.querySelector<HTMLElement>('.info-icon[role="button"]');
	expect(trigger, 'the cell info control is keyboard reachable').not.toBeNull();
	expect(trigger?.tabIndex).toBe(0);
	trigger!.focus();
	trigger!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
	flushSync();
	const panel = trigger!.querySelector<HTMLElement>('.info-panel');
	expect(panel, 'Enter pins the complete list').not.toBeNull();
	const content = cellText(panel);
	trigger!.dispatchEvent(new KeyboardEvent('keydown', { key: ' ', bubbles: true }));
	flushSync();
	expect(trigger!.querySelector('.info-panel')).toBeNull();
	trigger!.dispatchEvent(new KeyboardEvent('keydown', { key: ' ', bubbles: true }));
	flushSync();
	expect(cellText(trigger!.querySelector('.info-panel'))).toBe(content);
	return trigger!.querySelector<HTMLElement>('.info-panel')!;
}

async function poolBadgeExplanation(badge: HTMLElement): Promise<string> {
	return badge.getAttribute('title') ?? '';
}

import wi9FontSource from './FontEVCompare.svelte?raw';

async function mountUnratedPools(mode: 'normal' | 'dedication' = 'normal', cold = false): Promise<HTMLElement> {
	setDivineRate(250);
	vi.stubGlobal('fetch', vi.fn(async (input: string | URL | Request) => {
		const url = new URL(String(input), window.location.origin);
		const colors = ['RED', 'GREEN', 'BLUE'] as const;
		const rows = colors.map((color, index) => {
			const count = [1, 0, 2][index];
			return {
				...fontRow({ color, pool: mode === 'normal' ? [96, 94, 88][index] : [64, 58, 52][index],
					pricedGems: count, topEvaluated: cold ? undefined : false }),
				inputCost: 10,
				// GREEN carries the server's count-0 marker for a colour whose whole group is gated (internal/lab/font.go).
				poolBreakdown: cold ? [] : [count === 0 ? { tier: 'UNRATED', count, minPrice: 0, maxPrice: 0 } : { tier: 'UNRATED', count,
					minPrice: color === 'BLUE' ? (mode === 'normal' ? 18 : 40) : 35,
					maxPrice: color === 'BLUE' ? (mode === 'normal' ? 42 : 340) : 35 }],
				lowConfidenceGems: [{ name: 'Storm Burst of Repulsion', chaos: 527, listings: 1 }]
			};
		});
		const pool = { safe: rows.map((row) => ({ ...row, topEvaluated: undefined })), premium: rows, jackpot: rows };
		if (url.pathname.endsWith('/font')) return Response.json({ ...pool, best: null, lens: null });
		if (url.pathname.endsWith('/dedication')) return Response.json({
			variant: url.searchParams.get('variant'), entryFee: 0, skills: pool, transfigured: pool
		});
		throw new Error('Unexpected request ' + url.pathname);
	}));
	const target = document.body.appendChild(document.createElement('div'));
	flushSync(() => mounted.push(mount(FontEVCompare, { target, props: { league: 'Probe League', labMode: mode } })));
	await vi.waitFor(() => { flushSync(); expect(target.querySelectorAll('table.ft tbody tr').length).toBeGreaterThan(0); });
	return target;
}

function wi9OpenPool(target: HTMLElement, dedication = false): HTMLElement[] {
	[...target.querySelectorAll<HTMLButtonElement>('button')].find((button) => button.textContent?.includes('Pool Overview'))!.click();
	flushSync();
	if (dedication) {
		const select = target.querySelector<HTMLSelectElement>('.pool-variant-select select');
		if (select) {
			select.value = '21/20:transfigured';
			select.dispatchEvent(new Event('change', { bubbles: true }));
		} else {
			[...target.querySelectorAll<HTMLButtonElement>('.pool-variant-select button')].find((button) => button.textContent?.trim() === '21/20 Transfigured')!.click();
		}
		flushSync();
	}
	return [...target.querySelectorAll<HTMLElement>('.pool-color-card')];
}

function wi9TierRows(card: Element): Element[] {
	return [...card.querySelectorAll('.pool-tier-row:not(.pool-risky-row)')];
}

function wi9FontStyles(): HTMLStyleElement {
	const style = document.createElement('style');
	style.textContent = ':root { --color-lab-yellow: #eab308; }' + (wi9FontSource.match(/<style[^>]*>([\s\S]*?)<\/style>/)?.[1] ?? '');
	document.head.append(style);
	return style;
}

it('keep:happy WI-9 Font pools: the served UNRATED variant replaces the rated ladder', async () => {
	const target = await mountUnratedPools();
	const cards = wi9OpenPool(target);
	expect(cards).toHaveLength(3);
	for (const [index, card] of cards.entries()) {
		expect(cellText(card)).toContain('Tiers: not enough data');
		expect(wi9TierRows(card).map((row) => cellText(row.querySelector('.pool-tier-name')))).toEqual(index === 1 ? [] : ['UNRATED']);
		expect(cellText(card.querySelector('.pool-risky-row'))).toContain('RISKY');
		if (index === 1) continue;
		const row = wi9TierRows(card)[0];
		expect(cellText(row.querySelector('.pool-tier-count'))).toBe(index === 0 ? '1' : '2');
		expect((row.querySelector('.pool-tier-bar-fill') as HTMLElement).style.background).toBe('rgb(234, 179, 8)');
	}
});

it('keep:happy WI-9 Font pools: the UNRATED variant header shows the served priced coverage', async () => {
	const target = await mountUnratedPools();
	const cards = wi9OpenPool(target);
	expect(cards.map((card) => cellText(card.querySelector('.pool-count')))).toEqual(['1 of 96 priced', '0 of 94 priced', '2 of 88 priced']);
});

it('keep:happy WI-9 Font pools: the UNRATED row shows the served price range', async () => {
	const target = await mountUnratedPools();
	const cards = wi9OpenPool(target);
	expect([cards[0], cards[2]].map((card) => cellText(wi9TierRows(card)[0].querySelector('.pool-tier-range')))).toEqual(['35c', '18c — 42c']);
});

it('keep:happy WI-9 Font pools: the UNRATED row shows its pWin', async () => {
	const target = await mountUnratedPools();
	const cards = wi9OpenPool(target);
	expect([cards[0], cards[2]].map((card) => cellText(wi9TierRows(card)[0].querySelector('.pool-tier-pwin')))).toEqual(['3%', '7%']);
});

it('keep:happy WI-9 Font pool note: keyboard activation opens the selected variant explanation', async () => {
	const target = await mountUnratedPools();
	const cards = wi9OpenPool(target);
	const note = [...cards[1].querySelectorAll<HTMLElement>('div')].find((node) => cellText(node).startsWith('Tiers:') && node.querySelector('[role="button"]'))!;
	const trigger = note?.querySelector<HTMLElement>('[role="button"]');
	expect(trigger?.tabIndex).toBe(0);
	trigger!.focus();
	trigger!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
	flushSync();
	const panel = trigger!.querySelector('.info-panel');
	expect(cellText(panel)).toContain('Too few solid gems at 20/20 to rate tiers (needs 4).');
});

it('keep:happy WI-9 cold Font pools: absent topEvaluated retains the six rated rows', async () => {
	const target = await mountUnratedPools('normal', true);
	for (const card of wi9OpenPool(target)) {
		expect(cellText(card)).not.toContain('Tiers: not enough data');
		expect(wi9TierRows(card).map((row) => cellText(row.querySelector('.pool-tier-name')))).toEqual(['TOP', 'HIGH', 'MID-HIGH', 'MID', 'LOW', 'FLOOR']);
	}
});

it('keep:happy WI-9 Dedication pools: selected 21/20 Transfigured retains gem headers', async () => {
	const target = await mountUnratedPools('dedication');
	const cards = wi9OpenPool(target, true);
	for (const [index, card] of cards.entries()) {
		expect(cellText(card)).toContain('Tiers: not enough data');
		expect(cellText(card.querySelector('.pool-count'))).toBe([64, 58, 52][index] + ' gems');
		expect(wi9TierRows(card).map((row) => cellText(row.querySelector('.pool-tier-name')))).toEqual(index === 1 ? [] : ['UNRATED']);
		if (index === 1) continue;
		expect(cellText(wi9TierRows(card)[0].querySelector('.pool-tier-count'))).toBe(index === 0 ? '1' : '2');
	}
});

it('keep:happy WI-9 Dedication pools: selected 21/20 Transfigured shows its divine range', async () => {
	const target = await mountUnratedPools('dedication');
	const cards = wi9OpenPool(target, true);
	expect([cards[0], cards[2]].map((card) => cellText(wi9TierRows(card)[0].querySelector('.pool-tier-range')))).toEqual(['35c', '0.16d (40c) — 1.4d']);
});

it('keep:happy WI-9 Dedication pools: selected 21/20 Transfigured shows its pWin', async () => {
	const target = await mountUnratedPools('dedication');
	const cards = wi9OpenPool(target, true);
	expect([cards[0], cards[2]].map((card) => cellText(wi9TierRows(card)[0].querySelector('.pool-tier-pwin')))).toEqual(['5%', '11%']);
});

it('keep:happy WI-9 raw UNRATED list shows the served count and range without EV or BEST', async () => {
	const target = await mountUnratedPools();
	const cell = fontCell(target, '20/20', 'BLUE');
	expect(cellText(cell.querySelector('.uf-group'))).toBe('SOLID');
	expect([...cell.querySelector('.uf-row')!.children].map(cellText).join(' ')).toBe('UNRATED 2 · 18c — 42c');
	expect(cell.querySelector('.ev, .tier-lines, .tier-low, .tier-mid, .tier-top, .best-blue')).toBeNull();
});

it('keep:happy WI-9 raw UNRATED label grows from its minimum width to fit', async () => {
	const target = await mountUnratedPools();
	const cell = fontCell(target, '20/20', 'BLUE');
	const style = wi9FontStyles();
	try {
		const label = cell.querySelector<HTMLElement>('.uf-tier')!;
		expect(getComputedStyle(label).minWidth).toBe('42px');
		expect(getComputedStyle(label).width).toBe('auto');
	} finally { style.remove(); }
});

it('keep:happy WI-9 raw UNRATED tooltip lists the full served tier', async () => {
	const target = await mountUnratedPools();
	const cell = fontCell(target, '20/20', 'BLUE');
	expect(cellText(await openFontInfo(cell))).toContain('UNRATED: 2 gems, 18c — 42c');
});

it('keep:critical WI-9 raw UNRATED prices stay yellow without a trusted LOW, MID or TOP valuation', async () => {
	const target = await mountUnratedPools();
	const cell = fontCell(target, '20/20', 'BLUE');
	const label = cell.querySelector<HTMLElement>('.uf-tier')!;
	expect(label.textContent).toBe('UNRATED');
	expect(getComputedStyle(label).color).toBe('rgb(234, 179, 8)');
	expect(cell.querySelector('.tier-low, .tier-mid, .tier-top, .ev, .tier-lines')).toBeNull();
});

it('keep:regression WI-9 count-0 UNRATED marker renders no SOLID row in the raw list', async () => {
	const target = await mountUnratedPools();
	const cell = fontCell(target, '20/20', 'GREEN');
	expect([...cell.querySelectorAll('.uf-row')].map((node) => [...node.children].map(cellText).join(' '))).toEqual(['2.1d Storm Burst of Repulsion']);
	expect([...cell.querySelectorAll('.uf-group')].map(cellText)).toEqual(['RISKY']);
});

it('keep:regression WI-9 count-0 UNRATED marker lists no SOLID tier in the listed-prices tooltip', async () => {
	const target = await mountUnratedPools();
	const tip = cellText(await openFontInfo(fontCell(target, '20/20', 'GREEN')));
	expect(tip).toContain('Storm Burst of Repulsion: 2.1d (1 listing)');
	expect(tip).not.toContain('UNRATED: 0 gems');
	expect(tip).not.toContain('SOLID');
});
