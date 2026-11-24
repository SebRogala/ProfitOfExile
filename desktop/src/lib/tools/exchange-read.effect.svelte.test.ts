/**
 * The exchange-read store (POE-477) keeps the latest native press result for
 * pages that mount after the press, through the real `listen` callback.
 */
import { flushSync } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ExchangeReadEvent } from './exchange-read.svelte';

const mocks = vi.hoisted(() => ({
	/** The handlers the store registered, by Tauri event name. */
	handlers: {} as Record<string, (event: { payload: unknown }) => void>
}));

vi.mock('@tauri-apps/api/event', () => ({
	listen: vi.fn(async (name: string, handler: (event: { payload: unknown }) => void) => {
		mocks.handlers[name] = handler;
		return () => {
			delete mocks.handlers[name];
		};
	})
}));

/** One top row as `exchange_read::finish` serializes it (WI-3). */
const SELECTION = {
	mode: 'buy',
	currency: 'div',
	ratio: 0.0048,
	oneSide: 'left',
	left: 1,
	right: 208,
	stock: 1543,
	wantName: 'Divine Orb',
	haveName: 'Orb of Alteration'
} as const;

const cleanups: (() => void)[] = [];
let store: typeof import('./exchange-read.svelte');

beforeEach(async () => {
	vi.resetModules();
	mocks.handlers = {};
	store = await import('./exchange-read.svelte');
	cleanups.push(store.startExchangeReadStore());
});

afterEach(() => {
	cleanups.splice(0).forEach((c) => c());
});

/** Delivers one native payload through the store's registered listener. */
async function press(payload: unknown) {
	await vi.waitFor(() => expect(mocks.handlers['exchange-read']).toBeTypeOf('function'));
	mocks.handlers['exchange-read']({ payload });
	flushSync();
}

/** A page-like subscriber: records every latest read its effect observes. */
function subscribe(): (ExchangeReadEvent | null)[] {
	const seen: (ExchangeReadEvent | null)[] = [];
	cleanups.push(
		$effect.root(() => {
			$effect(() => {
				seen.push(store.latestExchangeRead());
			});
		})
	);
	flushSync();
	return seen;
}

describe('exchange-read store', () => {
	// keep:happy
	it('keeps a press read before any page subscribes, with its press time', async () => {
		await press({ pressedAtMs: 1_700_000_000_123, selection: SELECTION });

		const seen = subscribe();

		expect(seen).toStrictEqual([{ pressedAtMs: 1_700_000_000_123, selection: SELECTION }]);
	});

	// keep:critical — silent: a consumer that cannot tell a repeat press is new
	// leaves the Companion showing the previous press's status as current.
	it('hands a subscriber an equal-looking second press as a new read', async () => {
		const seen = subscribe();

		await press({ pressedAtMs: 1_700_000_000_100, selection: SELECTION });
		await press({ pressedAtMs: 1_700_000_000_900, selection: SELECTION });

		expect(seen).toStrictEqual([
			null,
			{ pressedAtMs: 1_700_000_000_100, selection: SELECTION },
			{ pressedAtMs: 1_700_000_000_900, selection: SELECTION }
		]);
	});

	// keep:critical — silent: a stale successful selection left beside a failed
	// press would be filled into the calculator as if the new press had read it.
	it('replaces a successful read with a later error, keeping its code and no selection', async () => {
		await press({ pressedAtMs: 1_700_000_000_100, selection: SELECTION });
		await press({ pressedAtMs: 1_700_000_000_900, error: 'grab-failed' });

		const seen = subscribe();

		expect(seen).toStrictEqual([{ pressedAtMs: 1_700_000_000_900, error: 'grab-failed' }]);
	});
});
