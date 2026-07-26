package handlers

import (
	"encoding/json"
	"log/slog"
	"net/http"
	"strconv"
	"strings"
	"time"

	"profitofexile/internal/lab"
	"profitofexile/internal/league"
	"profitofexile/internal/trade"
)

// collectiveRow is the JSON response shape for the collective endpoint.
// Used by both Normal and Dedication modes.
type collectiveRow struct {
	TransfiguredName     string               `json:"transfiguredName"`
	BaseName             string               `json:"baseName"`
	Variant              string               `json:"variant"`
	GemColor             string               `json:"gemColor"`
	ROI                  float64              `json:"roi"`
	ROIPct               float64              `json:"roiPct"`
	WeightedROI          float64              `json:"weightedRoi"`
	WeightedROIPct       float64              `json:"weightedRoiPct"`
	BasePrice            float64              `json:"basePrice"`
	TransfiguredPrice    float64              `json:"transfiguredPrice"`
	BaseListings         int                  `json:"baseListings"`
	TransfiguredListings int                  `json:"transfiguredListings"`
	Confidence           string               `json:"confidence"`
	Signal               string               `json:"signal"`
	PriceVelocity        float64              `json:"priceVelocity"`
	ListingVelocity      float64              `json:"listingVelocity"`
	CV                   float64              `json:"cv"`
	HistPosition         float64              `json:"histPosition"`
	WindowSignal         string               `json:"windowSignal"`
	AdvancedSignal       string               `json:"advancedSignal"`
	LiquidityTier        string               `json:"liquidityTier"`
	PriceTier            string               `json:"priceTier"`
	TierAction           string               `json:"tierAction"`
	SellUrgency          string               `json:"sellUrgency"`
	SellReason           string               `json:"sellReason"`
	Sellability          int                  `json:"sellability"`
	SellabilityLabel     string               `json:"sellabilityLabel"`
	Sparkline            []lab.SparklinePoint `json:"sparkline"`
	Low7Days             float64              `json:"low7d"`
	High7Days            float64              `json:"high7d"`
	SellConfidence       string               `json:"sellConfidence"`
	TradeConfidenceNote  string               `json:"tradeConfidenceNote,omitempty"`
	LowConfidence        bool                 `json:"lowConfidence,omitempty"`
	GCPRecipeCost        float64              `json:"gcpRecipeCost,omitempty"`
	GCPRecipeBase        float64              `json:"gcpRecipeBase,omitempty"`
	GCPRecipeSaves       float64              `json:"gcpRecipeSaves,omitempty"`
}

// sparklineWindowHours is the window sparkline responses have always covered.
// The lab cache stores a longer series than this — up to a 168-hour tail for
// gems that stopped appearing in snapshots — so the window is applied when the
// response is built, not when the cache is filled. Aliased from the lab
// constant so the served window and the populated window cannot drift apart.
const sparklineWindowHours = lab.SparklineWindowHours

// trimSparkline returns the suffix of pts newer than hours ago. Points are
// stored ascending by time, so the first point inside the window starts the
// suffix. hours <= 0 means "no trim" (the caller serves the full cached series).
func trimSparkline(pts []lab.SparklinePoint, hours int) []lab.SparklinePoint {
	if hours <= 0 || len(pts) == 0 {
		return pts
	}
	cutoff := time.Now().Add(-time.Duration(hours) * time.Hour)
	for i, p := range pts {
		t, err := time.Parse(time.RFC3339, p.Time)
		if err != nil {
			continue
		}
		if t.After(cutoff) {
			return pts[i:]
		}
	}
	return nil
}

// cachedSparklines collects the cached non-corrupted series for names at a
// single variant, keyed by name so the result is a drop-in for what
// Repository.SparklineData returns. Names with no cached points are omitted:
// on a warm cache that means the gem genuinely has no recent points, which is
// the same empty series the query would have produced.
func cachedSparklines(cache *lab.Cache, scope league.Scope, names []string, variant string, hours int) map[string][]lab.SparklinePoint {
	c := cache.For(scope)
	out := make(map[string][]lab.SparklinePoint, len(names))
	for _, n := range names {
		if pts := trimSparkline(c.Sparklines(n, variant), hours); len(pts) > 0 {
			out[n] = pts
		}
	}
	return out
}

