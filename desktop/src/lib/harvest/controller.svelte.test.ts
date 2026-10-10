/**
 * The Harvest Flipping controller, driven with fake dependencies: deferred
 * port promises (resolved with the fixture adapters' reads), in-memory prefs
 * with the `persisted()` shape, a `listen` that captures its handler, fake
 * timers, a recording clipboard and a fixed clock. Every assertion reads the
 * page model or the written prefs — what the player would see or keep.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { CurrencyExchangeHorizon } from '$lib/exchange/view';
import type { PersistedString } from '$lib/prefs.svelte';
import { createHarvestController, type HarvestControllerDependencies } from './controller.svelte';
import type { ExchangePriceRead, HarvestFamilyData } from './seam';
import { createExchangeFixture } from './sources/exchange-fixture';
import { loadHarvestFamilies } from './sources/harvest-fixture';
import type { FamilyBody } from './view';

const NOW = new Date('2026-09-10T00:00:00Z');
const DELAY_MS = 3000;
const DENSE = 'Metadata/Items/Currency/CurrencyDelveCraftingDefences';
const LUCENT = 'Metadata/Items/Currency/CurrencyDelveCraftingMana';
const DELIRIUM_WEAPONS = 'Metadata/Items/Currency/CurrencyAfflictionOrbWeapons';
const KEEPERS_REGEX = '"shu|san|gil|fac|fra|hol|gly"';

interface Deferred<T> {
	promise: Promise<T>;
	resolve: (value: T) => void;
	reject: (error: unknown) => void;
}

function deferred<T>(): Deferred<T> {
	let resolve!: (value: T) => void;
	let reject!: (error: unknown) => void;
	const promise = new Promise<T>((res, rej) => {
		resolve = res;
		reject = rej;
	});
	return { promise, resolve, reject };
}

function memoryPref(initial: string): PersistedString {
	let value = $state(initial);
	return {
		get value() {
			return value;
		},
		set value(next: string) {
			value = next;
		}
	};
}

const exchangeRead = (horizon: CurrencyExchangeHorizon) => createExchangeFixture(() => NOW)(horizon);

/** The fixture read with Lucent Fossil repriced, so a response is recognisable on screen. */
async function exchangeWithLucent(horizon: CurrencyExchangeHorizon, chaos: number): Promise<ExchangePriceRead> {
	const read = await exchangeRead(horizon);
	read.prices[LUCENT] = { ...read.prices[LUCENT], chaos };
	return read;
}

interface Call<T> {
	horizon: CurrencyExchangeHorizon;
	reply: Deferred<T>;
}

interface Harness {
	dependencies: HarvestControllerDependencies;
	exchangeCalls: Call<ExchangePriceRead>[];
	harvestCalls: Call<HarvestFamilyData>[];
	clipboard: string[];
	fireUpdated: () => void;
}

function harness(prefs: { picks?: string; horizon?: string; family?: string } = {}): Harness {
	const h: Harness = {
		exchangeCalls: [],
		harvestCalls: [],
		clipboard: [],
		fireUpdated: () => {
			throw new Error('listen was not called');
		},
		dependencies: null as unknown as HarvestControllerDependencies
	};
	h.dependencies = {
		loadExchangePrices: (horizon) => {
			const reply = deferred<ExchangePriceRead>();
			h.exchangeCalls.push({ horizon, reply });
			return reply.promise;
		},
		loadHarvestFamilies: (horizon) => {
			const reply = deferred<HarvestFamilyData>();
			h.harvestCalls.push({ horizon, reply });
			return reply.promise;
		},
		prefs: {
			picks: memoryPref(prefs.picks ?? '{}'),
			horizon: memoryPref(prefs.horizon ?? 'day'),
			family: memoryPref(prefs.family ?? 'fossil')
		},
		listen: async (event, handler) => {
			if (event === 'currency-exchange-updated') h.fireUpdated = handler;
			return () => {};
		},
		refetchDelay: () => DELAY_MS,
		setTimeout: (fn, ms) => setTimeout(fn, ms),
		clearTimeout: (timer) => clearTimeout(timer),
		setInterval: (fn, ms) => setInterval(fn, ms),
		clearInterval: (timer) => clearInterval(timer),
		writeClipboard: async (text) => {
			h.clipboard.push(text);
		},
		now: () => NOW,
		divineRate: () => null
	};
	return h;
}

/** What the page's horizon `$effect` does: `$effect` never runs in this server-compiled suite. */
function horizonEffect(controller: ReturnType<typeof createHarvestController>): void {
	controller.syncHorizon();
}

