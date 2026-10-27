/** POE-316's page-local calculator, matching Main.dc.html's renderVals. */
export type ChaosDivMode = 'sell' | 'buy';
export type ChaosDivCurrency = 'chaos' | 'div';
export type ChaosDivOneSide = 'left' | 'right';
export type ChaosDivAmountKind = 'items' | 'budget';

export interface ChaosDivInput {
	mode: ChaosDivMode;
	currency: ChaosDivCurrency;
	amount: number;
	amountKind?: ChaosDivAmountKind;
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
	budgetLeft?: number;
}

export interface ChaosDivBudget {
	items: number;
	cost: number;
	left: number;
	belowOneMessage: string | null;
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
	noOutbidMessage: string | null;
	rows: ChaosDivRow[];
	budget: ChaosDivBudget | null;
}

/** The canvas bounds work to 100,000 item counts on each live recalculation. */
export function normalizeChaosDivAmount(amount: number): number {
	return Number.isFinite(amount) ? Math.max(1, Math.min(100_000, Math.floor(amount))) : 1;
}

export function formatChaosDivWhole(value: number): string {
	return Math.round(value).toLocaleString('en-US');
}

export function formatChaosDivItems(count: number): string {
	return `${count} ${count === 1 ? 'item' : 'items'}`;
}

function ratioNumber(value: number): string {
	return Math.abs(value - Math.round(value)) < 0.005 ? String(Math.round(value)) : value.toFixed(2);
}

/** Snapped hundredths give exact whole-orb costs without ceil(100 * 0.07) becoming 8. */
function fitBudget(amount: number, ratio: number, valueIsPrice: boolean, unit: string): ChaosDivBudget | null {
	const ticks = BigInt(Math.round(ratio * 100));
	const numerator = valueIsPrice ? ticks : 100n;
	const denominator = valueIsPrice ? 100n : ticks;
	const count = BigInt(amount) * denominator / numerator;
	if (count > BigInt(Number.MAX_SAFE_INTEGER)) return null;
	let items = Number(count);
	const costOf = (n: number) => Number((BigInt(n) * numerator + denominator - 1n) / denominator);
	while (items > 0 && costOf(items) > amount) items--;
	const cost = costOf(items);
	const price = valueIsPrice ? ratio : 1 / ratio;
	return { items, cost, left: amount - cost, belowOneMessage: items === 0 ? `Budget below one item (${price}${unit})` : null };
}

/** Last index matching a monotone predicate; zero means no positive index matched. */
function lastMatching(max: number, predicate: (n: number) => boolean): number {
	let low = 0;
	let high = max;
	while (low < high) {
		const mid = low + Math.ceil((high - low) / 2);
		if (predicate(mid)) low = mid;
		else high = mid - 1;
	}
	return low;
}

/**
 * A large Budget count is bounded by currency orders, not an item-input cap.
 * For each currency amount only its six nearest display buckets can reach the
 * global nearest six. Binary searches skip the intervening item counts.
 */
function* budgetCandidates(amount: number, minimum: number, budget: number, currencyOf: (n: number) => number, display: (price: number) => string): Generator<number> {
	if (amount <= 100_000) {
		for (let n = minimum; n <= amount; n++) yield n;
		return;
	}
	for (let currency = 1; currency <= budget; currency++) {
		let n = lastMatching(amount, (items) => currencyOf(items) <= currency);
		const first = Math.max(minimum, lastMatching(amount, (items) => currencyOf(items) < currency) + 1);
		if (n < first || currencyOf(n) !== currency) continue;
		for (let bucket = 0; bucket < 6 && n >= first; bucket++) {
			yield n;
			const label = display(currency / n);
			const beforeBucket = lastMatching(n, (items) => items < first || display(currency / items) !== label);
			n = beforeBucket;
		}
	}
}

export function calculateChaosDiv(input: ChaosDivInput): ChaosDivResult {
	const amount = normalizeChaosDivAmount(input.amount);
	const snappedRatio = Math.round(input.ratio * 100) / 100;
	const ratio = Number.isFinite(snappedRatio) && snappedRatio > 0 ? snappedRatio : null;
	const sell = input.mode === 'sell';
	const valueIsPrice = sell ? input.oneSide === 'right' : input.oneSide === 'left';
	const price = ratio !== null ? (valueIsPrice ? ratio : 1 / ratio) : null;
	const rate = input.rate !== null && Number.isFinite(input.rate) && input.rate > 0 ? input.rate : null;
	const budgetMode = !sell && input.amountKind === 'budget';
	const budget = budgetMode && ratio !== null ? fitBudget(amount, ratio, valueIsPrice, input.currency === 'div' ? 'd' : 'c') : null;
	const itemCount = budgetMode ? budget?.items ?? 0 : amount;
	const toChaos = input.currency === 'div' ? rate : 1;
	const rawPerItem = price !== null && toChaos !== null ? price * toChaos : null;
	const perItem = rawPerItem !== null && Number.isFinite(rawPerItem) ? rawPerItem : null;
	const rawTotal = budgetMode
		? budget !== null && toChaos !== null ? Math.round(budget.cost * toChaos) : null
		: perItem === null ? null : Math.round(perItem * amount);
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
		const currencyOf = (items: number) => sell ? Math.ceil(items * low - 1e-9) - 1 : Math.floor(items * high + 1e-9) + 1;
		const candidates = budgetMode
			? budgetCandidates(itemCount, minimumItems ?? 1, amount, currencyOf, display)
			: (function* () { for (let n = minimumItems ?? 1; n <= itemCount; n++) yield n; })();
		for (const items of candidates) {
			const currency = currencyOf(items);
			if (budgetMode && currency > amount) continue;
			if (!Number.isSafeInteger(currency) || currency < 1) continue;
			const candidate = currency / items;
			if (!Number.isFinite(Math.abs(1 - candidate / price) * 100)) continue;
			const ratio = display(candidate);
			if (ratio === typedDisplay) continue;
			// Keep the largest order in each display bucket regardless of traversal order.
			const previous = buckets.get(ratio);
			if (!previous || items > previous.items) buckets.set(ratio, { price: candidate, currency, items });
		}
		const nearest = [...buckets.entries()].sort(([, a], [, b]) => sell ? b.price - a.price : a.price - b.price).slice(0, 6);
		for (const [ratio, bucket] of nearest) {
			rows.push({
				ratio,
				delta: `${sell ? '−' : '+'}${(Math.abs(1 - bucket.price / price) * 100).toFixed(1)}%`,
				iWant: sell ? bucket.currency : bucket.items,
				iHave: sell ? bucket.items : bucket.currency,
				leftOver: itemCount - bucket.items,
				...(budgetMode ? { budgetLeft: amount - bucket.currency } : {})
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
		minimumOrderMessage: !budgetMode && rows.length === 0 && ratio !== 1 && minimumItems !== null
			? `At this ratio the smallest whole order is ${formatChaosDivItems(minimumItems)} for 1${input.currency === 'div' ? 'd' : 'c'}.` : null,
		noOutbidMessage: budgetMode && budget !== null && budget.items > 0 && ratio !== 1 && rows.length === 0
			? 'No outbid fits the budget.' : null,
		rows,
		budget
	};
}