// nonNilSparkline returns pts, or an empty slice when pts is nil.
//
// collectiveRow.Sparkline carries no omitempty, so a nil slice marshals as
// `null` while an empty one marshals as `[]`. Both collective modes must emit
// the same shape, so every row literal runs its series through this.
func nonNilSparkline(pts []lab.SparklinePoint) []lab.SparklinePoint {
	if pts == nil {
		return []lab.SparklinePoint{}
	}
	return pts
}

// cachedCorruptedVariant is the only corrupted variant the lab cache stores
// series for — the Dedication (21/23c) pool. It mirrors the population query's
// filter in internal/lab.
const cachedCorruptedVariant = "21/23c"

// cachedCorruptedSparklines collects cached corrupted series for names, keyed
// by name. The second return reports whether the cache could serve the request
// at all: a cold cache, or a variant the cache never populates, must go to the
// database rather than hand back an empty series.
func cachedCorruptedSparklines(cache *lab.Cache, scope league.Scope, names []string, variant string, hours int) (map[string][]lab.SparklinePoint, bool) {
	if cache == nil || variant != cachedCorruptedVariant {
		return nil, false
	}
	c := cache.For(scope)
	// The corrupted corpus specifically: a populated non-corrupted map says
	// nothing about whether this one can serve the request.
	if !c.HasSparklinesCorrupted() {
		return nil, false
	}
	out := make(map[string][]lab.SparklinePoint, len(names))
	for _, n := range names {
		if pts := trimSparkline(c.SparklinesCorrupted(n, variant), hours); len(pts) > 0 {
			out[n] = pts
		}
	}
	return out, true
}

