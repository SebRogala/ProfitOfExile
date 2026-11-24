// @vitest-environment happy-dom
/**
 * SettingsPage mounted in the client runtime under happy-dom, so the Display
 * all-feeders switch is clicked for real. Rust is the stubbed `invoke`: every
 * call it records is an IPC write the page sent.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ calls: [] as [string, unknown][] }));

/** What the stubbed Rust answers the page's mount reads with; everything else answers null. */
const ANSWERS: Record<string, unknown> = {
	get_proxy_settings: { saved: '', inEffect: '', savedTrade: true, tradeInEffect: true, updaterProxy: '', blocked: '' },
	get_ocr_rects: [],
	get_widget_geometries: [],
	get_webview_gpu_disabled: false
};

vi.mock('@tauri-apps/api/core', () => ({
	invoke: vi.fn(async (cmd: string, args?: unknown) => {
		ipc.calls.push([cmd, args]);
		return Object.hasOwn(ANSWERS, cmd) ? ANSWERS[cmd] : null;
	})
}));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock('@tauri-apps/api/app', () => ({ getVersion: async () => '0.0.0-test' }));
vi.mock('@tauri-apps/api/window', () => ({
	availableMonitors: async () => [],
	currentMonitor: async () => null,
	primaryMonitor: async () => ({ scaleFactor: 1 })
}));
vi.mock('@tauri-apps/api/webviewWindow', () => ({
	getCurrentWebviewWindow: () => ({ listen: async () => () => {} })
}));
vi.mock('$lib/updater/settings-update.svelte', async () => {
	const actual = await vi.importActual<typeof import('$lib/updater/settings-update.svelte')>(
		'$lib/updater/settings-update.svelte'
	);
	return {
		...actual,
		createSettingsUpdateController: (snapshot: Parameters<typeof actual.createSettingsUpdateController>[0]) =>
			actual.createSettingsUpdateController(snapshot, {
				checkForUpdate: async () => null,
				relaunch: async () => {}
			})
	};
});

let { flushSync, mount, unmount } = await import('svelte');
let SettingsPage: typeof import('./SettingsPage.svelte').default;

// ssot.svelte.ts holds the choice at module level; fresh modules per case keep
// one case's click from showing up as the next case's stored choice.
beforeEach(async () => {
	vi.resetModules();
	({ flushSync, mount, unmount } = await import('svelte'));
	SettingsPage = (await import('./SettingsPage.svelte')).default;
});

let app: ReturnType<typeof mount> | null = null;

afterEach(() => {
	if (app) unmount(app);
	app = null;
	document.body.innerHTML = '';
	ipc.calls.length = 0;
});

async function settle(): Promise<void> {
	for (let i = 0; i < 10; i++) await Promise.resolve();
	flushSync();
}

/** The switch in the Display row whose label reads "Facetor's Lens: all feeders". */
function lensFeedersSwitch(): HTMLButtonElement {
	const row = [...document.querySelectorAll<HTMLElement>('.setting-row')].find(
		(r) => r.querySelector('.setting-label')?.textContent?.replaceAll('’', "'").trim() === "Facetor's Lens: all feeders"
	);
	const toggle = row?.querySelector<HTMLButtonElement>('button[role="switch"]');
	if (!toggle) throw new Error('the "Facetor\'s Lens: all feeders" switch did not render');
	return toggle;
}

const writes = () => ipc.calls.filter(([cmd]) => cmd === 'set_show_all_lens_feeders').map(([, args]) => args);

describe('SettingsPage all-feeders switch click', () => {
	// keep:happy
	it('each click writes the flipped choice through set_show_all_lens_feeders', async () => {
		app = mount(SettingsPage, { target: document.body });
		await settle();

		lensFeedersSwitch().click();
		flushSync();
		await settle();
		const afterOn = lensFeedersSwitch().getAttribute('aria-checked');
		lensFeedersSwitch().click();
		flushSync();
		await settle();

		expect(writes()).toEqual([{ show: true }, { show: false }]);
		expect([afterOn, lensFeedersSwitch().getAttribute('aria-checked')]).toEqual(['true', 'false']);
	});
});
