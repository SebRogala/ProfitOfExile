import { describe, expect, it, vi } from 'vitest';
import { render } from 'svelte/server';

// TopBar reaches the IPC from its effects and imports; the real modules
// cannot load outside a webview.
vi.mock('@tauri-apps/api/webviewWindow', () => ({ getCurrentWebviewWindow: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => null) }));

import TopBar from './TopBar.svelte';

describe('TopBar proxy error', () => {
	it('shows the proxy error badge with the reason', () => {
		const { body } = render(TopBar, { props: { status: { proxy_error: 'R' } } });

		expect(body).toMatch(/title="R"[^>]*>Proxy error<\/button>/);
	});

	it('shows no proxy error badge without a blocked proxy', () => {
		const { body } = render(TopBar, { props: { status: { proxy_error: null } } });

		expect(body).not.toContain('Proxy error');
	});
});
