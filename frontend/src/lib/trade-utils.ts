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
 * Build trade URL for cheapest corrupted 21/23 gems of a color, for Dedication lab input cost.
 * Filters: gem_level >= 21, quality >= 23, category gem.activegem (skills only).
 * Color filter via req_filters attribute caps (same pattern as FontEVCompare buyBaseUrl).
 * isTransfigured: true = only transfigured gems, false = exclude transfigured gems.
 */
export function cheapestCorrupted2123TradeUrl(color: string, isTransfigured: boolean, league: string): string {
	// Attribute caps to isolate pure-color gems (hybrids start at 98).
	const reqFilters: Record<string, any> = {};
	if (color === 'RED')   { reqFilters.dex = { max: 97 }; reqFilters.int = { max: 97 }; }
	if (color === 'GREEN') { reqFilters.str = { max: 97 }; reqFilters.int = { max: 97 }; }
	if (color === 'BLUE')  { reqFilters.str = { max: 97 }; reqFilters.dex = { max: 97 }; }

	const miscFilters: Record<string, any> = {
		gem_level: { min: 21 },
		quality: { min: 23 },
		corrupted: { option: 'true' },
	};

	// Transfigured gems have "alternate_art" = true on the trade site.
	if (isTransfigured) {
		miscFilters.gem_alternate_quality = { option: 'true' };
	}

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

	// For non-transfigured, we cannot filter "not transfigured" directly on the trade site.
	// The search will return all corrupted 21/23 skill gems of that color — users can manually
	// skip transfigured entries. This is a known trade site limitation.

	return `https://www.pathofexile.com/trade/search/${encodeURIComponent(league || 'Mirage')}?q=${encodeURIComponent(JSON.stringify(q))}`;
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
