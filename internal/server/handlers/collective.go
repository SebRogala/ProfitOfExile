package handlers

import (
	"context"
	"encoding/json"
	"errors"
	"log/slog"
	"net/http"
	"strconv"
	"strings"
	"time"

	"profitofexile/internal/lab"
	"profitofexile/internal/league"
	"profitofexile/internal/price/resolve"
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

// cachedCorruptedSparklines collects cached corrupted series for names, keyed
// by name. The second return reports whether the cache could serve the request
// at all: a cold cache, or a variant the cache never populates, must go to the
// database rather than hand back an empty series.
//
// The cache stores series for the Dedication variants only — lab.DedicationVariants
// is the population query's filter, so it is also the membership test here.
func cachedCorruptedSparklines(cache *lab.Cache, scope league.Scope, names []string, variant string, hours int) (map[string][]lab.SparklinePoint, bool) {
	if cache == nil || !lab.IsDedicationVariant(variant) {
		return nil, false
	}
	c := cache.For(scope)
	// Warmth is the whole corpus, both maps and both Dedication variants: one
	// read fills them together, so once it has run, a variant with no keys has no
	// series and the database has nothing to add. See lab.Cache.HasSparklines.
	if !c.HasSparklines() {
		return nil, false
	}
	out := make(map[string][]lab.SparklinePoint, len(names))
	for _, n := range names {
		if pts := lab.TrimSparkline(c.SparklinesCorrupted(n, variant), hours); len(pts) > 0 {
			out[n] = pts
		}
	}
	return out, true
}

// CollectiveAnalysis returns a ranked "what to farm now" list combining
// transfigure ROI with trend signals, ranked, counted and paged in SQL.
//
// A request carrying pageSize is paged (page, pageSize, sort, variant, color,
// budget, hideLowConfidence, search; see parseCollectivePage). One without it
// is a legacy request with today's params: variant, budget, limit (default 20,
// max 100), sort=pct and search. Released desktop builds send legacy requests.
func CollectiveAnalysis(repo *lab.Repository, cache *lab.Cache, scope league.Scope) http.HandlerFunc {
	return collectiveAnalysis(repo, repo, cache, scope)
}

// collectiveAnalysis takes the Dedication and Normal ranking sources
// separately, so a test can serve a fixed page for either mode.
func collectiveAnalysis(dedication dedicationCollectivePager, src lab.CollectiveSource, cache *lab.Cache, scope league.Scope) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Query().Get("mode") == "dedication" {
			serveDedicationCollective(w, r, dedication, scope)
			return
		}

		var input lab.CollectiveQuery
		var msg string
		if r.URL.Query().Has("pageSize") {
			input, msg = parseCollectivePage(r)
		} else {
			input, msg = parseCollectiveLegacy(r)
		}
		if msg == "" {
			if err := input.Validate(); err != nil {
				msg = err.Error()
			}
		}
		if msg != "" {
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(http.StatusBadRequest)
			json.NewEncoder(w).Encode(map[string]string{"error": msg})
			return
		}

		query, err := lab.QueryCollective(r.Context(), src, cache, scope, input)
		if err != nil {
			var queryErr *lab.QueryError
			if errors.As(err, &queryErr) {
				slog.Error("collective analysis: "+queryErr.Operation+" query failed", "error", queryErr.Err)
			} else {
				slog.Error("collective analysis: query failed", "error", err)
			}
			http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
			return
		}
		rows := assembleCollectiveRows(query)

		body := map[string]any{
			"count": len(rows),
			"total": query.Total,
			"data":  rows,
		}
		if !input.Legacy {
			body["page"] = input.Page
			body["pageSize"] = input.PageSize
		}
		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(body); err != nil {
			slog.Error("collective analysis: encode response", "error", err)
		}
	}
}

