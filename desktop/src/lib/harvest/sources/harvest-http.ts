/**
 * Harvest HTTP adapter — the live source behind `LoadExchangePrices` and
 * `LoadHarvestFamilies`. Both ports read one `GET /api/harvest` snapshot
 * (POE-286 D10), so the page's two loads share a single request per horizon.
 *
 * A settled response is never reused: once a read settles, the next pair of port
 * calls fetches again. Loads that overlap a still-pending read of the same
 * horizon share that pending response, so a refetch that starts before the
 * previous read settles can show the earlier snapshot until the next update.
 * A failed request rejects both ports.
 */
import { fetchHarvest } from '$lib/api';
import type { HarvestResponse } from '$lib/api';
import type { CurrencyExchangeHorizon } from '$lib/exchange/view';
import type { LoadExchangePrices, LoadHarvestFamilies } from '../seam';

export function createHarvestHttp(
	fetchOne: (horizon: CurrencyExchangeHorizon) => Promise<HarvestResponse> = fetchHarvest
) {
	let inflight: { horizon: CurrencyExchangeHorizon; promise: Promise<HarvestResponse> } | null = null;

	const read = (horizon: CurrencyExchangeHorizon): Promise<HarvestResponse> => {
		if (inflight?.horizon !== horizon) {
			const promise = fetchOne(horizon);
			inflight = { horizon, promise };
			// The derived promise is caught so a rejected read leaves no unhandled rejection.
			promise
				.finally(() => {
					if (inflight?.promise === promise) inflight = null;
				})
				.catch(() => {});
		}
		return inflight.promise;
	};

	return {
		loadExchangePrices: (horizon) => read(horizon).then((r) => r.prices),
		loadHarvestFamilies: (horizon) => read(horizon).then((r) => r.families)
	} satisfies { loadExchangePrices: LoadExchangePrices; loadHarvestFamilies: LoadHarvestFamilies };
}
