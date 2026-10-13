/**
 * The minimal bar, rendered with `svelte/server` (no DOM): how the next
 * direction is shown.
 */
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import MinimalBar from './MinimalBar.svelte';

function html(targetDirection: string | null): string {
	return render(MinimalBar, { props: { targetDirection } }).body;
}

/** Inner text of every `<tag>` whose class list holds `cls`. */
function texts(markup: string, tag: string, cls: string): string[] {
	const re = new RegExp(`<${tag}\\b[^>]*class="[^"]*\\b${cls}\\b[^"]*"[^>]*>([\\s\\S]*?)</${tag}>`, 'g');
	return [...markup.matchAll(re)].map((m) => m[1].replace(/<!--[\s\S]*?-->/g, '').trim());
}

describe('MinimalBar', () => {
	it('shows an arrow and the compass code for a door direction', () => {
		const markup = html('NE');
		expect(markup).toContain('class="arrow');
		expect(texts(markup, 'span', 'direction')).toEqual(['NE']);
	});

	it('shows Secret Passage without a direction arrow for a secret passage', () => {
		const markup = html('C');
		expect(markup).not.toContain('class="arrow');
		expect(texts(markup, 'span', 'direction')).toEqual(['Secret Passage']);
	});
});
