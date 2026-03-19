package lab

import "time"

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
}

// PriceP50 returns the P50 price percentile, or 0 if not available.
// Centralizes the key string to avoid typos at access sites.
func (mc MarketContext) PriceP50() float64 {
	if mc.PricePercentiles == nil {
		return 0
	}
	return mc.PricePercentiles["P50"]
}

// TierBoundaries holds minimum chaos price thresholds for each tier.
// A gem is TOP if chaos >= Top, HIGH if chaos >= High, MID if chaos >= Mid, otherwise LOW.
type TierBoundaries struct {
	Top  float64 `json:"top"`
	High float64 `json:"high"`
	Mid  float64 `json:"mid"`
}

// GemFeature holds pre-computed per-gem metrics for a single snapshot.
type GemFeature struct {
	Time              time.Time
	Name              string
	Variant           string
	Chaos             float64
	Listings          int
	Tier              string
	VelShortPrice     float64
	VelShortListing   float64
	VelMedPrice       float64
	VelMedListing     float64
	VelLongPrice      float64
	VelLongListing    float64
	CV                float64
	HistPosition      float64
	High7d            float64
	Low7d             float64
	FloodCount        int
	CrashCount        int
	ListingElasticity float64
	RelativePrice     float64
	RelativeListings  float64
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
	Recommendation   string
	Tier             string
}