async function settle(): Promise<void> {
	await vi.advanceTimersByTimeAsync(0);
}

/** Answer one call pair with the fixture reads (or the given exchange read). */
async function answer(
	h: Harness,
	index: number,
	exchange?: ExchangePriceRead
): Promise<void> {
	const ex = h.exchangeCalls[index];
	const hv = h.harvestCalls[index];
	hv.reply.resolve(await loadHarvestFamilies(hv.horizon));
	ex.reply.resolve(exchange ?? (await exchangeRead(ex.horizon)));
	await settle();
}

const latest = (h: Harness) => h.exchangeCalls.length - 1;

function body(controller: ReturnType<typeof createHarvestController>): FamilyBody {
	const b = controller.view.body;
	if (b?.kind !== 'family') throw new Error(`expected a family body, got ${b?.kind ?? 'none'}`);
	return b;
}

function storedPicks(h: Harness): Record<string, Record<string, string>> {
	return JSON.parse(h.dependencies.prefs.picks.value);
}

function started(prefs?: Parameters<typeof harness>[0]) {
	const h = harness(prefs);
	const controller = createHarvestController(h.dependencies);
	const stop = controller.start();
	horizonEffect(controller);
	return { h, controller, stop };
}

async function loaded(prefs?: Parameters<typeof harness>[0]) {
	const s = started(prefs);
	await answer(s.h, latest(s.h));
	return s;
}

/** Loaded, Dense picked, then a refresh that the server rejects. */
async function failedRefresh() {
	const s = await loaded();
	s.controller.togglePick(DENSE);
	void s.controller.load();
	s.h.exchangeCalls[latest(s.h)].reply.reject(new Error('server down'));
	await settle();
	return s;
}

/** Loaded, Dense picked, a Mercure publish, then the refetch answered with Lucent at 8c. */
async function refetched() {
	const s = await loaded();
	s.controller.togglePick(DENSE);
	s.h.fireUpdated();
	await vi.advanceTimersByTimeAsync(DELAY_MS);
	await answer(s.h, latest(s.h), await exchangeWithLucent('day', 8));
	return s;
}

const RESTORED = {
	picks: JSON.stringify({ fossil: { [DENSE]: 'keep' } }),
	horizon: 'recent',
	family: 'delirium'
};

beforeEach(() => {
	vi.useFakeTimers();
	vi.setSystemTime(NOW);
});

afterEach(() => {
	vi.useRealTimers();
	vi.restoreAllMocks();
});

describe('createHarvestController — populated', () => {
	it('reaches the ready state once both fixture ports answer', async () => {
		const { controller } = await loaded();
		expect(controller.view.state).toBe('ready');
	});

	it('opens Fossils on the verdict Yes — feed the cheap tiers', async () => {
		const { controller } = await loaded();
		expect(body(controller).verdict.headline).toBe('Yes — feed the cheap tiers');
	});

	it('opens Fossils with the headline +25.7c', async () => {
		const { controller } = await loaded();
		expect(body(controller).verdict.evText).toBe('+25.7c');
	});

	it('opens Fossils with the keepers regex of the fixture', async () => {
		const { controller } = await loaded();
		expect(body(controller).regex.keepers.text).toBe(KEEPERS_REGEX);
	});
});

describe('createHarvestController — failed refresh', () => {
	it('goes stale when a refresh is rejected', async () => {
		const { controller } = await failedRefresh();
		expect(controller.view.state).toBe('stale');
	});

	it('keeps the picked headline +24.3c through the rejected refresh', async () => {
		const { controller } = await failedRefresh();
		expect(body(controller).verdict.evText).toBe('+24.3c');
	});

	it('keeps the Dense pick through the rejected refresh', async () => {
		const { h } = await failedRefresh();
		expect(storedPicks(h)).toEqual({ fossil: { [DENSE]: 'keep' } });
	});
});

