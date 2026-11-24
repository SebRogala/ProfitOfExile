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
	lensUrl: string | null;
	costs: {
		name: string;
		sub: string;
		source: 'trade' | 'poe.ninja';
		cell: PriceCell;
		url: string | null;
	}[];
	source: string;
	evSubtitle: string;
	ev: EvCell;
	age: { text: string; stale: boolean } | null;
}

export const NEEDS_LENS_PRICE = '— needs the lens price';
export const NEEDS_SUPPORT_PRICE = '— needs the support price';
export const NEEDS_FEEDER_PRICE = '— needs a feeder price';

const FEEDER_NAMES = [
	'Exceptional Support Gem lv 3+',
	'Enhance Support lv 3',
	'Enlighten Support lv 3',
	'Empower Support lv 3',
];

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
	showAllLensFeeders = false,
): LensView {
	const loading = craft === undefined;
	const modern = loading || craft?.feeders !== undefined;
	const lensChaos = craft?.outcome.chaos ?? null;
	const cheapest = craft?.cheapest ?? null;
	const chosen = cheapest === null ? undefined : craft?.feeders?.[cheapest];
	const pricedChoice = chosen?.chaos != null;
	const indices = modern && showAllLensFeeders ? [0, 1, 2, 3] : [modern && pricedChoice ? cheapest! : 0];
	const costs: LensView['costs'] = indices.map((index) => {
		const feeder = modern ? craft?.feeders?.[index] : craft?.feeder;
		const source = index === 0 ? 'trade' : 'poe.ninja';
		let sub: string = source;
		if (modern && !showAllLensFeeders) sub = pricedChoice ? `${source} · cheapest of 4` : 'cheapest of 4';
		else if (modern && pricedChoice && index === cheapest) sub += ' · cheapest';
		return {
			name: FEEDER_NAMES[index], sub, source,
			cell: loading ? { kind: 'loading' } : cell(feeder?.chaos ?? null, format),
			url: feeder?.tradeUrl ?? null,
		};
	});
	const evValue = modern ? craft?.evCheapest : craft?.ev;
	let ev: EvCell;
	if (loading) {
		ev = { kind: 'loading' };
	} else if (lensChaos === null) {
		ev = { kind: 'missing', text: NEEDS_LENS_PRICE };
	} else if (evValue != null) {
		ev = { kind: 'value', text: formatSigned(evValue), negative: evValue < 0 };
	} else {
		ev = { kind: 'missing', text: modern ? NEEDS_FEEDER_PRICE : NEEDS_SUPPORT_PRICE };
	}
	// The card's age is its oldest input: the EV is only as fresh as that.
	const times = [craft?.outcome.fetchedAt, ...indices.map((index) =>
		modern ? craft?.feeders?.[index]?.fetchedAt : craft?.feeder.fetchedAt)]
		.filter((t): t is string => t != null)
		.map((t) => Date.parse(t));
	let age: LensView['age'] = null;
	if (times.length > 0) {
		const ageMs = nowMs - Math.min(...times);
		age = { text: ageLabel(ageMs), stale: ageMs >= FONT_ALERTS_STALE_MS };
	}
	return {
		lens: loading ? { kind: 'loading' } : cell(lensChaos, format),
		lensUrl: craft?.outcome.tradeUrl ?? null,
		costs,
		source: modern ? 'trade · poe.ninja' : 'trade · median of the 3 lowest',
		evSubtitle: modern ? 'lens − cheapest feeder' : 'lens − support',
		ev,
		age,
	};
}
