import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { ssot, applySnapshot, type ScreenSlice, type SsotSnapshot } from './ssot.svelte';
import type { MercenarySlice } from '../mercenaries/capture';
import { templeSliceDefault, type TempleSlice } from '../temple/slice';

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

/**
 * The module-flag half of the store (POE-128). Same re-import harness as the
 * market block: the guard records and the `modules` rune are module-level
 * mutable state, so each test gets its own instance.
 */
describe('module flags', () => {
	let mod: typeof import('./ssot.svelte');
	let invokeMock: ReturnType<typeof vi.mocked<typeof import('@tauri-apps/api/core').invoke>>;

	/** A snapshot carrying only league state, for the slices we are not exercising. */
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
		invokeMock.mockImplementation(async (command: string) =>
			command === 'get_ssot' ? { league, modules: {} } : undefined,
		);
		mod = await import('./ssot.svelte');
	});

	/** Put the store in a known module state through the production apply path. */
	function seedModules(modules: Record<string, boolean>): void {
		mod.applySnapshot({ league, modules });
	}

	/**
	 * Land a poll response carrying `modules` while a `mercenary` write is still
	 * outstanding — the window in which Rust cannot yet know the new value.
	 *
	 * The write is released before returning so no test leaks a pending invoke;
	 * the snapshot has already been applied by then, so the assertion still reads
	 * the guarded state.
	 */
	async function applySnapshotDuringMercenaryWrite(modules: Record<string, boolean>): Promise<void> {
		let releaseWrite: () => void = () => {};
		invokeMock.mockImplementation((command: string) => {
			if (command === 'set_module_enabled') {
				return new Promise<void>((resolve) => {
					releaseWrite = () => resolve();
				});
			}
			return Promise.resolve(undefined);
		});
		seedModules({ mercenary: false });

		const write = mod.setModuleEnabled('mercenary', true);
		mod.applySnapshot({ league, modules });

		releaseWrite();
		await write;
	}

	describe('applySnapshot', () => {
		it('applies the snapshot value for a module flag', () => {
			// Seeded to the opposite value, so a pass proves the apply wrote it.
			seedModules({ mercenary: false });
			mod.applySnapshot({ league, modules: { mercenary: true } });
			expect(mod.ssot.modules.mercenary).toBe(true);
		});

		it('records a module id the webview knows nothing about', () => {
			// Rust owns the registry: a module added there must reach the store
			// without a webview-side allow-list to update in lockstep.
			mod.applySnapshot({ league, modules: { from_the_future: true } });
			expect(mod.ssot.modules.from_the_future).toBe(true);
		});

		it('does not overwrite a module flag whose write round-trip is still outstanding', async () => {
			await applySnapshotDuringMercenaryWrite({ mercenary: false });
			expect(mod.ssot.modules.mercenary).toBe(true);
		});

		it('still applies another module flag carried by that same snapshot', async () => {
			// The guard is per key: `scout` was never written, so it takes the update
			// even though `mercenary` in the same payload is suppressed.
			await applySnapshotDuringMercenaryWrite({ mercenary: false, scout: true });
			expect(mod.ssot.modules.scout).toBe(true);
		});

		it('drops a local module flag the snapshot no longer reports', () => {
			seedModules({ mercenary: true, from_the_future: true });
			// A downgrade unregisters the module — the stale toggle must go with it.
			mod.applySnapshot({ league, modules: { mercenary: true } });
			expect(mod.ssot.modules).toEqual({ mercenary: true });
		});

		it('keeps a local module flag missing from the snapshot while its write is outstanding', async () => {
			// Rust has not seen the write yet, so its map has no such key — dropping
			// it here would flip the toggle back under the user's click.
			await applySnapshotDuringMercenaryWrite({});
			expect(mod.ssot.modules.mercenary).toBe(true);
		});

		it('keeps the known flags when the snapshot carries no modules map at all', () => {
			seedModules({ mercenary: true });
			// Absent means "not known" (malformed or older payload), not "no modules":
			// Rust always sends the map, empty at worst.
			mod.applySnapshot({ league });
			expect(mod.ssot.modules.mercenary).toBe(true);
		});

		it('does not let a module write guard a market field of the same name', async () => {
			// Module ids are free-form Rust data; only the `module:` prefix on the
			// guard key keeps them out of the market fields' namespace.
			let releaseWrite: () => void = () => {};
			invokeMock.mockImplementation((command: string) => {
				if (command === 'set_module_enabled') {
					return new Promise<void>((resolve) => {
						releaseWrite = () => resolve();
					});
				}
				return Promise.resolve(undefined);
			});
			mod.applySnapshot({ league, normalVariant: '1/20' });

			const write = mod.setModuleEnabled('normalVariant', true);
			mod.applySnapshot({ league, normalVariant: '20/0' });

			expect(mod.ssot.normalVariant).toBe('20/0');
			releaseWrite();
			await write;
		});
	});

	describe('setModuleEnabled', () => {
		it('mutates the flag before the invoke round-trip resolves', () => {
			// Never settles: anything observable now happened synchronously.
			invokeMock.mockImplementation(() => new Promise<void>(() => {}));
			seedModules({ mercenary: false });
			void mod.setModuleEnabled('mercenary', true);
			expect(mod.ssot.modules.mercenary).toBe(true);
		});

		it('writes through to the set_module_enabled command', async () => {
			seedModules({ mercenary: false });
			await mod.setModuleEnabled('mercenary', true);
			expect(callsOf('set_module_enabled')).toEqual([{ id: 'mercenary', enabled: true }]);
		});

		it('leaves the optimistic flag in place when the invoke rejects', async () => {
			// Rust rejects an unregistered id with an Err, and IPC can fail on its
			// own. Never throws — same catch-and-warn contract as fetchSsot.
			invokeMock.mockRejectedValue(new Error('unknown module id: mercenary'));
			seedModules({ mercenary: false });
			await expect(mod.setModuleEnabled('mercenary', true)).resolves.toBeUndefined();
			expect(mod.ssot.modules.mercenary).toBe(true);
		});

		it('yields to Rust truth on the next snapshot after a rejected invoke', async () => {
			invokeMock.mockRejectedValue(new Error('unknown module id: mercenary'));
			seedModules({ mercenary: false });
			await mod.setModuleEnabled('mercenary', true);
			// The failed write must not keep the toggle pinned to a value Rust
			// never accepted.
			mod.applySnapshot({ league, modules: { mercenary: false } });
			expect(mod.ssot.modules.mercenary).toBe(false);
		});

		it('does not unguard a still-outstanding second write when the first one settles', async () => {
			// Two clicks on the same toggle overlap: off, then on again before the
			// off round-trip has settled.
			const releases: Array<() => void> = [];
			invokeMock.mockImplementation((command: string) => {
				if (command === 'set_module_enabled') {
					return new Promise<void>((resolve) => {
						releases.push(() => resolve());
					});
				}
				return Promise.resolve(undefined);
			});
			seedModules({ mercenary: true });

			const off = mod.setModuleEnabled('mercenary', false);
			const on = mod.setModuleEnabled('mercenary', true);
			releases[0]();
			await off;

			// Rust has taken the first write only, so its map legitimately reports
			// `false` — the value that predates the second, still-outstanding write.
			mod.applySnapshot({ league, modules: { mercenary: false } });
			expect(mod.ssot.modules.mercenary).toBe(true);

			releases[1]();
			await on;
		});
	});
});

