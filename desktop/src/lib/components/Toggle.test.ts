/**
 * Toggle, rendered with `svelte/server`: the disabled state is in the markup
 * (native `disabled` makes it non-interactive, `aria-disabled` announces it).
 */
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import Toggle from './Toggle.svelte';

function button(props: Record<string, unknown>): string {
	return render(Toggle, { props }).body.match(/<button[^>]*>/)![0];
}

describe('Toggle', () => {
	it('is disabled and announced as such when disabled', () => {
		const html = button({ checked: true, label: 'X', disabled: true });
		expect(html).toMatch(/\sdisabled(=""|\s|>)/);
		expect(html).toContain('aria-disabled="true"');
	});

	it('stays interactive by default, as every existing caller uses it', () => {
		const html = button({ checked: false, label: 'X' });
		expect(html).not.toMatch(/\sdisabled(=""|\s|>)/);
		expect(html).toContain('aria-disabled="false"');
	});
});
