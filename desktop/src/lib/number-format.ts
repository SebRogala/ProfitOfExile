/**
 * The thousands separator the app groups numbers with (POE-323) — the pure
 * half. `stores/number-format.svelte.ts` holds the reader's pick and applies
 * it; every formatter that groups digits goes through `groupDigits`.
 *
 * Only the GROUP separator is configurable. The decimal separator stays "."
 * everywhere, so no mode may group with "." — `1.234` beside `1.5` would read
 * as one point two three four.
 */

export type NumberFormat = 'auto' | 'space' | 'comma' | 'none';

export const NUMBER_FORMAT_OPTIONS: { value: NumberFormat; label: string }[] = [
	{ value: 'auto', label: 'Auto' },
	{ value: 'space', label: 'Space' },
	{ value: 'comma', label: 'Comma' },
	{ value: 'none', label: 'None' }
];

/** The `ui_prefs` key (ADR-013): a view preference Rust never reads. */
export const NUMBER_FORMAT_PREF = 'numberFormat';

/** No-break space: it renders as a space and never wraps `11 225` across lines. */
const SPACE = ' ';

/** The read-side validator: an absent, older or garbage value is `auto`. */
export function parseNumberFormat(raw: string | null | undefined): NumberFormat {
	return raw === 'space' || raw === 'comma' || raw === 'none' ? raw : 'auto';
}

/**
 * The group separator a locale writes, read off `Intl` (`undefined` = the
 * webview's own locale: pl-PL answers a no-break space, en-US a comma). A
 * locale that groups with "." (de-DE) gets a space instead, because the
 * decimal point stays "."; one with no group separator gets none.
 */
export function localeGroupSeparator(locale: string | undefined): string {
	const part = new Intl.NumberFormat(locale).formatToParts(1234567).find((p) => p.type === 'group');
	if (!part) return '';
	return part.value === '.' ? SPACE : part.value;
}

/** The separator one mode writes; `auto` asks `localeGroupSeparator`. */
export function separatorFor(format: NumberFormat, locale: string | undefined): string {
	switch (format) {
		case 'space':
			return SPACE;
		case 'comma':
			return ',';
		case 'none':
			return '';
		case 'auto':
			return localeGroupSeparator(locale);
	}
}

/**
 * Group the integer part of an UNGROUPED number string (`-1234567.5`) in
 * threes with `separator`, leaving the sign and everything from the "." on
 * untouched. Rounding is the caller's: this only inserts separators.
 */
export function groupDigits(plain: string, separator: string): string {
	const dot = plain.indexOf('.');
	const whole = dot < 0 ? plain : plain.slice(0, dot);
	const rest = dot < 0 ? '' : plain.slice(dot);
	return whole.replace(/\B(?=(\d{3})+(?!\d))/g, () => separator) + rest;
}
