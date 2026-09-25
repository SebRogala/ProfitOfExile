import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from 'svelte/server';
import type { SettingsUpdateOffer } from '$lib/updater/settings-update.svelte';
import { store } from '$lib/stores/status.svelte';

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
