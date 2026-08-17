import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { ssot, applySnapshot } from './ssot.svelte';
import type { MercenarySlice } from '../mercenaries/capture';

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
			lastError: 'ocr engine slow',
			geometrySource: 'file',
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
				capture: null,
				learnedFamilies: [],
				lastError: null,
				geometrySource: 'default',
			},
		});
		expect(mod.ssot.mercenary.status).toBe('idle');
		expect(mod.ssot.mercenary.capture).toBeNull();
		expect(mod.ssot.mercenary.lastError).toBeNull();
		expect(mod.ssot.mercenary.learnedFamilies).toEqual([]);
	});
});
