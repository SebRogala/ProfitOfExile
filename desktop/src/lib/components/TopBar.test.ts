import { describe, expect, it, vi } from 'vitest';
import { render } from 'svelte/server';

// TopBar reaches the IPC from its effects and imports; the real modules
// cannot load outside a webview.
vi.mock('@tauri-apps/api/webviewWindow', () => ({ getCurrentWebviewWindow: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => null) }));

import TopBar, { accountView } from './TopBar.svelte';
import { registration, type Registration } from '$lib/stores/entitlements.svelte';

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


/** Fixture states (handoff fixtures.json → deviceMe), already normalized. */
const REGISTERED: Registration = { state: 'registered', alias: 'Gaming PC', role: 'beta' };
const REGISTERED_NO_ROLE: Registration = { state: 'registered', alias: 'Gaming PC', role: '' };
const UNREGISTERED: Registration = { state: 'unregistered', alias: '', role: '' };
const UNKNOWN: Registration = { state: 'unknown', alias: '', role: 'beta' };
const CHECKING: Registration = { state: 'checking', alias: '', role: '' };

/** The tooltip's lines as text, in order. */
function tipLines(html: string): string[] {
	return [...html.matchAll(/<div[^>]*>(.*?)<\/div>/g)].map((m) => m[1]);
}

describe('accountView', () => {
	it('shows a registered device as a filled disc with its initial', () => {
		const v = accountView(REGISTERED);

		expect(v).toMatchObject({ glyph: 'initial', initial: 'G', filled: true, dashed: false, ariaLabel: 'Account: registered as Gaming PC' });
		expect(tipLines(v.tooltipHtml)).toEqual(['Registered as “Gaming PC”', 'Role beta · click to open device identity']);
	});

	it('drops the role part of line 2 when the role is empty', () => {
		expect(tipLines(accountView(REGISTERED_NO_ROLE).tooltipHtml)[1]).toBe('click to open device identity');
	});

	it('shows an unregistered device as a hollow solid ring with the person icon', () => {
		const v = accountView(UNREGISTERED);

		expect(v).toMatchObject({ glyph: 'person', filled: false, dashed: false, ariaLabel: 'Account: not registered' });
		expect(tipLines(v.tooltipHtml)).toEqual(['Not registered', 'This device has no alias on the server yet. Click to set one.']);
	});

	it('shows an unknown registration as a dashed "?" that is not filled', () => {
		const v = accountView(UNKNOWN);

		expect(v).toMatchObject({ glyph: '?', filled: false, dashed: true, ariaLabel: 'Account: registration unknown' });
		expect(tipLines(v.tooltipHtml)).toEqual(['Registration unknown', 'Server unreachable or outdated. Click to check again.']);
	});

	it('shows checking as a dashed "…" with a one-line tooltip', () => {
		const v = accountView(CHECKING);

		expect(v).toMatchObject({ glyph: '…', filled: false, dashed: true, ariaLabel: 'Account: checking registration' });
		expect(tipLines(v.tooltipHtml)).toEqual(['Checking registration…']);
	});

	it('keeps a digit first character as it is', () => {
		expect(accountView({ ...REGISTERED, alias: '7 box' }).initial).toBe('7');
	});

	it('upper-cases the first character after leading whitespace', () => {
		expect(accountView({ ...REGISTERED, alias: '  ébène' }).initial).toBe('É');
	});

	it('takes a whole code point as the initial', () => {
		expect(accountView({ ...REGISTERED, alias: '😀 rig' }).initial).toBe('😀');
	});

	it('escapes markup in the alias for the tooltip but not the aria-label', () => {
		const v = accountView({ ...REGISTERED, alias: '<b>&"x' });

		expect(v.tooltipHtml).toContain('Registered as “&lt;b&gt;&amp;&quot;x”');
		expect(v.tooltipHtml).not.toContain('<b>&');
		expect(v.ariaLabel).toBe('Account: registered as <b>&"x');
	});
});

describe('TopBar account button', () => {
	function renderWith(r: Registration): string {
		Object.assign(registration, r);
		return render(TopBar, { props: { status: {} } }).body;
	}

	/** The account button's opening tag and content. */
	function accountButton(body: string): string {
		const m = body.match(/<button[^>]*class="account[^"]*"[^>]*>[\s\S]*?<\/button>/);
		expect(m, 'account button rendered').not.toBeNull();
		return m![0];
	}

	it('renders a registered device filled with its initial and label', () => {
		const b = accountButton(renderWith(REGISTERED));

		expect(b).toMatch(/class="account[^"]*\bregistered\b/);
		expect(b).not.toMatch(/class="account[^"]*\bdashed\b/);
		expect(b).toContain('aria-label="Account: registered as Gaming PC"');
		expect(b).toMatch(/>G</);
	});

	it('renders an unregistered device with the person icon and no fill', () => {
		const b = accountButton(renderWith(UNREGISTERED));

		expect(b).not.toMatch(/class="account[^"]*\bregistered\b/);
		expect(b).not.toMatch(/class="account[^"]*\bdashed\b/);
		expect(b).toContain('<svg');
		expect(b).toContain('aria-label="Account: not registered"');
	});

	it('renders unknown as a dashed "?"', () => {
		const b = accountButton(renderWith(UNKNOWN));

		expect(b).toMatch(/class="account[^"]*\bdashed\b/);
		expect(b).not.toMatch(/class="account[^"]*\bregistered\b/);
		expect(b).toMatch(/>\?</);
		expect(b).toContain('aria-label="Account: registration unknown"');
	});

	it('renders checking as a dashed "…"', () => {
		const b = accountButton(renderWith(CHECKING));

		expect(b).toMatch(/class="account[^"]*\bdashed\b/);
		expect(b).toMatch(/>…</);
		expect(b).toContain('aria-label="Account: checking registration"');
	});

	it('places the button after the rate chip and before the window controls', () => {
		const body = renderWith(REGISTERED);
		const rate = body.indexOf('class="rate-chip');
		const account = body.indexOf('class="account');
		const win = body.indexOf('class="window-controls');

		expect(rate).toBeGreaterThan(-1);
		expect(account).toBeGreaterThan(rate);
		expect(win).toBeGreaterThan(account);
	});
});
