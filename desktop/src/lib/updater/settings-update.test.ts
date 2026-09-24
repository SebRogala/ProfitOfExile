import { describe, expect, it, vi } from 'vitest';
import {
	createSettingsUpdateController,
	type SettingsUpdateDependencies,
	type SettingsUpdateOffer
} from './settings-update.svelte';

function offer(
	version: string,
	downloadAndInstall: SettingsUpdateOffer['downloadAndInstall'] = async () => {}
): SettingsUpdateOffer {
	return { version, downloadAndInstall };
}

function dependencies(
	checkForUpdate: SettingsUpdateDependencies['checkForUpdate'] = async () => null,
	relaunch: SettingsUpdateDependencies['relaunch'] = async () => {}
): SettingsUpdateDependencies {
	return { checkForUpdate, relaunch };
}

function deferred<T>() {
	let resolve!: (value: T) => void;
	const promise = new Promise<T>((settle) => {
		resolve = settle;
	});
	return { promise, resolve };
}

describe('settings update controller', () => {
	it('starts with the available update from the status snapshot', () => {
		const controller = createSettingsUpdateController({
			updateAvailable: true,
			updateVersion: '1.2.0'
		}, dependencies());

		expect(controller.status).toBe('available');
		expect(controller.version).toBe('1.2.0');
		expect(controller.error).toBe('');
		expect(controller.progress).toBe(0);
	});

	it('offers a background version while idle', () => {
		const controller = createSettingsUpdateController({
			updateAvailable: false,
			updateVersion: ''
		}, dependencies());

		controller.offerBackgroundUpdate('1.3.0');

		expect(controller.status).toBe('available');
		expect(controller.version).toBe('1.3.0');
	});

	it('shows the version returned by a manual check', async () => {
		const controller = createSettingsUpdateController({
			updateAvailable: false,
			updateVersion: ''
		}, dependencies(async () => offer('1.3.0')));

		await controller.checkForUpdates();

		expect(controller.status).toBe('available');
		expect(controller.version).toBe('1.3.0');
	});

	it('reports a manual check with no update as the latest-version message', async () => {
		const controller = createSettingsUpdateController({
			updateAvailable: false,
			updateVersion: ''
		}, dependencies(async () => null));

		await controller.checkForUpdates();

		expect(controller.status).toBe('idle');
		expect(controller.error).toBe('You are on the latest version.');
	});

	it('reports a failed manual check as an error', async () => {
		const controller = createSettingsUpdateController({
			updateAvailable: false,
			updateVersion: ''
		}, dependencies(async () => {
			throw new Error('network unavailable');
		}));

		await controller.checkForUpdates();

		expect(controller.status).toBe('error');
		expect(controller.error).toBe('network unavailable');
	});

	it('does not overwrite a check already in progress with a background offer', async () => {
		const check = deferred<SettingsUpdateOffer | null>();
		const controller = createSettingsUpdateController({
			updateAvailable: false,
			updateVersion: ''
		}, dependencies(() => check.promise));

		const checking = controller.checkForUpdates();
		controller.offerBackgroundUpdate('1.3.0');

		expect(controller.status).toBe('checking');
		expect(controller.version).toBe('');
		check.resolve(null);
		await checking;
	});

	it('does not overwrite a download already in progress with a background offer', async () => {
		const check = deferred<SettingsUpdateOffer | null>();
		const controller = createSettingsUpdateController({
			updateAvailable: true,
			updateVersion: '1.2.0'
		}, dependencies(() => check.promise));

		const installing = controller.installUpdate();
		controller.offerBackgroundUpdate('1.3.0');

		expect(controller.status).toBe('downloading');
		expect(controller.version).toBe('1.2.0');
		check.resolve(null);
		await installing;
	});

	it('does not overwrite an update error with a background offer', async () => {
		const controller = createSettingsUpdateController({
			updateAvailable: false,
			updateVersion: ''
		}, dependencies(async () => {
			throw new Error('download metadata failed');
		}));

		await controller.checkForUpdates();
		controller.offerBackgroundUpdate('1.3.0');

		expect(controller.status).toBe('error');
		expect(controller.version).toBe('');
		expect(controller.error).toBe('download metadata failed');
	});

	it('relaunches only after the download resolves', async () => {
		const downloadStarted = deferred<void>();
		const download = deferred<void>();
		const checkForUpdate = vi.fn(async () => offer('1.3.0', async () => {
			downloadStarted.resolve(undefined);
			await download.promise;
		}));
		const relaunch = vi.fn(async () => {});
		const controller = createSettingsUpdateController({
			updateAvailable: true,
			updateVersion: '1.2.0'
		}, dependencies(checkForUpdate, relaunch));

		const installing = controller.installUpdate();
		await downloadStarted.promise;

		expect(checkForUpdate).toHaveBeenCalledTimes(1);
		expect(relaunch).not.toHaveBeenCalled();

		download.resolve(undefined);
		await installing;

		expect(relaunch).toHaveBeenCalledTimes(1);
	});

	it('tracks byte progress until the download finishes', async () => {
		const downloadStarted = deferred<void>();
		const download = deferred<void>();
		let controller: ReturnType<typeof createSettingsUpdateController>;
		const checkForUpdate = vi.fn(async () => offer('1.3.0', async (onProgress) => {
			onProgress({ event: 'Started', data: { contentLength: 2048 } });
			onProgress({ event: 'Progress', data: { chunkLength: 512 } });
			downloadStarted.resolve(undefined);
			await download.promise;
			onProgress({ event: 'Finished' });
		}));
		controller = createSettingsUpdateController({
			updateAvailable: true,
			updateVersion: '1.2.0'
		}, dependencies(checkForUpdate));

		const installing = controller.installUpdate();
		await downloadStarted.promise;

		expect(controller.progress).toBe(512);

		download.resolve(undefined);
		await installing;

		expect(controller.progress).toBe(0);
	});

	it('reports a failed download without relaunching', async () => {
		const relaunch = vi.fn(async () => {});
		const controller = createSettingsUpdateController({
			updateAvailable: true,
			updateVersion: '1.2.0'
		}, dependencies(async () => offer('1.3.0', async () => {
			throw new Error('installer failed');
		}), relaunch));

		await controller.installUpdate();

		expect(controller.status).toBe('error');
		expect(controller.error).toBe('installer failed');
		expect(relaunch).not.toHaveBeenCalled();
	});

	it('keeps downloading when the install re-check finds no offer', async () => {
		const relaunch = vi.fn(async () => {});
		const controller = createSettingsUpdateController({
			updateAvailable: true,
			updateVersion: '1.2.0'
		}, dependencies(async () => null, relaunch));

		await controller.installUpdate();

		expect(controller.status).toBe('downloading');
		expect(controller.error).toBe('');
		expect(relaunch).not.toHaveBeenCalled();
	});
});
