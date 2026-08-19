/**
 * The one thing about the overlay manager that cannot be checked from inside
 * TypeScript: `TEMPLE_MODULE_ID` names a module in the RUST registry.
 *
 * `ssot.modules` is keyed by the ids `src-tauri/src/modules.rs` declares, and a
 * key that does not exist reads as `undefined` — which the temple lifecycle
 * effect treats (correctly) as "not polled yet" and never acts on. So a rename
 * on the Rust side would not throw, would not fail `svelte-check`, and would not
 * fail any behaviour test: the overlay would simply never appear again. Reading
 * the registry source is the only place the two sides meet.
 *
 * The source comes in through Vite's `?raw` rather than `node:fs` — this app
 * has no `@types/node`.
 */
import { describe, expect, it } from 'vitest';
import modulesSource from '../../../src-tauri/src/modules.rs?raw';
import { TEMPLE_MODULE_ID, TEMPLE_WINDOW_LABEL } from './manager';

/**
 * The `id:` literals the `MODULES` array registers, in declaration order.
 *
 * Scoped to that array rather than to the file: `modules.rs` also declares
 * `id:` literals in its own test fixtures ("nowork", "nowindow"), and a
 * whole-file match would let this test pass on a fixture while the real
 * registry entry had been renamed away. A `MODULES` block this regex cannot
 * find yields no ids at all, so the assertion fails rather than degrading.
 */
function rustModuleIds(): string[] {
	const registry = modulesSource.match(/pub const MODULES:[^=]*=\s*&\[([\s\S]*?)\n\];/)?.[1] ?? '';
	return [...registry.matchAll(/\bid:\s*"([^"]+)"/g)].map((m) => m[1]);
}

describe('the temple constants', () => {
	it('names a module the Rust registry actually declares', () => {
		expect(rustModuleIds()).toContain(TEMPLE_MODULE_ID);
	});

	it('keeps the window label and the module id spelled the same', () => {
		// They are separate constants because they are separate things — one is
		// a Tauri window label and a route segment, the other is Rust's. They
		// are equal today, and the capability entry plus the `/overlay/temple`
		// route directory are written for that spelling, so a divergence is a
		// deliberate act that has to update this test and those two places.
		expect(TEMPLE_WINDOW_LABEL).toBe(TEMPLE_MODULE_ID);
	});
});
