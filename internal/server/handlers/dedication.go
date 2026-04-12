package handlers

import (
	"encoding/json"
	"log/slog"
	"net/http"
	"sort"
	"time"

	"profitofexile/internal/lab"
)

// DedicationAnalysis returns pre-computed Dedication lab EV for corrupted 21/23 gems.
// Response splits results into skills (non-transfigured) and transfigured pools,
// each with safe/premium/jackpot modes. Includes entryFee from offering timing cache.
// GET /api/analysis/dedication
func DedicationAnalysis(repo *lab.Repository, cache *lab.Cache) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		limit, ok := parseLimit(w, r, 50, 500)
		if !ok {
			return
		}

		var skillResults, transfiguredResults []lab.DedicationResult
		cacheHit := false

		// Fast path: serve from cache.
		if cache != nil {
			analysis := cache.Dedication()
			if len(analysis.Skills) > 0 || len(analysis.Transfigured) > 0 {
				skillResults = analysis.Skills
				transfiguredResults = analysis.Transfigured
				cacheHit = true
			}
		}

		// Slow path: fall back to DB query.
		if !cacheHit {
			var err error
			skillResults, err = repo.LatestDedicationResults(r.Context(), "skill", "", limit)
			if err != nil {
				slog.Error("dedication analysis: query skills failed", "error", err)
				http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
				return
			}
			transfiguredResults, err = repo.LatestDedicationResults(r.Context(), "transfigured", "", limit)
			if err != nil {
				slog.Error("dedication analysis: query transfigured failed", "error", err)
				http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
				return
			}
		}

		// Split by mode.
		splitByMode := func(results []lab.DedicationResult) (safe, premium, jackpot []dedicationRow) {
			for _, dr := range results {
				row := toDedicationRow(dr)
				switch dr.Mode {
				case "safe":
					safe = append(safe, row)
				case "premium":
					premium = append(premium, row)
				case "jackpot":
					jackpot = append(jackpot, row)
				}
			}
			sortDedicationRows(safe)
			sortDedicationRows(premium)
			sortDedicationRows(jackpot)
			if len(safe) > limit {
				safe = safe[:limit]
			}
			if len(premium) > limit {
				premium = premium[:limit]
			}
			if len(jackpot) > limit {
				jackpot = jackpot[:limit]
			}
			return
		}

		skillSafe, skillPremium, skillJackpot := splitByMode(skillResults)
		transSafe, transPremium, transJackpot := splitByMode(transfiguredResults)

		// Extract entry fee from offering timing cache.
		var entryFee float64
		if cache != nil {
			if offeringJSON := cache.OfferingTiming(); len(offeringJSON) > 0 {
				var offerings []struct {
					Name         string  `json:"name"`
					CurrentPrice float64 `json:"currentPrice"`
				}
				if err := json.Unmarshal(offeringJSON, &offerings); err == nil {
					for _, o := range offerings {
						if o.Name == "Dedication to the Goddess" {
							entryFee = o.CurrentPrice
							break
						}
					}
				}
			}
		}

		resp := struct {
			Skills struct {
				Safe    []dedicationRow `json:"safe"`
				Premium []dedicationRow `json:"premium"`
				Jackpot []dedicationRow `json:"jackpot"`
			} `json:"skills"`
			Transfigured struct {
				Safe    []dedicationRow `json:"safe"`
				Premium []dedicationRow `json:"premium"`
				Jackpot []dedicationRow `json:"jackpot"`
			} `json:"transfigured"`
			EntryFee float64 `json:"entryFee"`
		}{
			EntryFee: entryFee,
		}

		resp.Skills.Safe = nonNil(skillSafe)
		resp.Skills.Premium = nonNil(skillPremium)
		resp.Skills.Jackpot = nonNil(skillJackpot)
		resp.Transfigured.Safe = nonNil(transSafe)
		resp.Transfigured.Premium = nonNil(transPremium)
		resp.Transfigured.Jackpot = nonNil(transJackpot)

		w.Header().Set("Content-Type", "application/json")
		w.Header().Set("Cache-Control", "public, max-age=30")
		if err := json.NewEncoder(w).Encode(resp); err != nil {
			slog.Error("dedication analysis: encode response", "error", err)
		}
	}
}

// dedicationRow is the JSON response shape for a single Dedication result.
type dedicationRow struct {
	Time              string               `json:"time"`
	Color             string               `json:"color"`
	GemType           string               `json:"gemType"`
	Pool              int                  `json:"pool"`
	Winners           int                  `json:"winners"`
	PWin              float64              `json:"pWin"`
	AvgWinRaw         float64              `json:"avgWinRaw"`
	EVRaw             float64              `json:"evRaw"`
	InputCost         float64              `json:"inputCost"`
	Profit            float64              `json:"profit"`
	FontsToHit        float64              `json:"fontsToHit"`
	JackpotGems       []lab.JackpotGemInfo `json:"jackpotGems,omitempty"`
	ThinPoolGems      int                  `json:"thinPoolGems"`
	LiquidityRisk     string               `json:"liquidityRisk"`
	PoolBreakdown     []lab.TierPoolInfo   `json:"poolBreakdown,omitempty"`
}

func toDedicationRow(dr lab.DedicationResult) dedicationRow {
	return dedicationRow{
		Time:          dr.Time.UTC().Format(time.RFC3339),
		Color:         dr.Color,
		GemType:       dr.GemType,
		Pool:          dr.Pool,
		Winners:       dr.Winners,
		PWin:          dr.PWin,
		AvgWinRaw:     dr.AvgWinRaw,
		EVRaw:         dr.EVRaw,
		InputCost:     dr.InputCost,
		Profit:        dr.Profit,
		FontsToHit:    dr.FontsToHit,
		JackpotGems:   dr.JackpotGems,
		ThinPoolGems:  dr.ThinPoolGems,
		LiquidityRisk: dr.LiquidityRisk,
		PoolBreakdown: dr.PoolBreakdown,
	}
}

func sortDedicationRows(rows []dedicationRow) {
	sort.Slice(rows, func(i, j int) bool {
		return rows[i].Profit > rows[j].Profit
	})
}

// nonNil ensures a nil slice is serialized as [] instead of null.
func nonNil(rows []dedicationRow) []dedicationRow {
	if rows == nil {
		return []dedicationRow{}
	}
	return rows
}
