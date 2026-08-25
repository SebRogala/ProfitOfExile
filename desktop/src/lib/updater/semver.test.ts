/**
 * The ordering a beta device's two update manifests are ranked by (POE-203).
 *
 * The case the feature turns on is `1.2.0-beta.3 < 1.2.0`: without it a beta
 * device stays on the beta manifest forever, because the beta artifact it is
 * already running is the newest thing that manifest will ever offer.
 */
import { describe, expect, it } from 'vitest';
import { compareVersions, higherUpdate } from './semver';

/** `compareVersions` as a readable claim, checked in both directions. */
function ordered(older: string, newer: string): boolean {
	return compareVersions(older, newer) === -1 && compareVersions(newer, older) === 1;
}

describe('compareVersions', () => {
	it('ranks a higher patch above a lower one', () => {
		expect(ordered('1.2.0', '1.2.1')).toBe(true);
	});

	it('ranks a higher minor above a lower one', () => {
		expect(ordered('1.2.9', '1.3.0')).toBe(true);
	});

	it('ranks a higher major above a lower one', () => {
		expect(ordered('1.9.9', '2.0.0')).toBe(true);
	});

	it('ranks the numeric parts numerically rather than as text', () => {
		// String comparison puts '1.10.0' below '1.9.0' and would hold a device
		// back a whole release line.
		expect(ordered('1.9.0', '1.10.0')).toBe(true);
	});

	it('ranks a prerelease below the release it leads to', () => {
		// The POE-203 acceptance case: the beta device must take stable 1.2.0.
		expect(ordered('1.2.0-beta.3', '1.2.0')).toBe(true);
	});

	it('ranks a later beta above an earlier one', () => {
		expect(ordered('1.2.0-beta.2', '1.2.0-beta.3')).toBe(true);
	});

	it('ranks numeric prerelease identifiers numerically rather than as text', () => {
		// 'beta.10' sorts below 'beta.2' as text — the tenth beta build would
		// never be offered to a device on the second.
		expect(ordered('1.2.0-beta.2', '1.2.0-beta.10')).toBe(true);
	});

	it('ranks a shorter prerelease below a longer one sharing its prefix', () => {
		expect(ordered('1.2.0-beta', '1.2.0-beta.1')).toBe(true);
	});

	it('ranks a numeric prerelease identifier below an alphanumeric one', () => {
		expect(ordered('1.2.0-1', '1.2.0-alpha')).toBe(true);
	});

	it('ranks a prerelease of a higher patch above the release below it', () => {
		// The prerelease rule is per-version, not global: 1.2.1-beta.1 is still
		// newer than 1.2.0, and a beta device must not be pinned to 1.2.0.
		expect(ordered('1.2.0', '1.2.1-beta.1')).toBe(true);
	});

	it('ranks two identical versions equal', () => {
		expect(compareVersions('1.2.0', '1.2.0')).toBe(0);
	});

	it('ignores build metadata, which carries no precedence', () => {
		expect(compareVersions('1.2.0+build.7', '1.2.0')).toBe(0);
	});

	it('accepts a leading v on a hand-written manifest version', () => {
		// CI writes a bare X.Y.Z; a manifest re-published by hand may not.
		expect(compareVersions('v1.2.0', '1.2.0')).toBe(0);
	});

	it('ranks a version it cannot parse below one it can', () => {
		// A manifest whose version field is broken must never win and route a
		// device onto its artifact.
		expect(ordered('not-a-version', '0.0.1')).toBe(true);
	});

	it('ranks two unparseable versions equal rather than picking one', () => {
		expect(compareVersions('nightly', 'latest')).toBe(0);
	});
});

describe('higherUpdate', () => {
	const stable = { version: '1.2.0' };
	const beta = { version: '1.3.0-beta.1' };

	it('keeps the higher-versioned of two offers', () => {
		expect(higherUpdate(stable, beta)).toBe(beta);
	});

	it('keeps the stable offer when the beta manifest is behind it', () => {
		// The acceptance case end to end: the beta arm still advertises the
		// prerelease the device is on, the stable arm has caught up.
		expect(higherUpdate({ version: '1.2.0' }, { version: '1.2.0-beta.3' })).toEqual({
			version: '1.2.0'
		});
	});

	it('keeps the first offer when both advertise the same version', () => {
		// Callers pass stable first, so a beta build promoted unchanged leaves
		// the device on the stable artifact.
		const promoted = { version: '1.2.0' };
		expect(higherUpdate(stable, promoted)).toBe(stable);
	});

	it('keeps the beta offer when the stable arm found nothing', () => {
		expect(higherUpdate(null, beta)).toBe(beta);
	});

	it('keeps the stable offer when the beta arm found nothing', () => {
		expect(higherUpdate(stable, null)).toBe(stable);
	});

	it('offers nothing when neither arm found anything', () => {
		expect(higherUpdate(null, null)).toBeNull();
	});
});
