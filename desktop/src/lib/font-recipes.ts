/**
 * View model for the Font EV "Recipes" Facetor's Lens 60% card (POE-329).
 * Pure: the server owns the quotes and the EV; this only turns them into
 * cells, the EV line and the footer age.
 */
import { ageLabel, FONT_ALERTS_STALE_MS, type PriceCell } from './compass/font-alerts-view';
import type { FontLensCraft } from './api';

export type EvCell =
	| { kind: 'value'; text: string; negative: boolean }
	| { kind: 'loading' }
	| { kind: 'missing'; text: string };

export interface LensView {
	lens: PriceCell;
	support: PriceCell;
	ev: EvCell;
	age: { text: string; stale: boolean } | null;
	tradeUrl: string | null;
}

export const NEEDS_LENS_PRICE = '— needs the lens price';
export const NEEDS_SUPPORT_PRICE = '— needs the support price';

function cell(chaos: number | null, format: (chaos: number) => string): PriceCell {
	return chaos === null ? { kind: 'missing' } : { kind: 'price', text: format(chaos) };
}

/**
 * `craft` undefined = the Font response has not arrived (loading); null = the
 * server sent no `lens` key (older server), which reads as unpriced.
 */
export function lensView(
	craft: FontLensCraft | null | undefined,
	nowMs: number,
	format: (chaos: number) => string,
	formatSigned: (chaos: number) => string,
): LensView {
	if (craft === undefined) {
		return { lens: { kind: 'loading' }, support: { kind: 'loading' }, ev: { kind: 'loading' }, age: null, tradeUrl: null };
	}
	const lensChaos = craft?.outcome.chaos ?? null;
	const supportChaos = craft?.feeder.chaos ?? null;
	let ev: EvCell;
	if (craft?.ev != null) {
		ev = { kind: 'value', text: formatSigned(craft.ev), negative: craft.ev < 0 };
	} else {
		ev = { kind: 'missing', text: lensChaos === null ? NEEDS_LENS_PRICE : NEEDS_SUPPORT_PRICE };
	}
	// The card's age is its oldest input: the EV is only as fresh as that.
	const times = [craft?.outcome.fetchedAt, craft?.feeder.fetchedAt]
		.filter((t): t is string => t != null)
		.map((t) => Date.parse(t));
	let age: LensView['age'] = null;
	if (times.length > 0) {
		const ageMs = nowMs - Math.min(...times);
		age = { text: ageLabel(ageMs), stale: ageMs >= FONT_ALERTS_STALE_MS };
	}
	return {
		lens: cell(lensChaos, format),
		support: cell(supportChaos, format),
		ev,
		age,
		tradeUrl: craft?.feeder.tradeUrl ?? null,
	};
}
