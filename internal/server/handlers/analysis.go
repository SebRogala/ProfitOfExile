package handlers

import (
	"context"
	"encoding/json"
	"log/slog"
	"math"
	"net/http"
	"sort"
	"strconv"
	"strings"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/lab"
)

// normalizeVariant converts frontend variant format ("1/0", "20/0") to DB format
// ("1", "20"). The DB stores variants without the "/0" suffix for zero-quality gems.
// Variants with quality ("1/20", "20/20") pass through unchanged.
// TODO: Remove this once POE-52 normalizes DB storage to full "level/quality" format.
func normalizeVariant(v string) string {
	if strings.HasSuffix(v, "/0") {
		return strings.TrimSuffix(v, "/0")
	}
	return v
}

// parseLimit extracts and validates the limit query parameter.
// Returns the parsed limit and true, or writes an error response and returns 0, false.
func parseLimit(w http.ResponseWriter, r *http.Request, defaultLimit, maxLimit int) (int, bool) {
	limit := defaultLimit
	if v := r.URL.Query().Get("limit"); v != "" {
		n, err := strconv.Atoi(v)
		if err != nil || n < 1 {
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(http.StatusBadRequest)
			json.NewEncoder(w).Encode(map[string]string{"error": "limit must be a positive integer"})
			return 0, false
		}
		if n > maxLimit {
			n = maxLimit
		}
		limit = n
	}
	return limit, true
}

// validTiers lists the allowed tier filter values for v2 gem endpoints.
var validTiers = map[string]bool{
	"TOP": true, "HIGH": true, "MID": true, "LOW": true,
}

// validateTier checks whether the tier parameter is valid.
// Returns true if valid (or empty). Writes a 400 response and returns false if invalid.
func validateTier(w http.ResponseWriter, tier string) bool {
	if tier == "" {
		return true
	}
	if !validTiers[tier] {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusBadRequest)
		json.NewEncoder(w).Encode(map[string]string{
			"error": "invalid tier: must be one of TOP, HIGH, MID, LOW",
		})
		return false
	}
	return true
}

// TransfigureAnalysis returns the latest transfigure ROI results.
// Query params: variant (optional, e.g. "20/20"), limit (default 50, max 500).
// Uses in-memory cache when available, falls back to DB query.
func TransfigureAnalysis(repo *lab.Repository, cache *lab.Cache) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		variant := normalizeVariant(r.URL.Query().Get("variant"))

		limit, ok := parseLimit(w, r, 50, 500)
		if !ok {
			return
		}

		var results []lab.TransfigureResult

		// Fast path: serve from cache.
		if cache != nil {
			if cached := cache.Transfigure(); len(cached) > 0 {
				results = filterTransfigure(cached, variant, limit)
			}
		}

		// Slow path: fall back to DB query.
		if results == nil {
			var err error
			results, err = repo.LatestTransfigureResults(r.Context(), variant, limit)
			if err != nil {
				slog.Error("transfigure analysis: query failed", "error", err, "variant", variant)
				http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
				return
			}
		}

		type row struct {
			Time                 string  `json:"time"`
			BaseName             string  `json:"baseName"`
			TransfiguredName     string  `json:"transfiguredName"`
			Variant              string  `json:"variant"`
			BasePrice            float64 `json:"basePrice"`
			TransfiguredPrice    float64 `json:"transfiguredPrice"`
			ROI                  float64 `json:"roi"`
			ROIPct               float64 `json:"roiPct"`
			BaseListings         int     `json:"baseListings"`
			TransfiguredListings int     `json:"transfiguredListings"`
			GemColor             string  `json:"gemColor"`
			Confidence           string  `json:"confidence"`
		}

		rows := make([]row, 0, len(results))
		for _, r := range results {
			rows = append(rows, row{
				Time:                 r.Time.UTC().Format(time.RFC3339),
				BaseName:             r.BaseName,
				TransfiguredName:     r.TransfiguredName,
				Variant:              r.Variant,
				BasePrice:            r.BasePrice,
				TransfiguredPrice:    r.TransfiguredPrice,
				ROI:                  r.ROI,
				ROIPct:               r.ROIPct,
				BaseListings:         r.BaseListings,
				TransfiguredListings: r.TransfiguredListings,
				GemColor:             r.GemColor,
				Confidence:           r.Confidence,
			})
		}

		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(map[string]any{
			"count": len(rows),
			"data":  rows,
		}); err != nil {
			slog.Error("transfigure analysis: encode response", "error", err)
		}
	}
}

// filterTransfigure filters and limits cached transfigure results.
// Results are sorted by ROI descending (matching the DB query order).
func filterTransfigure(all []lab.TransfigureResult, variant string, limit int) []lab.TransfigureResult {
	var filtered []lab.TransfigureResult
	for _, r := range all {
		if variant != "" && r.Variant != variant {
			continue
		}
		filtered = append(filtered, r)
	}

	sort.Slice(filtered, func(i, j int) bool {
		return filtered[i].ROI > filtered[j].ROI
	})

	if len(filtered) > limit {
		filtered = filtered[:limit]
	}
	return filtered
}

