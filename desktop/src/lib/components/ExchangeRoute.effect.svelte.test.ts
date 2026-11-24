// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { CurrencyExchangeLeg, CurrencyExchangePlay } from '$lib/api';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => ({})) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

let ExchangeRoute: typeof import('./ExchangeRoute.svelte').default;
let { flushSync, mount, unmount } = await import('svelte');
const apps: ReturnType<typeof mount>[] = [];
const writeText = vi.fn<(text: string) => Promise<void>>();

beforeEach(async () => {
	vi.resetModules();
	({ flushSync, mount, unmount } = await import('svelte'));
	ExchangeRoute = (await import('./ExchangeRoute.svelte')).default;
	(await import('$lib/stores/number-format.svelte')).setNumberFormat('comma');
	vi.useFakeTimers();
	writeText.mockReset().mockResolvedValue(undefined);
	vi.stubGlobal('navigator', { clipboard: { writeText } });
});

afterEach(async () => {
	for (const app of apps.splice(0)) await unmount(app);
	document.body.innerHTML = '';
	vi.clearAllTimers();
	vi.useRealTimers();
	vi.unstubAllGlobals();
});

function leg(name: string, action: 'buy' | 'sell'): CurrencyExchangeLeg {
	return {
		action,
		item: name,
		quote: 'Metadata/Items/Currency/CurrencyRerollRare',
		price: 10,
		priceItemQty: 1,
		priceQuoteQty: 10,
		fair: 10,
		fairOk: true,
		tick: 0.01,
		volume: 100,
		stock: 10,
		suspect: false,
		itemName: name,
		itemIcon: null,
		itemCategory: 'Currency',
		quoteName: 'Chaos Orb',
		quoteIcon: null,
		quoteCategory: 'Currency'
	};
}

function play(buyName: string, sellName: string): CurrencyExchangePlay {
	return {
		key: `direct:${buyName}:${sellName}`,
		mode: 'direct',
		legs: [leg(buyName, 'buy'), leg(sellName, 'sell')],
		roiPct: 0.1,
		edge: 0.1,
		roiPctRaw: 0.12,
		roi: 1,
		investment: 10,
		expectedRoi: 0.5,
		expectedRoiPct: 0.05,
		simEntries: 20,
		lowCoverage: false,
		turnover: 1000,
		tick: 0.01,
		depth: 100,
		suspect: false,
		hoursSeen: 6,
		lastHour: '2026-11-13T10:00:00Z',
		display: {
			units: 1,
			basis: 'posting',
			money: { investment: 10, roi: 1, expectedRoi: 0.5 },
			expectedPrinted: true,
			scale: { flips: 200, gain: 100, investment: 2000, hours: 2 },
			runInvestment: 2000,
			sortKeys: { expected: 0.5, roi: 1, fastestHours: 2 }
		}
	};
}

function renderRoute(fixture: CurrencyExchangePlay): HTMLElement {
	const target = document.createElement('div');
	document.body.append(target);
	apps.push(mount(ExchangeRoute, {
		target,
		props: { play: fixture, density: 'comfortable', apiBase: null, divineChaosRate: 200, showConvert: false }
	}));
	flushSync();
	return target;
}

function nameButton(target: HTMLElement, name: string): HTMLButtonElement {
	const button = [...target.querySelectorAll<HTMLButtonElement>('button')].find(
		(control) => control.textContent === name && control.title === name
	);
	expect(button, `item name "${name}" must be a button with its item-name title`).toBeDefined();
	return button!;
}

async function settle(): Promise<void> {
	for (let i = 0; i < 10; i++) await Promise.resolve();
	flushSync();
}

describe('ExchangeRoute item-name clipboard interaction', () => {
	it('keep:critical copies the exact sell name once: a wrong name silently drives a wrong trade search', async () => {
		const target = renderRoute(play('Chaos Orb', 'Divine Orb'));
		const sell = nameButton(target, 'Divine Orb');

		sell.click();
		await settle();

		expect(writeText).toHaveBeenCalledTimes(1);
		expect(writeText).toHaveBeenCalledWith('Divine Orb');
	});

	it('keep:happy moves the sole Copied feedback from Chaos Orb to another route for 1200 ms', async () => {
		const first = renderRoute(play('Chaos Orb', 'Divine Orb'));
		const second = renderRoute(play('Exalted Orb', 'Orb of Annulment'));
		const chaos = nameButton(first, 'Chaos Orb');
		const exalted = nameButton(second, 'Exalted Orb');
		let resolveWrite!: () => void;
		writeText.mockImplementationOnce(() => new Promise<void>((resolve) => { resolveWrite = resolve; }));

		chaos.click();
		await settle();
		expect(chaos.textContent).toBe('Chaos Orb');
		resolveWrite();
		await settle();
		expect(writeText).toHaveBeenNthCalledWith(1, 'Chaos Orb');
		expect(chaos.textContent).toBe('Copied');

		exalted.click();
		await settle();
		expect(chaos.textContent).toBe('Chaos Orb');
		expect(exalted.textContent).toBe('Copied');
		expect([...document.querySelectorAll('button')].filter((button) => button.textContent === 'Copied')).toHaveLength(1);

		vi.advanceTimersByTime(1200);
		await settle();
		expect(exalted.textContent).toBe('Exalted Orb');
	});
});