// CollectiveAnalysis returns a ranked "what to farm now" list combining
// transfigure ROI with trend signals.
// Query params: variant (optional), budget (optional, max base price), limit (default 20, max 100).
func CollectiveAnalysis(repo *lab.Repository, cache *lab.Cache, scope league.Scope) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		// Dedication mode: return corrupted 21/23 gems ranked by price.
		if r.URL.Query().Get("mode") == "dedication" {
			serveDedicationCollective(w, r, repo, cache, scope)
			return
		}

		variant := normalizeVariant(r.URL.Query().Get("variant"))

		var budget float64
		if v := r.URL.Query().Get("budget"); v != "" {
			b, err := strconv.ParseFloat(v, 64)
			if err != nil || b < 0 {
				w.Header().Set("Content-Type", "application/json")
				w.WriteHeader(http.StatusBadRequest)
				json.NewEncoder(w).Encode(map[string]string{"error": "budget must be a non-negative number"})
				return
			}
			budget = b
		}

		limit := 20
		if v := r.URL.Query().Get("limit"); v != "" {
			n, err := strconv.Atoi(v)
			if err != nil || n < 1 {
				w.Header().Set("Content-Type", "application/json")
				w.WriteHeader(http.StatusBadRequest)
				json.NewEncoder(w).Encode(map[string]string{"error": "limit must be a positive integer"})
				return
			}
			if n > 100 {
				n = 100
			}
			limit = n
		}

		// Fast path: serve from cache.
		var transfigure []lab.TransfigureResult
		var signals []lab.GemSignal
		var features []lab.GemFeature
		usedCache := false

		if cache != nil {
			ct := cache.For(scope).Transfigure()
			cs := cache.For(scope).GemSignals()
			cf := cache.For(scope).GemFeatures()
			if len(ct) > 0 && len(cs) > 0 {
				transfigure = filterTransfigure(ct, variant, 1000)
				signals = filterGemSignals(cs, variant, "", 5000)
				features = cf // features used for velocity/CV join, no need to filter heavily
				usedCache = true
			}
		}

		// Slow path: fall back to DB query.
		if !usedCache {
			var err error
			transfigure, err = repo.LatestTransfigureResults(r.Context(), scope, variant, 1000)
			if err != nil {
				slog.Error("collective analysis: transfigure query failed", "error", err)
				http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
				return
			}

			signals, err = repo.LatestGemSignals(r.Context(), scope, variant, "", 5000)
			if err != nil {
				slog.Error("collective analysis: gem signals query failed", "error", err)
				http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
				return
			}

			features, err = repo.LatestGemFeatures(r.Context(), scope, variant, "", 50000)
			if err != nil {
				slog.Error("collective analysis: gem features query failed", "error", err)
				http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
				return
			}
		}

		// Parse sort mode: "pct" for ROI%, "chaos" (default) for absolute ROI.
		var sortBy lab.SortMode
		if s := r.URL.Query().Get("sort"); s == "pct" {
			sortBy = lab.SortPct
		}
		// Empty sortBy lets RankCollective apply budget-aware default.

		searchName := r.URL.Query().Get("search")

		effectiveLimit := limit
		if searchName != "" {
			effectiveLimit = 1000 // search returns all matches, not top-N
		}
		results := lab.RankCollective(transfigure, signals, features, budget, effectiveLimit, sortBy)

		// Filter by gem name search (case-insensitive substring).
		if searchName != "" {
			q := strings.ToLower(searchName)
			var matched []lab.CollectiveResult
			for _, cr := range results {
				if strings.Contains(strings.ToLower(cr.TransfiguredName), q) {
					matched = append(matched, cr)
				}
			}
			results = matched
		}

		// Build base price index: baseName → variant → basePrice (for GCP recipe).
		type bKey struct{ name, variant string }
		basePriceIndex := make(map[bKey]float64, len(transfigure))
		for _, tr := range transfigure {
			basePriceIndex[bKey{tr.BaseName, tr.Variant}] = tr.BasePrice
		}

		// GCP recipe: for 20/20 gems, compare buying 20/20 base vs 20/0 base + 20×GCP.
		var gcpPrice float64
		if cache != nil {
			gcpPrice = cache.For(scope).GCPPrice()
		}
		if gcpPrice <= 0 {
			gcpPrice = 4.0
			slog.Warn("collective: GCP price not cached, using fallback", "fallback", gcpPrice)
		}

		// Sparkline data for the result gems.
		// Warm cache: every gem/variant series the pipeline populates is in
		// memory, so no gem_snapshots query is issued — not even for a gem the
		// cache has no series for, which genuinely has no recent points.
		// Cold cache: fall back to the queries. When a specific variant is
		// selected one query suffices; when "ALL variants", group gems by their
		// own variant and query per group so sparklines don't mix variant prices.
		sparkVariant := r.URL.Query().Get("variant")
		sparklines := make(map[string][]lab.SparklinePoint)

		if cache != nil && cache.For(scope).HasSparklines() {
			c := cache.For(scope)
			for _, cr := range results {
				v := sparkVariant
				if v == "" {
					v = cr.Variant
				}
				// Raw prices — normalization creates edge artifacts
				if pts := trimSparkline(c.Sparklines(cr.TransfiguredName, v), sparklineWindowHours); len(pts) > 0 {
					sparklines[cr.TransfiguredName] = pts
				}
			}
		} else if sparkVariant != "" {
			// Single variant — one query for all gems.
			sparkNames := make([]string, 0, len(results))
			for _, cr := range results {
				sparkNames = append(sparkNames, cr.TransfiguredName)
			}
			sp, err := repo.SparklineData(r.Context(), scope, sparkNames, sparkVariant, sparklineWindowHours)
			if err != nil {
				slog.Error("collective analysis: sparkline query failed", "error", err)
			} else {
				sparklines = sp // Raw prices — normalization creates edge artifacts
			}
		} else {
			// ALL variants — group by each gem's own variant.
			byVariant := make(map[string][]string) // variant -> []name
			for _, cr := range results {
				byVariant[cr.Variant] = append(byVariant[cr.Variant], cr.TransfiguredName)
			}
			for v, names := range byVariant {
				sp, err := repo.SparklineData(r.Context(), scope, names, v, sparklineWindowHours)
				if err != nil {
					slog.Error("collective analysis: sparkline query failed", "variant", v, "error", err)
					continue
				}
				for k, pts := range sp {
					sparklines[k] = pts
				}
			}
		}

		rows := make([]collectiveRow, 0, len(results))
		for _, cr := range results {
			r := collectiveRow{
				TransfiguredName:     cr.TransfiguredName,
				BaseName:             cr.BaseName,
				Variant:              cr.Variant,
				GemColor:             cr.GemColor,
				ROI:                  cr.ROI,
				ROIPct:               cr.ROIPct,
				WeightedROI:          cr.WeightedROI,
				WeightedROIPct:       cr.WeightedROIPct,
				BasePrice:            cr.BasePrice,
				TransfiguredPrice:    cr.TransfiguredPrice,
				BaseListings:         cr.BaseListings,
				TransfiguredListings: cr.TransfiguredListings,
				Confidence:           cr.Confidence,
				Signal:               cr.Signal,
				PriceVelocity:        cr.PriceVelocity,
				ListingVelocity:      cr.ListingVelocity,
				CV:                   cr.CV,
				HistPosition:         cr.HistPosition,
				WindowSignal:         cr.WindowSignal,
				AdvancedSignal:       cr.AdvancedSignal,
				LiquidityTier:        cr.LiquidityTier,
				PriceTier:            cr.PriceTier,
				TierAction:           cr.TierAction,
				SellUrgency:          cr.SellUrgency,
				SellReason:           cr.SellReason,
				Sellability:          cr.Sellability,
				SellabilityLabel:     cr.SellabilityLabel,
				Sparkline:            nonNilSparkline(sparklines[cr.TransfiguredName]),
				Low7Days:             cr.Low7Days,
				High7Days:            cr.High7Days,
				SellConfidence:       cr.SellConfidence,
				TradeConfidenceNote:  cr.TradeConfidenceNote,
				LowConfidence:       cr.LowConfidence,
			}

			// GCP recipe for 20/20 variants: buy 20/0 base + 20×GCP.
			// Always show — even when more expensive, it's useful context.
			if cr.Variant == "20/20" {
				if base20, ok := basePriceIndex[bKey{cr.BaseName, "20"}]; ok && base20 > 0 {
					recipeCost := base20 + 20*gcpPrice
					r.GCPRecipeCost = recipeCost
					r.GCPRecipeBase = base20
					r.GCPRecipeSaves = cr.BasePrice - recipeCost // negative = recipe is more expensive
				}
			}

			rows = append(rows, r)
		}

		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(map[string]any{
			"count": len(rows),
			"data":  rows,
		}); err != nil {
			slog.Error("collective analysis: encode response", "error", err)
		}
	}
}

