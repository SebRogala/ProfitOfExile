/**
 * The stash-search regex box, rendered with `svelte/server` (no DOM): the
 * label, the read-only regex, the count line and the Copy label. The count
 * text and the over-limit flag are `view.ts`'s; the Copy behaviour (clipboard,
 * "Copied" until the next change) is the controller's and tested there (WI-8).
 */
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import HarvestRegexBox from './HarvestRegexBox.svelte';
import type { RegexBox } from './view';

const KEEPERS: RegexBox = {
	text: '"shu|san|gil|fac|fra|hol|gly"',
	length: 29,
	count: '29 / 250 characters',
	overLimit: false
};

const OVER: RegexBox = {
	text: '"bla|arm|jew"',
	length: 263,
	count: '263 / 250 characters: too long for one stash search',
	overLimit: true
};

/** Inner text of every `<tag>` whose class list holds `cls` (Svelte comments and scope classes tolerated). */
function texts(markup: string, tag: string, cls: string): string[] {
	const re = new RegExp(`<${tag}\\b[^>]*class="[^"]*\\b${cls}\\b[^"]*"[^>]*>([\\s\\S]*?)</${tag}>`, 'g');
	return [...markup.matchAll(re)].map((m) => m[1].replace(/<!--[\s\S]*?-->/g, '').trim());
}

function html(box: RegexBox, copied = false): string {
	return render(HarvestRegexBox, {
		props: { label: 'Keepers regex', box, copied, oncopy: () => {} }
	}).body;
}

describe('HarvestRegexBox', () => {
	it('draws the label in its caption element', () => {
		expect(texts(html(KEEPERS), 'span', 'caption')).toEqual(['Keepers regex']);
	});

	it('shows the regex in a read-only input', () => {
		expect(html(KEEPERS)).toMatch(/<input\b[^>]*\breadonly\b[^>]*value="&quot;shu\|san\|gil\|fac\|fra\|hol\|gly&quot;"|<input\b[^>]*value="&quot;shu\|san\|gil\|fac\|fra\|hol\|gly&quot;"[^>]*\breadonly\b/);
	});

	it('draws the character count', () => {
		expect(texts(html(KEEPERS), 'span', 'count')).toEqual(['29 / 250 characters']);
	});

	it('marks the count amber over the limit', () => {
		expect(html(OVER)).toMatch(/class="[^"]*\bcount\b[^"]*\bover\b/);
	});

	it('leaves the count unmarked within the limit', () => {
		expect(html(KEEPERS)).not.toMatch(/class="[^"]*\bover\b/);
	});

	it('labels the button Copy before a copy', () => {
		const out = html(KEEPERS, false);
		expect(out).toMatch(/>\s*Copy\s*</);
		expect(out).not.toContain('Copied');
	});

	it('labels the button Copied after a copy', () => {
		expect(html(KEEPERS, true)).toMatch(/>\s*Copied\s*</);
	});
});
