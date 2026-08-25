/**
 * Which manifest a device is offered an update from (POE-203).
 *
 * The property under test is that a beta device gets the HIGHER of two
 * independent answers and that a half-failed check still produces the surviving
 * one — while a check that lost both arms throws, because "could not ask" must
 * never reach the user as "you are on the latest version".
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';

/** Offers the stub `Update` constructor built, newest last. */
const hoisted = vi.hoisted(() => ({
	constructed: [] as { version: string; close: ReturnType<typeof vi.fn> }[]
}));

// The Rust command behind the beta arm; the real core module cannot load
// outside a webview. Same shape as src/lib/stores/ssot.svelte.test.ts:8.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

vi.mock('@tauri-apps/plugin-updater', () => ({
	check: vi.fn(),
	// Stand-in for the plugin's `Update`: the real one is a webview resource
	// handle, and `check.ts` only ever constructs it from the Rust answer and
	// calls `close()` on the arm that lost.
	Update: class {
		version: string;
		close = vi.fn(async () => {});
		constructor(metadata: { version: string }) {
			this.version = metadata.version;
			hoisted.constructed.push(this);
		}
	}
}));

import { invoke } from '@tauri-apps/api/core';
import { check, type Update } from '@tauri-apps/plugin-updater';
import { entitlements } from '$lib/stores/entitlements.svelte';
import { BETA_MANIFEST_URL, checkForUpdate } from './check';

/** An offer from the stable arm — `check()` hands these back as-is. */
function stableOffer(version: string) {
	return { version, close: vi.fn(async () => {}) };
}

/** The same object, seen through the plugin's type, for `check`'s mock. */
function asUpdate(offer: ReturnType<typeof stableOffer>): Update {
	return offer as unknown as Update;
}

/** What `check_update_from_endpoint` answers for an available beta build. */
function betaMetadata(version: string) {
	return { rid: 7, currentVersion: '1.0.0', version, date: null, body: null, rawJson: {} };
}

let warn: ReturnType<typeof vi.spyOn>;

beforeEach(() => {
	vi.clearAllMocks();
	hoisted.constructed.length = 0;
	entitlements.role = '';
	entitlements.channel = 'stable';
	entitlements.features = [];
	// The half-failed paths log on purpose; one test below asserts on it.
	warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
});

afterEach(() => {
	warn.mockRestore();
});

describe('checkForUpdate on a stable device', () => {
	it('offers what the configured endpoint advertises', async () => {
		const offered = stableOffer('1.2.0');
		vi.mocked(check).mockResolvedValue(asUpdate(offered));
		vi.mocked(invoke).mockResolvedValue(betaMetadata('9.9.9'));

		expect(await checkForUpdate()).toBe(offered);
	});

	it('never asks the beta manifest', async () => {
		// The interaction IS the contract here: a stable device that read the
		// prerelease manifest would be offered a beta artifact.
		vi.mocked(check).mockResolvedValue(null);

		await checkForUpdate();

		expect(invoke).not.toHaveBeenCalled();
	});
});

describe('checkForUpdate on a beta device', () => {
	beforeEach(() => {
		entitlements.channel = 'beta';
	});

	it('asks the rolling beta manifest through the Rust endpoint override', async () => {
		vi.mocked(check).mockResolvedValue(null);
		vi.mocked(invoke).mockResolvedValue(null);

		await checkForUpdate();

		expect(invoke).toHaveBeenCalledWith('check_update_from_endpoint', {
			endpoint: BETA_MANIFEST_URL
		});
	});

	it('offers the beta build when it is the higher version', async () => {
		vi.mocked(check).mockResolvedValue(asUpdate(stableOffer('1.2.0')));
		vi.mocked(invoke).mockResolvedValue(betaMetadata('1.3.0-beta.1'));

		const offered = await checkForUpdate();

		expect(offered).toBe(hoisted.constructed[0]);
	});

	it('releases the stable offer it passed over', async () => {
		const passedOver = stableOffer('1.2.0');
		vi.mocked(check).mockResolvedValue(asUpdate(passedOver));
		vi.mocked(invoke).mockResolvedValue(betaMetadata('1.3.0-beta.1'));

		await checkForUpdate();

		expect(passedOver.close).toHaveBeenCalled();
	});

	it('leaves the offer it took open for the caller to install', async () => {
		vi.mocked(check).mockResolvedValue(asUpdate(stableOffer('1.2.0')));
		vi.mocked(invoke).mockResolvedValue(betaMetadata('1.3.0-beta.1'));

		const offered = await checkForUpdate();

		expect(hoisted.constructed[0].close).not.toHaveBeenCalled();
		expect(offered).toBe(hoisted.constructed[0]);
	});

	it('offers the stable build when the beta manifest has nothing newer', async () => {
		// The acceptance case: the beta device has rolled forward past every
		// build its own manifest advertises.
		const offered = stableOffer('1.2.0');
		vi.mocked(check).mockResolvedValue(asUpdate(offered));
		vi.mocked(invoke).mockResolvedValue(null);

		expect(await checkForUpdate()).toBe(offered);
	});

	it('offers the stable build when both manifests advertise the same version', async () => {
		// A beta build promoted to stable unchanged: the arms are passed to
		// `higherUpdate` stable-first precisely so the tie leaves the device on
		// the stable artifact rather than the prerelease one.
		const promoted = stableOffer('1.2.0');
		vi.mocked(check).mockResolvedValue(asUpdate(promoted));
		vi.mocked(invoke).mockResolvedValue(betaMetadata('1.2.0'));

		expect(await checkForUpdate()).toBe(promoted);
	});

	it('offers the beta build when the stable manifest has nothing newer', async () => {
		vi.mocked(check).mockResolvedValue(null);
		vi.mocked(invoke).mockResolvedValue(betaMetadata('1.3.0-beta.1'));

		expect(await checkForUpdate()).toBe(hoisted.constructed[0]);
	});

	it('offers nothing when neither manifest has anything newer', async () => {
		vi.mocked(check).mockResolvedValue(null);
		vi.mocked(invoke).mockResolvedValue(null);

		expect(await checkForUpdate()).toBeNull();
	});

	it('offers the beta build when the stable check failed', async () => {
		vi.mocked(check).mockRejectedValue(new Error('github unreachable'));
		vi.mocked(invoke).mockResolvedValue(betaMetadata('1.3.0-beta.1'));

		expect(await checkForUpdate()).toBe(hoisted.constructed[0]);
	});

	it('offers the stable build when the beta check failed', async () => {
		const offered = stableOffer('1.2.0');
		vi.mocked(check).mockResolvedValue(asUpdate(offered));
		vi.mocked(invoke).mockRejectedValue(new Error('beta manifest 404'));

		expect(await checkForUpdate()).toBe(offered);
	});

	it('names the arm it lost when only one answered', async () => {
		// A half-broken check is otherwise indistinguishable from a healthy one,
		// and the log is the only place that difference shows.
		vi.mocked(check).mockResolvedValue(null);
		vi.mocked(invoke).mockRejectedValue(new Error('beta manifest 404'));

		await checkForUpdate();

		expect(warn.mock.calls.map((c: unknown[]) => String(c[0])).join('\n')).toContain(
			'beta check failed'
		);
	});

	it('throws when neither manifest could be reached', async () => {
		// Never null: a network failure that read as "nothing newer" would pin
		// the device to its build and say everything was fine.
		vi.mocked(check).mockRejectedValue(new Error('github unreachable'));
		vi.mocked(invoke).mockRejectedValue(new Error('beta manifest 404'));

		await expect(checkForUpdate()).rejects.toThrow('github unreachable');
	});
});
