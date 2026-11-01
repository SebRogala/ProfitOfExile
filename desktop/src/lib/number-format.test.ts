import { describe, expect, it } from 'vitest';
import { groupDigits, localeGroupSeparator, parseNumberFormat, separatorFor } from './number-format';

const NBSP = ' ';

describe('separatorFor', () => {
	it('writes a no-break space for Space', () => {
		expect(groupDigits('11225', separatorFor('space', 'en-US'))).toBe(`11${NBSP}225`);
	});

	it('writes a comma for Comma, whatever the locale', () => {
		expect(groupDigits('11225', separatorFor('comma', 'pl-PL'))).toBe('11,225');
	});

	it('writes no separator for None', () => {
		expect(groupDigits('11225', separatorFor('none', 'en-US'))).toBe('11225');
	});

	it('follows a Polish locale under Auto', () => {
		expect(groupDigits('11225', separatorFor('auto', 'pl-PL'))).toBe(`11${NBSP}225`);
	});

	it('follows an English locale under Auto', () => {
		expect(groupDigits('11225', separatorFor('auto', 'en-US'))).toBe('11,225');
	});
});

describe('localeGroupSeparator', () => {
	it('swaps a locale that groups with "." for a space, since the decimal point stays "."', () => {
		expect(localeGroupSeparator('de-DE')).toBe(NBSP);
	});
});

describe('groupDigits', () => {
	it('groups every three digits of a seven-figure number', () => {
		expect(groupDigits('1234567', ',')).toBe('1,234,567');
	});

	it('leaves a three-digit number alone', () => {
		expect(groupDigits('999', ',')).toBe('999');
	});

	it('leaves the fraction ungrouped', () => {
		expect(groupDigits('1204.7512', ',')).toBe('1,204.7512');
	});

	it('keeps the minus sign outside the grouped digits', () => {
		expect(groupDigits('-5050', ',')).toBe('-5,050');
	});
});

describe('parseNumberFormat', () => {
	it.each(['space', 'comma', 'none', 'auto'] as const)('keeps the stored %s', (format) => {
		expect(parseNumberFormat(format)).toBe(format);
	});

	it.each([undefined, null, '', 'dot', 'constructor'])('reads %s as Auto', (raw) => {
		expect(parseNumberFormat(raw)).toBe('auto');
	});
});