// FontAnalysis returns the latest Font of Divine Skill EV results in three modes:
// Safe (LOW+ tier winners), Premium (MID-HIGH+ tier winners), and Jackpot (TOP only).
// Query params: variant (optional, e.g. "20/20"), limit (default 50, max 500).
// Uses in-memory cache when available, falls back to DB query.
func FontAnalysis(repo *lab.Repository, cache *lab.Cache) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		variant := normalizeVariant(r.URL.Query().Get("variant"))

		limit, ok := parseLimit(w, r, 50, 500)
		if !ok {
			return
		}

		var safeResults, premiumResults, jackpotResults []lab.FontResult
		cacheHit := false

		// Fast path: serve from cache.
		if cache != nil {
			analysis := cache.Font()
			if len(analysis.Safe) > 0 || len(analysis.Premium) > 0 || len(analysis.Jackpot) > 0 {
				safeResults = filterFont(analysis.Safe, variant, limit)
				premiumResults = filterFont(analysis.Premium, variant, limit)
				jackpotResults = filterFont(analysis.Jackpot, variant, limit)
				cacheHit = true
			}
		}

		// Slow path: fall back to DB query.
		if !cacheHit {
			var err error
			safeResults, err = repo.LatestFontResults(r.Context(), variant, "safe", limit)
			if err != nil {
				slog.Error("font analysis: query safe failed", "error", err, "variant", variant)
				http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
				return
			}
			premiumResults, err = repo.LatestFontResults(r.Context(), variant, "premium", limit)
			if err != nil {
				slog.Error("font analysis: query premium failed", "error", err, "variant", variant)
				http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
				return
			}
			jackpotResults, err = repo.LatestFontResults(r.Context(), variant, "jackpot", limit)
			if err != nil {
				slog.Error("font analysis: query jackpot failed", "error", err, "variant", variant)
				http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
				return
			}
		}

		type row struct {
			Time          string  `json:"time"`
			Color         string  `json:"color"`
			Variant       string  `json:"variant"`
			Pool          int     `json:"pool"`
			Winners       int     `json:"winners"`
			PWin          float64 `json:"pWin"`
			AvgWin        float64 `json:"avgWin"`
			AvgWinRaw     float64 `json:"avgWinRaw"`
			EV            float64 `json:"ev"`
			EVRaw         float64 `json:"evRaw"`
			InputCost     float64 `json:"inputCost"`
			Profit        float64 `json:"profit"`
			FontsToHit    float64              `json:"fontsToHit"`
			JackpotGems   []lab.JackpotGemInfo `json:"jackpotGems,omitempty"`
			ThinPoolGems  int                   `json:"thinPoolGems"`
			LiquidityRisk string                `json:"liquidityRisk"`
			PoolBreakdown []lab.TierPoolInfo    `json:"poolBreakdown,omitempty"`
		}

		toRows := func(results []lab.FontResult) []row {
			rows := make([]row, 0, len(results))
			for _, r := range results {
				rows = append(rows, row{
					Time:          r.Time.UTC().Format(time.RFC3339),
					Color:         r.Color,
					Variant:       r.Variant,
					Pool:          r.Pool,
					Winners:       r.Winners,
					PWin:          r.PWin,
					AvgWin:        r.AvgWin,
					AvgWinRaw:     r.AvgWinRaw,
					EV:            r.EV,
					EVRaw:         r.EVRaw,
					InputCost:     r.InputCost,
					Profit:        r.Profit,
					FontsToHit:    r.FontsToHit,
					JackpotGems:   r.JackpotGems,
					ThinPoolGems:  r.ThinPoolGems,
					LiquidityRisk: r.LiquidityRisk,
					PoolBreakdown: r.PoolBreakdown,
				})
			}
			return rows
		}

		safeRows := toRows(safeResults)
		premiumRows := toRows(premiumResults)
		jackpotRows := toRows(jackpotResults)

		// Enrich jackpot gems with GCP recipe (20/0 base + 20×GCP vs 20/20 base).
		if cache != nil {
			gcpPrice := cache.GCPPrice()
			if gcpPrice <= 0 {
				gcpPrice = 4.0
			}
			// Build base price index from transfigure cache: baseName → variant → price.
			type bKey struct{ name, variant string }
			basePrices := make(map[bKey]float64)
			if ct := cache.Transfigure(); len(ct) > 0 {
				for _, tr := range ct {
					basePrices[bKey{tr.BaseName, tr.Variant}] = tr.BasePrice
				}
			}
			for i := range jackpotRows {
				if jackpotRows[i].Variant != "20/20" && jackpotRows[i].Variant != "20" {
					continue
				}
				for j := range jackpotRows[i].JackpotGems {
					g := &jackpotRows[i].JackpotGems[j]
					baseName := lab.ExtractBaseName(g.Name)
					base20, ok := basePrices[bKey{baseName, "20"}]
					if !ok || base20 <= 0 {
						continue
					}
					base2020, ok := basePrices[bKey{baseName, "20/20"}]
					if !ok || base2020 <= 0 {
						continue
					}
					recipeCost := base20 + 20*gcpPrice
					g.GCPRecipeCost = recipeCost
					g.GCPRecipeBase = base20
					g.GCPRecipeSaves = base2020 - recipeCost
				}
			}
		}

		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(map[string]any{
			"safe":              safeRows,
			"premium":           premiumRows,
			"jackpot":           jackpotRows,
			"bestColorSafe":     bestColor(safeResults),
			"bestColorPremium":  bestColor(premiumResults),
			"bestColorJackpot":  bestColor(jackpotResults),
		}); err != nil {
			slog.Error("font analysis: encode response", "error", err)
		}
	}
}

// bestColor returns the color with the highest EV among the given font results.
func bestColor(results []lab.FontResult) string {
	evByColor := make(map[string]float64)
	for _, r := range results {
		evByColor[r.Color] += r.EV
	}
	var best string
	var bestEV float64
	for color, ev := range evByColor {
		if best == "" || ev > bestEV {
			best = color
			bestEV = ev
		}
	}
	return best
}

// filterFont filters and limits cached font results.
// Results are sorted by Profit descending (matching the DB query order).
func filterFont(all []lab.FontResult, variant string, limit int) []lab.FontResult {
	var filtered []lab.FontResult
	for _, r := range all {
		if variant != "" && r.Variant != variant {
			continue
		}
		filtered = append(filtered, r)
	}

	sort.Slice(filtered, func(i, j int) bool {
		return filtered[i].Profit > filtered[j].Profit
	})

	if len(filtered) > limit {
		filtered = filtered[:limit]
	}
	return filtered
}

