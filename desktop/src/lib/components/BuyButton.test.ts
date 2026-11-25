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
const URL_CLEAVE = 'https://www.pathofexile.com/trade/search/Probe%20League?q=cleave';
/** The neutral action ink the Rankings BUY links show (`--color-lab-text`). */
const ACTION_INK = '#e4e4e7';

interface Shown {
	tag: string;
	text: string;
	href: string | null;
	target: string | null;
	ariaDisabled: string | null;
	ariaLabel: string | null;
	title: string | null;
	color: string | undefined;
	width: string;
}

function token(name: string): string {
	return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

/** Renders BuyButton with `props` into the document with its stylesheet and returns the one element it draws. */
function rendered(props: Record<string, unknown>) {
	const BuyButton = modules['./BuyButton.svelte'];
	expect(BuyButton, 'desktop/src/lib/components/BuyButton.svelte does not exist').toBeDefined();
	const css = sources['./BuyButton.svelte']?.match(/<style[^>]*>([\s\S]*?)<\/style>/)?.[1] ?? '';
	const style = document.createElement('style');
	style.textContent = tokensCss + css;
	document.head.append(style);
	document.body.innerHTML = render(BuyButton, { props }).body;

	const elements = document.body.querySelectorAll('*');
	expect(elements, 'BuyButton renders exactly one element').toHaveLength(1);
	return elements[0];
}

/** Renders BuyButton with `props` and reads back what the player gets: the element, its link and its look. */
function shown(props: Record<string, unknown>): Shown {
	const el = rendered(props);
	const computed = getComputedStyle(el);
	const width = computed.width;
	return {
		tag: el.tagName,
		text: el.textContent?.trim() ?? '',
		href: el.getAttribute('href'),
		target: el.getAttribute('target'),
		ariaDisabled: el.getAttribute('aria-disabled'),
		ariaLabel: el.getAttribute('aria-label'),
		title: el.getAttribute('title'),
		// A disabled control's colour is not pinned: the design mock greys it,
		// FontEVCompare's old cascade kept the gem colour at half opacity.
		color: el.tagName === 'A' ? computed.color : undefined,
		width: width.endsWith('px') ? width : 'intrinsic'
	};
}

/**
 * The outline a keyboard player sees on `el` once it takes keyboard focus: the
 * declarations of every `:focus-visible` rule in the document whose selector,
 * minus that pseudo-class, matches `el`, as lower-cased value words.
 */
function focusVisibleOutline(el: ReturnType<typeof rendered>): { words: Set<string>; offset: string } {
	const words = new Set<string>();
	let offset = '';
	for (const styleElement of document.querySelectorAll('style')) {
		for (const rule of styleElement.sheet?.cssRules ?? []) {
			const styleRule = rule as unknown as { selectorText?: string; style?: CSSStyleDeclaration };
			if (!styleRule.selectorText || !styleRule.style) continue;
			const applies = styleRule.selectorText
				.split(',')
				.some((part) => part.includes(':focus-visible') && el.matches(part.replaceAll(':focus-visible', '').trim()));
			if (!applies) continue;
			for (const property of ['outline', 'outline-width', 'outline-style', 'outline-color']) {
				for (const word of styleRule.style.getPropertyValue(property).toLowerCase().split(/\s+/)) if (word) words.add(word);
			}
			offset = styleRule.style.getPropertyValue('outline-offset').trim() || offset;
		}
	}
	return { words, offset };
}

afterEach(() => {
	document.head.innerHTML = '';
	document.body.innerHTML = '';
});

describe('BuyButton', () => {
	it.each([
		{
			name: 'an action-coloured auto-width link with a title',
			props: { label: 'BUY', href: URL_CLEAVE, width: 'auto', title: 'Buy Cleave (20/20)' },
			expected: {
				tag: 'A',
				text: 'BUY',
				href: URL_CLEAVE,
				target: '_blank',
				ariaDisabled: null,
				ariaLabel: null,
				title: 'Buy Cleave (20/20)',
				color: '--color-lab-buy',
				width: 'intrinsic'
			}
		},
		{
			name: 'a disabled auto-width control with a title',
			props: { label: 'BUY', href: null, width: 'auto', title: 'League not detected' },
			expected: {
				tag: 'SPAN',
				text: 'BUY',
				href: null,
				target: null,
				ariaDisabled: 'true',
				ariaLabel: null,
				title: 'League not detected',
				color: undefined,
				width: 'intrinsic'
			}
		},
		{
			name: 'a disabled fixed control',
			props: { label: 'BLUE', href: null, colour: 'blue', width: 'fixed' },
			expected: { tag: 'SPAN', text: 'BLUE', href: null, target: null, ariaDisabled: 'true', ariaLabel: null, title: null, color: undefined, width: '56px' }
		}
	])('keep:happy: $name renders the supplied label, destination, colour and width', ({ props, expected }) => {
		const actual = shown(props);
		expect(actual).toEqual({ ...expected, color: expected.color === undefined ? undefined : token(expected.color) });
	});

	// The Rankings acceptance example: a BUY that names no gem colour reads in
	// the neutral action ink, which no gem colour shares.
	it('keep:happy: a link with no colour reads in the --color-lab-buy action ink, the text colour and no gem colour', () => {
		const actual = shown({ label: 'BUY', href: URL_CLEAVE, width: 'fixed' });

		expect({
			color: actual.color,
			buyToken: token('--color-lab-buy'),
			textToken: token('--color-lab-text'),
			gemTokens: ['--color-lab-red', '--color-lab-green', '--color-lab-blue'].map(token).includes(actual.color ?? '')
		}).toEqual({ color: ACTION_INK, buyToken: ACTION_INK, textToken: ACTION_INK, gemTokens: false });
	});

	it.each([
		{ element: 'link', props: { label: 'BUY', href: URL_CLEAVE, width: 'fixed', name: 'Buy Cleave (20/20)' }, expected: { tag: 'A', ariaLabel: 'Buy Cleave (20/20)' } },
		{
			element: 'disabled control',
			props: { label: 'BUY', href: null, width: 'fixed', title: 'League not detected', name: 'League not detected' },
			expected: { tag: 'SPAN', ariaLabel: 'League not detected' }
		}
	])('keep:happy: a named $element carries the name as its accessible name', ({ props, expected }) => {
		const actual = shown(props);
		expect({ tag: actual.tag, ariaLabel: actual.ariaLabel }).toEqual(expected);
	});

	it.each([
		{
			name: 'a red fixed link',
			props: { label: 'RED', href: URL_RED, colour: 'red', width: 'fixed' },
			expected: { tag: 'A', text: 'RED', href: URL_RED, target: '_blank', ariaDisabled: null, ariaLabel: null, title: null, color: '--color-lab-red', width: '56px' }
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
				ariaLabel: null,
				title: 'Buy cheapest GREEN base gem (20/20)',
				color: '--color-lab-green',
				width: '56px'
			}
		},
		{
			name: 'a blue fixed link',
			props: { label: 'BLUE', href: URL_BLUE, colour: 'blue', width: 'fixed' },
			expected: { tag: 'A', text: 'BLUE', href: URL_BLUE, target: '_blank', ariaDisabled: null, ariaLabel: null, title: null, color: '--color-lab-blue', width: '56px' }
		}
	])('keep:regression: $name keeps its gem colour beside the neutral action ink', ({ props, expected }) => {
		const actual = shown(props);
		expect(actual).toEqual({ ...expected, color: token(expected.color) });
	});

	// keep:critical — silent: with no visible focus a keyboard player cannot see
	// which BUY Enter opens, and opens a plausible trade search for the wrong item.
	it.each([
		{ name: 'an action link', props: { label: 'BUY', href: URL_CLEAVE, width: 'fixed' } },
		{ name: 'a red gem link', props: { label: 'RED', href: URL_RED, colour: 'red', width: 'fixed' } },
		{ name: 'a blue gem auto-width link', props: { label: 'BLUE', href: URL_BLUE, colour: 'blue', width: 'auto' } }
	])('keep:critical: keyboard focus on $name draws a 2px solid currentColor outline offset 2px', ({ props }) => {
		const { words, offset } = focusVisibleOutline(rendered(props));
		expect({ width: words.has('2px'), style: words.has('solid'), color: words.has('currentcolor'), offset }).toEqual({
			width: true,
			style: true,
			color: true,
			offset: '2px'
		});
	});
});
