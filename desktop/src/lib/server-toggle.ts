/**
 * What the top bar's DEBUG/PROD button does next.
 *
 * The button flips a dev build between the local server and a production one.
 * The production target is whatever `VITE_SERVER_URL` was at build time, and a
 * build made without it — every fresh Windows checkout until the variable is
 * set (docs/DEV-SETUP.md, "Build-time variables") — has none. Before this
 * module the fallback for a missing target was the LOCAL url itself, so one
 * click onto DEBUG was a one-way door: every later click "back to PROD" set
 * the local url again and the button kept saying DEBUG.
 *
 * Two rules close that door:
 *
 * - The url the app LEFT when it went to DEBUG is remembered, and it is the
 *   first choice on the way back — ahead of the built-in target, because it is
 *   where the user actually was (a staging host, a production url typed into
 *   Settings). The caller persists it (ADR-013 pref `devServerReturnUrl`) so
 *   a restart on DEBUG still knows the way home.
 * - With nothing to go back to, the button has no target: its tooltip says
 *   why, and the click opens Settings, where a server url can be typed —
 *   instead of quietly re-applying the local url.
 *
 * The decision lives here rather than in `components/TopBar.svelte` because a
 * `.svelte` file has no unit-test harness in this app — the same split
 * `overlay/clickthrough-report.ts` uses.
 */

/** The local server every dev build defaults to (`settings.rs`, `api.ts`). */
export const LOCAL_SERVER_URL = 'https://profitofexile.localhost';

/** What the button shows and does. `target === null` means the click goes to Settings. */
export interface ServerToggle {
	/** The side the app is on now: DEBUG on the local server, PROD anywhere else. */
	label: 'DEBUG' | 'PROD';
	/** The url one click applies, or null when there is nowhere to go (the caller opens Settings). */
	target: string | null;
	/** The tooltip — where the click goes, or why it cannot. */
	title: string;
}

/** Trailing slashes do not make a different server. */
function sameServer(a: string, b: string): boolean {
	return a.replace(/\/+$/, '') === b.replace(/\/+$/, '');
}

/**
 * Decide the button from the url in force, the build's production target and
 * the url remembered from the last flip to DEBUG.
 *
 * `current` is `status.server_url`, or '' before the status has landed; ''
 * reads as "not on the local server", which is what the old code did too.
 * A `builtProdUrl` that is itself the local url counts as no target: it is the
 * exact value that made the door one-way.
 */
export function serverToggle(current: string, builtProdUrl: string, remembered: string): ServerToggle {
	if (!sameServer(current, LOCAL_SERVER_URL)) {
		return { label: 'PROD', target: LOCAL_SERVER_URL, title: `Switch to the local server (${LOCAL_SERVER_URL})` };
	}
	const back = [remembered, builtProdUrl].find((u) => u && !sameServer(u, LOCAL_SERVER_URL)) ?? null;
	if (back) {
		return { label: 'DEBUG', target: back, title: `Switch back to ${back}` };
	}
	return {
		label: 'DEBUG',
		target: null,
		title:
			'No production server to switch back to: this build had no VITE_SERVER_URL ' +
			'and the app has not left the local server since it started. Click to open ' +
			'Settings and edit the server URL, or set the variable before `npx tauri dev`.'
	};
}
