/**
 * Which failed click-through setups are worth a warning.
 *
 * What this pins is the DECISION — level and wording from a rejection reason.
 * Whether `routes/(app)/+layout.svelte` routes an `error` to `console.error`
 * and an `info` to `console.info` is in a `.svelte` file with no harness, and
 * whether the setup fails at all is Win32; both are smoke.
 */
import { describe, expect, it } from 'vitest';
import { CLICKTHROUGH_WINDOW_GONE, clickthroughReport } from './clickthrough-report';

describe('reporting a failed click-through setup', () => {
	/** The line the owner has to act on: a live window, opaque to the mouse. */
	it('warns when the window is still there', () => {
		const report = clickthroughReport(
			'temple',
			"Overlay 'temple' is not click-through after setup: WS_EX_TRANSPARENT did not read back"
		);

		expect(report.level).toBe('error');
	});

	it('names the window and the reason in the line it logs', () => {
		const report = clickthroughReport('comparator', 'WS_EX_TRANSPARENT did not read back');

		expect(report.message).toContain('[comparator-overlay]');
		expect(report.message).toContain('WS_EX_TRANSPARENT did not read back');
	});

	/**
	 * The literal, not the constant — half of a cross-language pair.
	 *
	 * `CLICKTHROUGH_WINDOW_GONE` in `desktop/src-tauri/src/lib.rs` is what
	 * actually prefixes the message at runtime. A test written against this
	 * module's own constant would survive a rename here while Rust kept the old
	 * string, and every genuine warning would silently become an info line. The
	 * Rust half asserts the same literal.
	 */
	it('matches the marker Rust actually sends', () => {
		expect(CLICKTHROUGH_WINDOW_GONE).toBe('window-gone');
		expect(clickthroughReport('temple', "window-gone: Overlay 'temple' not found").level).toBe(
			'info'
		);
	});

	/** An overlay toggled off inside the ~1 s HWND wait. Nothing is left to eat
	 *  a click, so a warning here would cry wolf on every fast toggle. */
	it('does not warn when the window was gone before the setup ran', () => {
		const report = clickthroughReport(
			'temple',
			`${CLICKTHROUGH_WINDOW_GONE}: Overlay 'temple' not found after the setup delay`
		);

		expect(report.level).toBe('info');
	});

	/** The marker is a PREFIX, not a substring: a genuine failure whose Win32
	 *  detail happened to mention the phrase must still warn. */
	it('warns when the marker appears anywhere but the start', () => {
		const report = clickthroughReport(
			'temple',
			`Overlay 'temple' HWND not available after the setup delay (${CLICKTHROUGH_WINDOW_GONE})`
		);

		expect(report.level).toBe('error');
	});

	/** A Tauri invoke can reject with an Error; a line reading "[object Object]"
	 *  names nothing the owner can act on. */
	it('renders a thrown Error rather than an opaque object', () => {
		const report = clickthroughReport('timer', new Error('event loop closed'));

		expect(report.message).toContain('event loop closed');
	});
});