// parseCollectiveBudget reads budget as today: absent = 0, else a
// non-negative number. The second return is the 400 message, or "".
func parseCollectiveBudget(r *http.Request) (float64, string) {
	v := r.URL.Query().Get("budget")
	if v == "" {
		return 0, ""
	}
	b, err := strconv.ParseFloat(v, 64)
	if err != nil || b < 0 {
		return 0, "budget must be a non-negative number"
	}
	return b, ""
}

// parseCollectiveLegacy reads a request without pageSize exactly as before
// paging existed. The second return is the 400 message, or "".
func parseCollectiveLegacy(r *http.Request) (lab.CollectiveQuery, string) {
	params := r.URL.Query()
	budget, msg := parseCollectiveBudget(r)
	if msg != "" {
		return lab.CollectiveQuery{}, msg
	}
	limit := 20
	if v := params.Get("limit"); v != "" {
		n, err := strconv.Atoi(v)
		if err != nil || n < 1 {
			return lab.CollectiveQuery{}, "limit must be a positive integer"
		}
		limit = min(n, lab.CollectiveMaxPageSize)
	}
	return lab.CollectiveQuery{
		Variant:      resolve.NormalizeVariant(params.Get("variant")),
		SparkVariant: params.Get("variant"),
		Budget:       budget,
		Search:       params.Get("search"),
		Sort:         lab.LegacyCollectiveSort(params.Get("sort"), budget),
		Page:         1,
		PageSize:     limit,
		Legacy:       true,
	}, ""
}

var collectivePageSorts = map[string]lab.CollectiveSort{
	"":            lab.CollectiveSortPrice,
	"price":       lab.CollectiveSortPrice,
	"roi":         lab.CollectiveSortROI,
	"weightedRoi": lab.CollectiveSortWeightedROI,
	"roiPct":      lab.CollectiveSortROIPct,
}

// parseCollectivePage reads a paged request. Bounds and whitelists beyond the
// syntax are lab.CollectiveQuery.Validate's. The second return is the 400
// message, or "".
func parseCollectivePage(r *http.Request) (lab.CollectiveQuery, string) {
	params := r.URL.Query()
	q := lab.CollectiveQuery{Page: 1}
	n, err := strconv.Atoi(params.Get("pageSize"))
	if err != nil {
		return q, "pageSize must be an integer from 1 to 100"
	}
	q.PageSize = n
	if v := params.Get("page"); v != "" {
		n, err := strconv.Atoi(v)
		if err != nil || n < 1 {
			return q, "page must be a positive integer"
		}
		q.Page = n
	}
	sort, ok := collectivePageSorts[params.Get("sort")]
	if !ok {
		return q, "sort must be one of price, roi, weightedRoi, roiPct"
	}
	q.Sort = sort
	if params.Has("pool") {
		return q, "pool is only valid with mode=dedication"
	}
	q.SparkVariant = params.Get("variant")
	q.Variant = resolve.NormalizeVariant(q.SparkVariant)
	if color := params.Get("color"); color != "ALL" {
		q.Color = color
	}
	budget, msg := parseCollectiveBudget(r)
	if msg != "" {
		return q, msg
	}
	q.Budget = budget
	switch params.Get("hideLowConfidence") {
	case "", "false":
	case "true":
		q.HideLowConfidence = true
	default:
		return q, "hideLowConfidence must be true or false"
	}
	q.Search = params.Get("search")
	return q, ""
}

// dedicationCollectivePager is the Dedication SQL ranker. *lab.Repository
// implements it.
type dedicationCollectivePager interface {
	DedicationCollectivePage(ctx context.Context, scope league.Scope, q lab.DedicationCollectiveQuery) (lab.CollectivePage, error)
}

