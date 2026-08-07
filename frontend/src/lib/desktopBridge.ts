/**
 * Desktop pairing bridge.
 * Manages the pairing code and subscribes to Mercure events
 * from the desktop app for auto-filling gem names.
 */

const STORAGE_KEY = 'desktopPair';

export function getPairCode(): string | null {
	if (typeof window === 'undefined') return null;
	return localStorage.getItem(STORAGE_KEY);
}

export function setPairCode(code: string): void {
	if (typeof window === 'undefined') return;
	localStorage.setItem(STORAGE_KEY, code);
}

export function clearPairCode(): void {
	if (typeof window === 'undefined') return;
	localStorage.removeItem(STORAGE_KEY);
}

/**
 * Subscribe to desktop gem-detection events via Mercure SSE.
 *
 * Token fetch then EventSource, like connectMercure() in api.ts, but only that
 * far: this one doubles its backoff straight to the 60s cap without api.ts's
 * fast lane, jitter, debounced disconnect indicator, or hidden-tab deferral.
 * It carries one pair code and one topic, so the load it can put on the hub is
 * a fraction of the dashboard's.
 *
 * Returns an unsubscribe function that closes the EventSource.
 */
export function subscribeToDesktopGems(
	// `mode` is the lab mode the desktop scanned in, absent from older desktop
	// builds. The market is only interpretable against it: "21/23" means nothing
	// to a Normal-mode view, and "20/20" is a real market in Normal and no market
	// at all in Dedication.
	onGemsDetected: (gems: string[], variant: string, mode?: 'normal' | 'dedication') => void,
	onConnectionChange?: (connected: boolean) => void
): () => void {
	const pairCode = getPairCode();
	if (!pairCode) return () => {};

	let eventSource: EventSource | null = null;
	let tokenTimeout: ReturnType<typeof setTimeout> | null = null;
	let retries = 0;
	let closed = false;

	function retryDelay(): number {
		return Math.min(2000 * Math.pow(2, retries), 60000);
	}

	/**
	 * Drop a subscription. Detaches the handlers before closing so a queued
	 * event cannot re-enter them, and forgets the reference only if this is
	 * still the live source.
	 */
	function closeSource(source: EventSource) {
		source.onopen = null;
		source.onmessage = null;
		source.onerror = null;
		source.close();
		if (eventSource === source) eventSource = null;
	}

	async function connect() {
		if (closed) return;

		try {
			const resp = await fetch('/api/mercure/token');
			if (!resp.ok) throw new Error(`Token fetch failed: ${resp.status}`);
			const { token, url } = await resp.json();
			if (closed) return;
			if (eventSource) closeSource(eventSource);

			const authedUrl = new URL(url);
			authedUrl.searchParams.set('topic', `poe/desktop/${pairCode}`);
			authedUrl.searchParams.set('authorization', token);

			// Handlers are bound to this source rather than to whatever
			// `eventSource` points at when they fire, so a late event from a
			// superseded subscription cannot close the current one.
			const source = new EventSource(authedUrl.toString());
			eventSource = source;

			source.onopen = () => {
				onConnectionChange?.(true);
				retries = 0;
			};

			source.onmessage = (msg) => {
				try {
					const data = JSON.parse(msg.data);
					if (data.type === 'gems-detected' && Array.isArray(data.gems)) {
						onGemsDetected(data.gems, data.variant || '20/20', data.mode);
					} else {
						console.warn('[DesktopBridge] Unexpected message type:', data.type);
					}
				} catch (err) {
					console.warn('[DesktopBridge] Failed to parse Mercure message:', err, 'raw:', msg.data);
				}
			};

			source.onerror = () => {
				// A failed EventSource is not a finished one: the browser keeps
				// reconnecting it on its own and keeps delivering through the
				// handlers above. Leaving it open while connect() opens a second
				// subscription put two of them on the same topic, and since the
				// close below only ran on the success path, a token fetch that kept
				// failing left the first one running for the whole outage.
				closeSource(source);
				onConnectionChange?.(false);
				if (closed) return;
				if (tokenTimeout) clearTimeout(tokenTimeout);
				retries++;
				// Delay read after the increment: it is the one actually slept, and
				// the pre-increment read logged "retrying in 2s" before a 4s wait.
				const delay = retryDelay();
				console.warn('[DesktopBridge] SSE connection lost, retrying in', delay / 1000, 's');
				tokenTimeout = setTimeout(connect, delay);
			};

			// Token TTL is 30min — refresh before expiry
			if (tokenTimeout) clearTimeout(tokenTimeout);
			tokenTimeout = setTimeout(connect, 25 * 60 * 1000);
		} catch (err) {
			// Any live source is left alone here, and still reported as connected.
			// connect() also runs as the 25-minute token refresh, and a token fetch
			// that fails while the stream is healthy is no reason to drop the
			// subscription or the badge — the token is only needed to open a NEW
			// one, and the browser keeps delivering through the handlers above. The
			// error path closes its own source in onerror.
			//
			// The catch is also reached with no live source, and not only when the
			// fetch itself threw: closeSource() runs before `new URL(url)`, so a
			// token response carrying a missing or malformed url lands here having
			// already dropped the old subscription. `eventSource === null` is what
			// separates the two, and it is the only case where disconnected is the
			// true state. Reporting it unconditionally left the badge stuck on
			// disconnected for a whole outage — nothing corrects it until a
			// successful connect reaches onopen, at a 60s retry cap.
			if (eventSource === null) onConnectionChange?.(false);
			if (closed) return;
			retries++;
			const delay = retryDelay();
			console.warn('[DesktopBridge] Connection failed, retrying in', delay / 1000, 's:', err);
			if (tokenTimeout) clearTimeout(tokenTimeout);
			tokenTimeout = setTimeout(connect, delay);
		}
	}

	connect();

	return () => {
		closed = true;
		if (eventSource) closeSource(eventSource);
		if (tokenTimeout) clearTimeout(tokenTimeout);
		onConnectionChange?.(false);
	};
}
