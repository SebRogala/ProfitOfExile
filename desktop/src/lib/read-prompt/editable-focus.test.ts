// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from 'vitest';
import { reportEditableFocus } from './editable-focus';

let stop: (() => void) | undefined;
afterEach(() => { stop?.(); stop = undefined; document.body.innerHTML = ''; vi.restoreAllMocks(); });

it('keep:critical editable focus must not silently turn select-all into an OCR read', async () => {
	vi.spyOn(document, 'hasFocus').mockReturnValue(true);
	const input = document.createElement('input');
	document.body.append(input);
	input.focus();
	const reports: boolean[] = [];
	stop = reportEditableFocus((editable) => reports.push(editable));
	expect(reports.at(-1), 'initial activeElement').toBe(true);
	const cases = [
		['<input>', 'input', true],
		['<textarea></textarea>', 'textarea', true],
		['<div contenteditable="true" tabindex="0"></div>', 'div', true],
		['<div contenteditable="true"><span tabindex="0">child</span></div>', 'span', true],
		['<select><option>one</option></select>', 'select', true],
		['<div role="combobox" tabindex="0"></div>', 'div', true],
		['<button>read</button>', 'button', false]
	] as const;
	for (const [html, selector, expected] of cases) {
		document.body.innerHTML = html;
		const control = document.querySelector<HTMLElement>(selector)!;
		control.focus();
		await Promise.resolve();
		expect(reports.at(-1), html).toBe(expected);
		const selectAll = new KeyboardEvent('keydown', { key: 'a', ctrlKey: true, bubbles: true, cancelable: true });
		control.dispatchEvent(selectAll);
		expect(selectAll.defaultPrevented, 'native select-all remains available').toBe(false);
	}
	document.body.innerHTML = '<textarea></textarea>';
	document.querySelector('textarea')!.focus();
	await Promise.resolve();
	expect(reports.at(-1)).toBe(true);
	window.dispatchEvent(new Event('blur'));
	expect(reports.at(-1), 'inactive webview releases editable focus').toBe(false);
	window.dispatchEvent(new Event('focus'));
	expect(reports.at(-1), 'return to the focused editor').toBe(true);
	document.querySelector('textarea')!.blur();
	await Promise.resolve();
	expect(reports.at(-1), 'focusout to body').toBe(false);
	stop(); stop = undefined;
	expect(reports.at(-1), 'teardown clears native focus').toBe(false);
	const count = reports.length;
	document.querySelector('textarea')!.focus();
	window.dispatchEvent(new Event('focus'));
	await Promise.resolve();
	expect(reports).toHaveLength(count);
});
