/**
 * The DEBUG/PROD button must never be a one-way door.
 *
 * What this pins is the DECISION — label and target from the url in force, the
 * build's production target and the remembered url. Whether `TopBar.svelte`
 * sends a null-target click to Settings and persists the remembered url is in
 * a `.svelte` file with no harness.
 */
import { describe, expect, it } from 'vitest';
import { LOCAL_SERVER_URL, serverToggle } from './server-toggle';

const PROD = 'https://prod.example';
const STAGING = 'https://staging.example';

describe('on a non-local server', () => {
	it('offers the local server', () => {
		expect(serverToggle(PROD, PROD, '')).toEqual(
			expect.objectContaining({ label: 'PROD', target: LOCAL_SERVER_URL })
		);
	});

	it('reads an empty url (status not landed yet) as non-local, as before', () => {
		expect(serverToggle('', '', '').label).toBe('PROD');
	});
});

describe('on the local server', () => {
	it('goes back to the built-in production target', () => {
		expect(serverToggle(LOCAL_SERVER_URL, PROD, '')).toEqual(
			expect.objectContaining({ label: 'DEBUG', target: PROD })
		);
	});

	it('prefers the url the app actually left over the built-in target', () => {
		// The user was on staging when they flipped to DEBUG; "back" means staging.
		expect(serverToggle(LOCAL_SERVER_URL, PROD, STAGING).target).toBe(STAGING);
	});

	it('still goes back when only the remembered url exists', () => {
		// A build with no VITE_SERVER_URL — every fresh checkout — after one
		// flip from a Settings-typed production url.
		expect(serverToggle(LOCAL_SERVER_URL, '', PROD).target).toBe(PROD);
	});

	it('has no target, and says why, when there is nowhere to go back to', () => {
		// The regression this pins: the old fallback target WAS the local url,
		// so the button flipped to the same server forever.
		const t = serverToggle(LOCAL_SERVER_URL, '', '');
		expect(t.target).toBeNull();
		expect(t.label).toBe('DEBUG');
		expect(t.title).toMatch(/VITE_SERVER_URL/);
		expect(t.title).toMatch(/Settings/);
	});

	it('treats a built-in target equal to the local url as no target', () => {
		expect(serverToggle(LOCAL_SERVER_URL, LOCAL_SERVER_URL, '').target).toBeNull();
	});

	it('treats a remembered url equal to the local url as nothing remembered', () => {
		expect(serverToggle(LOCAL_SERVER_URL, '', LOCAL_SERVER_URL + '/').target).toBeNull();
	});

	it('ignores a trailing slash when deciding which side the app is on', () => {
		expect(serverToggle(LOCAL_SERVER_URL + '/', PROD, '').label).toBe('DEBUG');
	});
});
