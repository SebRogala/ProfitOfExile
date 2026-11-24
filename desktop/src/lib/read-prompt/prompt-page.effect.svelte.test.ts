// @vitest-environment happy-dom
// keep:happy
/**
 * The production read-prompt page mounted under happy-dom (POE-438): a
 * snapshot presenting no mechanic draws no prompt, a mercenary arm draws its text.
 * Only WidgetHost placement is doubled; the page's own content conditional decides.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => ({})) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock('$lib/overlay/widgets/WidgetHost.svelte', async () => ({
	default: (await import('./fixtures/WidgetHostDouble.svelte')).default
}));

// No vi.resetModules(): after a reset the mocked WidgetHost double and the page
// resolve different Svelte runtime instances, and rendering the snippet throws
// "Cannot read properties of null (reading 'nodes')". Each case sets the store instead.
const { flushSync, mount, unmount } = await import('svelte');
const PromptPage = (await import('../../routes/overlay/prompt/+page.svelte')).default;
const { ssot } = await import('$lib/stores/ssot.svelte');

beforeEach(() => {
	ssot.readHotkey = { shortcut: { key: 65, shift: false, ctrl: true, alt: false }, delayMs: 0, recalibrateSeconds: 5 };
	ssot.overlayDismiss = null;
});

let mounted: ReturnType<typeof mount> | null = null;
afterEach(() => {
	if (mounted) unmount(mounted);
	mounted = null;
	document.body.innerHTML = '';
});

function render(): HTMLElement {
	const target = document.createElement('div');
	document.body.appendChild(target);
	mounted = mount(PromptPage, { target });
	flushSync();
	return target;
}

describe('read prompt page', () => {
	it('keep:happy splits the recalibrate offer into a warning header and a body with Ctrl and A key chips', () => {
		ssot.readPrompt = {
			mechanic: null, noMatch: true, recalibrateOffer: true, recalibrating: false, seq: 5,
			gemUncertain: [false, false, false]
		};
		const target = render();
		const notice = target.querySelector('[data-kind]');
		const header = notice?.querySelector('[data-part="header"]');
		const body = notice?.querySelector('[data-part="body"]');
		expect(header?.textContent).toBe('No mechanics detected');
		expect(body?.textContent).toBe('Press Ctrl + A within 5 s to recalibrate');
		expect(Array.from(body?.querySelectorAll('kbd') ?? [], (key) => key.textContent)).toEqual(['Ctrl', 'A']);
		expect(notice?.getAttribute('data-kind')).toBe('warning');
	});

	it('draws no prompt when no mechanic is presented', () => {
		ssot.readPrompt = { mechanic: null, noMatch: false, recalibrating: false, seq: 1 };
		const target = render();
		expect(target.querySelector('[data-kind]')).toBeNull();
	});

	it('removes the detected text when a mercenary arm presentation expires', () => {
		ssot.readPrompt = { mechanic: 'mercenary', noMatch: false, recalibrating: false, seq: 2 };
		const target = render();
		const notices = target.querySelectorAll('[data-kind]');
		expect(notices).toHaveLength(1);
		expect(notices[0].getAttribute('data-kind')).toBe('info');
		expect(notices[0].querySelector('[data-part="header"]')?.textContent).toBe('Mercenary detected');
		expect(notices[0].querySelector('[data-part="body"]')?.textContent).toBe('Press Ctrl + A to read');

		ssot.readPrompt = { mechanic: null, noMatch: false, recalibrating: false, seq: 2 };
		flushSync();

		expect(target.querySelectorAll('[data-kind]')).toHaveLength(0);
		expect(target.textContent).not.toContain('Mercenary detected');
	});

	it('draws the recalibration sweep status with a progress spinner', () => {
		ssot.readPrompt = { mechanic: null, noMatch: false, recalibrating: true, seq: 3 };
		const target = render();
		const notices = target.querySelectorAll('[data-kind]');
		expect(notices).toHaveLength(1);
		expect(notices[0].getAttribute('data-kind')).toBe('info');
		expect(notices[0].querySelector('[data-part="header"]')?.textContent).toBe('No mechanics detected');
		expect(notices[0].querySelector('[data-part="body"]')?.textContent).toBe('Running a recalibration sweep…');
		expect(notices[0].querySelector('.spinner')).not.toBeNull();
	});

	it('asks for another press on the uncertain gem slot', () => {
		ssot.readPrompt = { mechanic: null, noMatch: false, recalibrating: false, seq: 4, gemUncertain: [false, false, true] };
		const target = render();
		const notices = target.querySelectorAll('[data-kind]');
		expect(notices).toHaveLength(1);
		expect(notices[0].getAttribute('data-kind')).toBe('warning');
		expect(notices[0].querySelector('[data-part="header"]')?.textContent).toBe('Gem 3 unclear');
		const body = notices[0].querySelector('[data-part="body"]');
		expect(body?.textContent).toBe('Press Ctrl + A after the Font reveals, or while hovering the gem');
		expect(Array.from(body?.querySelectorAll('kbd') ?? [], (key) => key.textContent)).toEqual(['Ctrl', 'A']);
	});
});
