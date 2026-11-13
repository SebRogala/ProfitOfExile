/**
 * The server base URL every webview server call builds on (POE-383).
 *
 * Rust owns the value: `settings::server_base` (the saved `server_url`, trailing
 * `/` trimmed) rides on the SSOT snapshot as `server`, and `applySnapshot` hands
 * it here. Every window that runs `startSsotStore()` gets it — overlays included,
 * which have no status store. There is no build-time or localhost fallback: until
 * the first snapshot lands the url is unknown (null).
 *
 * The url also stays null until the Data & Privacy notice is accepted
 * (`$lib/privacy/gate.svelte`, ADR-033): no webview request reaches our server
 * before that. The first slice starts the gate in this window; the acceptance
 * publishes the slice's url to the same waiters and listeners.
 *
 * A leaf on purpose: it imports neither `$lib/api` nor `ssot.svelte`, because
 * `api.ts` reads from here and `ssot.svelte.ts` imports `api.ts` at module top.
 * Its one import, the notice gate, is a leaf too.
 *
 * Usage:
 *   import { getServerUrl, whenServerUrl, onServerUrlChange } from '$lib/stores/server-base.svelte';
 *   // Read: serverBase.url (null until known), serverBase.defaultUrl (the build's default)
 *   // Wait: await whenServerUrl(signal) — rejects after SERVER_URL_WAIT_MS
 *   // Watch: onServerUrlChange((url, previous) => …) — returns the unsubscribe
 *   // Write: applyServerSlice(snap.server) — ssot.svelte.ts `applySnapshot` only
 */

import { initNoticeGate, noticeGate, onNoticeAccepted } from '$lib/privacy/gate.svelte';

/** `AppSsotSnapshot.server` on the Rust side (same shape as ssot.svelte.ts `ServerSlice`). */
export interface ServerSliceInput {
	url: string;
	defaultUrl: string;
}

/**
 * How long a request waits for the first url before it fails like a request to
 * an unreachable host. The SSOT poll keeps retrying meanwhile; the caller's own
 * next call waits again.
 */
export const SERVER_URL_WAIT_MS = 10_000;

/** Reactive store — mutate properties, never reassign the export. */
export const serverBase = $state({
	url: null as string | null,
	defaultUrl: ''
});

type Listener = (url: string, previous: string | null) => void;

const listeners = new Set<Listener>();
const waiters = new Set<(url: string) => void>();
/** The last slice's url, published as `serverBase.url` once the notice is accepted. */
let sliceUrl: string | null = null;
let stopAcceptWait: (() => void) | null = null;

/** The saved server base, or null before the first snapshot carries one. */
export function getServerUrl(): string | null {
	return serverBase.url;
}

function publish(): void {
	if (sliceUrl === null || !noticeGate.accepted || sliceUrl === serverBase.url) return;
	const url = sliceUrl;
	const previous = serverBase.url;
	serverBase.url = url;
	for (const resolve of [...waiters]) resolve(url);
	for (const fn of [...listeners]) fn(url, previous);
}

/**
 * Take the server slice from a snapshot. Absent or an empty url leaves the
 * current value alone; a repeated url calls nobody.
 */
export function applyServerSlice(slice: ServerSliceInput | undefined): void {
	if (!slice) return;
	if (slice.defaultUrl) serverBase.defaultUrl = slice.defaultUrl;
	if (!slice.url) return;
	sliceUrl = slice.url;
	if (stopAcceptWait === null) {
		initNoticeGate();
		stopAcceptWait = onNoticeAccepted(publish);
	}
	publish();
}

/**
 * Call `fn` on every url change. Registered after the url is known, it is
 * called once at once with `previous = null`. Returns the unsubscribe.
 */
export function onServerUrlChange(fn: Listener): () => void {
	listeners.add(fn);
	if (serverBase.url !== null) fn(serverBase.url, null);
	return () => { listeners.delete(fn); };
}

/**
 * The url, once known. Rejects with `signal.reason` when the signal is or
 * becomes aborted, and with `Error('server url not known')` after
 * `SERVER_URL_WAIT_MS`.
 */
export function whenServerUrl(signal?: AbortSignal): Promise<string> {
	if (signal?.aborted) return Promise.reject(signal.reason);
	if (serverBase.url !== null) return Promise.resolve(serverBase.url);
	return new Promise<string>((resolve, reject) => {
		const settle = () => {
			waiters.delete(onUrl);
			signal?.removeEventListener('abort', onAbort);
			clearTimeout(timer);
		};
		const onUrl = (url: string) => { settle(); resolve(url); };
		const onAbort = () => { settle(); reject(signal!.reason); };
		const timer = setTimeout(() => { settle(); reject(new Error('server url not known')); }, SERVER_URL_WAIT_MS);
		waiters.add(onUrl);
		signal?.addEventListener('abort', onAbort);
	});
}

/** Tests only: back to "nothing known", no listeners, no waiters. */
export function resetServerBaseForTests(): void {
	serverBase.url = null;
	serverBase.defaultUrl = '';
	listeners.clear();
	waiters.clear();
	sliceUrl = null;
	stopAcceptWait?.();
	stopAcceptWait = null;
}
