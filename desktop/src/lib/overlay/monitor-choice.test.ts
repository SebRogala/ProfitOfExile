/**
 * Which display an overlay window is built on (POE-237).
 *
 * Nothing else can check this: an overlay window has no test harness, and one
 * built on the wrong monitor is invisible from the machine running the suite —
 * it looks like a window that was never built at all.
 */
import { describe, expect, it } from 'vitest';
import { chooseMonitor, type GameMonitorInfo, type PositionedMonitor } from './monitor-choice';

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
