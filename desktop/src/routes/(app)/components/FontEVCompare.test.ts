/**
 * FontEVCompare, rendered before the Font response arrives: `svelte/server`'s
 * `render()` runs no `$effect`, so the component stays in its first state —
 * the Font fetch pending — and what is asserted is that markup.
 */
import { describe, expect, it, vi } from 'vitest';
import { render } from 'svelte/server';

vi.mock('@tauri-apps/api/app', () => ({ getVersion: async () => '0.0.0-test' }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: async () => null }));

const { default: FontEVCompare } = await import('./FontEVCompare.svelte');
const { ssot } = await import('$lib/stores/ssot.svelte');
const { exceptionalSupportTradeUrl } = await import('$lib/trade-utils');

function lensCard(body: string): string {
	const start = body.indexOf('aria-label="Facetor’s Lens · 60%"');
	expect(start, 'Lens card is not rendered while the Font fetch is pending').toBeGreaterThan(-1);
	return body.slice(start, body.indexOf('</article>', start));
}

describe('FontEVCompare while the Font response is pending', () => {
	it('shows the Lens prices and the EV as "pricing…", with no age', () => {
		const card = lensCard(render(FontEVCompare, { props: { labMode: 'normal' } }).body);
		expect(card.match(/pricing…/g)).toHaveLength(3);
		expect(card).not.toMatch(/\d+c</);
		expect(card).not.toMatch(/ago</);
	});

	it('keeps the existing Loading line before the Recipes section', () => {
		const body = render(FontEVCompare, { props: { labMode: 'normal' } }).body;
		expect(body.indexOf('Loading...')).toBeGreaterThan(-1);
		expect(body.indexOf('Loading...')).toBeLessThan(body.indexOf('Recipes'));
	});

	it('renders no Recipes section in Dedication mode', () => {
		expect(render(FontEVCompare, { props: { labMode: 'dedication' } }).body).not.toContain('Recipes');
	});
});

describe('FontEVCompare support-row BUY, with no server support search', () => {
	function supportBuy(league: string | null): string {
		ssot.league = league;
		try {
			const card = lensCard(render(FontEVCompare, { props: { labMode: 'normal' } }).body);
			const m = card.match(/<(a|span)[^>]*class="buy-btn buy-blue[^"]*"[^>]*>BUY<\/\1>/);
			expect(m, 'support BUY is not rendered').not.toBeNull();
			return m![0];
		} finally {
			ssot.league = null;
		}
	}

	it('links to the client-built Exceptional-support search once the league is detected', () => {
		const buy = supportBuy('New League');
		const href = buy.match(/href="([^"]*)"/)?.[1].replaceAll('&amp;', '&');
		expect(href).toBe(exceptionalSupportTradeUrl('New League'));
	});

	it('disables the BUY with the league-missing title when no league is detected', () => {
		const buy = supportBuy(null);
		expect(buy).toMatch(/^<span[^>]*aria-disabled="true"/);
		expect(buy).toContain('title="League not detected — trade link unavailable"');
	});
});