// TrendAnalysis returns the latest trend analysis results sourced from v2 GemSignals + GemFeatures.
// Query params: variant (optional, e.g. "20/20"), signal (optional, e.g. "TRAP"), limit (default 50, max 500).
// Uses in-memory cache when available, falls back to DB query.
// Response shape matches the original v1 endpoint for frontend compatibility.
func TrendAnalysis(repo *lab.Repository, cache *lab.Cache) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		variant := normalizeVariant(r.URL.Query().Get("variant"))
		signal := r.URL.Query().Get("signal")
		window := r.URL.Query().Get("window")
		advanced := r.URL.Query().Get("advanced")
		tier := r.URL.Query().Get("tier")

		limit, ok := parseLimit(w, r, 50, 500)
		if !ok {
			return
		}

		// Load signals and features from cache or DB.
		var signals []lab.GemSignal
		var features []lab.GemFeature
		usedCache := false

		if cache != nil {
			cs := cache.GemSignals()
			cf := cache.GemFeatures()
			if len(cs) > 0 && len(cf) > 0 {
				signals = cs
				features = cf
				usedCache = true
			}
		}

		if !usedCache {
			var err error
			signals, err = repo.LatestGemSignals(r.Context(), "", "", 50000)
			if err != nil {
				slog.Error("trend analysis: gem signals query failed", "error", err)
				http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
				return
			}
			features, err = repo.LatestGemFeatures(r.Context(), "", "", 50000)
			if err != nil {
				slog.Error("trend analysis: gem features query failed", "error", err)
				http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
				return
			}
		}

		// Index features by (name, variant) for joining.
		type gk struct{ name, variant string }
		featIndex := make(map[gk]*lab.GemFeature, len(features))
		for i := range features {
			f := &features[i]
			featIndex[gk{f.Name, f.Variant}] = f
		}

		// Build merged rows, applying filters.
		type row struct {
			Time              string  `json:"time"`
			Name              string  `json:"name"`
			Variant           string  `json:"variant"`
			GemColor          string  `json:"gemColor"`
			CurrentPrice      float64 `json:"currentPrice"`
			CurrentListings   int     `json:"currentListings"`
			PriceVelocity     float64 `json:"priceVelocity"`
			ListingVelocity   float64 `json:"listingVelocity"`
			CV                float64 `json:"cv"`
			Signal            string  `json:"signal"`
			HistPosition      float64 `json:"histPosition"`
			PriceHigh7Days    float64 `json:"priceHigh7d"`
			PriceLow7Days     float64 `json:"priceLow7d"`
			BaseListings      int     `json:"baseListings"`
			BaseVelocity      float64 `json:"baseVelocity"`
			RelativeLiquidity float64 `json:"relativeLiquidity"`
			LiquidityTier     string  `json:"liquidityTier"`
			WindowScore       float64 `json:"windowScore"`
			WindowSignal      string  `json:"windowSignal"`
			AdvancedSignal    string  `json:"advancedSignal"`
			PriceTier         string  `json:"priceTier"`
			TierAction        string  `json:"tierAction"`
			SellUrgency       string  `json:"sellUrgency"`
			SellReason        string  `json:"sellReason"`
			Sellability       int     `json:"sellability"`
			SellabilityLabel  string  `json:"sellabilityLabel"`
			PriceTrend        []int   `json:"priceTrend,omitempty"`
			ListingsTrend     []int   `json:"listingsTrend,omitempty"`
			BaseListingsTrend []int   `json:"baseListingsTrend,omitempty"`
		}

		// Collect filtered signals into pre-row list for sparkline enrichment.
		type sigWithFeat struct {
			sig  *lab.GemSignal
			feat *lab.GemFeature // may be nil
		}
		var filtered []sigWithFeat

		for i := range signals {
			s := &signals[i]
			if variant != "" && s.Variant != variant {
				continue
			}
			if signal != "" && s.Signal != signal {
				continue
			}
			if window != "" && s.WindowSignal != window {
				continue
			}
			if advanced != "" && s.AdvancedSignal != advanced {
				continue
			}
			if tier != "" && s.Tier != tier {
				continue
			}
			f := featIndex[gk{s.Name, s.Variant}]
			filtered = append(filtered, sigWithFeat{sig: s, feat: f})
			if len(filtered) >= limit {
				break
			}
		}

		// Collect window alert gems for sparkline enrichment (deduplicated).
		windowAlerts := map[string]bool{"BREWING": true, "OPENING": true, "OPEN": true, "CLOSING": true}
		type gemKey struct{ name, variant string }
		seen := make(map[gemKey]bool)
		var transNames, baseNames []string
		for _, sf := range filtered {
			key := gemKey{sf.sig.Name, sf.sig.Variant}
			if windowAlerts[sf.sig.WindowSignal] && !seen[key] {
				seen[key] = true
				transNames = append(transNames, sf.sig.Name)
				baseName := sf.sig.Name
				if idx := strings.LastIndex(sf.sig.Name, " of "); idx > 0 {
					baseName = sf.sig.Name[:idx]
				}
				baseNames = append(baseNames, baseName)
			}
		}

		// Load MarketContext for sparkline normalization.
		var trendMC *lab.MarketContext
		if cache != nil {
			trendMC = cache.MarketContext()
		}
		if trendMC == nil {
			trendMC, _ = repo.LatestMarketContext(r.Context())
		}

		// Batch fetch sparkline data grouped by variant.
		type trendData struct {
			prices, listings, baseListings []int
		}
		trends := make(map[gemKey]trendData)
		if len(transNames) > 0 {
			type variantGroup struct {
				transNames []string
				baseNames  []string
				gems       []gemKey
			}
			groups := make(map[string]*variantGroup)
			for i, name := range transNames {
				key := gemKey{name, ""}
				// Find the variant for this gem from filtered results.
				for _, sf := range filtered {
					if sf.sig.Name == name && windowAlerts[sf.sig.WindowSignal] {
						key.variant = sf.sig.Variant
						break
					}
				}
				g, exists := groups[key.variant]
				if !exists {
					g = &variantGroup{}
					groups[key.variant] = g
				}
				g.transNames = append(g.transNames, name)
				g.baseNames = append(g.baseNames, baseNames[i])
				g.gems = append(g.gems, key)
			}

			last4 := func(pts []lab.SparklinePoint) []lab.SparklinePoint {
				if len(pts) > 4 {
					return pts[len(pts)-4:]
				}
				return pts
			}

			for v, g := range groups {
				transSparklines, err := repo.SparklineData(r.Context(), g.transNames, v, 24*7)
				if err != nil {
					slog.Warn("trend analysis: trans sparkline batch failed", "variant", v, "error", err)
					transSparklines = make(map[string][]lab.SparklinePoint)
				}
				// Normalize trans sparkline prices with temporal coefficients.
				transSparklines = normalizeSparklines(transSparklines, trendMC, v)

				baseSparklines, err := repo.SparklineData(r.Context(), g.baseNames, v, 24*7)
				if err != nil {
					slog.Warn("trend analysis: base sparkline batch failed", "variant", v, "error", err)
					baseSparklines = make(map[string][]lab.SparklinePoint)
				}
				// Normalize base sparklines consistently with trans sparklines.
				baseSparklines = normalizeSparklines(baseSparklines, trendMC, v)

				for idx, key := range g.gems {
					td := trendData{}
					if pts := last4(transSparklines[key.name]); len(pts) >= 2 {
						for _, p := range pts {
							td.prices = append(td.prices, int(math.Round(p.Price)))
							td.listings = append(td.listings, p.Listings)
						}
					}
					if pts := last4(baseSparklines[g.baseNames[idx]]); len(pts) >= 2 {
						for _, p := range pts {
							td.baseListings = append(td.baseListings, p.Listings)
						}
					}
					trends[key] = td
				}
			}
		}

		rows := make([]row, 0, len(filtered))
		for _, sf := range filtered {
			s := sf.sig
			rr := row{
				Time:             s.Time.UTC().Format(time.RFC3339),
				Name:             s.Name,
				Variant:          s.Variant,
				Signal:           s.Signal,
				WindowSignal:     s.WindowSignal,
				AdvancedSignal:   s.AdvancedSignal,
				PriceTier:        s.Tier,
				TierAction:       lab.TierActionFor(s.Signal, s.WindowSignal, s.Tier),
				SellUrgency:      s.SellUrgency,
				SellReason:       s.SellReason,
				Sellability:      s.Sellability,
				SellabilityLabel: s.SellabilityLabel,
			}

			// Enrich from features.
			if f := sf.feat; f != nil {
				rr.GemColor = f.GemColor
				rr.CurrentPrice = f.Chaos
				rr.CurrentListings = f.Listings
				rr.PriceVelocity = f.VelLongPrice
				rr.ListingVelocity = f.VelLongListing
				rr.CV = f.CV
				rr.HistPosition = f.HistPosition
				rr.PriceHigh7Days = f.High7Days
				rr.PriceLow7Days = f.Low7Days
				rr.RelativeLiquidity = f.RelativeListings
				rr.LiquidityTier = lab.LiquidityTierFor(f.MarketDepth)
			}

			if td, ok := trends[gemKey{s.Name, s.Variant}]; ok {
				rr.PriceTrend = td.prices
				rr.ListingsTrend = td.listings
				rr.BaseListingsTrend = td.baseListings
			}
			rows = append(rows, rr)
		}

		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(map[string]any{
			"count": len(rows),
			"data":  rows,
		}); err != nil {
			slog.Error("trend analysis: encode response", "error", err)
		}
	}
}

