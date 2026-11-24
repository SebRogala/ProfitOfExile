/**
 * BuyButton, rendered with `svelte/server` into a happy-dom document that
 * carries the component's own stylesheet and the desktop tokens, so colour and
 * width are read as computed style rather than as class names.
 *
 * The component is loaded through a glob, not a static import: while the file
 * does not exist the glob is empty and the test fails on its expectation
 * instead of the whole file failing to resolve.
 */
import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import type { Component } from 'svelte';
import { Window } from 'happy-dom';
import tokensCss from '../../tokens.css?raw';

const modules = import.meta.glob<Component<Record<string, unknown>>>('./BuyButton.svelte', { import: 'default', eager: true });
const sources = import.meta.glob<string>('./BuyButton.svelte', { query: '?raw', import: 'default', eager: true });

// A happy-dom window of its own: under the happy-dom test environment the
// component compiles for the client and `svelte/server` cannot render it.
const window = new Window();
const document = window.document;
const getComputedStyle = window.getComputedStyle.bind(window);

const URL_RED = 'https://www.pathofexile.com/trade/search/Probe%20League?q=red';
const URL_GREEN = 'https://www.pathofexile.com/trade/search/Probe%20League?q=green';
const URL_BLUE = 'https://www.pathofexile.com/trade/search/Probe%20League?q=blue';

interface Shown {
	tag: string;
	text: string;
	href: string | null;
	target: string | null;
	ariaDisabled: string | null;
	title: string | null;
	color: string | undefined;
	width: string;
}

function token(name: string): string {
	return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

/** Renders BuyButton with `props` and reads back what the player gets: the element, its link and its look. */
function shown(props: Record<string, unknown>): Shown {
	const BuyButton = modules['./BuyButton.svelte'];
	expect(BuyButton, 'desktop/src/lib/components/BuyButton.svelte does not exist').toBeDefined();
	const css = sources['./BuyButton.svelte']?.match(/<style[^>]*>([\s\S]*?)<\/style>/)?.[1] ?? '';
	const style = document.createElement('style');
	style.textContent = tokensCss + css;
	document.head.append(style);
	document.body.innerHTML = render(BuyButton, { props }).body;

	const elements = document.body.querySelectorAll('*');
	expect(elements, 'BuyButton renders exactly one element').toHaveLength(1);
	const el = elements[0];
	const computed = getComputedStyle(el);
	const width = computed.width;
	return {
		tag: el.tagName,
		text: el.textContent?.trim() ?? '',
		href: el.getAttribute('href'),
		target: el.getAttribute('target'),
		ariaDisabled: el.getAttribute('aria-disabled'),
		title: el.getAttribute('title'),
		// A disabled control's colour is not pinned: the design mock greys it,
		// FontEVCompare's old cascade kept the gem colour at half opacity.
		color: el.tagName === 'A' ? computed.color : undefined,
		width: width.endsWith('px') ? width : 'intrinsic'
	};
}

afterEach(() => {
	document.head.innerHTML = '';
	document.body.innerHTML = '';
});

describe('BuyButton', () => {
	it.each([
		{
			name: 'a red fixed link',
			props: { label: 'RED', href: URL_RED, colour: 'red', width: 'fixed' },
			expected: { tag: 'A', text: 'RED', href: URL_RED, target: '_blank', ariaDisabled: null, title: null, color: '--color-lab-red', width: '56px' }
		},
		{
			name: 'a green fixed link with a title',
			props: { label: 'GREEN', href: URL_GREEN, colour: 'green', width: 'fixed', title: 'Buy cheapest GREEN base gem (20/20)' },
			expected: {
				tag: 'A',
				text: 'GREEN',
				href: URL_GREEN,
				target: '_blank',
				ariaDisabled: null,
				title: 'Buy cheapest GREEN base gem (20/20)',
				color: '--color-lab-green',
				width: '56px'
			}
		},
		{
			name: 'a blue auto-width link',
			props: { label: 'BUY BASE', href: URL_BLUE, colour: 'blue', width: 'auto' },
			expected: { tag: 'A', text: 'BUY BASE', href: URL_BLUE, target: '_blank', ariaDisabled: null, title: null, color: '--color-lab-blue', width: 'intrinsic' }
		},
		{
			name: 'a disabled auto-width control with a title',
			props: { label: 'BUY 20/0', href: null, colour: 'green', width: 'auto', title: 'League not detected' },
			expected: { tag: 'SPAN', text: 'BUY 20/0', href: null, target: null, ariaDisabled: 'true', title: 'League not detected', color: undefined, width: 'intrinsic' }
		},
		{
			name: 'a disabled fixed control',
			props: { label: 'BLUE', href: null, colour: 'blue', width: 'fixed' },
			expected: { tag: 'SPAN', text: 'BLUE', href: null, target: null, ariaDisabled: 'true', title: null, color: undefined, width: '56px' }
		}
	])('keep:happy: $name renders the supplied label, destination, colour and width', ({ props, expected }) => {
		const actual = shown(props);
		expect(actual).toEqual({ ...expected, color: expected.color === undefined ? undefined : token(expected.color) });
	});
});
