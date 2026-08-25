/**
 * The shipped overlay geometry has ONE home, and both consumers read it there.
 *
 * The bug this pins is invisible to every other gate. The owning layout
 * (`routes/(app)/+layout.svelte`) builds the real merc overlay and the Settings
 * position flow (`lib/pages/SettingsPage.svelte`) builds a config window from
 * the same numbers and PERSISTS whatever it is saved at. When the two carried
 * their own copies, raising one left the other behind and a Save from Settings
 * wrote the stale size back over the fresh default — permanently, since a
 * persisted value always wins. Found in review, 2026-08-25.
 *
 * Type-checking cannot catch it: two files each holding the literal `460`
 * compile perfectly. So the assertion reads the SOURCES, the same `?raw`
 * technique `overlay-tokens.test.ts` uses and for the same reason — this app
 * has no `@types/node` and the bundler already has these files.
 */
import { describe, expect, it } from 'vitest';
import layoutSource from '../../routes/(app)/+layout.svelte?raw';
import settingsSource from '../pages/SettingsPage.svelte?raw';
import { MERC_OVERLAY_DEFAULTS, physicalGeometry } from './overlay-defaults';

/** The two files allowed to place the merc strip, and nobody else. */
const consumers: [string, string][] = [
	['routes/(app)/+layout.svelte', layoutSource],
	['lib/pages/SettingsPage.svelte', settingsSource]
];

describe('one home for the shipped merc geometry', () => {
	it.each(consumers)('%s imports the constants rather than declaring its own', (_name, source) => {
		expect(source).toContain("from '$lib/overlay/overlay-defaults'");
		expect(source).toContain('MERC_OVERLAY_DEFAULTS');
	});

	// The specific regression: a second copy of the numbers. `MERC_OVERLAY_*`
	// consts declared locally are how the drift happened the first time.
	it.each(consumers)('%s declares no merc geometry constant of its own', (_name, source) => {
		expect(source).not.toMatch(/const\s+MERC_OVERLAY_DEFAULT_[XYWH]\s*=/);
	});

	// Importing the module is not the same as USING it for every axis. A
	// regression that replaced one field with a literal — `defaultH: 40` beside
	// a still-live `MERC_OVERLAY_DEFAULTS.w` — passes every assertion above,
	// which is exactly the single-axis drift this suite exists to stop. So each
	// consumer is checked against the fields it actually needs.
	it('SettingsPage takes both of the config row fields from the constants', () => {
		expect(settingsSource).toMatch(/defaultW:\s*MERC_OVERLAY_DEFAULTS\.w/);
		expect(settingsSource).toMatch(/defaultH:\s*MERC_OVERLAY_DEFAULTS\.h/);
	});

	it.each(['x', 'y', 'w', 'h'])(
		'the owning layout reads the shipped %s rather than a literal',
		(field) => {
			// The layout reads them through `physicalGeometry`, so the field
			// names appear on the converted object rather than on the constant.
			expect(layoutSource).toMatch(new RegExp(`shipped\\.${field}\\b`));
		}
	);

	it('the owning layout converts the constants rather than using them raw', () => {
		expect(layoutSource).toMatch(/physicalGeometry\(\s*MERC_OVERLAY_DEFAULTS/);
	});

	// A guard on the guard: if the constants were ever emptied, every assertion
	// above would still pass while the app placed the window at the origin.
	it('ships a placement with a real position and width', () => {
		expect(MERC_OVERLAY_DEFAULTS.w).toBeGreaterThan(0);
		expect(MERC_OVERLAY_DEFAULTS.x).toBeGreaterThan(0);
		expect(MERC_OVERLAY_DEFAULTS.y).toBeGreaterThan(0);
	});

	// Height is a constructor seed replaced on first paint, so it must be small
	// enough that a window which never gets a measurement is a thin strip rather
	// than a large empty box over the game.
	it('seeds a height no taller than a single line of the strip', () => {
		expect(MERC_OVERLAY_DEFAULTS.h).toBeGreaterThan(0);
		expect(MERC_OVERLAY_DEFAULTS.h).toBeLessThanOrEqual(60);
	});
});

describe('CSS pixels to physical pixels', () => {
	it('leaves an unscaled display alone', () => {
		expect(physicalGeometry({ x: 40, y: 300, w: 460, h: 40 }, 1)).toEqual({
			x: 40,
			y: 300,
			w: 460,
			h: 40
		});
	});

	// The 150 % Windows display this exists for: shipping 460 as a physical
	// width made the strip a third narrower than its content needed.
	it('scales every field on a 150 per cent display', () => {
		expect(physicalGeometry({ x: 40, y: 300, w: 460, h: 40 }, 1.5)).toEqual({
			x: 60,
			y: 450,
			w: 690,
			h: 60
		});
	});

	it('rounds rather than truncating a fractional result', () => {
		expect(physicalGeometry({ x: 0, y: 0, w: 461, h: 0 }, 1.25).w).toBe(576);
	});

	// `scaleFactor()` failing is handled by callers falling back to 1, but a
	// zero reaching here would collapse the window to nothing.
	it('treats a nonsensical scale factor as unscaled', () => {
		expect(physicalGeometry({ x: 40, y: 300, w: 460, h: 40 }, 0).w).toBe(460);
	});
});
