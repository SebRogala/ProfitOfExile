/**
 * Harvest HTTP adapter — the live source behind `LoadExchangePrices` and
 * `LoadHarvestFamilies`. Both ports read one `GET /api/harvest` snapshot
 * (POE-286 D10), so the page's two loads share a single request per horizon.
 *
 * A settled response is never reused: once a read settles, the next pair of port
 * calls fetches again. Only ports in the same controller generation and horizon
 * share a pending response; a newer generation always starts a fresh request.
 * A failed request rejects both ports.
 */
import { fetchHarvest } from '$lib/api';
import type { HarvestResponse } from '$lib/api';
import type { CurrencyExchangeHorizon } from '$lib/exchange/view';
import type { LoadExchangePrices, LoadHarvestFamilies } from '../seam';

export function createHarvestHttp(
	fetchOne: (horizon: CurrencyExchangeHorizon) => Promise<HarvestResponse> = fetchHarvest
): { loadExchangePrices: LoadExchangePrices; loadHarvestFamilies: LoadHarvestFamilies } {
	let inflight: { horizon: CurrencyExchangeHorizon; generation: number | undefined; promise: Promise<HarvestResponse> } | null = null;

	const read = (horizon: CurrencyExchangeHorizon, generation?: number): Promise<HarvestResponse> => {
		if (inflight?.horizon !== horizon || inflight.generation !== generation) {
			const promise = fetchOne(horizon);
			inflight = { horizon, generation, promise };
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
		loadExchangePrices: (horizon, generation) => read(horizon, generation).then((r) => r.prices),
		loadHarvestFamilies: (horizon, generation) => read(horizon, generation).then((r) => r.families)
	};
}
