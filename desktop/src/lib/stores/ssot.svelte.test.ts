import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { ssot, applySnapshot } from './ssot.svelte';

// The store reaches Rust through `invoke` only; the real core module cannot load
// outside a webview. Same shape as desktop/src/lib/compass/layout-loader.test.ts:13.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
// The `ssot-changed` listener is an optional eager nudge; stub it so the poll
// lifecycle tests are not at the mercy of a rejected real `listen`.
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => vi.fn()) }));

describe('applySnapshot', () => {
	it('maps league.name to the flat league field', () => {
		applySnapshot({ league: { name: 'Settlers' } });
		expect(ssot.league).toBe('Settlers');
	});

	it('maps a null league.name to null (fail-closed, not-yet-resolved)', () => {
		// Seed a resolved value first so a passing null-case proves the mapping
		// wrote null, not that it was already null.
		applySnapshot({ league: { name: 'Settlers' } });
		applySnapshot({ league: { name: null } });
		expect(ssot.league).toBeNull();
	});

	it('maps the resolving flag from the snapshot (true then false)', () => {
		applySnapshot({ league: { name: null }, resolving: true });
		expect(ssot.resolving).toBe(true);
		// A later snapshot with the flag cleared must flip it back — proves the
		// mapping writes the incoming value, not a one-way latch.
		applySnapshot({ league: { name: 'Mirage' }, resolving: false });
		expect(ssot.resolving).toBe(false);
	});

	it('defaults resolving to false when the field is absent', () => {
		applySnapshot({ league: { name: null }, resolving: true });
		applySnapshot({ league: { name: null } });
		expect(ssot.resolving).toBe(false);
	});

	it('maps the unreachable flag from the snapshot (true then false)', () => {
		applySnapshot({ league: { name: null }, resolving: true, unreachable: true });
		expect(ssot.unreachable).toBe(true);
		// A later snapshot with the flag cleared must flip it back — proves the
		// mapping writes the incoming value, not a one-way latch.
		applySnapshot({ league: { name: 'Mirage' }, resolving: false, unreachable: false });
		expect(ssot.unreachable).toBe(false);
	});

	it('defaults unreachable to false when the field is absent', () => {
		applySnapshot({ league: { name: null }, resolving: true, unreachable: true });
		applySnapshot({ league: { name: null } });
		expect(ssot.unreachable).toBe(false);
	});

	it('coerces a missing league slice to null instead of throwing', () => {
		applySnapshot({ league: { name: 'Settlers' } });
		// Malformed snapshot (e.g. an older/empty payload) must not leak undefined.
		applySnapshot({} as unknown as Parameters<typeof applySnapshot>[0]);
		expect(ssot.league).toBeNull();
	});
});

/**
 * The farming-market half of the store (POE-163) carries module-level mutable
 * state — the write counter and the per-field in-flight counters — so each test
 * re-imports the module to get its own instance. The `applySnapshot` block above
 * keeps using the statically imported instance and is unaffected.
 */