// serveDedicationCollective handles ?mode=dedication on the collective endpoint.
// Returns corrupted 21/23 gems ranked by price, mapped to the same response shape
// as the Normal collective so the frontend ByVariant/BestPlays components work unchanged.
func serveDedicationCollective(w http.ResponseWriter, r *http.Request, repo *lab.Repository, cache *lab.Cache, scope league.Scope) {
	limit := 100
	if v := r.URL.Query().Get("limit"); v != "" {
		if n, err := strconv.Atoi(v); err == nil && n > 0 {
			if n > 500 {
				n = 500
			}
			limit = n
		}
	}
	searchName := r.URL.Query().Get("search")

	// Load latest gem prices (corrupted gems are in the same snapshot).
	gems, _, err := repo.LatestGemPrices(r.Context(), scope)
	if err != nil {
		slog.Error("dedication collective: gem prices query failed", "error", err)
		http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
		return
	}

	// Load Dedication results for input costs.
	var dedicationResults []lab.DedicationResult
	if cache != nil {
		ded := cache.For(scope).Dedication()
		dedicationResults = append(dedicationResults, ded.Skills...)
		dedicationResults = append(dedicationResults, ded.Transfigured...)
	}

	results := lab.RankDedicationCollective(gems, dedicationResults, limit, searchName)

	// Map to the same row shape as Normal collective.
	rows := make([]collectiveRow, 0, len(results))
	for _, cr := range results {
		rows = append(rows, collectiveRow{
			TransfiguredName:     cr.TransfiguredName,
			BaseName:             cr.BaseName,
			Variant:              cr.Variant,
			GemColor:             cr.GemColor,
			ROI:                  cr.ROI,
			ROIPct:               cr.ROIPct,
			WeightedROI:          cr.ROI, // no risk adjustment for rankings
			WeightedROIPct:       cr.ROIPct,
			BasePrice:            cr.BasePrice,
			TransfiguredPrice:    cr.TransfiguredPrice,
			BaseListings:         0,
			TransfiguredListings: cr.TransfiguredListings,
			Confidence:           cr.Confidence,
			SellConfidence:       cr.SellConfidence,
			LowConfidence:        cr.LowConfidence,
			// Dedication rows carry no series, but the field has no omitempty:
			// without this the endpoint emits `null` where the Normal mode
			// emits `[]`.
			Sparkline: nonNilSparkline(nil),
		})
	}

	w.Header().Set("Content-Type", "application/json")
	if err := json.NewEncoder(w).Encode(map[string]any{
		"count": len(rows),
		"data":  rows,
	}); err != nil {
		slog.Error("dedication collective: encode response", "error", err)
	}
}

