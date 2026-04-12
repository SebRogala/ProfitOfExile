/**
 * Shared trade URL utilities.
 * Used by BestPlays, Comparator, and FontEVCompare for "Buy Base" / trade links.
 */

/**
 * Extract base gem name from transfigured name.
 * "Kinetic Blast of Clustering" -> "Kinetic Blast"
 */
export function baseGemName(name: string): string {
	const idx = name.lastIndexOf(' of ');
	return idx > 0 ? name.substring(0, idx) : name;
}

/**
 * Build GGG trade search URL for a base gem with variant filters.
 * Parameters: gem name (transfigured), variant string ("20/20"), league name.
 * Returns: full trade URL with query.
 */
export function baseGemTradeUrl(name: string, variant: string, league: string): string {
	const base = baseGemName(name);
	const parts = variant.split('/');
	const level = parseInt(parts[0]) || 0;
	const quality = parts.length > 1 ? parseInt(parts[1]) : 0;

	const miscFilters: Record<string, any> = { corrupted: { option: 'false' } };
	if (level >= 20) miscFilters.gem_level = { min: level, max: level };
	if (quality === 20) miscFilters.quality = { min: 20, max: 20 };

	const q = {
		query: {
			type: base,
			status: { option: 'securable' },
			filters: {
				type_filters: { filters: { category: { option: 'gem' } } },
				misc_filters: { filters: miscFilters },
				trade_filters: { filters: { sale_type: { option: 'priced' }, collapse: { option: 'true' } } },
			},
		},
		sort: { price: 'asc' },
	};
	return `https://www.pathofexile.com/trade/search/${encodeURIComponent(league || 'Mirage')}?q=${encodeURIComponent(JSON.stringify(q))}`;
}

/**
 * Build trade URL for cheapest corrupted 21/23 gems of a color, for Dedication lab input cost.
 */
export function cheapestCorrupted2123TradeUrl(color: string, isTransfigured: boolean, league: string): string {
	const reqFilters: Record<string, any> = {};
	if (color === 'RED')   { reqFilters.dex = { max: 97 }; reqFilters.int = { max: 97 }; }
	if (color === 'GREEN') { reqFilters.str = { max: 97 }; reqFilters.int = { max: 97 }; }
	if (color === 'BLUE')  { reqFilters.str = { max: 97 }; reqFilters.dex = { max: 97 }; }

	const miscFilters: Record<string, any> = {
		gem_level: { min: 21 },
		quality: { min: 23 },
		corrupted: { option: 'true' },
	};

	// "Transfigured Gem" filter on the trade site: "Yes" or "No".
	miscFilters.gem_transfigured = { option: isTransfigured ? 'true' : 'false' };

	const q: any = {
		query: {
			status: { option: 'securable' },
			filters: {
				type_filters: { filters: { category: { option: 'gem.activegem' } } },
				req_filters: { filters: reqFilters },
				misc_filters: { filters: miscFilters },
				trade_filters: { filters: { sale_type: { option: 'priced' }, collapse: { option: 'true' } } },
			},
		},
		sort: { price: 'asc' },
	};
	return `https://www.pathofexile.com/trade/search/${encodeURIComponent(league || 'Mirage')}?q=${encodeURIComponent(JSON.stringify(q))}`;
}
