/**
 * The app's ONE update check (POE-203).
 *
 * Every caller goes through `checkForUpdate()` — the background poll in
 * `stores/status.svelte.ts` and both of `SettingsPage.svelte`'s calls — so a
 * beta device cannot be offered one thing by the status bar and another by the
 * Settings button.
 *
 * **Stable devices are unchanged**: one `check()` against the endpoint in
 * `tauri.conf.json`.
 *
 * **Beta devices check both manifests and take the higher version.** The two
 * arms run independently and a failure in one never costs the other its
 * answer; only a check that lost BOTH arms throws, so a caller can still tell
 * "nothing newer" from "could not ask".
 *
 * The beta arm does not go through the plugin's `check()`: it has no per-call
 * endpoint option in any released 2.x (verified against 2.10.0 and 2.10.1), so
 * the Rust command `check_update_from_endpoint` builds the updater against the
 * beta manifest instead and answers with the SAME metadata the plugin returns.
 * Wrapping it in the plugin's `Update` class here means the download and
 * install path is the plugin's own, for both channels.
 */
import { invoke } from '@tauri-apps/api/core';
import { check, Update } from '@tauri-apps/plugin-updater';
import { entitlements } from '$lib/stores/entitlements.svelte';
import { higherUpdate } from './semver';

/**
 * The rolling beta manifest — a GitHub release tagged `desktop-beta` whose
 * `latest.json` each `v-desktop-X.Y.Z-beta.N` build overwrites. It is a
 * prerelease, which is why `/releases/latest` (the stable endpoint in
 * `tauri.conf.json`) keeps ignoring it.
 */
export const BETA_MANIFEST_URL =
	'https://github.com/SebRogala/ProfitOfExile/releases/download/desktop-beta/latest.json';

/** Ask the beta manifest, through the Rust endpoint override. */
async function checkBetaManifest(): Promise<Update | null> {
	const metadata = await invoke<ConstructorParameters<typeof Update>[0] | null>(
		'check_update_from_endpoint',
		{ endpoint: BETA_MANIFEST_URL }
	);
	return metadata ? new Update(metadata) : null;
}

/**
 * Release an update offer this app decided against.
 *
 * Each answered check registers a resource in Rust that lives until the app
 * exits, and the background poll runs every 30 minutes — so the arm that lost
 * is closed rather than left behind. Failing to close is not worth surfacing:
 * the cost is one leaked handle.
 */
async function discard(update: Update | null, kept: Update | null): Promise<void> {
	if (!update || update === kept) return;
	await update.close().catch((e) => console.warn('[updater] could not release the unused offer:', e));
}

/**
 * The update this device should be offered, or `null` when there is none.
 *
 * Throws when no arm could answer — a network failure must not read as
 * "you are on the latest version".
 */
export async function checkForUpdate(): Promise<Update | null> {
	if (entitlements.channel !== 'beta') return check();

	// Stable first: `higherUpdate` keeps its first argument on a tie, so a beta
	// build promoted to stable unchanged leaves the device on the stable one.
	const [stableArm, betaArm] = await Promise.allSettled([check(), checkBetaManifest()]);

	if (stableArm.status === 'rejected' && betaArm.status === 'rejected') {
		console.warn('[updater] beta check failed:', betaArm.reason);
		throw stableArm.reason;
	}

	const stable = stableArm.status === 'fulfilled' ? stableArm.value : null;
	const beta = betaArm.status === 'fulfilled' ? betaArm.value : null;
	if (stableArm.status === 'rejected') {
		console.warn('[updater] stable check failed, using the beta manifest alone:', stableArm.reason);
	}
	if (betaArm.status === 'rejected') {
		console.warn('[updater] beta check failed, using the stable manifest alone:', betaArm.reason);
	}

	const kept = higherUpdate(stable, beta);
	await discard(stable, kept);
	await discard(beta, kept);
	return kept;
}
