import { describe, it, expect, vi } from 'vitest';

// api.ts pulls in Tauri + the status store at module load; neither is needed for
// the pure variant mapping under test.
vi.mock('@tauri-apps/api/app', () => ({ getVersion: async () => '0.0.0-test' }));
vi.mock('$lib/stores/status.svelte', () => ({ store: { status: { server_url: '' } } }));

const { displayVariant } = await import('./api');

/**
 * The variant strings the UI filters on (ByVariant.svelte, FontEVCompare.svelte).
 * Kept literal here on purpose: if the backend format and this list drift apart
 * again, the "1/0" and "20/0" tabs silently render "No data for this variant".
 */
const UI_VARIANTS = ['1/0', '1/20', '20/0', '20/20'];

describe('displayVariant', () => {
	it('restores the /0 suffix on a level-1 zero-quality variant so the 1/0 tab matches', () => {
		// Backend stores/serves this as "1" (internal/lab/transfigure.go).
		expect(displayVariant('1')).toBe('1/0');
		expect(UI_VARIANTS).toContain(displayVariant('1'));
	});

	it('restores the /0 suffix on a level-20 zero-quality variant so the 20/0 tab matches', () => {
		expect(displayVariant('20')).toBe('20/0');
		expect(UI_VARIANTS).toContain(displayVariant('20'));
	});

	it('leaves variants that already carry a quality untouched', () => {
		expect(displayVariant('1/20')).toBe('1/20');
		expect(displayVariant('20/20')).toBe('20/20');
	});

	it('leaves the corrupted Dedication variant untouched', () => {
		// "21/23" must not become "21/23/0" — Dedication rows are filtered by pool,
		// but the variant is still displayed verbatim.
		expect(displayVariant('21/23')).toBe('21/23');
	});

	it('leaves a missing variant empty rather than inventing "/0"', () => {
		expect(displayVariant('')).toBe('');
	});
});
