/**
 * The decision a refused exit from widget-config mode produces.
 *
 * What is pinned here is the MAPPING — exit answer (+ re-assert answer) to
 * `{keepConfigMode, error}`. The ordering it serves lives in
 * `WidgetHost.svelte`, which has no unit-test harness in this app: that the
 * invoke happens BEFORE `configMode` is cleared, that the re-assert is sent
 * before the error is shown, and that `widget-config-end` is emitted only on
 * the Ok are covered by the guide's Windows smoke list, not here.
 *
 * The regression behind it: the host used to clear `configMode` — and with it
 * the Save/Cancel bar, the only controls a monitor-sized overlay window has —
 * before asking Rust to hand the window back to the game, and a refusal was
 * only logged.
 */
import { describe, expect, it } from 'vitest';
import { configExitDecision } from './widget-config-exit';

/** The re-assert the host sends after a refused exit, when it lands. */
const REASSERTED = { ok: true } as const;
/** …and when it does not. */
const NOT_REASSERTED = { ok: false, error: 'window vanished' } as const;

describe('leaving widget-config mode', () => {
	it('closes the session when the window confirmed the exit', () => {
		expect(configExitDecision({ ok: true }, null)).toEqual({ keepConfigMode: false, error: '' });
	});

	it('stays in config mode when the window refused the exit', () => {
		expect(configExitDecision({ ok: false, error: 'boom' }, REASSERTED).keepConfigMode).toBe(true);
	});

	/** Still true when the recovery failed too: dropping out of config mode on a
	 *  window that may be interactive is the worse of the two failures. */
	it('stays in config mode even when the re-assert failed as well', () => {
		expect(configExitDecision({ ok: false, error: 'boom' }, NOT_REASSERTED).keepConfigMode).toBe(
			true
		);
	});

	/** The bar is the only report an overlay window can give: no devtools, no
	 *  status line. A refusal that showed the ordinary "Drag to move" hint would
	 *  read as an exit that worked. */
	it('names the reason the window gave, in the bar', () => {
		const decision = configExitDecision(
			{ ok: false, error: "set_ignore_cursor_events(true) failed for 'temple': no window handle" },
			REASSERTED
		);

		expect(decision.error).toContain("set_ignore_cursor_events(true) failed for 'temple'");
	});

	/** Rust cleared its config-mode flag on the way out even though the call
	 *  failed, so the hook would make this window click-through again on the next
	 *  mouse move; the re-assert is what puts the flag back and keeps the bar
	 *  pressable. Having landed it, "press again" is honest advice. */
	it('offers the buttons as the retry when config mode was re-asserted', () => {
		expect(configExitDecision({ ok: false, error: 'boom' }, REASSERTED).error).toMatch(
			/Press Save or Cancel again/
		);
	});

	/** Without the re-assert the bar may stop responding within a mouse move, so
	 *  promising a retry on it would strand the user. Settings is the way back. */
	it('sends the user to Settings when the re-assert failed too', () => {
		expect(configExitDecision({ ok: false, error: 'boom' }, NOT_REASSERTED).error).toMatch(
			/Configure in Settings/
		);
	});

	/** A host that skipped the re-assert has left the flag cleared in Rust, which
	 *  is the unrecoverable case however the omission happened. */
	it('treats a refusal with no re-assert as unrecoverable', () => {
		expect(configExitDecision({ ok: false, error: 'boom' }, null).error).toMatch(
			/Configure in Settings/
		);
	});

	/** A rejection is not always a string: a Tauri invoke can reject with an
	 *  Error, and a bar reading "[object Object]" names nothing. */
	it('renders a thrown Error rather than an opaque object', () => {
		const decision = configExitDecision({ ok: false, error: new Error('no window') }, REASSERTED);

		expect(decision.error).toContain('no window');
	});
});