// QualityAnalysis returns the latest quality-roll ROI results.
// Query params: variant (optional, maps to level: "1"/"1/20" -> level 1, "20"/"20/20" -> level 20), limit (default 50, max 500).
// Uses in-memory cache when available, falls back to DB query.
func QualityAnalysis(repo *lab.Repository, cache *lab.Cache) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		variant := normalizeVariant(r.URL.Query().Get("variant"))

		limit, ok := parseLimit(w, r, 50, 500)
		if !ok {
			return
		}

		var results []lab.QualityResult

		// Fast path: serve from cache.
		if cache != nil {
			if cached := cache.Quality(); len(cached) > 0 {
				results = filterQuality(cached, variant, limit)
			}
		}

		// Slow path: fall back to DB query.
		if results == nil {
			var err error
			results, err = repo.LatestQualityResults(r.Context(), variant, limit)
			if err != nil {
				slog.Error("quality analysis: query failed", "error", err, "variant", variant)
				http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
				return
			}
		}

		type row struct {
			Time       string  `json:"time"`
			Name       string  `json:"name"`
			Level      int     `json:"level"`
			BuyPrice   float64 `json:"buyPrice"`
			PriceQ20   float64 `json:"priceQ20"`
			ROI4       float64 `json:"roi4"`
			ROI6       float64 `json:"roi6"`
			ROI10      float64 `json:"roi10"`
			ROI15      float64 `json:"roi15"`
			ROI20      float64 `json:"roi20"`
			AvgROI     float64 `json:"avgRoi"`
			GCPPrice   float64 `json:"gcpPrice"`
			Listings0  int     `json:"listings0"`
			Listings20 int     `json:"listings20"`
			GemColor   string  `json:"gemColor"`
			Confidence string  `json:"confidence"`
		}

		rows := make([]row, 0, len(results))
		for _, r := range results {
			rows = append(rows, row{
				Time:       r.Time.UTC().Format(time.RFC3339),
				Name:       r.Name,
				Level:      r.Level,
				BuyPrice:   r.BuyPrice,
				PriceQ20:   r.PriceQ20,
				ROI4:       r.ROI4,
				ROI6:       r.ROI6,
				ROI10:      r.ROI10,
				ROI15:      r.ROI15,
				ROI20:      r.ROI20,
				AvgROI:     r.AvgROI,
				GCPPrice:   r.GCPPrice,
				Listings0:  r.Listings0,
				Listings20: r.Listings20,
				GemColor:   r.GemColor,
				Confidence: r.Confidence,
			})
		}

		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(map[string]any{
			"count": len(rows),
			"data":  rows,
		}); err != nil {
			slog.Error("quality analysis: encode response", "error", err)
		}
	}
}

// filterQuality filters and limits cached quality results.
// Results are sorted by AvgROI descending (matching the DB query order).
// Variant maps to level: "1"/"1/20" -> level 1, "20"/"20/20" -> level 20.
func filterQuality(all []lab.QualityResult, variant string, limit int) []lab.QualityResult {
	var level int
	filterByLevel := false
	if variant != "" {
		filterByLevel = true
		level = 20
		if variant == "1" || variant == "1/20" {
			level = 1
		}
	}

	var filtered []lab.QualityResult
	for _, r := range all {
		if filterByLevel && r.Level != level {
			continue
		}
		filtered = append(filtered, r)
	}

	sort.Slice(filtered, func(i, j int) bool {
		return filtered[i].AvgROI > filtered[j].AvgROI
	})

	if len(filtered) > limit {
		filtered = filtered[:limit]
	}
	return filtered
}

// AnalysisStatus returns cache health information.
func AnalysisStatus(cache *lab.Cache, pool *pgxpool.Pool, league string) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")

		if cache == nil {
			if err := json.NewEncoder(w).Encode(map[string]any{
				"cached":      false,
				"transfigure": 0,
				"fontSafe":    0,
				"fontPremium": 0,
				"fontJackpot": 0,
				"quality":     0,
				"trends":      0,
			}); err != nil {
				slog.Error("analysis status: encode response", "error", err)
			}
			return
		}

		lastUpdated := cache.LastUpdated()
		cached := !lastUpdated.IsZero()

		fontAnalysis := cache.Font()
		resp := map[string]any{
			"cached":      cached,
			"league":      league,
			"transfigure": len(cache.Transfigure()),
			"fontSafe":    len(fontAnalysis.Safe),
			"fontPremium": len(fontAnalysis.Premium),
			"fontJackpot": len(fontAnalysis.Jackpot),
			"quality":     len(cache.Quality()),
			"trends":      len(cache.GemSignals()),
		}
		if cached {
			resp["lastUpdated"] = lastUpdated.UTC().Format(time.RFC3339)
		}
		if nf := cache.NextFetch(); !nf.IsZero() {
			resp["nextFetch"] = nf.UTC().Format(time.RFC3339)
		}

		// Divine→chaos rate for display in the header.
		if dr := cache.DivineRate(); dr > 0 {
			resp["divinePrice"] = dr
		} else if pool != nil {
			// Fallback to DB query if cache not yet populated.
			var divRate float64
			if err := pool.QueryRow(r.Context(),
				`SELECT chaos FROM currency_snapshots WHERE currency_id = 'divine' ORDER BY time DESC LIMIT 1`,
			).Scan(&divRate); err != nil {
				slog.Warn("analysis status: divine rate query failed", "error", err)
			} else {
				resp["divinePrice"] = divRate
				cache.SetDivineRate(divRate)
			}
		}

		if err := json.NewEncoder(w).Encode(resp); err != nil {
			slog.Error("analysis status: encode response", "error", err)
		}
	}
}

// SignalHistory returns the last N signal snapshots for a specific gem.
// GET /api/analysis/history?name=Spark+of+Nova&variant=20/20&limit=4
func SignalHistory(repo *lab.Repository) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		name := r.URL.Query().Get("name")
		variant := normalizeVariant(r.URL.Query().Get("variant"))
		if name == "" || variant == "" {
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(http.StatusBadRequest)
			json.NewEncoder(w).Encode(map[string]string{"error": "name and variant are required"})
			return
		}

		limit := 4
		if v := r.URL.Query().Get("limit"); v != "" {
			n, err := strconv.Atoi(v)
			if err != nil || n < 1 {
				w.Header().Set("Content-Type", "application/json")
				w.WriteHeader(http.StatusBadRequest)
				json.NewEncoder(w).Encode(map[string]string{"error": "limit must be a positive integer (max 20)"})
				return
			}
			if n > 20 {
				n = 20
			}
			limit = n
		}

		changes, err := repo.SignalHistory(r.Context(), name, variant, limit)
		if err != nil {
			slog.Error("signal history: query failed", "error", err, "name", name, "variant", variant)
			http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
			return
		}

		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(map[string]any{
			"name":    name,
			"variant": variant,
			"count":   len(changes),
			"history": changes,
		}); err != nil {
			slog.Error("signal history: encode response", "error", err)
		}
	}
}