/**
 * The mercenary slice (POE-165) is Rust-owned and read-only in the webview, so
 * it has no write path and no guard record — what there IS to get wrong is the
 * apply: taking the slice whole, and leaving the last known one alone when a
 * payload does not carry it. Same re-import harness as the blocks above so the
 * default state is pristine rather than whatever an earlier test left behind.
 */
describe('mercenary slice', () => {
	let mod: typeof import('./ssot.svelte');

	const league = { name: 'Mirage' };

	/** A slice as Rust publishes one: a live capture of a one-row recruit window. */
	function liveSlice(): MercenarySlice {
		return {
			status: 'live',
			burstSpeaker: null,
			capture: {
				capturedAtMs: 1_700_000_000_000,
				live: true,
				scale: 1,
				screen: [2560, 1440],
				header: { name: 'Cai, the Lout', class: 'Shock Ambusher', level: 70, wager: 1028 },
				rows: [
					{
						index: 0,
						skill: {
							raw: 'Ice Shot',
							ids: ['mercenary.skill_11495'],
							name: 'Ice Shot',
							score: 0.99,
							state: 'matched',
						},
						supports: [],
					},
				],
			},
			learnedFamilies: ['Return--3'],
			pooledFamilies: ['Return--3'],
			seededFamilies: ['Fork'],
			lastError: 'ocr engine slow',
			geometrySource: 'file',
			sourcesOff: ['guide-a'],
			sync: {
				lastPullMs: 1_700_000_000_000,
				lastPull: 'merged',
				pooledSamples: 1,
				queuedUploads: 0,
				lastError: null,
			},
			trade: {
				status: 'idle',
				queryHash: null,
				url: null,
				result: null,
				error: null,
				searchesUsed: 0,
			},
			tradeAuto: true,
			tierFloor: 3,
		};
	}

	beforeEach(async () => {
		vi.resetModules();
		vi.clearAllMocks();
		const core = await import('@tauri-apps/api/core');
		vi.mocked(core.invoke).mockResolvedValue({ league });
		mod = await import('./ssot.svelte');
	});

	it('reports the module off with no capture before the first snapshot', () => {
		expect(mod.ssot.mercenary.status).toBe('off');
		expect(mod.ssot.mercenary.capture).toBeNull();
	});

	it('applies the slice the snapshot carries', () => {
		mod.applySnapshot({ league, mercenary: liveSlice() });
		expect(mod.ssot.mercenary.status).toBe('live');
		expect(mod.ssot.mercenary.capture?.header.name).toBe('Cai, the Lout');
		expect(mod.ssot.mercenary.capture?.rows[0].skill.name).toBe('Ice Shot');
		expect(mod.ssot.mercenary.geometrySource).toBe('file');
		expect(mod.ssot.mercenary.learnedFamilies).toEqual(['Return--3']);
		// The pool's two fields ride the same whole-slice apply as the rest —
		// they are what the page's chips and its pool line read.
		expect(mod.ssot.mercenary.pooledFamilies).toEqual(['Return--3']);
		// The gem-art seeds ride the same apply (POE-208) — family names, not
		// `--<tier>` keys, and their own chip group reads them.
		expect(mod.ssot.mercenary.seededFamilies).toEqual(['Fork']);
		expect(mod.ssot.mercenary.sync.lastPull).toBe('merged');
		expect(mod.ssot.mercenary.sync.pooledSamples).toBe(1);
	});

	it('keeps the last known slice when the snapshot carries no mercenary field', () => {
		mod.applySnapshot({ league, mercenary: liveSlice() });
		mod.applySnapshot({ league });
		expect(mod.ssot.mercenary.status).toBe('live');
		expect(mod.ssot.mercenary.capture?.rows).toHaveLength(1);
	});

	it('replaces the whole slice instead of merging it into the previous one', () => {
		// The window closed: Rust retired the capture, dropped the error and
		// forgot the learned template. A field-wise merge would leave the old row,
		// the old error and the old family standing under the new status.
		mod.applySnapshot({ league, mercenary: liveSlice() });
		mod.applySnapshot({
			league,
			mercenary: {
				status: 'idle',
				burstSpeaker: null,
				capture: null,
				learnedFamilies: [],
				pooledFamilies: [],
				seededFamilies: [],
				lastError: null,
				geometrySource: 'default',
				sourcesOff: [],
				sync: {
					lastPullMs: null,
					lastPull: 'never',
					pooledSamples: 0,
					queuedUploads: 0,
					lastError: null,
				},
				trade: {
					status: 'idle',
					queryHash: null,
					url: null,
					result: null,
					error: null,
					searchesUsed: 0,
				},
				tradeAuto: true,
				tierFloor: 3,
			},
		});
		expect(mod.ssot.mercenary.status).toBe('idle');
		expect(mod.ssot.mercenary.capture).toBeNull();
		expect(mod.ssot.mercenary.lastError).toBeNull();
		// The pool's two fields are part of the whole-slice replacement too: a
		// field-wise merge would leave the previous device's pooled chips and a
		// stale "merged" line standing under the new slice.
		expect(mod.ssot.mercenary.pooledFamilies).toEqual([]);
		// Same whole-slice rule for the seeds: a family-wise merge would leave
		// a chip for a seed the new store no longer holds, and its ✕ would
		// blocklist a family nothing had seeded.
		expect(mod.ssot.mercenary.seededFamilies).toEqual([]);
		expect(mod.ssot.mercenary.sync.lastPull).toBe('never');
		expect(mod.ssot.mercenary.learnedFamilies).toEqual([]);
		expect(mod.ssot.mercenary.sourcesOff).toEqual([]);
	});

	/**
	 * The enabled-guide echo (POE-199) is what the page and the verdict overlay
	 * both evaluate against, so it has to survive the apply like any other
	 * field of the slice — a dropped echo would silently re-enable a guide the
	 * user switched off, on both windows at once.
	 */
	it('applies the enabled-guide echo the snapshot carries', () => {
		mod.applySnapshot({ league, mercenary: liveSlice() });
		expect(mod.ssot.mercenary.sourcesOff).toEqual(['guide-a']);
	});

	/**
	 * Writing goes through Rust, not through the rune: the command carries the
	 * off-list, and the store re-fetches so the echo Rust accepted is what both
	 * windows read. A local optimistic write would put the page one poll ahead
	 * of the overlay, which is the disagreement POE-199 exists to end.
	 */
	it('sends the off-list to Rust and re-fetches rather than writing the rune', async () => {
		const core = await import('@tauri-apps/api/core');
		const invokeMock = vi.mocked(core.invoke);
		invokeMock.mockResolvedValue({ league });

		const rejection = await mod.setMercSourcesOff(['guide-b']);

		expect(rejection).toBeNull();
		expect(invokeMock.mock.calls.filter((c) => c[0] === 'merc_set_sources_off')).toEqual([
			['merc_set_sources_off', { sourcesOff: ['guide-b'] }],
		]);
		expect(invokeMock.mock.calls.some((c) => c[0] === 'get_ssot')).toBe(true);
	});

	/**
	 * Rust VALIDATES the ids, so a refusal is a thing the user did and has to
	 * come back to the caller AND reach the app log — the only error channel a
	 * shipped build can open. Throwing, or warning to a console nobody sees,
	 * would leave a toggle that silently does nothing.
	 */
	it('returns the rejection and logs it when Rust refuses the off-list', async () => {
		const core = await import('@tauri-apps/api/core');
		const invokeMock = vi.mocked(core.invoke);
		invokeMock.mockImplementation(async (command: string) => {
			if (command === 'merc_set_sources_off') throw new Error('"guide-zzz" is not a guide');
			return { league };
		});

		const rejection = await mod.setMercSourcesOff(['guide-zzz']);

		expect(rejection).toContain('guide-zzz');
		const logged = invokeMock.mock.calls.find((c) => c[0] === 'app_log_from_frontend');
		expect(logged?.[1]).toMatchObject({
			msg: expect.stringContaining('merc_set_sources_off failed'),
		});
	});

	/**
	 * The trade half (POE-202) rides the same whole-slice apply as the rest, and
	 * every field below differs from the empty default on purpose — a dropped
	 * field would land on its default and pass a laxer assertion by coincidence.
	 */
	function tradedSlice(): MercenarySlice {
		return {
			...liveSlice(),
			trade: {
				status: 'done',
				queryHash: 'a1b2c3',
				url: 'https://www.pathofexile.com/trade/search/Mirage/abc',
				result: {
					queryHash: 'a1b2c3',
					league: 'Mirage',
					total: 87,
					listings: [
						{
							chaosPrice: 1250,
							currency: 'divine',
							amount: 5,
							account: 'SellerOne',
							indexedAt: '2026-08-26T01:00:00Z',
						},
					],
					floorChaos: 1250,
					medianChaos: 1400,
					fetchedAtMs: 1_700_000_000_000,
					truncated: true,
				},
				error: null,
				searchesUsed: 2,
			},
			tradeAuto: false,
			tierFloor: 1,
		};
	}

	/**
	 * The trade state is STORED on the Rust slice rather than composed from
	 * settings, and the merc overlay is expected to read the same one — so a
	 * field this apply drops is a field two windows disagree about.
	 */
	it('applies the trade state the snapshot carries', () => {
		mod.applySnapshot({ league, mercenary: tradedSlice() });
		expect(mod.ssot.mercenary.trade.status).toBe('done');
		expect(mod.ssot.mercenary.trade.queryHash).toBe('a1b2c3');
		expect(mod.ssot.mercenary.trade.url).toContain('/trade/search/Mirage/abc');
		expect(mod.ssot.mercenary.trade.result?.total).toBe(87);
		expect(mod.ssot.mercenary.trade.result?.truncated).toBe(true);
		expect(mod.ssot.mercenary.trade.searchesUsed).toBe(2);
	});

	/**
	 * Both settings echoes default to the permissive end (`true` / `3`), so an
	 * apply that skipped them would silently re-arm an auto-search the user
	 * switched off and re-tighten a floor they widened.
	 */
	it('applies the auto-search and tier-floor echoes the snapshot carries', () => {
		mod.applySnapshot({ league, mercenary: tradedSlice() });
		expect(mod.ssot.mercenary.tradeAuto).toBe(false);
		expect(mod.ssot.mercenary.tierFloor).toBe(1);
	});

	/**
	 * Fail-closed on absence, same rule as the capture: a payload that does not
	 * carry the slice is a malformed or older one, and blanking the listings
	 * the reader is looking at would be a lie about what the search found.
	 */
	it('keeps the last known trade result when the snapshot carries no mercenary field', () => {
		mod.applySnapshot({ league, mercenary: tradedSlice() });
		mod.applySnapshot({ league });
		expect(mod.ssot.mercenary.trade.result?.total).toBe(87);
		expect(mod.ssot.mercenary.tradeAuto).toBe(false);
		expect(mod.ssot.mercenary.tierFloor).toBe(1);
	});

	/**
	 * The trigger loop in Rust reads the toggle every tick, so the write goes
	 * to Rust and the echo comes back through a re-fetch. A local optimistic
	 * write would let the page believe the auto-search is off while the loop
	 * that actually decides is still searching.
	 */
	it('sends the auto flag to Rust and re-fetches rather than writing the rune', async () => {
		const core = await import('@tauri-apps/api/core');
		const invokeMock = vi.mocked(core.invoke);
		invokeMock.mockResolvedValue({ league });

		const rejection = await mod.setMercTradeAuto(false);

		expect(rejection).toBeNull();
		expect(invokeMock.mock.calls.filter((c) => c[0] === 'merc_set_trade_auto')).toEqual([
			['merc_set_trade_auto', { auto: false }],
		]);
		expect(invokeMock.mock.calls.some((c) => c[0] === 'get_ssot')).toBe(true);
	});

	/**
	 * Same round trip for the floor, and the argument NAME is the contract:
	 * Tauri matches command parameters by name, so a renamed key reaches Rust
	 * as a missing argument and the control silently does nothing.
	 */
	it('sends the tier floor to Rust and re-fetches rather than writing the rune', async () => {
		const core = await import('@tauri-apps/api/core');
		const invokeMock = vi.mocked(core.invoke);
		invokeMock.mockResolvedValue({ league });

		const rejection = await mod.setMercTierFloor(1);

		expect(rejection).toBeNull();
		expect(invokeMock.mock.calls.filter((c) => c[0] === 'merc_set_tier_floor')).toEqual([
			['merc_set_tier_floor', { floor: 1 }],
		]);
		expect(invokeMock.mock.calls.some((c) => c[0] === 'get_ssot')).toBe(true);
	});

	/**
	 * Rust clamps the floor to 1..=3 and refuses anything else. The refusal has
	 * to come back to the caller AND reach the app log — the only error channel
	 * a shipped build can open — or the select silently snaps back on the next
	 * poll with no explanation.
	 */
	it('returns the rejection and logs it when Rust refuses the tier floor', async () => {
		const core = await import('@tauri-apps/api/core');
		const invokeMock = vi.mocked(core.invoke);
		invokeMock.mockImplementation(async (command: string) => {
			if (command === 'merc_set_tier_floor') throw new Error('tier floor must be 1..=3, got 7');
			return { league };
		});

		const rejection = await mod.setMercTierFloor(7);

		expect(rejection).toContain('tier floor must be 1..=3');
		const logged = invokeMock.mock.calls.find((c) => c[0] === 'app_log_from_frontend');
		expect(logged?.[1]).toMatchObject({
			msg: expect.stringContaining('merc_set_tier_floor failed'),
		});
	});
});


