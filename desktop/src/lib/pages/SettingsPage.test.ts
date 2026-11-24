import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from 'svelte/server';
import type { SettingsUpdateOffer } from '$lib/updater/settings-update.svelte';
import { store } from '$lib/stores/status.svelte';
import { ssot } from '$lib/stores/ssot.svelte';

type CapturedController = {
	status: string;
	version: string;
	error: string;
	checkForUpdates: () => Promise<void>;
	installUpdate: () => Promise<void>;
	offerBackgroundUpdate: (version: string) => void;
};

const pageHarness = vi.hoisted(() => ({
	checkForUpdate: vi.fn(async () => null as SettingsUpdateOffer | null),
	relaunch: vi.fn(async () => {}),
	snapshot: null as object | null,
	controller: null as CapturedController | null
}));

vi.mock('$lib/updater/settings-update.svelte', async () => {
	const actual = await vi.importActual<typeof import('$lib/updater/settings-update.svelte')>(
		'$lib/updater/settings-update.svelte'
	);
	return {
		...actual,
		createSettingsUpdateController: (
			snapshot: Parameters<typeof actual.createSettingsUpdateController>[0]
		) => {
			pageHarness.snapshot = snapshot;
			const controller = actual.createSettingsUpdateController(snapshot, {
				checkForUpdate: pageHarness.checkForUpdate,
				relaunch: pageHarness.relaunch
			});
			pageHarness.controller = controller;
			return controller;
		}
	};
});

import SettingsPage from './SettingsPage.svelte';

function updateOffer(
	version: string,
	downloadAndInstall: SettingsUpdateOffer['downloadAndInstall'] = async () => {}
): SettingsUpdateOffer {
	return { version, downloadAndInstall };
}

function renderSettingsPage(): string {
	return render(SettingsPage).body;
}

describe('SettingsPage update recovery', () => {
	beforeEach(() => {
		store.updateAvailable = true;
		store.updateVersion = '1.2.0';
		pageHarness.checkForUpdate.mockReset();
		pageHarness.relaunch.mockReset();
		pageHarness.snapshot = null;
		pageHarness.controller = null;
	});

	afterEach(() => {
		store.updateAvailable = false;
		store.updateVersion = '';
	});

	it('passes the live update store to the controller factory', () => {
		renderSettingsPage();

		expect(pageHarness.snapshot).toBe(store);
	});

	it('clears the vanished offer held by SettingsPage', async () => {
		pageHarness.checkForUpdate.mockResolvedValueOnce(null);

		renderSettingsPage();
		const controller = pageHarness.controller;
		expect(controller).not.toBeNull();

		await controller!.installUpdate();

		expect(controller!.status).toBe('idle');
		expect(controller!.error).toBe('You are on the latest version.');
		expect(store.updateAvailable).toBe(false);
		expect(store.updateVersion).toBe('');
		expect(pageHarness.relaunch).not.toHaveBeenCalled();

		const recoveredMarkup = renderSettingsPage();
		expect(recoveredMarkup).toContain('Check for Updates');
		expect(recoveredMarkup).not.toMatch(/Install(?: &amp;| &) Restart/);

		pageHarness.checkForUpdate.mockResolvedValueOnce(updateOffer('1.3.0'));
		await controller!.checkForUpdates();

		expect(controller!.status).toBe('available');
		expect(controller!.version).toBe('1.3.0');
	});
});

/** The markup from the Proxy field to the trade toggle below it. */
function proxyBlock(markup: string): string {
	const start = markup.indexOf('for="proxy-url"');
	const end = markup.indexOf('Use the proxy for trade requests');
	expect(start).toBeGreaterThan(-1);
	expect(end).toBeGreaterThan(start);
	return markup.slice(start, end);
}

