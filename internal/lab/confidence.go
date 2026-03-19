package lab

import (
	"math"
	"time"
)

// safeIndex retrieves element at index i from s, returning defaultVal when i is
// out of bounds or s is nil. Used for temporal bias slice lookups.
func safeIndex(s []float64, i int, defaultVal float64) float64 {
	if i < 0 || i >= len(s) {
		return defaultVal
	}
	return s[i]
}

// clampFloat64 returns v clamped to [min, max].
func clampFloat64(v, min, max float64) float64 {
	if v < min {
		return min
	}
	if v > max {
		return max
	}
	return v
}

// clampInt returns v clamped to [min, max].
func clampInt(v, min, max int) int {
	if v < min {
		return min
	}
	if v > max {
		return max
	}
	return v
}

// signalBaseConfidence returns the base confidence percentage for a signal type.
// This maps signal types to their inherent trustworthiness as a prediction basis.
// TRAP signals are inherently untrustworthy (low base) while HERD/DUMPING patterns
// are strong directional signals (high base).
func signalBaseConfidence(signal string) float64 {
	switch signal {
	case "HERD", "DUMPING":
		return 65
	case "RISING", "FALLING", "RECOVERY":
		return 40
	case "STABLE":
		return 55
	case "TRAP":
		return 15
	default:
		return 40
	}
}

// windowAgreement measures consistency across short-, medium-, and long-term velocity
// windows. Weights short+medium agreement most heavily, since recent convergence
// is the most actionable signal.
//
// Returns:
//   - 1.4 when all three non-zero windows agree on direction
//   - 1.0 when short and medium agree, or short is absent but med+long agree, or fewer than 2 windows have data
//   - 0.6 when short is present and disagrees with medium (conflicting near-term data)
func windowAgreement(short, med, long float64) float64 {
	signOf := func(v float64) int {
		if v > 0 {
			return 1
		}
		if v < 0 {
			return -1
		}
		return 0
	}

	ss := signOf(short)
	ms := signOf(med)
	ls := signOf(long)

	// Count non-zero signs.
	nonZero := 0
	if ss != 0 {
		nonZero++
	}
	if ms != 0 {
		nonZero++
	}
	if ls != 0 {
		nonZero++
	}

	// Fewer than 2 non-zero velocities: insufficient data for agreement.
	if nonZero < 2 {
		return 1.0
	}

	// All three non-zero and same sign: full agreement.
	if nonZero == 3 && ss == ms && ms == ls {
		return 1.4
	}

	// Short and medium agree (both non-zero, same sign): recent trend consistent.
	if ss != 0 && ms != 0 && ss == ms {
		return 1.0
	}

	// Short is absent (zero) but medium and long agree: consistent medium-term signal.
	// Short-term velocity not yet observed — treat as neutral, not conflicting.
	if ss == 0 && ms != 0 && ls != 0 && ms == ls {
		return 1.0
	}

	// Conflicting: short is present and disagrees with medium direction.
	return 0.6
}

// profileModifier returns a multiplier based on the gem's behavioral history.
// Priority order: unstable (flood/crash) > volatile (high CV) > predictable (low CV + elastic).
func profileModifier(f GemFeature) float64 {
	// Flood or crash history overrides everything -- gem is structurally unstable.
	if f.FloodCount > 2 || f.CrashCount > 2 {
		return 0.7
	}

	// High coefficient of variation = price is unpredictable.
	if f.CV > 50 {
		return 0.8
	}

	// Low CV + negative listing elasticity = predictable price discovery.
	if f.CV <= 30 && f.ListingElasticity < 0 {
		return 1.2
	}

	return 1.0
}

// marketModifier returns a multiplier based on the gem's market structure.
// Thin listings at outlier prices suggest manipulation and reduce confidence.
func marketModifier(f GemFeature) float64 {
	// Outlier-priced gem with very thin listings = manipulation risk.
	if f.RelativePrice > 2.0 && f.RelativeListings < 0.5 {
		return 0.7
	}

	return 1.0
}

// computeConfidence calculates the confidence score (0-100) and phase modifier
// for a gem signal using a five-factor model:
//
//  1. Base confidence from signal type (inherent trustworthiness)
//  2. Temporal alignment (time-of-day and day-of-week bias)
//  3. Cross-window agreement (short/med/long velocity direction)
//  4. Gem profile modifier (behavioral stability)
//  5. Market context modifier (positioning risk)
//
// Modifiers are combined multiplicatively and power-dampened (exponent 0.4) to
// prevent extreme swings while preserving directional impact. The power dampening
// compresses the modifier product toward 1.0: values above 1.0 are reduced and
// values below 1.0 are amplified, keeping confidence in a meaningful range.
func computeConfidence(signal string, f GemFeature, mc MarketContext, snapTime time.Time) (int, float64) {
	// 1. Base confidence from signal type.
	base := signalBaseConfidence(signal)

	// 2. Temporal alignment.
	hour := snapTime.UTC().Hour()
	weekday := int(snapTime.UTC().Weekday())
	hourBias := safeIndex(mc.HourlyBias, hour, 1.0)
	weekBias := safeIndex(mc.WeekdayBias, weekday, 1.0)
	hourVol := safeIndex(mc.HourlyVolatility, hour, 0.0)
	temporal := clampFloat64(hourBias*weekBias, 0.5, 1.5)
	// High volatility at current hour reduces temporal confidence.
	if hourVol > 0.05 {
		temporal *= 0.9
	}
	phaseModifier := temporal

	// 3. Cross-window agreement.
	crossWindow := windowAgreement(f.VelShortPrice, f.VelMedPrice, f.VelLongPrice)

	// 4. Gem profile modifier.
	profile := profileModifier(f)

	// 5. Market context modifier.
	market := marketModifier(f)

	// 6. Combine modifiers with power dampening.
	// Power < 1 compresses the modifier product toward 1.0, preventing
	// extreme amplification while preserving directional impact.
	modProduct := temporal * crossWindow * profile * market
	adjustment := math.Pow(modProduct, 0.4)
	raw := base * adjustment

	return clampInt(int(raw), 0, 100), phaseModifier
}
