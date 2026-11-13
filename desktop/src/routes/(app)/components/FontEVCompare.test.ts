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