// CompareAnalysis returns side-by-side comparison of 1-5 specific gems.
// Query params: gems (comma-separated, required, max 5), variant (optional),
// mode (optional: "dedication" for corrupted 21/23c pool scoring).
// tradeCache may be nil — trade enrichment is skipped when unavailable.
func CompareAnalysis(repo *lab.Repository, cache *lab.Cache, tradeCache *trade.TradeCache, scope league.Scope) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		start := time.Now()
		// Single deferred telemetry emit captures every return path with rich fields.
		// Locals are populated as the request flows; closure captures by reference.
		var (
			spMs       int64
			outcome    = "ok"
			statusCode = http.StatusOK
			variant    string
			usedCache  bool
			gemCount   int
			rowCount   int
			warnCount  int
		)
		defer func() {
			slog.Info("compare_analysis",
				"duration_ms", time.Since(start).Milliseconds(),
				"sparkline_ms", spMs,
				"outcome", outcome,
				"status_code", statusCode,
				"gems", gemCount,
				"variant", variant,
				"cache_hit", usedCache,
				"rows", rowCount,
				"warnings", warnCount,
			)
		}()

		gemsParam := r.URL.Query().Get("gems")
		if gemsParam == "" {
			outcome = "bad_request"
			statusCode = http.StatusBadRequest
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(http.StatusBadRequest)
			json.NewEncoder(w).Encode(map[string]string{"error": "gems parameter is required"})
			return
		}

		names := strings.Split(gemsParam, ",")
		if len(names) > 5 {
			outcome = "bad_request"
			statusCode = http.StatusBadRequest
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(http.StatusBadRequest)
			json.NewEncoder(w).Encode(map[string]string{"error": "maximum 5 gems allowed"})
			return
		}

		// Trim whitespace and filter empty names.
		filtered := names[:0]
		for _, n := range names {
			n = strings.TrimSpace(n)
			if n != "" {
				filtered = append(filtered, n)
			}
		}
		names = filtered
		gemCount = len(names)

		if len(names) == 0 {
			outcome = "bad_request"
			statusCode = http.StatusBadRequest
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(http.StatusBadRequest)
			json.NewEncoder(w).Encode(map[string]string{"error": "at least one gem name is required"})
			return
		}

		mode := r.URL.Query().Get("mode")

		// Dedication mode: score gems against the corrupted 21/23c pool.
		// Mark outcome explicitly so the deferred telemetry tags this branch
		// (serveDedicationCompare has its own logic but this handler's emit
		// still fires — outcome="dedication" disambiguates the path).
		if mode == "dedication" {
			outcome = "dedication"
			serveDedicationCompare(w, r, repo, cache, scope, names)
			return
		}

		variant = normalizeVariant(r.URL.Query().Get("variant"))

		// Fast path: serve from cache.
		var transfigure []lab.TransfigureResult
		var signals []lab.GemSignal
		var features []lab.GemFeature

		if cache != nil {
			ct := cache.For(scope).Transfigure()
			cs := cache.For(scope).GemSignals()
			cf := cache.For(scope).GemFeatures()
			if len(ct) > 0 && len(cs) > 0 {
				transfigure = filterTransfigure(ct, variant, 1000)
				signals = filterGemSignals(cs, variant, "", 5000)
				features = cf
				usedCache = true
			}
		}

		// Slow path: fall back to DB query.
		if !usedCache {
			var err error
			transfigure, err = repo.LatestTransfigureResults(r.Context(), scope, variant, 1000)
			if err != nil {
				outcome = "db_error"
				statusCode = http.StatusInternalServerError
				slog.Error("compare analysis: transfigure query failed", "error", err)
				http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
				return
			}

			signals, err = repo.LatestGemSignals(r.Context(), scope, variant, "", 5000)
			if err != nil {
				outcome = "db_error"
				statusCode = http.StatusInternalServerError
				slog.Error("compare analysis: gem signals query failed", "error", err)
				http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
				return
			}

			features, err = repo.LatestGemFeatures(r.Context(), scope, variant, "", 50000)
			if err != nil {
				outcome = "db_error"
				statusCode = http.StatusInternalServerError
				slog.Error("compare analysis: gem features query failed", "error", err)
				http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
				return
			}
		}

		// Load sparkline data (last 12 hours).
		var warnings []string
		// NOTE: sparkline failure logs + appends a warning then CONTINUES (no early return),
		// so spMs reflects the actual query duration even on error (not zero).
		spStart := time.Now()
		var sparklines map[string][]lab.SparklinePoint
		// The cache keys a series by name AND variant, so it cannot reproduce
		// the mixed-variant series an empty variant returns today — that case
		// stays on the query.
		if variant != "" && cache != nil && cache.For(scope).HasSparklines() {
			sparklines = cachedSparklines(cache, scope, names, variant, sparklineWindowHours)
		} else {
			sp, err := repo.SparklineData(r.Context(), scope, names, variant, sparklineWindowHours)
			if err != nil {
				slog.Error("compare analysis: sparkline query failed", "error", err)
				sp = make(map[string][]lab.SparklinePoint)
				warnings = append(warnings, "Sparkline data temporarily unavailable")
			}
			sparklines = sp
		}
		spMs = time.Since(spStart).Milliseconds()

		// Sparkline normalization removed — temporal coefficients create edge artifacts

		results := lab.BuildCompareResults(names, transfigure, signals, features, sparklines, variant)

		rows := buildCompareRows(results, tradeCache, scope)
		rowCount = len(rows)
		warnCount = len(warnings)

		resp := map[string]any{
			"count": len(rows),
			"data":  rows,
		}
		if len(warnings) > 0 {
			resp["warnings"] = warnings
		}

		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(resp); err != nil {
			outcome = "encode_error"
			slog.Error("compare analysis: encode response", "error", err)
		}
	}
}