// parseDedicationLegacy reads a Dedication request without pageSize as before
// paging existed: limit default 100, capped at 500, an unparsable limit keeps
// the default, budget ignored, price order.
func parseDedicationLegacy(r *http.Request, variant string) lab.DedicationCollectiveQuery {
	limit := 100
	if v := r.URL.Query().Get("limit"); v != "" {
		if n, err := strconv.Atoi(v); err == nil && n > 0 {
			limit = min(n, lab.DedicationLegacyMaxLimit)
		}
	}
	return lab.DedicationCollectiveQuery{
		Variant:  variant,
		Search:   r.URL.Query().Get("search"),
		Sort:     lab.CollectiveSortPrice,
		Page:     1,
		PageSize: limit,
		Legacy:   true,
	}
}

// parseDedicationPage reads a paged Dedication request. Bounds and whitelists
// beyond the syntax are lab.DedicationCollectiveQuery.Validate's. The second
// return is the 400 message, or "".
func parseDedicationPage(r *http.Request, variant string) (lab.DedicationCollectiveQuery, string) {
	params := r.URL.Query()
	q := lab.DedicationCollectiveQuery{Variant: variant, Page: 1}
	n, err := strconv.Atoi(params.Get("pageSize"))
	if err != nil {
		return q, "pageSize must be an integer from 1 to 100"
	}
	q.PageSize = n
	if v := params.Get("page"); v != "" {
		n, err := strconv.Atoi(v)
		if err != nil || n < 1 {
			return q, "page must be a positive integer"
		}
		q.Page = n
	}
	sort, ok := collectivePageSorts[params.Get("sort")]
	if !ok {
		return q, "sort must be one of price, roi, weightedRoi, roiPct"
	}
	q.Sort = sort
	q.Pool = params.Get("pool")
	if color := params.Get("color"); color != "ALL" {
		q.Color = color
	}
	budget, msg := parseCollectiveBudget(r)
	if msg != "" {
		return q, msg
	}
	q.Budget = budget
	switch params.Get("hideLowConfidence") {
	case "", "false":
	case "true":
		q.HideLowConfidence = true
	default:
		return q, "hideLowConfidence must be true or false"
	}
	q.Search = params.Get("search")
	return q, ""
}

