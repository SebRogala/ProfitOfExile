import { describe, it, expect, vi } from 'vitest';

// api.ts pulls in Tauri + the status store at module load; neither is needed for
// the pure variant mapping under test.
vi.mock('@tauri-apps/api/app', () => ({ getVersion: async () => '0.0.0-test' }));
vi.mock('$lib/stores/status.svelte', () => ({ store: { status: { server_url: '' } } }));

const { displayVariant, signalTransitionLabel } = await import('./api');

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

/**
 * The endpoint serves a gem's ring within the server's 14-day retention window,
 * so a gem that stopped trading answers with old transitions. The overlay
 * renders this label verbatim next to a live price, which is what makes a bare
 * time-of-day on a week-old transition a lie rather than a rounding.
 */
describe('signalTransitionLabel', () => {
	const now = new Date(2026, 7, 5, 18, 30);

	it('shows the time of day for a transition from earlier today', () => {
		const label = signalTransitionLabel(new Date(2026, 7, 5, 14, 23).toISOString(), now);
		// Clock reading, whatever the runner's locale does with 12h/24h.
		expect(label).toMatch(/^\d{1,2}[:.]\d{2}/);
		expect(label).not.toContain('ago');
	});

	it('shows the age instead of a time of day for yesterday', () => {
		// 23:50 yesterday is under 19 hours old, and still not today: the label
		// counts calendar days because that is how "1d ago" is read.
		expect(signalTransitionLabel(new Date(2026, 7, 4, 23, 50).toISOString(), now)).toBe('1d ago');
	});

	it('shows the age for a gem that stopped signalling a week ago', () => {
		expect(signalTransitionLabel(new Date(2026, 6, 29, 14, 23).toISOString(), now)).toBe('7d ago');
	});

	it('shows the age at the far edge of the server retention window', () => {
		// signalHistorySeedMaxDays = 14 — the oldest transition that can be served.
		expect(signalTransitionLabel(new Date(2026, 6, 22, 9, 0).toISOString(), now)).toBe('14d ago');
	});

	it('shows the time of day for a timestamp slightly ahead of the clock', () => {
		// Client/server clock skew must not render as a negative age.
		expect(signalTransitionLabel(new Date(2026, 7, 5, 18, 35).toISOString(), now)).not.toContain(
			'ago'
		);
	});

	it('renders nothing for an unparseable timestamp rather than "Invalid Date"', () => {
		expect(signalTransitionLabel('', now)).toBe('');
		expect(signalTransitionLabel('not-a-date', now)).toBe('');
	});
});
