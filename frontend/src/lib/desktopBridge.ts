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
 * Follows the same token-fetch + EventSource pattern as connectMercure() in api.ts.
 *
 * Returns an unsubscribe function that closes the EventSource.
 */
export function subscribeToDesktopGems(
	onGemsDetected: (gems: string[], variant: string) => void,
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

	async function connect() {
		if (closed) return;

		try {
			const resp = await fetch('/api/mercure/token');
			if (!resp.ok) throw new Error(`Token fetch failed: ${resp.status}`);
			const { token, url } = await resp.json();
			if (closed) return;
			if (eventSource) eventSource.close();

			const authedUrl = new URL(url);
			authedUrl.searchParams.set('topic', `poe/desktop/${pairCode}`);
			authedUrl.searchParams.set('authorization', token);

			eventSource = new EventSource(authedUrl.toString());

			eventSource.onopen = () => {
				onConnectionChange?.(true);
				retries = 0;
			};

			eventSource.onmessage = (msg) => {
				try {
					const data = JSON.parse(msg.data);
					if (data.type === 'gems-detected' && Array.isArray(data.gems)) {
						onGemsDetected(data.gems, data.variant || '20/20');
					} else {
						console.warn('[DesktopBridge] Unexpected message type:', data.type);
					}
				} catch (err) {
					console.warn('[DesktopBridge] Failed to parse Mercure message:', err, 'raw:', msg.data);
				}
			};

			eventSource.onerror = () => {
				console.warn('[DesktopBridge] SSE connection lost, retrying in', retryDelay() / 1000, 's');
				onConnectionChange?.(false);
				if (closed) return;
				if (tokenTimeout) clearTimeout(tokenTimeout);
				retries++;
				tokenTimeout = setTimeout(connect, retryDelay());
			};

			// Token TTL is 30min — refresh before expiry
			if (tokenTimeout) clearTimeout(tokenTimeout);
			tokenTimeout = setTimeout(connect, 25 * 60 * 1000);
		} catch (err) {
			console.warn('[DesktopBridge] Connection failed, retrying in', retryDelay() / 1000, 's:', err);
			onConnectionChange?.(false);
			if (closed) return;
			retries++;
			if (tokenTimeout) clearTimeout(tokenTimeout);
			tokenTimeout = setTimeout(connect, retryDelay());
		}
	}

	connect();

	return () => {
		closed = true;
		if (eventSource) eventSource.close();
		if (tokenTimeout) clearTimeout(tokenTimeout);
		onConnectionChange?.(false);
	};
}