// ---------------------------------------------------------------------------
// V2 Analysis Endpoints (POE-62: scaffolding; data populated by POE-56 through POE-61)
// ---------------------------------------------------------------------------

// MarketContextAnalysis returns the latest market context snapshot.
// No query params — returns a single object.
// Uses in-memory cache when available, falls back to DB query.
func MarketContextAnalysis(repo *lab.Repository, cache *lab.Cache) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		var mc *lab.MarketContext

		// Fast path: serve from cache.
		if cache != nil {
			mc = cache.MarketContext()
		}

		// Slow path: fall back to DB query.
		if mc == nil {
			var err error
			mc, err = repo.LatestMarketContext(r.Context())
			if err != nil {
				slog.Error("market context: query failed", "error", err)
				http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
				return
			}
		}

		w.Header().Set("Content-Type", "application/json")
		if mc == nil {
			if err := json.NewEncoder(w).Encode(map[string]any{"data": nil}); err != nil {
				slog.Error("market context: encode response", "error", err)
			}
			return
		}

		type resp struct {
			Time               string             `json:"time"`
			PricePercentiles   map[string]float64 `json:"pricePercentiles"`
			ListingPercentiles map[string]float64 `json:"listingPercentiles"`
			VelocityMean       float64            `json:"velocityMean"`
			VelocitySigma      float64            `json:"velocitySigma"`
			ListingVelMean     float64            `json:"listingVelMean"`
			ListingVelSigma    float64            `json:"listingVelSigma"`
			TotalGems          int                `json:"totalGems"`
			TotalListings      int                `json:"totalListings"`
			TierBoundaries     lab.TierBoundaries `json:"tierBoundaries"`
			HourlyBias         []float64          `json:"hourlyBias"`
			HourlyVolatility   []float64          `json:"hourlyVolatility"`
			HourlyActivity     []float64          `json:"hourlyActivity"`
			WeekdayBias        []float64          `json:"weekdayBias"`
			WeekdayVolatility  []float64          `json:"weekdayVolatility"`
			WeekdayActivity    []float64          `json:"weekdayActivity"`
		}

		// Nil-coalesce temporal slices so JSON encodes [] instead of null,
		// preventing frontend TypeError when iterating these fields.
		hourlyBias := mc.HourlyBias
		if hourlyBias == nil {
			hourlyBias = make([]float64, 24)
		}
		hourlyVol := mc.HourlyVolatility
		if hourlyVol == nil {
			hourlyVol = make([]float64, 24)
		}
		hourlyAct := mc.HourlyActivity
		if hourlyAct == nil {
			hourlyAct = make([]float64, 24)
		}
		weekdayBias := mc.WeekdayBias
		if weekdayBias == nil {
			weekdayBias = make([]float64, 7)
		}
		weekdayVol := mc.WeekdayVolatility
		if weekdayVol == nil {
			weekdayVol = make([]float64, 7)
		}
		weekdayAct := mc.WeekdayActivity
		if weekdayAct == nil {
			weekdayAct = make([]float64, 7)
		}

		if err := json.NewEncoder(w).Encode(map[string]any{
			"data": resp{
				Time:               mc.Time.UTC().Format(time.RFC3339),
				PricePercentiles:   mc.PricePercentiles,
				ListingPercentiles: mc.ListingPercentiles,
				VelocityMean:       mc.VelocityMean,
				VelocitySigma:      mc.VelocitySigma,
				ListingVelMean:     mc.ListingVelMean,
				ListingVelSigma:    mc.ListingVelSigma,
				TotalGems:          mc.TotalGems,
				TotalListings:      mc.TotalListings,
				TierBoundaries:     mc.TierBoundaries,
				HourlyBias:         hourlyBias,
				HourlyVolatility:   hourlyVol,
				HourlyActivity:     hourlyAct,
				WeekdayBias:        weekdayBias,
				WeekdayVolatility:  weekdayVol,
				WeekdayActivity:    weekdayAct,
			},
		}); err != nil {
			slog.Error("market context: encode response", "error", err)
		}
	}
}

// GemFeaturesAnalysis returns the latest pre-computed gem feature metrics.
// Query params: variant (optional), tier (optional), limit (default 50, max 500).
// Uses in-memory cache when available, falls back to DB query.
func GemFeaturesAnalysis(repo *lab.Repository, cache *lab.Cache) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		variant := normalizeVariant(r.URL.Query().Get("variant"))
		tier := r.URL.Query().Get("tier")
		if !validateTier(w, tier) {
			return
		}

		limit, ok := parseLimit(w, r, 50, 500)
		if !ok {
			return
		}

		var results []lab.GemFeature
		cacheHit := false

		// Fast path: serve from cache.
		if cache != nil {
			if cached := cache.GemFeatures(); len(cached) > 0 {
				results = filterGemFeatures(cached, variant, tier, limit)
				cacheHit = true
			}
		}

		// Slow path: fall back to DB query when cache was not consulted.
		if !cacheHit {
			var err error
			results, err = repo.LatestGemFeatures(r.Context(), variant, tier, limit)
			if err != nil {
				slog.Error("gem features: query failed", "error", err, "variant", variant, "tier", tier)
				http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
				return
			}
		}

		type row struct {
			Time                  string  `json:"time"`
			Name                  string  `json:"name"`
			Variant               string  `json:"variant"`
			Chaos                 float64 `json:"chaos"`
			Listings              int     `json:"listings"`
			Tier                  string  `json:"tier"`
			VelShortPrice         float64 `json:"velShortPrice"`
			VelShortListing       float64 `json:"velShortListing"`
			VelMedPrice           float64 `json:"velMedPrice"`
			VelMedListing         float64 `json:"velMedListing"`
			VelLongPrice          float64 `json:"velLongPrice"`
			VelLongListing        float64 `json:"velLongListing"`
			CV                    float64 `json:"cv"`
			HistPosition          float64 `json:"histPosition"`
			High7Days                float64 `json:"high7d"`
			Low7Days                 float64 `json:"low7d"`
			FloodCount            int     `json:"floodCount"`
			CrashCount            int     `json:"crashCount"`
			ListingElasticity     float64 `json:"listingElasticity"`
			RelativePrice         float64 `json:"relativePrice"`
			RelativeListings      float64 `json:"relativeListings"`
			SellProbabilityFactor float64 `json:"sellProbabilityFactor"`
			StabilityDiscount     float64 `json:"stabilityDiscount"`
		}

		rows := make([]row, 0, len(results))
		for _, f := range results {
			rows = append(rows, row{
				Time:                  f.Time.UTC().Format(time.RFC3339),
				Name:                  f.Name,
				Variant:               f.Variant,
				Chaos:                 f.Chaos,
				Listings:              f.Listings,
				Tier:                  f.Tier,
				VelShortPrice:         f.VelShortPrice,
				VelShortListing:       f.VelShortListing,
				VelMedPrice:           f.VelMedPrice,
				VelMedListing:         f.VelMedListing,
				VelLongPrice:          f.VelLongPrice,
				VelLongListing:        f.VelLongListing,
				CV:                    f.CV,
				HistPosition:          f.HistPosition,
				High7Days:                f.High7Days,
				Low7Days:                 f.Low7Days,
				FloodCount:            f.FloodCount,
				CrashCount:            f.CrashCount,
				ListingElasticity:     f.ListingElasticity,
				RelativePrice:         f.RelativePrice,
				RelativeListings:      f.RelativeListings,
				SellProbabilityFactor: f.SellProbabilityFactor,
				StabilityDiscount:     f.StabilityDiscount,
			})
		}

		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(map[string]any{
			"count": len(rows),
			"data":  rows,
		}); err != nil {
			slog.Error("gem features: encode response", "error", err)
		}
	}
}

