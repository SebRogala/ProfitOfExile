/**
 * Semver ordering for the updater's two channels (POE-203).
 *
 * A beta device is offered whichever of two manifests carries the higher
 * version, so something has to rank `1.2.0-beta.3` against `1.2.0` — and the
 * answer that matters is the one the spec names: a prerelease sorts BELOW the
 * release it leads to, so a beta device is never stranded on the beta manifest
 * once the stable build ships.
 *
 * Pure and dependency-free on purpose: the two consumers are an async Tauri
 * path with no test harness, so the ordering rules live here where they can be
 * pinned directly.
 *
 * Implements the semver 2.0.0 precedence rules that this app can reach:
 * major/minor/patch numerically, release above prerelease, prerelease
 * identifiers left to right (numeric numerically and below alphanumeric,
 * alphanumeric ASCII-wise, a shorter run of identifiers below a longer one
 * that shares its prefix), build metadata ignored.
 */

/** A version split into its comparable parts. */
interface Parsed {
	/** major, minor, patch. */
	main: [number, number, number];
	/** Dot-separated prerelease identifiers; empty for a release. */
	pre: string[];
}

/**
 * Split a version, or `null` when it is not a version this can rank.
 *
 * A leading `v` is tolerated defensively: the versions that reach here are the
 * `version` fields of two update manifests, and while the CI-written one is a
 * bare `X.Y.Z` (`.github/workflows/desktop.yml` strips the `v-desktop-` tag
 * prefix before writing it), a manifest re-published by hand can pick the `v`
 * habit up. Build metadata is accepted and discarded, per semver.
 */
function parse(version: string): Parsed | null {
	const cleaned = version.trim().replace(/^v/i, '');
	const m = /^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?$/.exec(cleaned);
	if (!m) return null;
	return {
		main: [Number(m[1]), Number(m[2]), Number(m[3])],
		pre: m[4] ? m[4].split('.') : []
	};
}

/** Rank one prerelease identifier against another. */
function compareIdentifier(a: string, b: string): number {
	const aNumeric = /^\d+$/.test(a);
	const bNumeric = /^\d+$/.test(b);
	if (aNumeric && bNumeric) {
		const x = Number(a);
		const y = Number(b);
		return x === y ? 0 : x < y ? -1 : 1;
	}
	// Semver: numeric identifiers always have lower precedence than
	// alphanumeric ones, so `1.2.0-1` sorts below `1.2.0-alpha`.
	if (aNumeric) return -1;
	if (bNumeric) return 1;
	return a === b ? 0 : a < b ? -1 : 1;
}

/** Rank two prerelease runs, both known non-empty. */
function comparePrerelease(a: string[], b: string[]): number {
	const shared = Math.min(a.length, b.length);
	for (let i = 0; i < shared; i++) {
		const ranked = compareIdentifier(a[i], b[i]);
		if (ranked !== 0) return ranked;
	}
	// A larger set of identifiers wins when everything before it is equal:
	// `1.2.0-beta` < `1.2.0-beta.1`.
	return a.length === b.length ? 0 : a.length < b.length ? -1 : 1;
}

/**
 * `-1` when `a` is older than `b`, `1` when newer, `0` when they rank equal.
 *
 * A version this cannot parse ranks BELOW one it can, and two unparseable
 * versions rank equal — a manifest carrying a version string nothing here
 * recognises must never win a comparison and push a device onto it.
 */
export function compareVersions(a: string, b: string): number {
	const pa = parse(a);
	const pb = parse(b);
	if (!pa && !pb) return 0;
	if (!pa) return -1;
	if (!pb) return 1;

	for (let i = 0; i < 3; i++) {
		if (pa.main[i] !== pb.main[i]) return pa.main[i] < pb.main[i] ? -1 : 1;
	}

	if (pa.pre.length === 0 && pb.pre.length === 0) return 0;
	// A release outranks its own prereleases: 1.2.0-beta.3 < 1.2.0.
	if (pa.pre.length === 0) return 1;
	if (pb.pre.length === 0) return -1;
	return comparePrerelease(pa.pre, pb.pre);
}

/**
 * The higher-versioned of two update offers, either of which may be absent.
 *
 * Structural rather than typed against the updater plugin so it can be tested
 * without one: anything carrying a `version` ranks.
 *
 * **A tie keeps `a`.** Callers pass the STABLE arm first, so two manifests
 * advertising the same version — the normal state of affairs the moment a beta
 * build is promoted unchanged — leave the device on the stable artifact.
 */
export function higherUpdate<T extends { version: string }>(a: T | null, b: T | null): T | null {
	if (!a) return b;
	if (!b) return a;
	return compareVersions(b.version, a.version) > 0 ? b : a;
}
