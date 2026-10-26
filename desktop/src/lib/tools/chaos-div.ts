/** POE-316's page-local calculator, matching Main.dc.html's renderVals. */
export type ChaosDivMode = 'sell' | 'buy';
export type ChaosDivCurrency = 'chaos' | 'div';
export type ChaosDivOneSide = 'left' | 'right';

export interface ChaosDivInput {
	mode: ChaosDivMode;
	currency: ChaosDivCurrency;
	amount: number;
	ratio: number;
	oneSide: ChaosDivOneSide;
	rate: number | null;
}

export const CHAOS_DIV_DEFAULTS: Omit<ChaosDivInput, 'rate'> = {
	mode: 'sell',
	currency: 'chaos',
	amount: 1,
	ratio: 1,
	oneSide: 'right'
};

/** Changing trade direction swaps the game's item side while preserving the price. */
export function switchChaosDivMode(state: Pick<ChaosDivInput, 'mode' | 'oneSide'>, mode: ChaosDivMode): Pick<ChaosDivInput, 'mode' | 'oneSide'> {
	return { mode, oneSide: mode === state.mode ? state.oneSide : state.oneSide === 'left' ? 'right' : 'left' };
}

export interface ChaosDivRow {
	ratio: string;
	delta: string;
	iWant: number;
	iHave: number;
	leftOver: number;
}

export interface ChaosDivResult {
	amount: number;
	ratio: number | null;
	perItemChaos: number | null;
	perItemC: string;
	perItemD: string;
	totalChaos: number | null;
	totalDivine: string | null;
	split: string | null;
	tableShown: boolean;
	minimumOrderMessage: string | null;
	rows: ChaosDivRow[];
}

/** The canvas bounds work to 100,000 item counts on each live recalculation. */
export function normalizeChaosDivAmount(amount: number): number {
	return Number.isFinite(amount) ? Math.max(1, Math.min(100_000, Math.floor(amount))) : 1;
}

export function formatChaosDivWhole(value: number): string {
	return Math.round(value).toLocaleString('en-US');
}

function ratioNumber(value: number): string {
	return Math.abs(value - Math.round(value)) < 0.005 ? String(Math.round(value)) : value.toFixed(2);
}

export function calculateChaosDiv(input: ChaosDivInput): ChaosDivResult {
	const amount = normalizeChaosDivAmount(input.amount);
	const snappedRatio = Math.round(input.ratio * 100) / 100;
	const ratio = Number.isFinite(snappedRatio) && snappedRatio > 0 ? snappedRatio : null;
	const sell = input.mode === 'sell';
	const valueIsPrice = sell ? input.oneSide === 'right' : input.oneSide === 'left';
	const price = ratio !== null ? (valueIsPrice ? ratio : 1 / ratio) : null;
	const rate = input.rate !== null && Number.isFinite(input.rate) && input.rate > 0 ? input.rate : null;
	const toChaos = input.currency === 'div' ? rate : 1;
	const rawPerItem = price !== null && toChaos !== null ? price * toChaos : null;
	const perItem = rawPerItem !== null && Number.isFinite(rawPerItem) ? rawPerItem : null;
	const rawTotal = perItem === null ? null : Math.round(perItem * amount);
	const totalChaos = rawTotal !== null && Number.isFinite(rawTotal) ? rawTotal : null;
	const rawDivine = totalChaos !== null && rate !== null ? totalChaos / rate : null;
	const totalDivine = rawDivine !== null && Number.isFinite(rawDivine) ? rawDivine : null;
	const perDivine = perItem !== null && rate !== null ? perItem / rate : null;
	const perItemC = perItem === null ? '—' : perItem >= 10 ? formatChaosDivWhole(perItem) : perItem.toFixed(perItem < 0.1 ? 4 : 2);
	const perItemD = perDivine === null || !Number.isFinite(perDivine)
		? '—' : perDivine.toFixed(perDivine < 0.1 ? 4 : 2);
	const whole = totalDivine === null ? null : Math.floor(totalDivine);
	const split = whole !== null && totalChaos !== null && rate !== null
		? `${whole}d + ${Math.round(totalChaos - whole * rate)}c` : null;

	const rows: ChaosDivRow[] = [];
	// Inverting 1 / 49 again rounds above 49; use the snapped item count directly.
	const minimumItems = ratio !== null && price !== null && Number.isFinite(price) && price > 0
		? Math.ceil(valueIsPrice ? 1 / ratio : ratio) : null;
	if (ratio !== null && price !== null && Number.isFinite(price) && ratio !== 1) {
		const low = valueIsPrice ? ratio - 0.005 : 1 / (ratio + 0.005);
		const high = valueIsPrice ? ratio + 0.005 : 1 / (ratio - 0.005);
		const display = (candidate: number) => {
			const value = ratioNumber(valueIsPrice ? candidate : 1 / candidate);
			return input.oneSide === 'left' ? `1 : ${value}` : `${value} : 1`;
		};
		const typedDisplay = display(price);
		const buckets = new Map<string, { price: number; currency: number; items: number }>();
		for (let items = minimumItems ?? 1; items <= amount; items++) {
			const currency = sell ? Math.ceil(items * low - 1e-9) - 1 : Math.floor(items * high + 1e-9) + 1;
			if (!Number.isSafeInteger(currency) || currency < 1) continue;
			const candidate = currency / items;
			if (!Number.isFinite(Math.abs(1 - candidate / price) * 100)) continue;
			const ratio = display(candidate);
			if (ratio === typedDisplay) continue;
			// Counts increase, so each replacement is the largest order in this bucket.
			buckets.set(ratio, { price: candidate, currency, items });
		}
		const nearest = [...buckets.entries()].sort(([, a], [, b]) => sell ? b.price - a.price : a.price - b.price).slice(0, 6);
		for (const [ratio, bucket] of nearest) {
			rows.push({
				ratio,
				delta: `${sell ? '−' : '+'}${(Math.abs(1 - bucket.price / price) * 100).toFixed(1)}%`,
				iWant: sell ? bucket.currency : bucket.items,
				iHave: sell ? bucket.items : bucket.currency,
				leftOver: amount - bucket.items
			});
		}
	}
	return {
		amount,
		ratio,
		perItemChaos: perItem === null ? null : perItem < 10 ? Number(perItem.toFixed(perItem < 0.1 ? 4 : 2)) : Math.round(perItem),
		perItemC,
		perItemD,
		totalChaos,
		totalDivine: totalDivine === null ? null : totalDivine.toFixed(2),
		split,
		tableShown: rows.length > 0,
		minimumOrderMessage: rows.length === 0 && ratio !== 1 && minimumItems !== null
			? `At this ratio the smallest whole order is ${minimumItems} items for 1${input.currency === 'div' ? 'd' : 'c'}.` : null,
		rows
	};
}
