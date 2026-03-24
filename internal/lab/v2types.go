package lab

import (
	"fmt"
	"time"
)

// MarketContext holds pre-computed market-wide statistics for a single snapshot.
type MarketContext struct {
	Time               time.Time
	PricePercentiles   map[string]float64 // P10, P25, P50, P75, P90, P99
	ListingPercentiles map[string]float64 // P10, P25, P50, P75, P90, P99
	VelocityMean       float64
	VelocitySigma      float64
	ListingVelMean     float64
	ListingVelSigma    float64
	TotalGems          int
	TotalListings      int
	TierBoundaries     TierBoundaries
	HourlyBias         []float64 // 24 entries, one per UTC hour
	HourlyVolatility   []float64 // 24 entries — σ of price changes per hour
	HourlyActivity     []float64 // 24 entries — ratio of moving gems per hour (0.0-1.0)
	WeekdayBias        []float64 // 7 entries, Sun=0..Sat=6 (matches time.Weekday)
	WeekdayVolatility  []float64 // 7 entries — σ of price changes per weekday
	WeekdayActivity    []float64 // 7 entries — ratio of moving gems per weekday (0.0-1.0)

	// Temporal normalization fields (POE-68).
	TemporalCoefficient float64 // coefficient for this snapshot's time; 1.0 = no adjustment
	TemporalMode        string  // "none", "hourly", "weekday_hour"
	TemporalBuckets     []byte  // raw JSONB, keyed by variant

	// Per-variant baselines for scoring calibration.
	VariantStats map[string]VariantBaseline `json:"variant_stats,omitempty"`
}

// VariantBaseline holds per-variant market statistics used to calibrate
// scoring functions (e.g., sigmoid center for sell probability).
type VariantBaseline struct {
	MedianListings float64        `json:"median_listings"`
	MedianCV       float64        `json:"median_cv"`
	MedianPrice    float64        `json:"median_price"`
	GemCount       int            `json:"gem_count"`
	Tiers          TierBoundaries `json:"tiers"`
}

// ValidateTemporalSlices checks that all temporal slices have the expected lengths:
// 24 for hourly fields and 7 for weekday fields. Returns an error describing the
// first violation found, or nil when all lengths are correct.
func (mc MarketContext) ValidateTemporalSlices() error {
	if len(mc.HourlyBias) != 24 {
		return fmt.Errorf("MarketContext: HourlyBias has %d elements, want 24", len(mc.HourlyBias))
	}
	if len(mc.HourlyVolatility) != 24 {
		return fmt.Errorf("MarketContext: HourlyVolatility has %d elements, want 24", len(mc.HourlyVolatility))
	}
	if len(mc.HourlyActivity) != 24 {
		return fmt.Errorf("MarketContext: HourlyActivity has %d elements, want 24", len(mc.HourlyActivity))
	}
	if len(mc.WeekdayBias) != 7 {
		return fmt.Errorf("MarketContext: WeekdayBias has %d elements, want 7", len(mc.WeekdayBias))
	}
	if len(mc.WeekdayVolatility) != 7 {
		return fmt.Errorf("MarketContext: WeekdayVolatility has %d elements, want 7", len(mc.WeekdayVolatility))
	}
	if len(mc.WeekdayActivity) != 7 {
		return fmt.Errorf("MarketContext: WeekdayActivity has %d elements, want 7", len(mc.WeekdayActivity))
	}
	return nil
}

// PriceP50 returns the P50 price percentile, or 0 if not available.
// Centralizes the key string to avoid typos at access sites.
func (mc MarketContext) PriceP50() float64 {
	if mc.PricePercentiles == nil {
		return 0
	}
	return mc.PricePercentiles["P50"]
}

// TierBoundaries holds dynamic tier boundary thresholds produced by recursive
// average splitting. Boundaries are sorted descending: boundaries[0] = TOP
// threshold, boundaries[1] = HIGH threshold, etc.
type TierBoundaries struct {
	Boundaries []float64 `json:"boundaries"`
	Names      []string  `json:"names,omitempty"` // if nil, use global TierNames
}

// GemFeature holds pre-computed per-gem metrics for a single snapshot.
type GemFeature struct {
	Time              time.Time
	Name              string
	Variant           string
	Chaos             float64
	Listings          int
	Tier              string // per-variant tier (for Font Comparator)
	GlobalTier        string // cross-variant tier (for BestPlays "all" view)
	VelShortPrice     float64
	VelShortListing   float64
	VelMedPrice       float64
	VelMedListing     float64
	VelLongPrice      float64
	VelLongListing    float64
	CV                float64
	CVShort           float64 // 6h coefficient of variation (for stability discount)
	HistPosition      float64
	High7Days            float64
	Low7Days             float64
	FloodCount        int
	CrashCount        int
	ListingElasticity     float64
	RelativePrice         float64
	RelativeListings      float64
	MarketDepth           float64 // listings / VariantBaseline.MedianListings (per-variant, league-invariant)
	MarketRegime          string  // "TEMPORAL" (depth >= 0.4) or "CASCADE" (depth < 0.4)
	SellProbabilityFactor float64 // 0.3-1.0, calibrated from listing count
	StabilityDiscount     float64 // 0.7-1.0, from CVShort (6h)
}

// GemSignal holds the computed signal and confidence for a single gem at a snapshot.
type GemSignal struct {
	Time             time.Time
	Name             string
	Variant          string
	Signal           string
	Confidence       int
	SellUrgency      string
	SellReason       string
	Sellability      int
	SellabilityLabel string
	WindowSignal     string
	AdvancedSignal   string
	PhaseModifier    float64
	Recommendation    string
	Tier              string
	RiskAdjustedValue float64 // price * sell_probability * stability_discount
	QuickSellPrice    float64 // aggressive undercut estimate
	SellConfidence    string  // "SAFE", "FAIR", "RISKY"
}
