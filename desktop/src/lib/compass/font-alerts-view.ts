/**
 * View model for the Font alert stack: which alert panels a font read calls
 * for, and the exceptional-support price rows. Pure — no Tauri, no window
 * code — so the overlay widget and the main-window card share it.
 */

/** The exceptional-supports section of the server's Font analysis. */
export interface ExceptionalSupportsPayload {
	source: string;
	variant: string;
	asOf: string | null;
	divineRate: number;
	gems: { name: string; chaos: number | null }[];
}

export type PriceCell = { kind: 'price'; text: string } | { kind: 'loading' } | { kind: 'missing' };

export interface ExceptionalView {
	rows: { name: string; cell: PriceCell }[];
	footer: string;
	age: { text: string; stale: boolean } | null;
}

export type AlertPanel = 'exceptional' | 'transfigure';

/** Interim handoff value; the temple's market staleness (market.rs STALE_AFTER_MS) is the same 2 h. */
export const FONT_ALERTS_STALE_MS = 2 * 60 * 60 * 1000;

/**
 * Shown ONLY before the first payload, so the loading state has names and a
 * source line. The server's list is the owner; a test pins these to its golden.
 */
export const LOADING_SUPPORT_NAMES = ['Enhance Support', 'Enlighten Support', 'Empower Support'] as const;
export const LOADING_SOURCE = 'poe.ninja';

const MINUTE_MS = 60_000;
const HOUR_MS = 60 * MINUTE_MS;

export function ageLabel(ageMs: number): string {
	if (ageMs < MINUTE_MS) return '<1m';
	if (ageMs < HOUR_MS) return `${Math.floor(ageMs / MINUTE_MS)}m ago`;
	return `${Math.floor(ageMs / HOUR_MS)}h ago`;
}

function footerFor(source: string): string {
	return `${source} · level 1, uncorrupted`;
}

export function exceptionalView(
	payload: ExceptionalSupportsPayload | null,
	nowMs: number,
	format: (chaos: number) => string,
): ExceptionalView {
	if (payload === null) {
		return {
			rows: LOADING_SUPPORT_NAMES.map((name) => ({ name, cell: { kind: 'loading' } })),
			footer: footerFor(LOADING_SOURCE),
			age: null,
		};
	}
	const rows = payload.gems.map(({ name, chaos }) => ({
		name,
		cell: chaos === null ? ({ kind: 'missing' } as const) : ({ kind: 'price', text: format(chaos) } as const),
	}));
	let age: ExceptionalView['age'] = null;
	if (payload.asOf !== null) {
		const ageMs = nowMs - Date.parse(payload.asOf);
		age = { text: ageLabel(ageMs), stale: ageMs >= FONT_ALERTS_STALE_MS };
	}
	return { rows, footer: footerFor(payload.source), age };
}

const PANEL_BY_CRAFT: Record<string, AlertPanel> = {
	exchange_exceptional: 'exceptional',
	transform_direct: 'transfigure',
};

/** Keeps the input order: the Rust side owns it. */
export function alertPanels(crafts: readonly string[]): AlertPanel[] {
	return crafts.flatMap((craft) => {
		const panel = PANEL_BY_CRAFT[craft];
		return panel === undefined ? [] : [panel];
	});
}
