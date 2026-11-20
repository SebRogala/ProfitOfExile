// @vitest-environment happy-dom
/**
 * Select.svelte mounted in the client runtime under happy-dom. The file is
 * byte-identical to the web copy (mirror-identity), so the "web document" case
 * renders the web tree's component: the same source without desktop's token
 * stylesheet.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import Select from './Select.svelte';
import selectSource from './Select.svelte?raw';
import tokensCss from '../../tokens.css?raw';

const options = [
	{ value: 'a', label: 'Alpha' },
	{ value: 'b', label: 'Beta' },
	{ value: 'c', label: 'Gamma' }
];

const componentCss = (() => {
	const match = selectSource.match(/<style>([\s\S]*?)<\/style>/);
	if (!match) throw new Error('Select.svelte has no <style> block');
	return match[1];
})();

let mounted: Record<string, unknown> | null = null;

function addStyle(css: string) {
	const el = document.createElement('style');
	el.textContent = css;
	document.head.appendChild(el);
}

function render(props: Record<string, unknown>, withTokens: boolean): HTMLSelectElement {
	addStyle(componentCss);
	if (withTokens) addStyle(tokensCss);
	mounted = mount(Select, { target: document.body, props: props as never });
	flushSync();
	return document.body.querySelector('select') as HTMLSelectElement;
}

afterEach(() => {
	if (mounted) unmount(mounted);
	mounted = null;
	document.head.innerHTML = '';
	document.body.innerHTML = '';
});

describe('Select', () => {
	// keep:happy
	it('renders full width with its id in the desktop document', () => {
		const select = render({ value: 'a', options, id: 'variant' }, true);
		expect(select.getAttribute('id')).toBe('variant');
		expect(getComputedStyle(select).width).toBe('100%');
	});

	// keep:happy
	it('keeps its intrinsic width and carries no id in the web document', () => {
		const select = render({ value: 'a', options }, false);
		expect(select.hasAttribute('id')).toBe(false);
		expect(getComputedStyle(select).width).toBe('auto');
	});

	// keep:happy
	it('selects the bound value and writes a change back through the binding', () => {
		const onchange = vi.fn();
		const props = $state({ value: 'b', options, onchange });
		const select = render(props, false);
		expect(select.value).toBe('b');
		expect(select.selectedOptions[0].textContent).toBe('Beta');

		// Svelte's select binding reads `select.querySelector(':checked')`, which
		// happy-dom does not match against option selectedness; answer it from
		// selectedIndex, the browser's own notion of the pick.
		const query = select.querySelector.bind(select);
		vi.spyOn(select, 'querySelector').mockImplementation(((q: string) =>
			q === ':checked' ? (select.options[select.selectedIndex] ?? null) : query(q)) as typeof select.querySelector);
		select.selectedIndex = 2;
		select.dispatchEvent(new Event('change', { bubbles: true }));
		flushSync();

		expect(props.value).toBe('c');
		expect(onchange).toHaveBeenCalledTimes(1);
	});
});