// serveDedicationCompare handles the Dedication lab compare path.
// Queries corrupted 21/23c gem prices, loads Dedication analysis for input costs,
// and auto-detects pool (skill vs transfigured) from gem names.
func serveDedicationCompare(w http.ResponseWriter, r *http.Request, repo *lab.Repository, cache *lab.Cache, scope league.Scope, names []string) {
	// Load corrupted gem prices for the requested names.
	gemPrices, err := repo.CorruptedGemPricesByNames(r.Context(), scope, names)
	if err != nil {
		slog.Error("compare analysis (dedication): corrupted gem prices query failed", "error", err)
		http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
		return
	}

	// Load Dedication analysis results for input cost context.
	// Fast path: cache.
	var dedicationResults []lab.DedicationResult
	if cache != nil {
		ded := cache.For(scope).Dedication()
		dedicationResults = append(dedicationResults, ded.Skills...)
		dedicationResults = append(dedicationResults, ded.Transfigured...)
	}

	// Slow path: fall back to DB if cache is empty.
	if len(dedicationResults) == 0 {
		skills, err := repo.LatestDedicationResults(r.Context(), scope, "skill", "", 100)
		if err != nil {
			slog.Error("compare analysis (dedication): dedication skills query failed", "error", err)
			http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
			return
		}
		trans, err := repo.LatestDedicationResults(r.Context(), scope, "transfigured", "", 100)
		if err != nil {
			slog.Error("compare analysis (dedication): dedication transfigured query failed", "error", err)
			http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
			return
		}
		dedicationResults = append(skills, trans...)
	}

	// Load sparkline data for corrupted gems (last 12 hours).
	// The cache populates corrupted series for the Dedication variant only, so
	// the read is guarded on it: any other variant has no cached series and must
	// go to the query rather than read an empty series out of a warm cache.
	var warnings []string
	sparklines, cached := cachedCorruptedSparklines(cache, scope, names, cachedCorruptedVariant, sparklineWindowHours)
	if !cached {
		sp, err := repo.SparklineDataCorrupted(r.Context(), scope, names, cachedCorruptedVariant, sparklineWindowHours)
		if err != nil {
			slog.Error("compare analysis (dedication): sparkline query failed", "error", err)
			sp = make(map[string][]lab.SparklinePoint)
			warnings = append(warnings, "Sparkline data temporarily unavailable")
		}
		sparklines = sp
	}

	results := lab.BuildDedicationCompareResults(names, gemPrices, dedicationResults, sparklines)

	rows := buildCompareRows(results, nil, scope) // no trade enrichment for Dedication mode

	resp := map[string]any{
		"count": len(rows),
		"data":  rows,
		"mode":  "dedication",
	}
	if len(warnings) > 0 {
		resp["warnings"] = warnings
	}

	w.Header().Set("Content-Type", "application/json")
	if err := json.NewEncoder(w).Encode(resp); err != nil {
		slog.Error("compare analysis (dedication): encode response", "error", err)
	}
}

