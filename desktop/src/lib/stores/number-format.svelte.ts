/**
 * The reader's "Number format" pick (POE-323) and the one function every
 * desktop formatter groups thousands through. Pure rules live in
 * `$lib/number-format`.
 *
 * Persisted as the `numberFormat` `ui_prefs` key (ADR-013): only the main
 * window's pages format grouped numbers, and Rust never reads the value, so it
 * needs no typed settings field. An absent key (any settings file older than
 * POE-323) or an unknown value reads as `auto`.
 *
 * `initNumberFormatStore()` is called once from `routes/(app)/+layout.svelte`.
 * Before it runs — and in unit tests, which never call it — the store answers
 * from a local value that `setNumberFormat` writes.
 */
import { persisted, type PersistedString } from '$lib/prefs.svelte';
import {
	NUMBER_FORMAT_PREF,
	groupDigits,
	parseNumberFormat,
	separatorFor,
	type NumberFormat
} from '$lib/number-format';

let pref = $state.raw<PersistedString | null>(null);
let local = $state<NumberFormat>('auto');

/** The webview's locale is fixed for the session, so `auto` resolves once. */
let autoSeparator: string | null = null;

export function initNumberFormatStore(): void {
	if (!pref) pref = persisted(NUMBER_FORMAT_PREF, 'auto');
}

/** The pick in force. Reactive: read it inside `$derived` or markup. */
export function numberFormat(): NumberFormat {
	return parseNumberFormat(pref ? pref.value : local);
}

export function setNumberFormat(format: NumberFormat): void {
	if (pref) pref.value = format;
	else local = format;
}

/** The separator the pick writes, `auto` taken from the webview's locale. */
export function groupSeparator(): string {
	const format = numberFormat();
	if (format !== 'auto') return separatorFor(format, undefined);
	autoSeparator ??= separatorFor('auto', undefined);
	return autoSeparator;
}

/** Group an ungrouped number string (`11225`, `-1204.75`) the way the reader picked. */
export function groupNumber(plain: string): string {
	return groupDigits(plain, groupSeparator());
}
