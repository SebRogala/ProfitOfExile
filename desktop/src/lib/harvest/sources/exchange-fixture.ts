/**
 * Currency Exchange fixture adapter — the mock behind `LoadExchangePrices`.
 * Going live replaces this file with a fetch of the server's read model.
 *
 * The fixture states its age as `updatedMinutesAgo`, which is not a server
 * shape: the adapter turns it into `lastUpdated = now − minutes`. Both horizons
 * serve the same fixture prices.
 */
import exchange from '../__fixtures__/exchange.json';
import type { ExchangeItemPrice, ExchangePriceRead, LifeforcePrice, LoadExchangePrices } from '../seam';

const MINUTE_MS = 60_000;

export function createExchangeFixture(now: () => Date = () => new Date()): LoadExchangePrices {
	return async (horizon) => {
		const prices: Record<string, ExchangeItemPrice> = {};
		for (const [itemId, p] of Object.entries(exchange.prices)) {
			prices[itemId] = { chaos: p.chaos, divine: p.divine, lastSeenChaos: null };
		}
		const copyLifeforce = (l: LifeforcePrice): LifeforcePrice => ({ ...l });
		const read: ExchangePriceRead = {
			league: exchange.league,
			horizon,
			lastUpdated: new Date(now().getTime() - exchange.updatedMinutesAgo * MINUTE_MS).toISOString(),
			priceHour: exchange.priceHour,
			warm: true,
			divineChaosRate: exchange.divineChaosRate,
			prices,
			lifeforce: {
				Primal: copyLifeforce(exchange.lifeforce.Primal),
				Vivid: copyLifeforce(exchange.lifeforce.Vivid),
				Wild: copyLifeforce(exchange.lifeforce.Wild)
			}
		};
		return read;
	};
}

export const loadExchangePrices: LoadExchangePrices = createExchangeFixture();