describe('createHarvestController — horizon loads', () => {
	it('drops a superseded recent response that lands after the day response', async () => {
		const { h, controller } = await loaded();
		controller.setHorizon('recent');
		horizonEffect(controller);
		const recent = latest(h);
		controller.setHorizon('day');
		horizonEffect(controller);
		await answer(h, latest(h), await exchangeWithLucent('day', 8));

		await answer(h, recent, await exchangeWithLucent('recent', 7));

		expect(body(controller).verdict.reason).toBe(
			'Buy Lucent Fossil (8.0c, cheapest feeder) and reroll until a keeper.'
		);
	});

	it('holds the day horizon after the superseded recent response lands', async () => {
		const { h, controller } = await loaded();
		controller.setHorizon('recent');
		horizonEffect(controller);
		const recent = latest(h);
		controller.setHorizon('day');
		horizonEffect(controller);
		await answer(h, latest(h));
		await answer(h, recent);

		expect(controller.exchange?.horizon).toBe('day');
	});

	it('loads once per explicit horizon change', async () => {
		const { h, controller } = await loaded();
		const before = h.exchangeCalls.length;

		controller.setHorizon('recent');
		horizonEffect(controller);

		expect(h.exchangeCalls.map((c) => c.horizon).slice(before)).toEqual(['recent']);
	});

	it('reading view after a horizon change starts no request', async () => {
		const { h, controller } = await loaded();
		const before = h.exchangeCalls.length;
		h.dependencies.prefs.horizon.value = 'recent';

		void controller.view;

		expect(h.exchangeCalls.length).toBe(before);
	});

	it('a Recent pref restored before the first answer loads Recent and drops the Day answer', async () => {
		const { h, controller } = started();
		h.dependencies.prefs.horizon.value = 'recent';
		horizonEffect(controller);

		await answer(h, 1, await exchangeWithLucent('recent', 7));
		await answer(h, 0, await exchangeWithLucent('day', 8));

		expect(body(controller).verdict.reason).toBe(
			'Buy Lucent Fossil (7.0c, cheapest feeder) and reroll until a keeper.'
		);
	});

	it('a Recent pref restored before the first answer shows Recent 6h in the status line', async () => {
		const { h, controller } = started();
		h.dependencies.prefs.horizon.value = 'recent';
		horizonEffect(controller);

		await answer(h, 1, await exchangeWithLucent('recent', 7));
		await answer(h, 0, await exchangeWithLucent('day', 8));

		expect(controller.view.status?.text).toContain('prices: Currency Exchange, Recent 6h');
	});

	it('a Recent pref restored after the first answer reloads and shows the Recent response', async () => {
		const { h, controller } = started();
		await answer(h, 0, await exchangeWithLucent('day', 8));

		h.dependencies.prefs.horizon.value = 'recent';
		horizonEffect(controller);
		await answer(h, 1, await exchangeWithLucent('recent', 7));

		expect(controller.exchange?.horizon).toBe('recent');
		expect(body(controller).verdict.reason).toBe(
			'Buy Lucent Fossil (7.0c, cheapest feeder) and reroll until a keeper.'
		);
	});
});

describe('createHarvestController — Mercure refetch', () => {
	it('does not reload before refetchDelay has passed', async () => {
		const { h } = await loaded();
		const calls = h.exchangeCalls.length;

		h.fireUpdated();
		await vi.advanceTimersByTimeAsync(DELAY_MS - 1);

		expect(h.exchangeCalls.length).toBe(calls);
	});

	it('shows the refetched prices', async () => {
		const { controller } = await refetched();
		expect(body(controller).verdict.reason).toBe(
			'Buy Lucent Fossil (8.0c, cheapest feeder) and reroll until a keeper.'
		);
	});

	it('keeps the picks through the refetch', async () => {
		const { controller } = await refetched();
		expect(body(controller).split.label).toBe('1 type moved by you');
	});

	it('the disposer stops a pending refetch', async () => {
		const { h, stop } = await loaded();
		const calls = h.exchangeCalls.length;

		h.fireUpdated();
		stop();
		await vi.advanceTimersByTimeAsync(DELAY_MS * 2);

		expect(h.exchangeCalls.length).toBe(calls);
	});
});

describe('createHarvestController — restored prefs', () => {
	it('opens on the restored Delirium Orbs tab', async () => {
		const { controller } = await loaded(RESTORED);
		expect(controller.view.tabs.find((t) => t.active)?.label).toBe('Delirium Orbs');
	});

	it('loads the restored recent horizon', async () => {
		const { h } = await loaded(RESTORED);
		expect(h.exchangeCalls[0].horizon).toBe('recent');
	});

	it('shows the restored Dense pick on switching to Fossils', async () => {
		const { controller } = await loaded(RESTORED);
		controller.setFamily('fossil');
		expect(body(controller).split.label).toBe('1 type moved by you');
	});
});

