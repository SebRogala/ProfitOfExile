import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { VIEW_PATHS, type View } from './navigation.svelte';

/**
 * `viewToPath` and `go` are two halves of one mapping, and the round-trip alone
 * cannot catch a wrong-but-absorbed path: if `viewToPath('mercenaries')` returned
 * '/mercs', `go` would fall through to 'lab' — a round-trip test would fail, but
 * one that only pinned `go('/mercenaries')` would not. So the exact string is
 * pinned per view as well.
 *
 * The store now reads and writes the `navView` preference (POE-187), which makes
 * two things true for every test here. The pref module reaches Rust through
 * `invoke`, so it is mocked. And both modules hold module-level state — the
 * pref's loaded map, its per-key dirty flag — so each test imports a fresh store
 * rather than inheriting whatever the previous one parked it on.
 */
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => ({})) }));

type Store = typeof import('./navigation.svelte');

/**
 * Every view this build has, read off the source table rather than hand-listed:
 * a View added with a `VIEW_PATHS` entry gets covered without anyone having to
 * remember to extend this file. Taken from the statically imported module — the
 * table is a constant, so it is the same one every re-imported instance holds.
 */
const views = Object.keys(VIEW_PATHS) as View[];

let invokeMock: ReturnType<typeof vi.mocked<typeof import('@tauri-apps/api/core').invoke>>;

beforeEach(async () => {
	vi.resetModules();
	vi.clearAllMocks();
	const core = await import('@tauri-apps/api/core');
	invokeMock = vi.mocked(core.invoke);
});

afterEach(() => {
	vi.useRealTimers();
});

/** Calls of one Tauri command, in order, with their argument objects. */
function callsOf(command: string): unknown[] {
	return invokeMock.mock.calls.filter((c) => c[0] === command).map((c) => c[1]);
}

/** Yield to the macrotask queue, which drains the prefs load's promise chain. */
function settle(): Promise<void> {
	return new Promise((resolve) => setTimeout(resolve, 0));
}

/** A fresh store whose prefs load has already answered with `prefs`. */
async function loadStoreWith(prefs: Record<string, string>): Promise<Store> {
	invokeMock.mockImplementation(async (command: string) =>
		command === 'get_ui_prefs' ? prefs : undefined,
	);
	const mod = await import('./navigation.svelte');
	await settle();
	return mod;
}

/**
 * A fresh store whose prefs load is still in flight. `deliver` lands the stored
 * map and returns once the store has seen it — the launch window in which the
 * user can already click a tool.
 */
async function loadStorePending(): Promise<{
	mod: Store;
	deliver: (prefs: Record<string, string>) => Promise<void>;
}> {
	let respond: (prefs: Record<string, string>) => void = () => {};
	invokeMock.mockImplementation((command: string) => {
		if (command === 'get_ui_prefs') {
			return new Promise<Record<string, string>>((resolve) => {
				respond = resolve;
			});
		}
		return Promise.resolve(undefined);
	});
	const mod = await import('./navigation.svelte');
	return {
		mod,
		deliver: async (prefs) => {
			respond(prefs);
			await settle();
		},
	};
}

describe('parseView', () => {
	it.each(views)("keeps '%s', which is a view this build has", async (view) => {
		const { parseView } = await loadStoreWith({});
		expect(parseView(view)).toBe(view);
	});

	it("falls back to 'lab' for a name no view claims", async () => {
		const { parseView } = await loadStoreWith({});
		// A pref written by a build whose view this one dropped.
		expect(parseView('compass')).toBe('lab');
	});

	it("falls back to 'lab' for 'dev' when the build is not a dev build", async () => {
		// The layout renders the Dev page only under import.meta.env.DEV, so a
		// release build restoring a dev-session pref would open on a blank pane.
		vi.stubEnv('DEV', false);
		try {
			const { parseView } = await loadStoreWith({});
			expect(parseView('dev')).toBe('lab');
		} finally {
			vi.unstubAllEnvs();
		}
	});

	it("falls back to 'lab' for the empty string", async () => {
		const { parseView } = await loadStoreWith({});
		expect(parseView('')).toBe('lab');
	});

	it("falls back to 'lab' for an Object.prototype key", async () => {
		const { parseView } = await loadStoreWith({});
		// `raw in VIEW_PATHS` would accept this and hand the layout a value that
		// matches no page, blanking the app.
		expect(parseView('constructor')).toBe('lab');
	});
});

describe('viewToPath', () => {
	it("maps 'lab' to '/' — the root path, not '/lab'", async () => {
		const { viewToPath } = await loadStoreWith({});
		expect(viewToPath('lab')).toBe('/');
	});

	it("maps 'settings' to '/settings'", async () => {
		const { viewToPath } = await loadStoreWith({});
		expect(viewToPath('settings')).toBe('/settings');
	});

	it("maps 'dev' to '/dev'", async () => {
		const { viewToPath } = await loadStoreWith({});
		expect(viewToPath('dev')).toBe('/dev');
	});

	it("maps 'mercenaries' to '/mercenaries'", async () => {
		const { viewToPath } = await loadStoreWith({});
		expect(viewToPath('mercenaries')).toBe('/mercenaries');
	});

	it("maps 'temple' to '/temple'", async () => {
		const { viewToPath } = await loadStoreWith({});
		expect(viewToPath('temple')).toBe('/temple');
	});

	it("maps 'currency-exchange' to '/currency-exchange'", async () => {
		const { viewToPath } = await loadStoreWith({});
		expect(viewToPath('currency-exchange')).toBe('/currency-exchange');
	});
});