describe('SettingsPage blocked proxy', () => {
	afterEach(() => {
		store.status = null;
	});

	it('shows the blocked proxy reason on the Proxy field', () => {
		store.status = { proxy_error: 'R' } as unknown as typeof store.status;

		expect(proxyBlock(renderSettingsPage())).toMatch(/class="setting-error[^"]*" role="alert">R</);
	});

	it('shows no proxy error without a blocked proxy', () => {
		store.status = { proxy_error: null } as unknown as typeof store.status;

		expect(proxyBlock(renderSettingsPage())).not.toMatch(/class="setting-error[^"]*" role="alert"/);
	});
});

describe('SettingsPage About section', () => {
	it('shows the GGG non-affiliation sentence verbatim', () => {
		expect(renderSettingsPage()).toContain(
			"This product isn't affiliated with or endorsed by Grinding Gear Games in any way."
		);
	});
});

const LENS_FEEDERS_LABEL = "Facetor's Lens: all feeders";

/** The markup of Settings › Display, from its heading to the end of its section. */
function displaySection(markup: string): string {
	const start = markup.search(/<h2[^>]*>Display<\/h2>/);
	expect(start, 'Settings has no Display section').toBeGreaterThan(-1);
	return markup.slice(start, markup.indexOf('</section>', start));
}

/** A row label as the player reads it: entities decoded, curly apostrophes folded to ASCII. */
function labelText(raw: string): string {
	return raw.replaceAll('&#39;', "'").replaceAll('&apos;', "'").replaceAll('&amp;', '&').replaceAll('’', "'").trim();
}

/** The switch markup in the Display row labelled `label`. */
function displaySwitch(markup: string, label: string): string {
	const rows = displaySection(markup).split(/class="setting-row[\s"]/).slice(1);
	const row = rows.find((r) => labelText(r.match(/class="setting-label[^"]*">([^<]*)</)?.[1] ?? '') === label);
	expect(row, `Display has no "${label}" row`).toBeDefined();
	const toggle = row!.match(/<button[^>]*role="switch"[^>]*>/)?.[0];
	expect(toggle, `the "${label}" row has no switch`).toBeDefined();
	return toggle!;
}

describe('SettingsPage Display all-feeders toggle', () => {
	afterEach(() => {
		ssot.showAllLensFeeders = false;
	});

	// keep:happy
	it('lists the all-feeders toggle right after Number format, with no note of its own', () => {
		const display = displaySection(renderSettingsPage());
		const labels = [...display.matchAll(/class="setting-label[^"]*">([^<]*)</g)].map((m) => labelText(m[1]));
		expect(labels).toEqual(['Number format', LENS_FEEDERS_LABEL, 'Render without GPU']);
		expect(display.match(/class="setting-note/g)).toHaveLength(1);
	});

	// keep:happy
	it.each([false, true])('shows the stored all-feeders choice %s on the switch', (show) => {
		ssot.showAllLensFeeders = show;

		expect(displaySwitch(renderSettingsPage(), LENS_FEEDERS_LABEL)).toContain(`aria-checked="${show}"`);
	});
});

describe('SettingsPage Read exchange row', () => {
	afterEach(() => {
		ssot.readHotkey = null;
		ssot.exchangeRead = null;
	});

	it('keep:happy: renders the Read exchange shortcut from the SSOT right after the Read screen rows', () => {
		ssot.readHotkey = { shortcut: { key: 120, shift: true, ctrl: false, alt: false }, delayMs: 0, recalibrateSeconds: 5 };
		ssot.exchangeRead = { shortcut: { key: 2, shift: false, ctrl: false, alt: true } };

		const markup = renderSettingsPage();
		const readScreen = markup.indexOf('aria-label="Read screen: Shift + F9"');
		const lastReadScreenRow = markup.indexOf('>Recalibrate window<');
		const readExchange = markup.indexOf('aria-label="Read exchange: Alt + Right click"');
		const nextSetting = markup.indexOf('>Global timeout<');
		expect(readScreen).toBeGreaterThan(-1);
		expect(lastReadScreenRow).toBeGreaterThan(readScreen);
		expect(readExchange).toBeGreaterThan(lastReadScreenRow);
		expect(nextSetting).toBeGreaterThan(readExchange);
	});
});
