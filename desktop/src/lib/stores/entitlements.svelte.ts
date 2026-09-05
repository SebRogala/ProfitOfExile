/**
 * Per-device entitlements — release channel and hidden features (POE-203).
 *
 * The server answers `GET /api/device/me` for the device identity POE-102
 * already attaches to every request: `{ role, channel, features[] }`, with
 * editor and admin devices getting `channel: "beta"` and the hidden feature
 * ids — `merc`, `exchange`, `temple`.
 *
 * **The device id is a precondition, not a detail.** `X-Device-ID` IS the
 * question; without it the server can only answer for an anonymous device, and
 * it answers 200 — so a fetch made before `get_status` landed would write a
 * blank role and no features over the store and look like a settled answer.
 * A load with no device id therefore does not fetch at all; it counts as
 * not-yet-loaded and is retried.
 *
 * **Retried until it lands.** A single-shot fetch strands an entitled device on
 * stable for the whole session whenever the network is not up yet at launch, so
 * a failed attempt backs off (`RETRY_DELAYS_MS`) and tries again until a
 * device-identified answer arrives. `initStatusStore()` also re-runs the load
 * from its 30-minute update tick, so a promote on the server reaches a running
 * app without a restart.
 *
 * **Hiding, not securing.** Every gated module ships in every build; this only
 * decides whether the app draws them. Nothing here is a permission check.
 *
 * **The default is the quiet one.** Until an answer lands — and on a device
 * that is simply offline, that is the whole session — the device is on `stable`
 * with no features. Each failure logs a warning and shows the user nothing: an
 * entitlement they do not have is not an error, and the one person who does
 * have it will notice the module missing.
 *
 * **A grant is one server's answer.** The local server and production hold
 * different roles for the same device, so a `server_url` change withdraws the
 * grant (`resetEntitlements()`) and asks the new server at once
 * (`refreshEntitlements()`) — the status store does both. The refresh is also
 * what the identify dialog calls on open, so a promote the tester was just
 * told about shows up when they look, not on the next 30-minute tick.
 *
 * Usage:
 *   import { entitlements, hasFeature, MERC_FEATURE } from '$lib/stores/entitlements.svelte';
 *   // Read: entitlements.channel / entitlements.features / entitlements.role
 *   // Read: hasFeature(MERC_FEATURE)
 *   // Call loadEntitlements() from initStatusStore() — at startup and on its tick.
 *   // Call refreshEntitlements() to ask NOW; resetEntitlements() + refresh on a server switch.
 */

/** The two update channels the server can put a device on. */
export type Channel = 'stable' | 'beta';

/** The feature id that reveals the mercenaries module. */
export const MERC_FEATURE = 'merc';

/** The feature id that reveals the Currency Exchange module. */
export const EXCHANGE_FEATURE = 'exchange';

/** The feature id that reveals the Temple of Atzoatl module. */
export const TEMPLE_FEATURE = 'temple';

/** A validated `/api/device/me` answer. */
export interface Entitlements {
	/** `devices.role` — informational; nothing gates on the role directly. */
	role: string;
	channel: Channel;
	features: string[];
}

/**
 * What an un-contacted, unreachable or unparseable server means.
 *
 * A FACTORY, not a shared constant: callers own the object they get back —
 * the store assigns its `features` array onto a rune — and one shared array
 * would let a mutation downstream rewrite what every later failure falls back
 * to.
 */
function defaultEntitlements(): Entitlements {
	return { role: '', channel: 'stable', features: [] };
}

/**
 * Narrow a `/api/device/me` body to entitlements, defaulting every field the
 * server did not send in the shape this app expects.
 *
 * Only the literal `"beta"` selects the beta channel, and only string entries
 * survive into `features`: a field that arrives as the wrong type is a server
 * the client cannot read, and the safe reading of an unreadable answer is the
 * one that grants nothing.
 */
export function normalizeEntitlements(raw: unknown): Entitlements {
	if (!raw || typeof raw !== 'object') return defaultEntitlements();
	const body = raw as Record<string, unknown>;
	return {
		role: typeof body.role === 'string' ? body.role : '',
		channel: body.channel === 'beta' ? 'beta' : 'stable',
		features: Array.isArray(body.features)
			? body.features.filter((f): f is string => typeof f === 'string')
			: []
	};
}

/** Reactive store — mutate properties, never reassign the export. */
export const entitlements = $state<Entitlements>(defaultEntitlements());

/** Whether this device may see a feature. Reactive — reads the store. */
export function hasFeature(feature: string): boolean {
	return entitlements.features.includes(feature);
}

/**
 * The wait before each retry, by number of attempts already spent.
 *
 * Short at the front so a device whose network came up a second after launch
 * is entitled almost immediately; the last entry is the CAP a device that
 * cannot reach the server settles into, so an offline session costs one
 * request every five minutes rather than a poll.
 */
export const RETRY_DELAYS_MS = [5_000, 15_000, 60_000, 5 * 60 * 1000];

/** How long to wait after `attempt` failed attempts (0-based). */
export function retryDelayMs(attempt: number): number {
	const i = Math.min(Math.max(attempt, 0), RETRY_DELAYS_MS.length - 1);
	return RETRY_DELAYS_MS[i];
}