describe('farming market fields', () => {
	let mod: typeof import('./ssot.svelte');
	let invokeMock: ReturnType<typeof vi.mocked<typeof import('@tauri-apps/api/core').invoke>>;

	/** A snapshot carrying only league state, for the fields we are not exercising. */
	const league = { name: 'Mirage' };

	/** Calls of one Tauri command, in order, with their argument objects. */
	function callsOf(command: string): unknown[] {
		return invokeMock.mock.calls.filter((c) => c[0] === command).map((c) => c[1]);
	}

	beforeEach(async () => {
		vi.resetModules();
		vi.clearAllMocks();
		const core = await import('@tauri-apps/api/core');
		invokeMock = vi.mocked(core.invoke);
		// Default: `get_ssot` answers with a league-only snapshot, every setter succeeds.
		invokeMock.mockImplementation(async (command: string) =>
			command === 'get_ssot' ? { league } : undefined,
		);
		mod = await import('./ssot.svelte');
	});

	/** Seed all three market fields to known-good values that differ from the ones
	 *  each test then writes, so a passing assertion proves the write happened
	 *  rather than that the value was already there. */
	function seedKnownGood(): void {
		mod.applySnapshot({
			league,
			normalVariant: '1/20',
			dedicationVariant: '21/23',
			dedicationPool: 'skill',
		});
	}

	describe('applySnapshot', () => {
		it('maps the three market fields from the snapshot', () => {
			seedKnownGood();
			mod.applySnapshot({
				league,
				normalVariant: '20/0',
				dedicationVariant: '21/20',
				dedicationPool: 'transfigured',
			});
			expect(mod.ssot.normalVariant).toBe('20/0');
			expect(mod.ssot.dedicationVariant).toBe('21/20');
			expect(mod.ssot.dedicationPool).toBe('transfigured');
		});

		it('keeps the last known good value when the field is absent', () => {
			seedKnownGood();
			mod.applySnapshot({ league });
			expect(mod.ssot.normalVariant).toBe('1/20');
		});

		it('keeps the last known good value when the field is out of domain', () => {
			seedKnownGood();
			mod.applySnapshot({ league, normalVariant: '99/99' });
			expect(mod.ssot.normalVariant).toBe('1/20');
		});

		it('keeps the last known good value when the field is an empty string', () => {
			seedKnownGood();
			mod.applySnapshot({ league, normalVariant: '' });
			expect(mod.ssot.normalVariant).toBe('1/20');
		});

		it('does not write back when the field is an empty string', () => {
			seedKnownGood();
			invokeMock.mockClear();
			// Empty means Rust has nothing set yet — nothing to heal, unlike a
			// non-empty value Rust would keep stamping onto uploaded sessions.
			mod.applySnapshot({ league, normalVariant: '' });
			expect(callsOf('set_normal_variant')).toEqual([]);
		});

		it('heals Rust with the displayed value when the field is out of domain', () => {
			seedKnownGood();
			invokeMock.mockClear();
			mod.applySnapshot({ league, normalVariant: '99/99' });
			// Exactly one write-back, carrying what the UI shows — not the rejected
			// value and not a hardcoded default.
			expect(callsOf('set_normal_variant')).toEqual([{ variant: '1/20' }]);
		});

		it('does not throw on a malformed snapshot', () => {
			seedKnownGood();
			expect(() =>
				mod.applySnapshot({} as unknown as Parameters<typeof mod.applySnapshot>[0]),
			).not.toThrow();
		});

		it('keeps all three market fields at their last known good values on a malformed snapshot', () => {
			seedKnownGood();
			mod.applySnapshot({} as unknown as Parameters<typeof mod.applySnapshot>[0]);
			expect(mod.ssot.normalVariant).toBe('1/20');
			expect(mod.ssot.dedicationVariant).toBe('21/23');
			expect(mod.ssot.dedicationPool).toBe('skill');
		});
	});

	describe('setNormalVariant', () => {
		it('mutates the rune before the invoke round-trip resolves', () => {
			// Never settles: anything observable now happened synchronously.
			invokeMock.mockImplementation(() => new Promise<void>(() => {}));
			seedKnownGood();
			void mod.setNormalVariant('20/20');
			expect(mod.ssot.normalVariant).toBe('20/20');
		});

		it('writes through to the set_normal_variant command', async () => {
			seedKnownGood();
			await mod.setNormalVariant('20/20');
			expect(callsOf('set_normal_variant')).toEqual([{ variant: '20/20' }]);
		});

		it('leaves the optimistic value in place when the invoke rejects', async () => {
			invokeMock.mockRejectedValue(new Error('command failed'));
			seedKnownGood();
			// Never throws — same catch-and-warn contract as fetchSsot.
			await expect(mod.setNormalVariant('20/20')).resolves.toBeUndefined();
			expect(mod.ssot.normalVariant).toBe('20/20');
		});

		it('yields to Rust truth on the next snapshot after a rejected invoke', async () => {
			invokeMock.mockRejectedValue(new Error('command failed'));
			seedKnownGood();
			await mod.setNormalVariant('20/20');
			// The failed write must not keep the field pinned to the optimistic value.
			mod.applySnapshot({ league, normalVariant: '1/0' });
			expect(mod.ssot.normalVariant).toBe('1/0');
		});
	});

	describe('setDedicationVariant / setDedicationPool', () => {
		it('writes through to the set_dedication_variant command', async () => {
			seedKnownGood();
			await mod.setDedicationVariant('21/20');
			expect(mod.ssot.dedicationVariant).toBe('21/20');
			expect(callsOf('set_dedication_variant')).toEqual([{ variant: '21/20' }]);
		});

		it('writes through to the set_dedication_pool command', async () => {
			seedKnownGood();
			await mod.setDedicationPool('transfigured');
			expect(mod.ssot.dedicationPool).toBe('transfigured');
			expect(callsOf('set_dedication_pool')).toEqual([{ pool: 'transfigured' }]);
		});
	});

	describe('setDedicationSelection', () => {
		it('mutates variant and pool in one synchronous step', () => {
			invokeMock.mockImplementation(() => new Promise<void>(() => {}));
			seedKnownGood();
			void mod.setDedicationSelection('21/20', 'transfigured');
			expect(mod.ssot.dedicationVariant).toBe('21/20');
			expect(mod.ssot.dedicationPool).toBe('transfigured');
		});

		it('writes through to both dedication commands', async () => {
			seedKnownGood();
			await mod.setDedicationSelection('21/20', 'transfigured');
			expect(callsOf('set_dedication_variant')).toEqual([{ variant: '21/20' }]);
			expect(callsOf('set_dedication_pool')).toEqual([{ pool: 'transfigured' }]);
		});

		it('cannot be observed half-applied by a snapshot landing between the two invokes', async () => {
			let releaseVariantWrite: () => void = () => {};
			invokeMock.mockImplementation((command: string) => {
				if (command === 'set_dedication_variant') {
					return new Promise<void>((resolve) => {
						releaseVariantWrite = () => resolve();
					});
				}
				return Promise.resolve(undefined);
			});
			seedKnownGood();

			const selection = mod.setDedicationSelection('21/20', 'transfigured');
			// Rust has taken the variant but not yet the pool — the torn pair nobody picked.
			mod.applySnapshot({ league, dedicationVariant: '21/20', dedicationPool: 'skill' });
			expect(mod.ssot.dedicationPool).toBe('transfigured');
			expect(mod.ssot.dedicationVariant).toBe('21/20');

			releaseVariantWrite();
			await selection;
		});
	});

	describe('poll-vs-write ordering guard', () => {
		it('does not overwrite a field whose write round-trip is still outstanding', async () => {
			let releaseWrite: () => void = () => {};
			invokeMock.mockImplementation((command: string) => {
				if (command === 'set_normal_variant') {
					return new Promise<void>((resolve) => {
						releaseWrite = () => resolve();
					});
				}
				return Promise.resolve(undefined);
			});
			seedKnownGood();

			const write = mod.setNormalVariant('20/20');
			// A poll dispatched before the write lands mid-flight, still carrying the old value.
			mod.applySnapshot({ league, normalVariant: '1/20' });
			expect(mod.ssot.normalVariant).toBe('20/20');

			releaseWrite();
			await write;
		});

		/**
		 * Drive the ordering the in-flight guard alone misses: a `get_ssot` is
		 * dispatched, a write is issued *and settles*, and only then does the stale
		 * response arrive. Returns once the stale snapshot has been applied.
		 */
		async function applyStaleSnapshotAfterCompletedWrite(): Promise<void> {
			let deliverSnapshot: (snap: unknown) => void = () => {};
			invokeMock.mockImplementation((command: string) => {
				if (command === 'get_ssot') {
					return new Promise((resolve) => {
						deliverSnapshot = resolve;
					});
				}
				return Promise.resolve(undefined);
			});
			seedKnownGood();

			const poll = mod.fetchSsot();
			await mod.setNormalVariant('20/20');
			deliverSnapshot({ league: { name: 'Settlers' }, normalVariant: '1/20' });
			await poll;
		}

		it('does not overwrite a field with a snapshot dispatched before the write', async () => {
			await applyStaleSnapshotAfterCompletedWrite();
			expect(mod.ssot.normalVariant).toBe('20/20');
		});

		it('still applies the rest of that same stale snapshot', async () => {
			// The guard is per-field: league was never written, so it takes the update.
			await applyStaleSnapshotAfterCompletedWrite();
			expect(mod.ssot.league).toBe('Settlers');
		});
	});

	describe('poll lifecycle', () => {
		beforeEach(() => {
			vi.useFakeTimers();
		});

		afterEach(() => {
			vi.useRealTimers();
		});

		it('re-fetches get_ssot once per lazy poll interval', async () => {
			const stop = mod.startSsotStore();
			await vi.advanceTimersByTimeAsync(0);
			expect(callsOf('get_ssot')).toHaveLength(1);
			await vi.advanceTimersByTimeAsync(3000);
			expect(callsOf('get_ssot')).toHaveLength(2);
			stop();
		});

		it('stops re-fetching once the store is stopped', async () => {
			const stop = mod.startSsotStore();
			await vi.advanceTimersByTimeAsync(0);
			stop();
			await vi.advanceTimersByTimeAsync(9000);
			expect(callsOf('get_ssot')).toHaveLength(1);
		});
	});
});
