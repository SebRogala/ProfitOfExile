// @vitest-environment node
/**
 * The /privacy page, rendered: everything it shows comes from the shared
 * notice.json. `svelte/server`'s `render()`, so what is asserted is the markup.
 * The node environment makes the component compile for the server; under the
 * default jsdom one it compiles for the client and `render()` throws.
 */
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import Page from './+page.svelte';
import { notice } from '$lib/privacy/notice-data';

const page = render(Page);

describe('/privacy page', () => {
	it('tells search engines not to index it', () => {
		expect(page.head).toContain('<meta name="robots" content="noindex"');
	});

	it.each(notice.sections.map((s) => [s.id, s.heading]))('renders the heading of section %s', (_id, heading) => {
		expect(page.body).toContain(`>${heading}</h2>`);
	});

	it('renders the sections in the notice order', () => {
		const positions = notice.sections.map((s) => page.body.indexOf(`id="${s.id}"`));
		expect(positions.every((p) => p >= 0)).toBe(true);
		expect(positions).toEqual([...positions].sort((a, b) => a - b));
	});

	it('shows the notice revision', () => {
		expect(page.body).toMatch(new RegExp(`data-testid="notice-revision"[^>]*>${notice.revision}<`));
	});
});