/**
 * Ends the backoff sleep in progress early — set while a chain sleeps, null
 * otherwise. `refreshEntitlements()` calls it so a chain minutes into the
 * five-minute cap asks again now.
 */
let wake: (() => void) | null = null;

/** Sleep `ms`, or less when `wake` is called. */
function backoff(ms: number): Promise<void> {
	return new Promise((resolve) => {
		const timer = setTimeout(done, ms);
		function done() {
			clearTimeout(timer);
			wake = null;
			resolve();
		}
		wake = done;
	});
}

/**
 * The device id `/api/device/me` will be answered for, or `''` when the status
 * store has not delivered one yet.
 *
 * `$lib/stores/status.svelte` is imported DYNAMICALLY: it imports this module,
 * so a static import would close an evaluation cycle. By the time this runs,
 * every module in it is evaluated.
 */
async function currentDeviceId(): Promise<string> {
	const { store } = await import('$lib/stores/status.svelte');
	return store.status?.device_id ?? '';
}

/**
 * One attempt. The device-identified answer, or null when there is none yet.
 *
 * Returned rather than written: the chain decides whether an answer still
 * stands (see `runLoad` — one that arrives after a refresh was asked for may
 * be the previous server's).
 *
 * `$lib/api` is imported DYNAMICALLY for the same reason as the status store:
 * it reads `store.status`, which imports this module.
 */
async function attemptLoad(): Promise<Entitlements | null> {
	try {
		const deviceId = await currentDeviceId();
		if (!deviceId) {
			// Not an error and not an answer: the server would reply 200 for an
			// anonymous device and that reply would read as "entitled to nothing".
			console.warn('[entitlements] no device id yet — not asking /api/device/me, will retry');
			return null;
		}
		const { fetchDeviceMe } = await import('$lib/api');
		return normalizeEntitlements(await fetchDeviceMe());
	} catch (e) {
		// Not user-facing: the device keeps the stable/no-features default.
		console.warn(
			'[entitlements] /api/device/me failed — staying on stable with no hidden features, will retry:',
			e
		);
		return null;
	}
}

function write(answered: Entitlements): void {
	entitlements.role = answered.role;
	entitlements.channel = answered.channel;
	entitlements.features = answered.features;
}

/** The load chain in flight, so two callers cannot start two of them. */
let loading: Promise<void> | null = null;

/**
 * Ask the server what this device is entitled to, retrying until it answers.
 *
 * The returned promise settles when an answer has been WRITTEN THROUGH, which
 * on an unreachable server is never — every caller must therefore treat it as
 * unbounded and bound its own wait (`initStatusStore()` races it against
 * `ENTITLEMENTS_FIRST_CHECK_WAIT_MS`). It never rejects.
 *
 * Concurrent calls share one chain: the 30-minute refresh must not stack a
 * second retry loop on top of a startup load that is still backing off.
 */
export function loadEntitlements(): Promise<void> {
	if (!loading) {
		loading = runLoad().finally(() => {
			loading = null;
		});
	}
	return loading;
}

/**
 * Raised by `refreshEntitlements()` while a chain is running: the answer the
 * chain is waiting on may be for a server the app has since left, so it is
 * dropped and the chain asks once more. Cleared at the top of every attempt.
 */
let askAgain = false;

/**
 * Ask NOW rather than on the chain's schedule.
 *
 * `loadEntitlements()` joins a chain that is already running, and a running
 * chain may be minutes into a backoff against a server that was down — or
 * mid-request against a server the app just switched away from. Two callers
 * cannot wait that out: the status store on a `server_url` change, where the
 * grant on screen belongs to the previous server, and the identify dialog,
 * where the tester is looking for the promote they were just told about. This
 * ends a sleeping backoff at once, makes a chain that is mid-request drop that
 * answer and ask again, and with no chain running simply starts one. The
 * returned promise is the chain's, and unbounded like it.
 */
export function refreshEntitlements(): Promise<void> {
	if (loading) {
		askAgain = true;
		wake?.();
		return loading;
	}
	return loadEntitlements();
}

/**
 * Back to "entitled to nothing" — for a server switch.
 *
 * A grant is one server's answer about this device; the local server and
 * production hold different roles for the same fingerprint, so carrying a
 * grant across a `server_url` change would draw modules the new server never
 * granted. The switch resets first and refreshes second. The store's own
 * failure path never resets (a refresh that cannot reach the server keeps the
 * landed grant), so this is the one place a grant is withdrawn without a
 * server saying so.
 */
export function resetEntitlements(): void {
	write(defaultEntitlements());
}

async function runLoad(): Promise<void> {
	let attempt = 0;
	for (;;) {
		askAgain = false;
		const answered = await attemptLoad();
		if (answered && !askAgain) {
			write(answered);
			return;
		}
		if (answered) {
			// A refresh was asked for while that request was in flight, so the
			// answer may be the previous server's: drop it and ask again at once.
			attempt = 0;
			continue;
		}
		await backoff(retryDelayMs(attempt++));
	}
}