/**
 * The temple slice (POE-171). Rust-owned and read-only like the mercenary one,
 * so the apply half is the same contract — whole replace, keep the last known
 * on absence — and gets the same coverage.
 *
 * What is NEW here is the four settings commands. They do not write the rune:
 * Rust validates them, echoes the accepted value back onto the slice and this
 * store re-fetches, so what the tests below pin is the command name, the
 * argument shape, the re-fetch, and — the part with a user consequence — that a
 * REJECTION reaches the app log and comes back to the caller instead of
 * vanishing into a console nobody can open in a shipped build.
 */
describe('temple slice', () => {
	let mod: typeof import('./ssot.svelte');
	let invokeMock: ReturnType<typeof vi.mocked<typeof import('@tauri-apps/api/core').invoke>>;

	const league = { name: 'Mirage' };

	/** Calls of one Tauri command, in order, with their argument objects. */
	function callsOf(command: string): unknown[] {
		return invokeMock.mock.calls.filter((c) => c[0] === command).map((c) => c[1]);
	}

	/** A slice as Rust publishes one after a completed read. */
	function readSlice(): TempleSlice {
		return {
			...templeSliceDefault(),
			status: 'read',
			layout: {
				slots: [
					{ slot: 'C1', name: 'Locus of Corruption', tier: 3, exact: true, known: true, current: true }
				],
				doors: ['C1-C2'],
				uncertain: [],
				unresolvedIncident: [],
				markerError: null,
				current: 'C1',
				scale: 0.99,
				ncc: 0.94,
				confidence: 'high',
				origin: [900, 900],
				// All 13, as Rust publishes them — `LayoutView.centres` is
				// `[[i32; 2]; 13]` and the Entrance sits at the origin (index 11
				// of `Slot::ALL`). Kept identical to the Rust sample in
				// `temple/slice.rs`, so the two fixtures cannot drift.
				centres: [
					[900, 465],
					[795, 569],
					[1005, 569],
					[690, 673],
					[900, 673],
					[1110, 673],
					[585, 777],
					[795, 777],
					[1005, 777],
					[1215, 777],
					[690, 881],
					[900, 900],
					[1110, 881]
				],
				// POE-244's two fields. The store copies the slice through
				// verbatim and reads neither, so the fixture carries the shape
				// and not a board's worth of rectangles.
				rois: [],
				diamond: null
			},
			panel: {
				room: 'Locus of Corruption',
				roomRect: [1300, 100, 152, 20],
				offers: [],
				incursionsRemaining: 6
			},
			advice: {
				recommendations: [
					{
						headline: 'upgrade → Locus of Corruption',
						doorsLabel: 'C1-C2',
						doors: ['C1-C2'],
						architectIndex: 0,
						ev: 12,
						risk: null,
						reasons: ['R1: connects toward the top']
					}
				],
				gambles: [],
				mapAction: 'continue',
				warnings: [],
				forcedKill: false
			},
			mode: 'chase',
			keys: 2,
			unknownRooms: ['D3'],
			lastReadAt: 1_700_000_000_000
		};
	}

	beforeEach(async () => {
		vi.resetModules();
		vi.clearAllMocks();
		const core = await import('@tauri-apps/api/core');
		invokeMock = vi.mocked(core.invoke);
		invokeMock.mockImplementation(async (command: string) =>
			command === 'get_ssot' ? { league } : undefined,
		);
		mod = await import('./ssot.svelte');
	});

	describe('applying a snapshot', () => {
		it('reports the Rust default before the first snapshot', () => {
			expect(mod.ssot.temple).toEqual(templeSliceDefault());
		});

		it('applies the slice the snapshot carries', () => {
			mod.applySnapshot({ league, temple: readSlice() });
			expect(mod.ssot.temple.status).toBe('read');
			expect(mod.ssot.temple.layout?.current).toBe('C1');
			expect(mod.ssot.temple.advice?.recommendations[0].reasons).toEqual([
				'R1: connects toward the top'
			]);
			expect(mod.ssot.temple.keys).toBe(2);
		});

		it('keeps the last known slice when the snapshot carries no temple field', () => {
			mod.applySnapshot({ league, temple: readSlice() });
			mod.applySnapshot({ league });
			expect(mod.ssot.temple.status).toBe('read');
			expect(mod.ssot.temple.layout?.doors).toEqual(['C1-C2']);
		});

		it('replaces the whole slice instead of merging it into the previous one', () => {
			// The panel closed: Rust published a board-less slice. A field-wise
			// merge would leave the old layout and the old recommendation
			// standing under a status that says there is no panel — a move the
			// player could still act on, against a board that is gone.
			mod.applySnapshot({ league, temple: readSlice() });
			mod.applySnapshot({
				league,
				temple: { ...templeSliceDefault(), status: 'panel_not_visible', keys: 2 },
			});
			expect(mod.ssot.temple.status).toBe('panel_not_visible');
			expect(mod.ssot.temple.layout).toBeNull();
			expect(mod.ssot.temple.advice).toBeNull();
			expect(mod.ssot.temple.unknownRooms).toEqual([]);
		});

		it('takes an incomplete payload as it stands rather than filling it from the last one', () => {
			// A complete replacement and a field-wise merge look identical while
			// Rust sends every field, so the difference only shows on a payload
			// that does NOT — an older or truncated one. Whole-replace is the
			// contract: a board that is gone must not be filled back in from the
			// previous read, because a stale layout under a fresh status is a
			// move the player could still act on.
			mod.applySnapshot({ league, temple: readSlice() });
			mod.applySnapshot({
				league,
				temple: { status: 'idle', keys: 1 } as unknown as TempleSlice,
			});
			expect(mod.ssot.temple.status).toBe('idle');
			expect(mod.ssot.temple.layout).toBeNull();
			expect(mod.ssot.temple.advice).toBeNull();
		});

		it('lands an omitted field as the null its type promises, not as undefined', () => {
			// The types are read as a guarantee by every consumer — the overlay
			// draws `unknownRooms.length` — so a truncated payload that left
			// them `undefined` made the declared shape a lie, and the crash
			// would land in the surface rather than here.
			mod.applySnapshot({
				league,
				temple: { status: 'idle', keys: 1 } as unknown as TempleSlice,
			});
			expect(mod.ssot.temple.panel).toBeNull();
			expect(mod.ssot.temple.mode).toBeNull();
			expect(mod.ssot.temple.lastReadAt).toBeNull();
			expect(mod.ssot.temple.calibration).toBeNull();
			expect(mod.ssot.temple.lastError).toBeNull();
			expect(mod.ssot.temple.unknownRooms).toEqual([]);
		});

		it('defaults a missing waitingForPanel to false, never undefined', () => {
			// A build from before POE-249 sends no flag at all, and the notice's
			// gate is `slice.waitingForPanel && !overlayShowsBoard(...)`. An
			// `undefined` there is falsy by accident rather than by contract, and
			// it makes the declared boolean a lie for anything that later reads it
			// as one.
			mod.applySnapshot({
				league,
				temple: { status: 'idle', keys: 1 } as unknown as TempleSlice,
			});
			expect(mod.ssot.temple.waitingForPanel).toBe(false);
		});

		it('defaults an offer\'s missing grade and lineTop to null, never undefined', () => {
			// The same rule one level deeper (POE-249). Both fields are
			// `serde(default)` in Rust, so a payload from a build before them
			// carries an offer without them — and `offerBoxes` decides whether
			// to print a rating line by testing `grade === null`. An
			// `undefined` there is falsy by accident, and it makes the declared
			// `string | null` a lie.
			const stale = {
				...readSlice(),
				panel: {
					room: 'Locus of Corruption',
					roomRect: null,
					// An offer as a pre-POE-249 build sends it: no `grade`, no
					// `lineTop`. Cast because the declared type is exactly what
					// this payload does not satisfy.
					offers: [
						{
							index: 0,
							architectName: 'Guatelitzi',
							kind: 'upgrade',
							printedTarget: "Sadist's Den",
							displayName: 'Torment Cells',
							builtTier: 2,
							rect: null
						}
					],
					incursionsRemaining: 6
				}
			} as unknown as TempleSlice;
			mod.applySnapshot({ league, temple: stale });
			const offer = mod.ssot.temple.panel?.offers[0];
			expect(offer?.grade).toBeNull();
			expect(offer?.lineTop).toBeNull();
			// The fields that WERE sent are untouched, so the filling cannot be
			// passing by blanking the offer.
			expect(offer?.architectName).toBe('Guatelitzi');
			expect(offer?.builtTier).toBe(2);
		});

		it('keeps the payload\'s own value for a field it does carry', () => {
			// The filling above must not reach a field that IS present, or a
			// board would be blanked by the very code meant to type it honestly.
			mod.applySnapshot({ league, temple: readSlice() });
			expect(mod.ssot.temple.layout?.doors).toEqual(['C1-C2']);
			expect(mod.ssot.temple.unknownRooms).toEqual(['D3']);
			expect(mod.ssot.temple.lastReadAt).toBe(1_700_000_000_000);
		});

		it('ignores a payload whose status is not one Rust publishes', () => {
			// `status` is the field every surface switches on. An unrecognised
			// one falls through every branch at once: the overlay decides it has
			// no board and draws nothing, the page renders an empty badge, and
			// neither says why. Keeping the last known board is the honest
			// answer — it is what the reader last actually saw.
			mod.applySnapshot({ league, temple: readSlice() });
			mod.applySnapshot({
				league,
				temple: { ...readSlice(), status: 'panel_missing' } as unknown as TempleSlice,
			});
			expect(mod.ssot.temple.status).toBe('read');
			expect(mod.ssot.temple.layout?.current).toBe('C1');
		});

		it('reports an unknown status to the app log rather than dropping it silently', async () => {
			mod.applySnapshot({
				league,
				temple: { ...readSlice(), status: '' } as unknown as TempleSlice,
			});
			await Promise.resolve();
			const logged = callsOf('app_log_from_frontend') as { msg: string }[];
			expect(logged).toHaveLength(1);
			expect(logged[0].msg).toContain('unknown status');
		});

		it('reports a status that stays bad once, not once per poll', async () => {
			// Whatever produced the bad status sits behind a 3-second poll, so
			// the same payload comes back twenty times a minute. The repeats add
			// nothing the first line did not say and bury the lines around them.
			const bad = { ...readSlice(), status: 'panel_missing' } as unknown as TempleSlice;
			mod.applySnapshot({ league, temple: bad });
			mod.applySnapshot({ league, temple: bad });
			await Promise.resolve();
			expect(callsOf('app_log_from_frontend')).toHaveLength(1);
		});

		it('reports a second, different bad status', async () => {
			// Deduplication is per value, not a latch: a payload that went from
			// one unrecognised status to another is new information, and a
			// blanket "already reported" would hide it.
			mod.applySnapshot({
				league,
				temple: { ...readSlice(), status: 'panel_missing' } as unknown as TempleSlice,
			});
			mod.applySnapshot({
				league,
				temple: { ...readSlice(), status: 'board_gone' } as unknown as TempleSlice,
			});
			await Promise.resolve();
			const logged = callsOf('app_log_from_frontend') as { msg: string }[];
			expect(logged).toHaveLength(2);
			expect(logged[1].msg).toContain('board_gone');
		});

		it('reports the same status again once a good payload has come between', async () => {
			// A recovery ends the episode. The next occurrence is a new fault,
			// and a permanent mute would leave it unrecorded.
			const bad = { ...readSlice(), status: 'panel_missing' } as unknown as TempleSlice;
			mod.applySnapshot({ league, temple: bad });
			mod.applySnapshot({ league, temple: readSlice() });
			mod.applySnapshot({ league, temple: bad });
			await Promise.resolve();
			expect(callsOf('app_log_from_frontend')).toHaveLength(2);
		});
	});

	describe('the settings commands', () => {
		it('sends the key count under the argument name the command takes', async () => {
			expect(await mod.setTempleKeys(2)).toBeNull();
			expect(callsOf('temple_set_keys')).toEqual([{ keys: 2 }]);
		});

		it('sends the config flags as one nested object', async () => {
			const config = { artefactsOfTheVaal: false, scarabOfTimelines: true };
			expect(await mod.setTempleConfig(config)).toBeNull();
			expect(callsOf('temple_set_config')).toEqual([{ config }]);
		});

		it('sends the four profile fields as one nested object', async () => {
			const profile = {
				apexScore: 3.5,
				pathCost: 1.25,
				rerollUntilFavourable: true,
				r4KeepUpgradeTargets: false,
			};
			expect(await mod.setTempleProfile(profile)).toBeNull();
			expect(callsOf('temple_set_profile')).toEqual([{ profile }]);
		});

		it('re-arms with no arguments', async () => {
			expect(await mod.rearmTemple()).toBeNull();
			expect(callsOf('temple_rearm')).toEqual([{}]);
		});

		it('re-fetches the snapshot so the echo lands without waiting out a poll', async () => {
			// The slice is Rust-owned, so the control the user just moved only
			// updates when a snapshot comes back. Without this the checkbox
			// would sit at its old value for up to a poll interval.
			const before = callsOf('get_ssot').length;
			await mod.setTempleKeys(0);
			expect(callsOf('get_ssot').length).toBe(before + 1);
		});

		it('does not re-fetch after a rejected command', async () => {
			// Nothing changed in Rust, so there is nothing to fetch — and a
			// fetch here would overwrite nothing while costing a round trip.
			invokeMock.mockImplementation(async (command: string) => {
				if (command === 'temple_set_keys') throw new Error('rejected');
				return command === 'get_ssot' ? { league } : undefined;
			});
			const before = callsOf('get_ssot').length;
			await mod.setTempleKeys(9);
			expect(callsOf('get_ssot').length).toBe(before);
		});
	});

	describe('a rejected command', () => {
		/** Make one temple command reject with Rust's own validation message. */
		function rejectWith(command: string, message: string): void {
			invokeMock.mockImplementation(async (name: string) => {
				if (name === command) throw new Error(message);
				return name === 'get_ssot' ? { league } : undefined;
			});
		}

		it('hands the rejection back to the caller instead of throwing', async () => {
			rejectWith('temple_set_keys', 'an incursion drops at most 2 opening stones, got 9');
			// Never throws — a rejected setting must not take the page with it.
			const error = await mod.setTempleKeys(9);
			expect(error).toContain('at most 2 opening stones');
		});

		it('reaches the app log, which is the only channel a shipped build has', async () => {
			// console.warn goes nowhere in a release webview (no devtools), so a
			// validation rejection that only warned would be invisible to the
			// user AND to a log dump.
			rejectWith('temple_set_profile', 'apex_score must be a finite number ≥ 0, got NaN');
			await mod.setTempleProfile({
				apexScore: Number.NaN,
				pathCost: 0,
				rerollUntilFavourable: false,
				r4KeepUpgradeTargets: true,
			});
			const logged = callsOf('app_log_from_frontend') as { msg: string }[];
			expect(logged).toHaveLength(1);
			expect(logged[0].msg).toContain('temple_set_profile');
			expect(logged[0].msg).toContain('apex_score must be a finite number');
		});

		it('says nothing to the log when the command is accepted', async () => {
			await mod.setTempleConfig({ artefactsOfTheVaal: true, scarabOfTimelines: false });
			expect(callsOf('app_log_from_frontend')).toHaveLength(0);
		});
	});

	describe('templeDebugCapture', () => {
		it('returns the report Rust produced', async () => {
			const report = { dumpDir: '/tmp/poe/1', anchored: true };
			invokeMock.mockImplementation(async (command: string) =>
				command === 'temple_debug_capture' ? report : { league },
			);
			expect(await mod.templeDebugCapture()).toEqual({ report, error: null });
			expect(callsOf('temple_debug_capture')).toEqual([{ imagePath: null }]);
		});

		it('returns the failure and logs it, rather than a null report with no reason', async () => {
			// This is the command a user runs BECAUSE something else went wrong;
			// a silent failure here teaches nothing.
			invokeMock.mockImplementation(async (command: string) => {
				if (command === 'temple_debug_capture') throw new Error('no monitor');
				return { league };
			});
			const { report, error } = await mod.templeDebugCapture();
			expect(report).toBeNull();
			expect(error).toContain('no monitor');
			expect(callsOf('app_log_from_frontend')).toHaveLength(1);
		});
	});
});

