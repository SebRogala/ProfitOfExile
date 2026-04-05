package lab

import (
	"math"
	"time"
)

// UncertainTooltip explains why UNCERTAIN replaces directional predictions
// (RISING/FALLING) — the Baca's dog rule: below 50% accuracy is no better
// than random. Raw market data (velocity, CV, listings) is still available
// for the user's own assessment.
const UncertainTooltip = "Directional prediction accuracy is below 50% (coin flip) — showing raw market data instead. Price velocity and listing trends are available for your own assessment."

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
// UNCERTAIN signals get a low base because directional prediction accuracy is
// below coin flip (50%).
func signalBaseConfidence(signal string) float64 {
	switch signal {
	case "HERD", "DUMPING", "DEMAND":
		return 65
	case "UNCERTAIN", "RECOVERY":
		return 40
	case "STABLE", "CAUTION":
		return 55
	default:
		return 40
	}
}

// windowAgreement measures consistency across short-, medium-, and long-term velocity
// windows. Weights short+medium agreement most heavily, since recent convergence
// is the most actionable signal.
//
// The signal parameter adjusts behavior for STABLE signals: when all windows agree
// on a tiny drift direction, this is noise (noisy-stable), not a strong confidence
// signal. STABLE gets a penalty (0.8) instead of the usual boost (1.4).
//
// Returns:
//   - 1.4 when all three non-zero windows agree on direction (non-STABLE signals)
//   - 0.8 when all three non-zero windows agree on direction (STABLE signal — noisy-stable penalty)
//   - 1.0 when short and medium agree, or short is absent but med+long agree, or fewer than 2 windows have data
//   - 0.6 when short is present and disagrees with medium (conflicting near-term data)
func windowAgreement(short, med, long float64, signal string) float64 {
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
		// For STABLE signals, consistent near-zero velocity across all windows
		// is noise, not a confidence signal. Penalize instead of boosting.
		if signal == "STABLE" {
			return 0.8
		}
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
//
// The signal parameter adjusts behavior for DUMPING signals: crash/flood history
// is corroborating evidence for DUMPING (a gem that has crashed before is more
// likely to keep dumping), so it returns an amplifier (1.2) instead of a reducer (0.7).
func profileModifier(f GemFeature, signal string) float64 {
	// Flood or crash history.
	if f.FloodCount > 2 || f.CrashCount > 2 {
		// For DUMPING signals, crash/flood history is corroborating evidence —
		// a gem that has crashed before is MORE likely to keep dumping.
		if signal == "DUMPING" {
			return 1.2
		}
		// For other signals, instability reduces confidence in predictions.
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

	// 3. Cross-window agreement (signal-aware: STABLE penalized for agreement).
	crossWindow := windowAgreement(f.VelShortPrice, f.VelMedPrice, f.VelLongPrice, signal)

	// 4. Gem profile modifier (signal-aware: DUMPING amplified by crash history).
	profile := profileModifier(f, signal)

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
