/**
 * The reader's "Number format" pick (POE-323) and the one function every
 * desktop formatter groups thousands through. Pure rules live in
 * `$lib/number-format`.
 *
 * Rust owns the typed pick in bossLoot.numberFormat. Both app and overlay
 * layouts already poll SSOT in their own WebView; no pref writer remains.
 * A pending write supplies immediate same-window feedback until its native
 * echo arrives. It is transient, never a second persisted preference.
 */
import { invoke } from '@tauri-apps/api/core';
import { ssot, fetchSsot } from './ssot.svelte';
import {
	groupDigits,
	parseNumberFormat,
	separatorFor,
	type NumberFormat
} from '$lib/number-format';

let pendingFormat = $state<NumberFormat | null>(null);
let writeSequence = 0;

/** The webview's locale is fixed for the session, so `auto` resolves once. */
let autoSeparator: string | null = null;

export function initNumberFormatStore(): void {
	// Both layouts own SSOT startup and cleanup. Kept for the existing app caller.
}

/** The pick in force. Reactive: read it inside `$derived` or markup. */
export function numberFormat(): NumberFormat {
	return pendingFormat ?? parseNumberFormat(ssot.bossLoot?.numberFormat);
}

export async function setNumberFormat(format: NumberFormat): Promise<void> {
	const sequence = ++writeSequence;
	pendingFormat = format;
	try {
		await invoke('set_number_format', { format });
		await fetchSsot();
	} catch (error) {
		console.warn('[number-format] set_number_format failed:', error);
	} finally {
		// An older completion cannot clear a newer click's immediate feedback.
		if (sequence === writeSequence) pendingFormat = null;
	}
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
