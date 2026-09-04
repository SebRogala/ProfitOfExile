/**
 * Which display an overlay window is built on (POE-237).
 *
 * Nothing else can check this: an overlay window has no test harness, and one
 * built on the wrong monitor is invisible from the machine running the suite —
 * it looks like a window that was never built at all.
 */
import { describe, expect, it } from 'vitest';
import {
	builtOnStaleMonitor,
	chooseMonitor,
	gameMonitorAfterBuild,
	type GameMonitorInfo,
	type PositionedMonitor
} from './monitor-choice';

/** Named so an assertion says which display was picked, not `[object Object]`. */
interface NamedMonitor extends PositionedMonitor {
	name: string;
}

const PRIMARY: NamedMonitor = { name: 'primary', position: { x: 0, y: 0 } };
/** To the LEFT of the primary, so its origin is negative — the arrangement a
 *  sign error in the match silently gets right on a right-hand monitor. */
const SECOND: NamedMonitor = { name: 'second', position: { x: -1920, y: 0 } };

const AVAILABLE = [PRIMARY, SECOND];

/** Rust's answer for the second display. */
const ON_SECOND: GameMonitorInfo = {
	id: 131074,
	x: -1920,
	y: 0,
	width: 1920,
	height: 1080
};

describe('chooseMonitor', () => {
	it('builds on the display the game is on', () => {
		expect(chooseMonitor(ON_SECOND, AVAILABLE, PRIMARY)?.name).toBe('second');
	});

	it('builds on the primary when nothing has seen the game window yet', () => {
		expect(chooseMonitor(null, AVAILABLE, PRIMARY)?.name).toBe('primary');
	});

	it('builds on the primary when the game monitor is not one the webview lists', () => {
		const unplugged: GameMonitorInfo = { ...ON_SECOND, x: 3840, y: 0 };

		expect(chooseMonitor(unplugged, AVAILABLE, PRIMARY)?.name).toBe('primary');
	});

	it('matches on the whole corner, not the x alone', () => {
		const stacked: NamedMonitor = { name: 'stacked', position: { x: 0, y: -1080 } };
		const above: GameMonitorInfo = { ...ON_SECOND, x: 0, y: -1080 };

		expect(chooseMonitor(above, [PRIMARY, stacked], PRIMARY)?.name).toBe('stacked');
	});

	it('carries the caller through when it could not resolve a primary either', () => {
		const unplugged: GameMonitorInfo = { ...ON_SECOND, x: 3840, y: 0 };

		expect(chooseMonitor(unplugged, AVAILABLE, null)).toBeNull();
	});
});

describe('builtOnStaleMonitor', () => {
	/**
	 * The POE-245 regression. The create asks `get_game_monitor` before the
	 * constructor and the answer can move while the window is still being built
	 * — `set_overlay_clickthrough` alone spends ~1 s waiting for the WebView2
	 * HWND. Rust's one `game-monitor-changed` for that move is dropped by the
	 * layout (no settled window to rebuild) and never resent, so without this
	 * check the overlay stays on the display the game left until the module is
	 * toggled.
	 */
	it('reports a window built on the display the game has since left', () => {
		expect(builtOnStaleMonitor(PRIMARY.position, ON_SECOND, false)).toBe(true);
	});

	it('leaves a window built on the display the game is still on alone', () => {
		expect(builtOnStaleMonitor(SECOND.position, ON_SECOND, false)).toBe(false);
	});

	/** Corners, not ids — the two enumerations do not share an id space, which is
	 *  the same reason `chooseMonitor` matches on the corner. */
	it('compares the whole corner, not the x alone', () => {
		const stackedAbove: GameMonitorInfo = { ...ON_SECOND, x: 0, y: -1080 };

		expect(builtOnStaleMonitor(PRIMARY.position, stackedAbove, false)).toBe(true);
	});

	/**
	 * Nothing has seen PoE in the foreground, so the window is on the primary ON
	 * PURPOSE (the pre-POE-237 fallback). Calling that stale would fail every
	 * creation made before the game was ever focused, and the module would spend
	 * its whole retry budget and give up.
	 */
	it('does not call a window stale when there is no game monitor to compare it to', () => {
		expect(builtOnStaleMonitor(PRIMARY.position, null, false)).toBe(false);
		expect(builtOnStaleMonitor(PRIMARY.position, undefined, false)).toBe(false);
	});

	it('does not call a window stale when no window was built', () => {
		expect(builtOnStaleMonitor(null, ON_SECOND, false)).toBe(false);
	});

	/**
	 * POE-237's soft failure, not a stale window: Rust named a display this
	 * webview does not list, so the build went to the primary knowing it was the
	 * wrong screen and said so in the log. Retrying lands on the primary again,
	 * three times, and then gives the module up — no overlay at all, which is
	 * strictly worse than one on the wrong display.
	 */
	it('leaves a build that could not reach the game alone', () => {
		expect(builtOnStaleMonitor(PRIMARY.position, ON_SECOND, true)).toBe(false);
	});
});

describe('gameMonitorAfterBuild', () => {
	/** Rust writes `AppState.game_monitor` before it emits, so a query made after
	 *  any notice sees at least what that notice carried — and, unlike the
	 *  notice, it also sees a game that moved away and back during the build. */
	it('prefers the fresh query over a notice recorded during the build', () => {
		const stale: GameMonitorInfo = { ...ON_SECOND, id: 1, x: 3840, y: 0 };

		expect(gameMonitorAfterBuild(stale, ON_SECOND)).toBe(ON_SECOND);
	});

	/**
	 * The hole the recorded notice exists to fill. `get_game_monitor` failing
	 * hands the caller `null`, and without the fallback that reads as "nothing
	 * to correct" — leaving the window on the display the notice had already
	 * said the game left.
	 */
	it('falls back to the recorded notice when the query could not answer', () => {
		expect(gameMonitorAfterBuild(ON_SECOND, null)).toBe(ON_SECOND);
	});

	it('answers nothing when neither source has a display', () => {
		expect(gameMonitorAfterBuild(null, undefined)).toBeNull();
	});
});
