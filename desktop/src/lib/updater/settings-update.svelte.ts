import { relaunch } from '@tauri-apps/plugin-process';
import { checkForUpdate } from '$lib/updater/check';

export type UpdateStatus = 'idle' | 'checking' | 'available' | 'downloading' | 'error';

export type UpdateProgress = {
	event: string;
	data?: {
		contentLength?: number;
		chunkLength?: number;
	};
};

export type SettingsUpdateSnapshot = {
	updateAvailable: boolean;
	updateVersion: string;
};

export type SettingsUpdateOffer = {
	version: string;
	downloadAndInstall: (onProgress: (progress: UpdateProgress) => void) => Promise<void>;
};

export type SettingsUpdateDependencies = {
	checkForUpdate: () => Promise<SettingsUpdateOffer | null>;
	relaunch: () => Promise<void>;
};

const productionDependencies: SettingsUpdateDependencies = {
	checkForUpdate: async () => (await checkForUpdate()) as SettingsUpdateOffer | null,
	relaunch
};

function errorMessage(error: unknown): string {
	return (error as { message?: string } | null)?.message || String(error);
}

export function createSettingsUpdateController(
	snapshot: SettingsUpdateSnapshot,
	dependencies: SettingsUpdateDependencies = productionDependencies
) {
	let status = $state<UpdateStatus>(snapshot.updateAvailable ? 'available' : 'idle');
	let version = $state(snapshot.updateVersion || '');
	let error = $state('');
	let progress = $state(0);

	async function checkForUpdates(): Promise<void> {
		status = 'checking';
		error = '';
		try {
			const update = await dependencies.checkForUpdate();
			if (update) {
				status = 'available';
				version = update.version;
			} else {
				status = 'idle';
				error = 'You are on the latest version.';
			}
		} catch (caught) {
			status = 'error';
			error = errorMessage(caught);
		}
	}

	async function installUpdate(): Promise<void> {
		status = 'downloading';
		error = '';
		try {
			const update = await dependencies.checkForUpdate();
			if (!update) return;
			await update.downloadAndInstall((downloadProgress) => {
				if (downloadProgress.event === 'Started' && downloadProgress.data?.contentLength) {
					progress = 0;
				} else if (downloadProgress.event === 'Progress') {
					progress += downloadProgress.data?.chunkLength ?? 0;
				} else if (downloadProgress.event === 'Finished') {
					progress = 0;
				}
			});
			await dependencies.relaunch();
		} catch (caught) {
			status = 'error';
			error = errorMessage(caught);
		}
	}

	function offerBackgroundUpdate(backgroundVersion: string): void {
		if (status !== 'idle') return;
		status = 'available';
		version = backgroundVersion;
	}

	return {
		get status() {
			return status;
		},
		get version() {
			return version;
		},
		get error() {
			return error;
		},
		get progress() {
			return progress;
		},
		checkForUpdates,
		installUpdate,
		offerBackgroundUpdate
	};
}