describe('go', () => {
	/**
	 * A store parked on a view other than `view`, so a pass proves `go` wrote it.
	 * The seed is the STORED pref, not a `go` call: seeding through the same
	 * function under test would let one broken mapping satisfy both halves.
	 */
	function storeSeededAwayFrom(view: View): Promise<Store> {
		return loadStoreWith({ navView: view === 'lab' ? 'settings' : 'lab' });
	}

	it.each(views)("selects '%s' from the path viewToPath gives for it", async (view) => {
		const mod = await storeSeededAwayFrom(view);
		mod.nav.go(mod.viewToPath(view));
		expect(mod.nav.view).toBe(view);
	});

	it("falls back to 'lab' for a path no view claims", async () => {
		const mod = await storeSeededAwayFrom('lab');
		mod.nav.go('/mercs');
		expect(mod.nav.view).toBe('lab');
	});

	it("falls back to 'lab' for the empty path", async () => {
		const mod = await storeSeededAwayFrom('lab');
		mod.nav.go('');
		expect(mod.nav.view).toBe('lab');
	});

	it("falls back to 'lab' for a path naming an Object.prototype key", async () => {
		const mod = await storeSeededAwayFrom('lab');
		// An object lookup would answer with `Object.prototype.constructor` here,
		// which `?? 'lab'` does not catch because it is not nullish.
		mod.nav.go('constructor');
		expect(mod.nav.view).toBe('lab');
	});

	it('persists the view name under the navView key, not the path', async () => {
		const mod = await loadStoreWith({});
		vi.useFakeTimers();
		mod.nav.go('/temple');
		// Writes are debounced per key inside prefs.svelte.ts.
		await vi.advanceTimersByTimeAsync(300);
		expect(callsOf('set_ui_pref')).toEqual([{ key: 'navView', value: 'temple' }]);
	});

	it("persists 'lab' for a path no view claims, not the unmapped path", async () => {
		// Validate-on-read hides a bad write from `nav.view`, so the only place a
		// missing fallback shows is what the next launch restores from.
		const mod = await loadStoreWith({});
		vi.useFakeTimers();
		mod.nav.go('/mercs');
		await vi.advanceTimersByTimeAsync(300);
		expect(callsOf('set_ui_pref')).toEqual([{ key: 'navView', value: 'lab' }]);
	});

	it("persists 'lab' for an Object.prototype path, not an inherited function", async () => {
		// The read-side twin above passes even under an object-lookup `go`,
		// because `parseView` narrows the bad value back to 'lab'. What the
		// STORE was asked to write is the only observable that catches it.
		const mod = await loadStoreWith({});
		vi.useFakeTimers();
		mod.nav.go('constructor');
		await vi.advanceTimersByTimeAsync(300);
		expect(callsOf('set_ui_pref')).toEqual([{ key: 'navView', value: 'lab' }]);
	});
});

describe('restoring the last view on launch', () => {
	it('opens on the stored view once the prefs load lands', async () => {
		const mod = await loadStoreWith({ navView: 'temple' });
		expect(mod.nav.view).toBe('temple');
	});

	it("opens on 'lab' while the prefs load is still in flight", async () => {
		// The first paint happens here, before Rust has answered: every window
		// starts on Lab and only moves if the stored value says otherwise.
		const { mod } = await loadStorePending();
		expect(mod.nav.view).toBe('lab');
	});

	it("opens on 'lab' when the stored view is not one this build has", async () => {
		const mod = await loadStoreWith({ navView: 'compass' });
		expect(mod.nav.view).toBe('lab');
	});

	it("opens on 'lab' when nothing was ever stored", async () => {
		const mod = await loadStoreWith({});
		expect(mod.nav.view).toBe('lab');
	});

	it("opens on 'lab' when the prefs load fails outright", async () => {
		// A settings file Rust cannot read must cost the user their last tab, not
		// the whole app.
		invokeMock.mockImplementation(async (command: string) => {
			if (command === 'get_ui_prefs') throw new Error('settings file unreadable');
			return undefined;
		});
		const mod = await import('./navigation.svelte');
		await settle();
		expect(mod.nav.view).toBe('lab');
	});
});

describe('a pick made before the prefs load lands', () => {
	it('survives the stored view arriving afterwards', async () => {
		const { mod, deliver } = await loadStorePending();
		mod.nav.go('/mercenaries');
		await deliver({ navView: 'temple' });
		// The load must not drag the user off the tool they just clicked.
		expect(mod.nav.view).toBe('mercenaries');
	});

	it("survives even when the pick is 'lab', the value it started on", async () => {
		const { mod, deliver } = await loadStorePending();
		// Nothing about the rune changes here, so only the pref's dirty flag
		// records that the user chose — without it the stored view wins.
		mod.nav.go('/');
		await deliver({ navView: 'temple' });
		expect(mod.nav.view).toBe('lab');
	});

	it('does not block a stored view when the user picked nothing', async () => {
		// The counterpart: the pre-load rule must not latch the default for
		// everyone who launches and waits.
		const { mod, deliver } = await loadStorePending();
		await deliver({ navView: 'temple' });
		expect(mod.nav.view).toBe('temple');
	});
});