// serveDedicationCollective handles ?mode=dedication on the collective endpoint:
// the corrupted gems of one Dedication variant (?variant=, default 21/23c),
// ranked, counted and paged in SQL, mapped to the same row shape as the Normal
// collective so the ByVariant/BestPlays components work unchanged.
func serveDedicationCollective(w http.ResponseWriter, r *http.Request, pager dedicationCollectivePager, scope league.Scope) {
	variant, ok := parseDedicationVariant(w, r)
	if !ok {
		return
	}

	var input lab.DedicationCollectiveQuery
	var msg string
	if r.URL.Query().Has("pageSize") {
		input, msg = parseDedicationPage(r, variant)
	} else {
		input = parseDedicationLegacy(r, variant)
	}
	if msg == "" {
		if err := input.Validate(); err != nil {
			msg = err.Error()
		}
	}
	if msg != "" {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusBadRequest)
		json.NewEncoder(w).Encode(map[string]string{"error": msg})
		return
	}

	page, err := pager.DedicationCollectivePage(r.Context(), scope, input)
	if err != nil {
		slog.Error("dedication collective: page query failed", "error", err)
		http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
		return
	}

	rows := make([]collectiveRow, 0, len(page.Rows))
	for _, cr := range page.Rows {
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

	body := map[string]any{
		"count": len(rows),
		"total": page.Total,
		"data":  rows,
	}
	if !input.Legacy {
		body["page"] = input.Page
		body["pageSize"] = input.PageSize
	}
	w.Header().Set("Content-Type", "application/json")
	if err := json.NewEncoder(w).Encode(body); err != nil {
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

		variant = resolve.NormalizeVariant(r.URL.Query().Get("variant"))

		// Fast path: serve from cache.
		var transfigure []lab.TransfigureResult
		var signals []lab.GemSignal
		var features []lab.GemFeature

		// All three corpora, all three warmth flags — see CollectiveAnalysis. This
		// is the desktop's per-gem path: the in-game scan calls it once per gem, so
		// a length-derived cold read costs three queries per gem per scan.
		if cache != nil {
			ct, transfigureWarm := cache.For(scope).Transfigure()
			cs, signalsWarm := cache.For(scope).GemSignals()
			cf, featuresWarm := cache.For(scope).GemFeatures()
			if transfigureWarm && signalsWarm && featuresWarm {
				transfigure = lab.FilterTransfigure(ct, variant, 1000)
				signals = lab.FilterGemSignals(cs, variant, "", 5000)
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
			sparklines = lab.CachedSparklines(cache, scope, names, variant, lab.SparklineWindowHours)
		} else {
			sp, err := repo.SparklineData(r.Context(), scope, names, variant, lab.SparklineWindowHours)
			if err != nil {
				slog.Error("compare analysis: sparkline query failed", "error", err)
				sp = make(map[string][]lab.SparklinePoint)
				warnings = append(warnings, "Sparkline data temporarily unavailable")
			}
			sparklines = sp
		}
		spMs = time.Since(spStart).Milliseconds()

		// Sparkline normalization removed — temporal coefficients create edge artifacts

		// Double-corruption EV for the tiebreaker (POE-125). Only for the input
		// variants the calculator models — every other variant gets the
		// pre-POE-125 comparison unchanged, with no query paid for it.
		//
		// A failure here degrades the comparison rather than failing it: the EV
		// is an enhancement on top of a complete answer, so the request carries
		// a warning and drops the tiebreaker.
		var doubleCorrupt map[string]lab.DoubleCorruptResult
		if lab.IsDoubleCorruptVariant(variant) {
			if cache != nil && cache.For(scope).HasDoubleCorrupt() {
				doubleCorrupt = lab.SelectDoubleCorruptByNames(
					cache.For(scope).DoubleCorruptByVariant(variant), names)
			} else {
				dcResults, err := repo.DoubleCorruptResultsByNames(r.Context(), scope, variant, names)
				if err != nil {
					slog.Error("compare analysis: double corrupt query failed", "error", err)
					warnings = append(warnings, "Double-corruption data temporarily unavailable")
				} else {
					doubleCorrupt = lab.SelectDoubleCorruptByNames(dcResults, names)
				}
			}
		}

		results := lab.BuildCompareResults(names, transfigure, signals, features, sparklines, variant, doubleCorrupt)

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
// Queries one variant's corrupted gem prices (?variant=, default 21/23c), loads
// the matching Dedication analysis for input costs, and auto-detects pool
// (skill vs transfigured) from gem names.
func serveDedicationCompare(w http.ResponseWriter, r *http.Request, repo *lab.Repository, cache *lab.Cache, scope league.Scope, names []string) {
	variant, ok := parseDedicationVariant(w, r)
	if !ok {
		return
	}

	// Both the prices and the input costs come out of the same cached corpus, so
	// this is one warmth decision for the whole handler. The corpus carries every
	// corrupted gem of the variant, not just the rankable ones, so the names this
	// path answers for that the font can never hand out — a Vaal gem — still find
	// their price and are still marked as not an outcome.
	var gemPrices []lab.GemPrice
	var dedicationResults []lab.DedicationResult
	warm := cache != nil && cache.For(scope).HasDedication()

	if warm {
		c := cache.For(scope)
		gemPrices = lab.SelectGemPricesByNames(c.CorruptedGemPrices(variant), names)
		ded, _ := c.Dedication()
		dedicationResults = append(dedicationResults, lab.FilterDedicationVariant(ded.Skills, variant)...)
		dedicationResults = append(dedicationResults, lab.FilterDedicationVariant(ded.Transfigured, variant)...)
	} else {
		// Cold cache: the three reads the corpus replaces.
		var err error
		gemPrices, err = repo.CorruptedGemPricesByNames(r.Context(), scope, names, variant)
		if err != nil {
			slog.Error("compare analysis (dedication): corrupted gem prices query failed", "error", err)
			http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
			return
		}
		skills, err := repo.LatestDedicationResults(r.Context(), scope, variant, "skill", "", 100)
		if err != nil {
			slog.Error("compare analysis (dedication): dedication skills query failed", "error", err)
			http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
			return
		}
		trans, err := repo.LatestDedicationResults(r.Context(), scope, variant, "transfigured", "", 100)
		if err != nil {
			slog.Error("compare analysis (dedication): dedication transfigured query failed", "error", err)
			http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
			return
		}
		dedicationResults = append(skills, trans...)
	}

	// Load sparkline data for corrupted gems (last 12 hours).
	// The cache populates corrupted series for the Dedication variants only, so
	// the read is guarded on it: any other variant has no cached series and must
	// go to the query rather than read an empty series out of a warm cache.
	var warnings []string
	sparklines, cached := cachedCorruptedSparklines(cache, scope, names, variant, lab.SparklineWindowHours)
	if !cached {
		sp, err := repo.SparklineDataCorrupted(r.Context(), scope, names, variant, lab.SparklineWindowHours)
		if err != nil {
			slog.Error("compare analysis (dedication): sparkline query failed", "error", err)
			sp = make(map[string][]lab.SparklinePoint)
			warnings = append(warnings, "Sparkline data temporarily unavailable")
		}
		sparklines = sp
	}

	results := lab.BuildDedicationCompareResults(names, gemPrices, dedicationResults, sparklines, variant)

	rows := buildCompareRows(results, nil, scope) // no trade enrichment for Dedication mode

	resp := map[string]any{
		"count":   len(rows),
		"data":    rows,
		"mode":    "dedication",
		"variant": variant,
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
	TransfiguredName     string               `json:"transfiguredName"`
	BaseName             string               `json:"baseName"`
	Variant              string               `json:"variant"`
	GemColor             string               `json:"gemColor"`
	ROI                  float64              `json:"roi"`
	ROIPct               float64              `json:"roiPct"`
	BasePrice            float64              `json:"basePrice"`
	TransfiguredPrice    float64              `json:"transfiguredPrice"`
	Confidence           string               `json:"confidence"`
	Signal               string               `json:"signal"`
	CV                   float64              `json:"cv"`
	PriceVelocity        float64              `json:"priceVelocity"`
	ListingVelocity      float64              `json:"listingVelocity"`
	HistPosition         float64              `json:"histPosition"`
	Sparkline            []lab.SparklinePoint `json:"sparkline"`
	Recommendation       string               `json:"recommendation"`
	SellUrgency          string               `json:"sellUrgency"`
	SellReason           string               `json:"sellReason"`
	Sellability          int                  `json:"sellability"`
	SellabilityLabel     string               `json:"sellabilityLabel"`
	PriceTier            string               `json:"priceTier"`
	TierAction           string               `json:"tierAction"`
	WindowSignal         string               `json:"windowSignal"`
	BaseListings         int                  `json:"baseListings"`
	LiquidityTier        string               `json:"liquidityTier"`
	TransListings        int                  `json:"transListings"`
	TransfiguredListings int                  `json:"transfiguredListings"`
	WeightedROI          float64              `json:"weightedRoi"`
	WeightedROIPct       float64              `json:"weightedRoiPct"`
	Low7Days             float64              `json:"low7d"`
	High7Days            float64              `json:"high7d"`
	SellConfidence       string               `json:"sellConfidence"`
	SellConfidenceReason string               `json:"sellConfidenceReason"`
	QuickSellPrice       float64              `json:"quickSellPrice"`
	RiskAdjustedPrice    float64              `json:"riskAdjustedPrice"`
	// Double-corruption (POE-125). DoubleCorruptModel is the estimated-model
	// marker — the odds behind these numbers come from community documentation,
	// not from GGG, and the UI must badge them rather than present them as
	// confirmed. DoubleCorruptTiebreak marks the one recommendation decided by
	// double-corrupt profit instead of by the Font score.
	// DoubleCorruptPricedProbability is the share of the outcome distribution the
	// EV is computed over; the rest is mass the corrupted market does not price.
	DoubleCorruptEV                float64                  `json:"doubleCorruptEv,omitempty"`
	DoubleCorruptProfit            float64                  `json:"doubleCorruptProfit,omitempty"`
	DoubleCorruptModel             string                   `json:"doubleCorruptModel,omitempty"`
	DoubleCorruptTiebreak          bool                     `json:"doubleCorruptTiebreak,omitempty"`
	DoubleCorruptPricedProbability float64                  `json:"doubleCorruptPricedProbability,omitempty"`
	Trade                          *trade.TradeLookupResult `json:"trade,omitempty"`
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

			DoubleCorruptEV:                cr.DoubleCorruptEV,
			DoubleCorruptProfit:            cr.DoubleCorruptProfit,
			DoubleCorruptModel:             cr.DoubleCorruptModel,
			DoubleCorruptTiebreak:          cr.DoubleCorruptTiebreak,
			DoubleCorruptPricedProbability: cr.DoubleCorruptPricedProbability,
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

		// Warmth comes from the cache, never from the size of the result. A query
		// that legitimately matches nothing is an answer, and re-asking the
		// database for it turned every mistyped character into a DISTINCT ...
		// ILIKE over gem_snapshots (POE-152).
		var names []string
		cacheWarm := false
		if corrupted {
			// Corrupted gem name autocomplete (for Dedication lab).
			if cache != nil {
				names, cacheWarm = cache.For(scope).CorruptedGemNamesSearch(q, isTransfigured, limit)
			}
			if !cacheWarm {
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
				names, cacheWarm = cache.For(scope).GemNamesSearch(q, limit)
			}
			if !cacheWarm {
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

// GemDictionary returns known gem names for OCR matching: gem_colors unioned
// with the names this league's market has shown.
//
// The desktop OCR matcher consumes this: recognising the gem under the cursor
// must not depend on whether the market prices it *now*. GemNamesAutocomplete
// stays market-scoped — it drives search over gems that actually have analysis
// data.
//
// It is served from the cache because it sits on the critical path of an in-game
// feature: the desktop blocks on it before the gem scan loop, and POE-149
// measured this endpoint at 0.144 s → 3.0 s under DB contention, which lands as
// OCR that does not start. Its HTTP Cache-Control does nothing for that caller —
// the desktop fetches with reqwest, which has no HTTP cache.
//
// Query params: transfigured (default true; "false" returns skill gems).
func GemDictionary(repo *lab.Repository, cache *lab.Cache, scope league.Scope) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		transfigured := r.URL.Query().Get("transfigured") != "false" // default true

		// Warmth comes from the cache, never from the size of the list: an empty
		// dictionary is a legitimate answer for a league whose snapshots and
		// gem_colors are both empty, and reading it as cold would query twice on
		// every request forever.
		var names []string
		warm := false
		if cache != nil {
			names, warm = cache.For(scope).GemDictionary(transfigured)
		}
		if !warm {
			var err error
			names, err = repo.GemNameDictionary(r.Context(), scope, transfigured)
			if err != nil {
				slog.Error("gem dictionary: query failed", "error", err, "transfigured", transfigured)
				http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
				return
			}
		}
		if names == nil {
			names = []string{}
		}

		w.Header().Set("Content-Type", "application/json")
		// The dictionary only ever grows, so an hour-stale copy is missing names
		// rather than wrong about them. Inert for the desktop, which has no HTTP
		// cache; kept for any browser or proxy in front of this.
		w.Header().Set("Cache-Control", "public, max-age=3600")
		if err := json.NewEncoder(w).Encode(map[string]any{"names": names}); err != nil {
			slog.Error("gem dictionary: encode response", "error", err)
		}
	}
}