// filterGemFeatures filters and limits cached gem features.
// Results are sorted by Chaos descending (matching the DB query order).
func filterGemFeatures(all []lab.GemFeature, variant, tier string, limit int) []lab.GemFeature {
	var filtered []lab.GemFeature
	for _, f := range all {
		if variant != "" && f.Variant != variant {
			continue
		}
		if tier != "" && f.Tier != tier {
			continue
		}
		filtered = append(filtered, f)
	}

	sort.Slice(filtered, func(i, j int) bool {
		return filtered[i].Chaos > filtered[j].Chaos
	})

	if len(filtered) > limit {
		filtered = filtered[:limit]
	}
	return filtered
}

// GemSignalsAnalysis returns the latest pre-computed gem signals.
// Query params: variant (optional), tier (optional), limit (default 50, max 500).
// Uses in-memory cache when available, falls back to DB query.
func GemSignalsAnalysis(repo *lab.Repository, cache *lab.Cache) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		variant := normalizeVariant(r.URL.Query().Get("variant"))
		tier := r.URL.Query().Get("tier")
		if !validateTier(w, tier) {
			return
		}

		limit, ok := parseLimit(w, r, 50, 500)
		if !ok {
			return
		}

		var results []lab.GemSignal
		cacheHit := false

		// Fast path: serve from cache.
		if cache != nil {
			if cached := cache.GemSignals(); len(cached) > 0 {
				results = filterGemSignals(cached, variant, tier, limit)
				cacheHit = true
			}
		}

		// Slow path: fall back to DB query when cache was not consulted.
		if !cacheHit {
			var err error
			results, err = repo.LatestGemSignals(r.Context(), variant, tier, limit)
			if err != nil {
				slog.Error("gem signals: query failed", "error", err, "variant", variant, "tier", tier)
				http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
				return
			}
		}

		type row struct {
			Time              string  `json:"time"`
			Name              string  `json:"name"`
			Variant           string  `json:"variant"`
			Signal            string  `json:"signal"`
			Confidence        int     `json:"confidence"`
			SellUrgency       string  `json:"sellUrgency"`
			SellReason        string  `json:"sellReason"`
			Sellability       int     `json:"sellability"`
			SellabilityLabel  string  `json:"sellabilityLabel"`
			WindowSignal      string  `json:"windowSignal"`
			AdvancedSignal    string  `json:"advancedSignal"`
			PhaseModifier     float64 `json:"phaseModifier"`
			Recommendation    string  `json:"recommendation"`
			Tier              string  `json:"tier"`
			RiskAdjustedValue float64 `json:"riskAdjustedValue"`
			QuickSellPrice    float64 `json:"quickSellPrice"`
			SellConfidence    string  `json:"sellConfidence"`
		}

		rows := make([]row, 0, len(results))
		for _, s := range results {
			rows = append(rows, row{
				Time:              s.Time.UTC().Format(time.RFC3339),
				Name:              s.Name,
				Variant:           s.Variant,
				Signal:            s.Signal,
				Confidence:        s.Confidence,
				SellUrgency:       s.SellUrgency,
				SellReason:        s.SellReason,
				Sellability:       s.Sellability,
				SellabilityLabel:  s.SellabilityLabel,
				WindowSignal:      s.WindowSignal,
				AdvancedSignal:    s.AdvancedSignal,
				PhaseModifier:     s.PhaseModifier,
				Recommendation:    s.Recommendation,
				Tier:              s.Tier,
				RiskAdjustedValue: s.RiskAdjustedValue,
				QuickSellPrice:    s.QuickSellPrice,
				SellConfidence:    s.SellConfidence,
			})
		}

		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(map[string]any{
			"count": len(rows),
			"data":  rows,
		}); err != nil {
			slog.Error("gem signals: encode response", "error", err)
		}
	}
}

// normalizeSparklines applies temporal normalization to sparkline price data
// so the sparkline visually matches what the signal classifier sees.
// The repository returns raw prices; this function divides each price by
// the temporal coefficient for that point's timestamp and variant.
// mc may be nil — in that case the data is returned unchanged.
func normalizeSparklines(sparklines map[string][]lab.SparklinePoint, mc *lab.MarketContext, variant string) map[string][]lab.SparklinePoint {
	if mc == nil || mc.TemporalMode == "none" || mc.TemporalMode == "" || len(mc.TemporalBuckets) == 0 {
		return sparklines
	}

	// Parse bucket data ONCE — CoefficientAt parses JSON on every call which is
	// too expensive when iterating thousands of sparkline points.
	var bucketData map[string][]lab.TemporalBucket
	if err := json.Unmarshal(mc.TemporalBuckets, &bucketData); err != nil {
		return sparklines
	}

	result := make(map[string][]lab.SparklinePoint, len(sparklines))
	for name, pts := range sparklines {
		normalized := make([]lab.SparklinePoint, len(pts))
		for i, p := range pts {
			normalized[i] = p
			t, err := time.Parse(time.RFC3339, p.Time)
			if err != nil {
				continue // keep raw price on parse error
			}
			coeff := lab.LookupCoefficient(bucketData, mc.TemporalMode, t, variant)
			if coeff > 0 {
				normalized[i].Price = p.Price / coeff
			}
		}
		result[name] = normalized
	}
	return result
}

