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
	ssot.readHotkey = { shortcut: { key: 65, shift: false, ctrl: true, alt: false }, delayMs: 0 };
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
	it('draws no prompt when no mechanic is presented', () => {
		ssot.readPrompt = { mechanic: null, noMatch: false, seq: 1 };
		const target = render();
		expect(target.querySelector('.prompt')).toBeNull();
	});

	it('draws the detected text for a mercenary arm', () => {
		ssot.readPrompt = { mechanic: 'mercenary', noMatch: false, seq: 2 };
		const target = render();
		const prompts = target.querySelectorAll('.prompt');
		expect(prompts).toHaveLength(1);
		expect(prompts[0].textContent).toBe('Detected Mercenary - press Ctrl + A to read');
	});
});
