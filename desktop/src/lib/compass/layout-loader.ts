import { invoke } from '@tauri-apps/api/core';
import type { LabLayout } from './navigation';

/**
 * Difficulties tried, in order, when nothing has told us which one is live.
 * Hardest first: a server that has today's Uber layout is the common case, and
 * the lower difficulties are only reached when it does not.
 */
export const DEFAULT_DIFFICULTY_ORDER = ['Uber', 'Merciless', 'Cruel', 'Normal'];

/** Gap between `get_status` polls while `server_url` is still empty. */
export const STATUS_RETRY_MS = 2000;

/**
 * `get_status` polls one load attempt makes before giving up: the immediate one
 * plus 15 retries, so roughly 30 seconds — long enough to cover a slow server
 * start.
 *
 * The budget is per call, held in a local, and that is the point. It used to be
 * a module-scoped counter shared by every caller for the life of the window,
 * which made it a lifetime budget rather than a timeout: each LayoutChanged nav
 * event and each lab-layout-updated push heads its own chain, so a handful of
 * events with no server_url spent the whole allowance in milliseconds, and once
 * spent it never came back — a window that had loaded a layout perfectly well
 * would refuse to retry a later transient outage (server restart, backend
 * re-init) even once. The terminal state renders nothing at all and says
 * nothing, so the overlay just silently vanished.
 *
 * Trade-off taken deliberately: a per-call budget lets concurrent chains
 * overlap, so N events arriving during one outage cost N polls every 2s instead
 * of one. That is bounded — every chain ends within ~30s — and the events that
 * start chains are sporadic (a planner action or a server push), so the
 * redundant polling is small and self-limiting. The shared counter bounded that
 * overlap tighter at the cost of never recovering, which is the worse failure.
 */
export const MAX_STATUS_ATTEMPTS = 16;

function sleep(ms: number): Promise<void> {
	return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * Poll `get_status` until it reports a server URL, or the attempt budget runs
 * out. Returns null on exhaustion — the caller renders nothing, so the give-up
 * has to be logged where a release build can still read it.
 */
async function waitForServerUrl(log: (msg: string) => void): Promise<string | null> {
	for (let attempt = 1; attempt <= MAX_STATUS_ATTEMPTS; attempt++) {
		const status = await invoke<any>('get_status');
		const serverUrl = status?.server_url;
		if (serverUrl) return serverUrl;
		if (attempt === MAX_STATUS_ATTEMPTS) break;
		// Announced once, not once per poll. Three overlay windows run this
		// loader, and the app log's UI buffer holds 50 entries — a line per poll
		// per window would evict everything else during a slow server start,
		// which is exactly when the surrounding log lines are worth reading.
		if (attempt === 1) {
			const budgetSeconds = ((MAX_STATUS_ATTEMPTS - 1) * STATUS_RETRY_MS) / 1000;
			log(`no server_url yet, retrying every ${STATUS_RETRY_MS / 1000}s for up to ${budgetSeconds}s`);
		}
		await sleep(STATUS_RETRY_MS);
	}
	log(`giving up layout fetch after ${MAX_STATUS_ATTEMPTS} status checks`);
	return null;
}

/**
 * Fetch the current lab layout for the overlay windows.
 *
 * Shared by the compass, path-strip and timer overlays, which each had their
 * own copy of this and so needed the same bug fixed three times.
 *
 * Returns null for every failure — no server URL, no layout published for any
 * candidate difficulty, a throwing IPC or fetch. Each is logged through the
 * caller's `log`; callers keep their previous state on null rather than
 * clearing it.
 */
export async function fetchLabLayout(opts: {
	/** Difficulty named by the event that triggered this fetch, if any. */
	preferredDifficulty?: string;
	/** Difficulty this window has already settled on, if any. */
	lockedDifficulty?: string | null;
	log: (msg: string) => void;
}): Promise<LabLayout | null> {
	const { preferredDifficulty, lockedDifficulty, log } = opts;
	const diff = preferredDifficulty ?? lockedDifficulty;
	const diffs = diff ? [diff] : DEFAULT_DIFFICULTY_ORDER;
	try {
		const serverUrl = await waitForServerUrl(log);
		if (!serverUrl) return null;
		for (const d of diffs) {
			const r = await fetch(`${serverUrl}/api/lab/layout/${d}`);
			if (r.ok) return (await r.json()) as LabLayout;
		}
		log(`no layout found for: ${diffs.join(', ')}`);
		return null;
	} catch (e) {
		log(`fetchLayout error: ${e}`);
		return null;
	}
}