// MarketOverview returns an aggregated market overview built from cached data.
// No query params — returns a single object with market stats, sell confidence
// spread, signal distribution, temporal mode, and divine rate.
func MarketOverview(cache *lab.Cache, pool *pgxpool.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")

		type overview struct {
			AvgTransPrice        float64        `json:"avgTransPrice"`
			AvgTransPriceDelta   float64        `json:"avgTransPriceDelta"`
			AvgBaseListings      float64        `json:"avgBaseListings"`
			AvgBaseListingsDelta float64        `json:"avgBaseListingsDelta"`
			ActiveGems           int            `json:"activeGems"`
			MostVolatileColor    string         `json:"mostVolatileColor"`
			MostVolatileCV       float64        `json:"mostVolatileCV"`
			MostStableColor      string         `json:"mostStableColor"`
			MostStableCV         float64        `json:"mostStableCV"`
			TemporalMode         string         `json:"temporalMode"`
			DivineRate           float64        `json:"divineRate"`
			SellConfidenceSpread map[string]int `json:"sellConfidenceSpread"`
			SignalDistribution   map[string]int `json:"signalDistribution"`
			Offerings            []offeringTiming `json:"offerings,omitempty"`
		}

		resp := overview{
			SellConfidenceSpread: map[string]int{"SAFE": 0, "FAIR": 0, "RISKY": 0},
			SignalDistribution:   map[string]int{"STABLE": 0, "UNCERTAIN": 0, "HERD": 0, "DUMPING": 0, "TRAP": 0},
			TemporalMode:         "none",
		}

		if cache == nil {
			if err := json.NewEncoder(w).Encode(resp); err != nil {
				slog.Error("market overview: encode response", "error", err)
			}
			return
		}

		// Divine rate.
		resp.DivineRate = math.Round(cache.DivineRate())

		// Temporal mode from MarketContext.
		if mc := cache.MarketContext(); mc != nil {
			resp.TemporalMode = mc.TemporalMode
			if resp.TemporalMode == "" {
				resp.TemporalMode = "none"
			}
		}

		// Aggregate from gem features for price/listings/volatile/stable.
		if feats := cache.GemFeatures(); len(feats) > 0 {
			var totalPrice, totalListings float64
			colorCV := make(map[string][]float64)

			for _, f := range feats {
				totalPrice += f.Chaos
				totalListings += float64(f.Listings)
				if f.GemColor != "" {
					colorCV[f.GemColor] = append(colorCV[f.GemColor], f.CV)
				}
			}

			resp.ActiveGems = len(feats)
			resp.AvgTransPrice = math.Round(totalPrice / float64(len(feats)))
			resp.AvgBaseListings = math.Round(totalListings / float64(len(feats)))

			// Most volatile / most stable by avg CV per color.
			type colorStat struct {
				color string
				avgCV float64
			}
			var stats []colorStat
			for color, cvs := range colorCV {
				sum := 0.0
				for _, v := range cvs {
					sum += v
				}
				stats = append(stats, colorStat{color, math.Round(sum / float64(len(cvs)))})
			}
			sort.Slice(stats, func(i, j int) bool { return stats[i].avgCV > stats[j].avgCV })
			if len(stats) > 0 {
				resp.MostVolatileColor = stats[0].color
				resp.MostVolatileCV = stats[0].avgCV
				resp.MostStableColor = stats[len(stats)-1].color
				resp.MostStableCV = stats[len(stats)-1].avgCV
			}
		}

		// Sell confidence spread and signal distribution from GemSignals.
		if signals := cache.GemSignals(); len(signals) > 0 {
			for _, s := range signals {
				if s.SellConfidence != "" {
					resp.SellConfidenceSpread[s.SellConfidence]++
				}
				if s.Signal != "" {
					resp.SignalDistribution[s.Signal]++
				}
			}
		}

		// Lab offering timing: serve from cache when available, compute on miss.
		if cache != nil {
			if cached := cache.OfferingTiming(); cached != nil {
				_ = json.Unmarshal(cached, &resp.Offerings)
			}
		}
		if len(resp.Offerings) == 0 && pool != nil {
			resp.Offerings = ComputeOfferingTimings(r.Context(), pool)
			// Populate cache for next request.
			if cache != nil && len(resp.Offerings) > 0 {
				if data, err := json.Marshal(resp.Offerings); err == nil {
					cache.SetOfferingTiming(data)
				}
			}
		}

		if err := json.NewEncoder(w).Encode(resp); err != nil {
			slog.Error("market overview: encode response", "error", err)
		}
	}
}

// offeringTiming holds price timing analysis for a lab offering (Gift, Dedication, etc.).
type offeringTiming struct {
	Name           string            `json:"name"`
	FragmentID     string            `json:"fragmentId"`
	CurrentPrice   float64           `json:"currentPrice"`
	CheapHours     []giftTimingEntry `json:"cheapHours,omitempty"`
	ExpensiveHours []giftTimingEntry `json:"expensiveHours,omitempty"`
	CheapDays      []giftDayEntry    `json:"cheapDays,omitempty"`
	ExpensiveDays  []giftDayEntry    `json:"expensiveDays,omitempty"`
	HourlyMedians     []giftTimingEntry `json:"hourlyMedians,omitempty"`
	TodayHourMedians  []giftTimingEntry `json:"todayHourMedians,omitempty"`
	Sparkline         []offeringSparkPt `json:"sparkline,omitempty"`
}

// offeringSparkPt is a single sparkline data point for offering price history.
type offeringSparkPt struct {
	Time  string  `json:"time"`
	Price float64 `json:"price"`
}

// giftHourMedian holds hourly median price (internal).
type giftHourMedian struct {
	Hour   int
	Median float64
}

// giftTimingEntry is the JSON-serialized hour timing.
type giftTimingEntry struct {
	Hour   int     `json:"hour"`
	Median float64 `json:"median"`
}

// giftDayEntry is the JSON-serialized weekday timing.
type giftDayEntry struct {
	Day    string  `json:"day"`
	Median float64 `json:"median"`
}

// ComputeOfferingTimings computes timing analysis for all lab offerings.
func ComputeOfferingTimings(ctx context.Context, pool *pgxpool.Pool) []offeringTiming {
	offerings := []struct {
		name       string
		fragmentID string
	}{
		{"Gift to the Goddess", "offer-gift"},
		{"Dedication to the Goddess", "offer-dedication"},
	}
	dayNames := []string{"Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"}

	var result []offeringTiming
	for _, off := range offerings {
		ot := computeOfferingTiming(ctx, pool, off.name, off.fragmentID, dayNames)
		if ot != nil {
			result = append(result, *ot)
		}
	}
	return result
}

