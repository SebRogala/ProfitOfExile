import { expect, it } from 'vitest';
import { escapeHtml } from './html';

it('keep:happy: escapes HTML text to exact entities', () => {
	const cases = [
		['&', '&amp;'],
		['<', '&lt;'],
		['>', '&gt;'],
		['"', '&quot;'],
		["'", '&#39;'],
		['A&B <"C\'>', 'A&amp;B &lt;&quot;C&#39;&gt;']
	];

	expect(cases.map(([input]) => escapeHtml(input))).toEqual(cases.map(([, expected]) => expected));
});