/** The reference measurement — 1920x1200 at 1.0 IS the reference fixture. */
const referenceScreen: ScreenSlice = {
	width: 1920,
	height: 1200,
	uiScale: 1.0,
	source: 'merc-frame',
	measuredAtMs: 1_700_000_000_000,
	verifiedThisSession: true,
	monitorId: 65_537,
	origin: [0, 0]
};

// A `ScreenSlice` carrying a source string Rust never emits. Deliberately not a
// test case: there is no runtime behaviour to assert (reading back a value the
// literal just set would be a tautology), and the `@ts-expect-error` below IS
// the check — `npm run check` (svelte-check, run in CI) fails when an annotated
// line stops erroring. Widening `ScreenScaleSource` to `string` would let a
// typo reach the consumers POE-214 names, and would delete this error, which is
// precisely what `@ts-expect-error` reports as a failure.
const rejectedScreenSource: ScreenSlice = {
	...referenceScreen,
	// @ts-expect-error — the union IS the wire contract (Rust's kebab-case
	// `ScreenScaleSource`); see the note above.
	source: 'merc-fram'
};

describe('screen slice (POE-214)', () => {
	// The literals are annotated `SsotSnapshot`, which makes `npm run check`
	// fail if the field or its shape drifts from Rust's; the runtime assertions
	// below cover the rune the Settings "Screen geometry" card reads (POE-227).

	it('projects a measured screen onto the rune, whole', () => {
		const snap: SsotSnapshot = { league: { name: 'Mirage' }, screen: referenceScreen };

		applySnapshot(snap);

		expect(ssot.screen).toEqual(referenceScreen);
	});

	// `screen: null` rather than an absent key, because that is the payload Rust
	// actually sends: `Option<ScreenSlice>` has no `skip_serializing_if`, so an
	// unmeasured screen arrives as an explicit null.
	it('clears the rune when a later snapshot reports no measurement', () => {
		applySnapshot({ league: { name: 'Mirage' }, screen: referenceScreen });
		const snap: SsotSnapshot = { league: { name: 'Settlers' }, screen: null };

		applySnapshot(snap);

		// NOT fail-closed on absence like the two module slices: this slice is
		// droppable by design (`ssot::drop_if_mismatched` clears it when the
		// capture's dimensions change), and keeping the last known measurement
		// would leave the geometry card advertising a scale the app has thrown
		// away.
		expect(ssot.screen).toBeNull();
	});

	it('applies the rest of a snapshot that carries a screen slice', () => {
		const snap: SsotSnapshot = { league: { name: 'Mirage' }, screen: referenceScreen };

		applySnapshot(snap);

		expect(ssot.league).toBe('Mirage');
	});
});