// computeOfferingTiming queries fragment_snapshots for a single offering's
// price patterns and returns structured timing data. Returns nil if no data.
func computeOfferingTiming(ctx context.Context, pool *pgxpool.Pool, name, fragmentID string, dayNames []string) *offeringTiming {
	ot := offeringTiming{Name: name, FragmentID: fragmentID}

	// Current price.
	var currentPrice *float64
	if err := pool.QueryRow(ctx, `
		SELECT chaos FROM fragment_snapshots
		WHERE fragment_id = $1
		ORDER BY time DESC LIMIT 1`, fragmentID).Scan(&currentPrice); err != nil {
		slog.Warn("offering timing: current price query failed", "fragment", fragmentID, "error", err)
		return nil
	}
	if currentPrice == nil {
		return nil
	}
	ot.CurrentPrice = math.Round(*currentPrice)

	// Hourly medians (14-day window).
	hourRows, err := pool.Query(ctx, `
		SELECT EXTRACT(HOUR FROM time)::int AS h,
		       PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY chaos) AS median
		FROM fragment_snapshots
		WHERE fragment_id = $1 AND time > NOW() - INTERVAL '14 days'
		GROUP BY 1 HAVING COUNT(*) >= 3
		ORDER BY median`, fragmentID)
	if err != nil {
		slog.Warn("offering timing: hourly query failed", "fragment", fragmentID, "error", err)
	} else {
		var hours []giftHourMedian
		for hourRows.Next() {
			var hm giftHourMedian
			if err := hourRows.Scan(&hm.Hour, &hm.Median); err != nil {
				slog.Warn("offering timing: hourly scan failed", "fragment", fragmentID, "error", err)
				continue
			}
			hours = append(hours, hm)
		}
		hourRows.Close()
		if err := hourRows.Err(); err != nil {
			slog.Warn("offering timing: hourly iteration error", "fragment", fragmentID, "error", err)
		}
		// Store all hourly medians for prediction chart line.
		for _, h := range hours {
			ot.HourlyMedians = append(ot.HourlyMedians, giftTimingEntry{Hour: h.Hour, Median: math.Round(h.Median)})
		}
		sort.Slice(ot.HourlyMedians, func(i, j int) bool { return ot.HourlyMedians[i].Hour < ot.HourlyMedians[j].Hour })

		if len(hours) >= 4 {
			for _, h := range hours[:3] {
				ot.CheapHours = append(ot.CheapHours, giftTimingEntry{Hour: h.Hour, Median: math.Round(h.Median)})
			}
			for i := len(hours) - 1; i >= len(hours)-3 && i >= 0; i-- {
				ot.ExpensiveHours = append(ot.ExpensiveHours, giftTimingEntry{Hour: hours[i].Hour, Median: math.Round(hours[i].Median)})
			}
			// Sort by hour, not by price.
			sort.Slice(ot.CheapHours, func(i, j int) bool { return ot.CheapHours[i].Hour < ot.CheapHours[j].Hour })
			sort.Slice(ot.ExpensiveHours, func(i, j int) bool { return ot.ExpensiveHours[i].Hour < ot.ExpensiveHours[j].Hour })
		}
	}

	// Today's weekday-specific hourly medians (for prediction accuracy).
	todayDOW := int(time.Now().UTC().Weekday())
	todayRows, err := pool.Query(ctx, `
		SELECT EXTRACT(HOUR FROM time)::int AS h,
		       PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY chaos) AS median
		FROM fragment_snapshots
		WHERE fragment_id = $1 AND time > NOW() - INTERVAL '14 days'
		  AND EXTRACT(DOW FROM time)::int = $2
		GROUP BY 1 HAVING COUNT(*) >= 1
		ORDER BY h`, fragmentID, todayDOW)
	if err != nil {
		slog.Warn("offering timing: today hourly query failed", "fragment", fragmentID, "error", err)
	} else {
		for todayRows.Next() {
			var hm giftHourMedian
			if err := todayRows.Scan(&hm.Hour, &hm.Median); err != nil {
				slog.Warn("offering timing: today hourly scan failed", "fragment", fragmentID, "error", err)
				continue
			}
			ot.TodayHourMedians = append(ot.TodayHourMedians, giftTimingEntry{Hour: hm.Hour, Median: math.Round(hm.Median)})
		}
		todayRows.Close()
		if err := todayRows.Err(); err != nil {
			slog.Warn("offering timing: today hourly iteration error", "fragment", fragmentID, "error", err)
		}
	}

	// Weekday medians (14-day window).
	type dayMedian struct {
		Day    int
		Median float64
	}
	dayRows, err := pool.Query(ctx, `
		SELECT EXTRACT(DOW FROM time)::int AS d,
		       PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY chaos) AS median
		FROM fragment_snapshots
		WHERE fragment_id = $1 AND time > NOW() - INTERVAL '14 days'
		GROUP BY 1 HAVING COUNT(*) >= 5
		ORDER BY median`, fragmentID)
	if err != nil {
		slog.Warn("offering timing: weekday query failed", "fragment", fragmentID, "error", err)
	} else {
		var days []dayMedian
		for dayRows.Next() {
			var dm dayMedian
			if err := dayRows.Scan(&dm.Day, &dm.Median); err != nil {
				slog.Warn("offering timing: weekday scan failed", "fragment", fragmentID, "error", err)
				continue
			}
			days = append(days, dm)
		}
		dayRows.Close()
		if err := dayRows.Err(); err != nil {
			slog.Warn("offering timing: weekday iteration error", "fragment", fragmentID, "error", err)
		}
		if len(days) >= 4 {
			type dayWithIdx struct {
				idx int
				entry giftDayEntry
			}
			var cheapD, expD []dayWithIdx
			for i := 0; i < 2 && i < len(days); i++ {
				cheapD = append(cheapD, dayWithIdx{days[i].Day, giftDayEntry{Day: dayNames[days[i].Day], Median: math.Round(days[i].Median)}})
			}
			for i := len(days) - 1; i >= len(days)-2 && i >= 0; i-- {
				expD = append(expD, dayWithIdx{days[i].Day, giftDayEntry{Day: dayNames[days[i].Day], Median: math.Round(days[i].Median)}})
			}
			// Sort by weekday order (Sun=0 .. Sat=6).
			sort.Slice(cheapD, func(i, j int) bool { return cheapD[i].idx < cheapD[j].idx })
			sort.Slice(expD, func(i, j int) bool { return expD[i].idx < expD[j].idx })
			for _, d := range cheapD { ot.CheapDays = append(ot.CheapDays, d.entry) }
			for _, d := range expD { ot.ExpensiveDays = append(ot.ExpensiveDays, d.entry) }
		}
	}

	// Sparkline: 3-day price history, 1 point per hour.
	sparkRows, err := pool.Query(ctx, `
		SELECT time_bucket('1 hour', time) AS bucket, AVG(chaos) AS avg_price
		FROM fragment_snapshots
		WHERE fragment_id = $1 AND time > NOW() - INTERVAL '3 days'
		GROUP BY 1 ORDER BY 1`, fragmentID)
	if err != nil {
		slog.Warn("offering timing: sparkline query failed", "fragment", fragmentID, "error", err)
	} else {
		for sparkRows.Next() {
			var t time.Time
			var p float64
			if err := sparkRows.Scan(&t, &p); err != nil {
				slog.Warn("offering timing: sparkline scan failed", "fragment", fragmentID, "error", err)
				continue
			}
			ot.Sparkline = append(ot.Sparkline, offeringSparkPt{
				Time:  t.UTC().Format(time.RFC3339),
				Price: math.Round(p),
			})
		}
		sparkRows.Close()
		if err := sparkRows.Err(); err != nil {
			slog.Warn("offering timing: sparkline iteration error", "fragment", fragmentID, "error", err)
		}
	}

	return &ot
}

// filterGemSignals filters and limits cached gem signals.
// Results are sorted by Confidence descending, then Sellability descending (matching the DB query order).
func filterGemSignals(all []lab.GemSignal, variant, tier string, limit int) []lab.GemSignal {
	var filtered []lab.GemSignal
	for _, s := range all {
		if variant != "" && s.Variant != variant {
			continue
		}
		if tier != "" && s.Tier != tier {
			continue
		}
		filtered = append(filtered, s)
	}

	sort.Slice(filtered, func(i, j int) bool {
		if filtered[i].Confidence != filtered[j].Confidence {
			return filtered[i].Confidence > filtered[j].Confidence
		}
		return filtered[i].Sellability > filtered[j].Sellability
	})

	if len(filtered) > limit {
		filtered = filtered[:limit]
	}
	return filtered
}