describe('createHarvestController — actions', () => {
	it('togglePick writes the fossil entry and leaves the delirium entry intact', async () => {
		const { h, controller } = await loaded({ picks: JSON.stringify({ delirium: { [DELIRIUM_WEAPONS]: 'keep' } }) });

		controller.togglePick(DENSE);

		expect(storedPicks(h)).toEqual({
			delirium: { [DELIRIUM_WEAPONS]: 'keep' },
			fossil: { [DENSE]: 'keep' }
		});
	});

	it('togglePick on a picked chip returns it to the computed side', async () => {
		const { h, controller } = await loaded();
		controller.togglePick(DENSE);

		controller.togglePick(DENSE);

		expect(storedPicks(h)).toEqual({});
	});

	it('moveTier(LOW) on Fossils leaves no LOW card among the feeders', async () => {
		const { controller } = await loaded();
		controller.moveTier('LOW');
		expect(body(controller).feeders.cards.map((c) => c.tier)).not.toContain('LOW');
	});

	it('moveTier(LOW) on Fossils puts all eleven LOW types among the keepers', async () => {
		const { controller } = await loaded();
		controller.moveTier('LOW');
		expect(body(controller).keepers.cards.find((c) => c.tier === 'LOW')?.chips).toHaveLength(11);
	});

	it('resetFamily clears the fossil entry only and the delirium entry survives', async () => {
		const { h, controller } = await loaded({
			picks: JSON.stringify({ delirium: { [DELIRIUM_WEAPONS]: 'keep' }, fossil: { [DENSE]: 'keep' } })
		});

		controller.resetFamily();

		expect(storedPicks(h)).toEqual({ delirium: { [DELIRIUM_WEAPONS]: 'keep' } });
	});
});

describe('createHarvestController — Copy', () => {
	it('copyRegex(keepers) writes the keepers regex to the clipboard', async () => {
		const { h, controller } = await loaded();
		await controller.copyRegex('keepers');
		expect(h.clipboard).toEqual([KEEPERS_REGEX]);
	});

	it('copyRegex(keepers) makes the keepers box read Copied and leaves feeders at Copy', async () => {
		const { controller } = await loaded();
		await controller.copyRegex('keepers');
		expect(controller.copied).toEqual({ feeders: false, keepers: true });
	});

	it('a pick that changes the keepers regex turns Copied back into Copy', async () => {
		const { controller } = await loaded();
		await controller.copyRegex('keepers');

		controller.togglePick(DENSE);

		expect(controller.copied.keepers).toBe(false);
	});

	it('a rejected clipboard write leaves the box at Copy', async () => {
		const { h, controller } = await loaded();
		h.dependencies.writeClipboard = async () => {
			throw new Error('denied');
		};

		await controller.copyRegex('keepers');

		expect(controller.copied.keepers).toBe(false);
	});

	it('a rejected clipboard write logs the failure as a warning', async () => {
		const { h, controller } = await loaded();
		const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
		h.dependencies.writeClipboard = async () => {
			throw new Error('denied');
		};

		await controller.copyRegex('keepers');

		expect(warn).toHaveBeenCalledWith('[harvest] clipboard write failed:', 'denied');
	});
});

describe('createHarvestController — one divine rate', () => {
	it('with divineRate null the status line reads 1 div = 360c', async () => {
		const { controller } = await loaded();
		expect(controller.view.status?.text).toContain('1 div = 360c');
	});

	it('with divineRate null the fossil divine line reads 15 feeders', async () => {
		const { controller } = await loaded();
		expect(body(controller).verdict.divineLine).toContain('15 feeders');
	});

	it('a shared divineRate answer sets the status line rate', async () => {
		const h = harness();
		h.dependencies.divineRate = () => 720;
		const controller = createHarvestController(h.dependencies);
		controller.start();
		horizonEffect(controller);
		await answer(h, 0);

		expect(controller.view.status?.text).toContain('1 div = 720c');
	});

	it('a shared divineRate answer sets the divine line', async () => {
		const h = harness();
		h.dependencies.divineRate = () => 720;
		const controller = createHarvestController(h.dependencies);
		controller.start();
		horizonEffect(controller);
		await answer(h, 0);

		expect(body(controller).verdict.divineLine).toContain('29 feeders');
	});
});

describe('createHarvestController — before prices', () => {
	it('stays loading while Harvest has answered and prices have not', async () => {
		const { h, controller } = started();
		h.harvestCalls[0].reply.resolve(await loadHarvestFamilies('day'));
		await settle();
		expect(controller.view.state).toBe('loading');
	});

	it('shows the family tabs while prices load', async () => {
		const { h, controller } = started();
		h.harvestCalls[0].reply.resolve(await loadHarvestFamilies('day'));
		await settle();
		expect(controller.view.tabs.map((t) => t.label)).toContain('Fossils');
	});

	it('a first load that fails shows Couldn\'t reach the server', async () => {
		const { h, controller } = started();
		h.exchangeCalls[0].reply.reject(new Error('down'));
		await settle();
		expect(controller.view.stateLine).toBe("Couldn't reach the server");
	});
});