// compareRow is the JSON response shape for the compare endpoint (shared between
// normal and dedication modes).
type compareRow struct {
	TransfiguredName     string              `json:"transfiguredName"`
	BaseName             string              `json:"baseName"`
	Variant              string              `json:"variant"`
	GemColor             string              `json:"gemColor"`
	ROI                  float64             `json:"roi"`
	ROIPct               float64             `json:"roiPct"`
	BasePrice            float64             `json:"basePrice"`
	TransfiguredPrice    float64             `json:"transfiguredPrice"`
	Confidence           string              `json:"confidence"`
	Signal               string              `json:"signal"`
	CV                   float64             `json:"cv"`
	PriceVelocity        float64             `json:"priceVelocity"`
	ListingVelocity      float64             `json:"listingVelocity"`
	HistPosition         float64             `json:"histPosition"`
	Sparkline            []lab.SparklinePoint `json:"sparkline"`
	Recommendation       string              `json:"recommendation"`
	SellUrgency          string              `json:"sellUrgency"`
	SellReason           string              `json:"sellReason"`
	Sellability          int                 `json:"sellability"`
	SellabilityLabel     string              `json:"sellabilityLabel"`
	PriceTier            string              `json:"priceTier"`
	TierAction           string              `json:"tierAction"`
	WindowSignal         string              `json:"windowSignal"`
	BaseListings         int                 `json:"baseListings"`
	LiquidityTier        string              `json:"liquidityTier"`
	TransListings        int                 `json:"transListings"`
	TransfiguredListings int                 `json:"transfiguredListings"`
	WeightedROI          float64             `json:"weightedRoi"`
	WeightedROIPct       float64             `json:"weightedRoiPct"`
	Low7Days             float64             `json:"low7d"`
	High7Days            float64             `json:"high7d"`
	SellConfidence       string              `json:"sellConfidence"`
	SellConfidenceReason string              `json:"sellConfidenceReason"`
	QuickSellPrice       float64             `json:"quickSellPrice"`
	RiskAdjustedPrice    float64             `json:"riskAdjustedPrice"`
	Trade                *trade.TradeLookupResult `json:"trade,omitempty"`
}

// buildCompareRows converts CompareResult slices into the JSON-serializable row format.
// tradeCache may be nil — trade enrichment is skipped when unavailable.
func buildCompareRows(results []lab.CompareResult, tradeCache *trade.TradeCache, scope league.Scope) []compareRow {
	rows := make([]compareRow, 0, len(results))
	for _, cr := range results {
		rw := compareRow{
			TransfiguredName:     cr.TransfiguredName,
			BaseName:             cr.BaseName,
			Variant:              cr.Variant,
			GemColor:             cr.GemColor,
			ROI:                  cr.ROI,
			ROIPct:               cr.ROIPct,
			BasePrice:            cr.BasePrice,
			TransfiguredPrice:    cr.TransfiguredPrice,
			Confidence:           cr.Confidence,
			Signal:               cr.Signal,
			CV:                   cr.CV,
			PriceVelocity:        cr.PriceVelocity,
			ListingVelocity:      cr.ListingVelocity,
			HistPosition:         cr.HistPosition,
			Sparkline:            cr.Sparkline,
			Recommendation:       cr.Recommendation,
			SellUrgency:          cr.SellUrgency,
			SellReason:           cr.SellReason,
			Sellability:          cr.Sellability,
			SellabilityLabel:     cr.SellabilityLabel,
			PriceTier:            cr.PriceTier,
			TierAction:           cr.TierAction,
			WindowSignal:         cr.WindowSignal,
			BaseListings:         cr.BaseListings,
			LiquidityTier:        cr.LiquidityTier,
			TransListings:        cr.TransListings,
			TransfiguredListings: cr.TransListings,
			WeightedROI:          cr.WeightedROI,
			WeightedROIPct:       cr.WeightedROIPct,
			Low7Days:             cr.Low7Days,
			High7Days:            cr.High7Days,
			SellConfidence:       cr.SellConfidence,
			SellConfidenceReason: cr.SellConfidenceReason,
			QuickSellPrice:       cr.QuickSellPrice,
			RiskAdjustedPrice:    cr.RiskAdjustedPrice,
		}

		// Enrich with cached trade data when available.
		if tradeCache != nil {
			if tradeResult, ok := tradeCache.For(scope).Get(trade.CacheKey(cr.TransfiguredName, cr.Variant)); ok {
				rw.Trade = tradeResult
			}
		}

		rows = append(rows, rw)
	}
	return rows
}

// GemNamesAutocomplete returns distinct transfigured gem names matching a query.
// Query params: q (required, matches all words in any order), limit (default 10, max 50).
func GemNamesAutocomplete(repo *lab.Repository, cache *lab.Cache, scope league.Scope) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		q := r.URL.Query().Get("q")
		if q == "" {
			w.Header().Set("Content-Type", "application/json")
			json.NewEncoder(w).Encode(map[string]any{"names": []string{}})
			return
		}

		limit := 10
		if v := r.URL.Query().Get("limit"); v != "" {
			n, err := strconv.Atoi(v)
			if err != nil || n < 1 {
				w.Header().Set("Content-Type", "application/json")
				w.WriteHeader(http.StatusBadRequest)
				json.NewEncoder(w).Encode(map[string]string{"error": "limit must be a positive integer"})
				return
			}
			if n > 500 {
				n = 500
			}
			limit = n
		}

		// When corrupted=true, use corrupted gem name pools instead of transfigured.
		corrupted := r.URL.Query().Get("corrupted") == "true"
		isTransfigured := r.URL.Query().Get("transfigured") != "false" // default true

		var names []string
		if corrupted {
			// Corrupted gem name autocomplete (for Dedication lab).
			if cache != nil {
				names = cache.For(scope).CorruptedGemNamesSearch(q, isTransfigured, limit)
			}
			if names == nil {
				all, err := repo.CorruptedGemNamesAutocomplete(r.Context(), scope, isTransfigured, limit*10)
				if err != nil {
					slog.Error("gem names autocomplete: corrupted query failed", "error", err, "q", q)
					http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
					return
				}
				// Filter in memory to match cache search behaviour.
				words := strings.Fields(strings.ToLower(q))
				for _, name := range all {
					lower := strings.ToLower(name)
					match := true
					for _, w := range words {
						if !strings.Contains(lower, w) {
							match = false
							break
						}
					}
					if match {
						names = append(names, name)
						if len(names) >= limit {
							break
						}
					}
				}
			}
		} else {
			// Fast path: in-memory search over cached gem names (~200 entries).
			// Falls back to DB query only if cache is empty (cold start).
			if cache != nil {
				names = cache.For(scope).GemNamesSearch(q, limit)
			}
			if names == nil {
				var err error
				names, err = repo.GemNamesAutocomplete(r.Context(), scope, q, limit)
				if err != nil {
					slog.Error("gem names autocomplete: query failed", "error", err, "q", q)
					http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
					return
				}
			}
		}

		if names == nil {
			names = []string{}
		}

		w.Header().Set("Content-Type", "application/json")
		// Cache autocomplete responses briefly.
		w.Header().Set("Cache-Control", "public, max-age=60")
		if err := json.NewEncoder(w).Encode(map[string]any{
			"names": names,
		}); err != nil {
			slog.Error("gem names autocomplete: encode response", "error", err)
		}

	}
}

// GemDictionary returns known gem names from static game data (gem_colors),
// independent of league, prices and market activity.
//
// The desktop OCR matcher consumes this: recognising the gem under the cursor
// must not depend on whether the market prices it. GemNamesAutocomplete stays
// market-scoped — it drives search over gems that actually have analysis data.
//
// Query params: transfigured (default true; "false" returns skill gems).
func GemDictionary(repo *lab.Repository, scope league.Scope) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		transfigured := r.URL.Query().Get("transfigured") != "false" // default true

		names, err := repo.GemNameDictionary(r.Context(), scope, transfigured)
		if err != nil {
			slog.Error("gem dictionary: query failed", "error", err, "transfigured", transfigured)
			http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
			return
		}
		if names == nil {
			names = []string{}
		}

		w.Header().Set("Content-Type", "application/json")
		// Static game data — safe to cache far longer than the market endpoints.
		w.Header().Set("Cache-Control", "public, max-age=3600")
		if err := json.NewEncoder(w).Encode(map[string]any{"names": names}); err != nil {
			slog.Error("gem dictionary: encode response", "error", err)
		}
	}
}
