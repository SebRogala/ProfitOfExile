package lab

import "time"

// velocityWindow computes rate of change per hour using points within
// the given duration from the most recent point. Time-based, not point-count-based --
// works identically regardless of data cadence (30min, 5min, etc.).
func velocityWindow(points []PricePoint, window time.Duration, extract func(PricePoint) float64) float64 {
	n := len(points)
	if n < 2 {
		return 0
	}

	cutoff := points[n-1].Time.Add(-window)

	// Find first point where Time >= cutoff.
	start := n
	for i := 0; i < n; i++ {
		if !points[i].Time.Before(cutoff) {
			start = i
			break
		}
	}

	// Need at least 2 points in window.
	if n-start < 2 {
		return 0
	}

	first := points[start]
	last := points[n-1]

	hours := last.Time.Sub(first.Time).Hours()
	if hours <= 0 {
		return 0
	}

	return sanitizeFloat((extract(last) - extract(first)) / hours)
}

// velocity computes the rate of change per hour using a 2-hour window.
// Delegates to the time-based velocityWindow.
func velocity(points []PricePoint, extract func(PricePoint) float64) float64 {
	return velocityWindow(points, 2*time.Hour, extract)
}

// VelocityWindow is the exported variant for use by the optimizer.
func VelocityWindow(points []PricePoint, window time.Duration, extract func(PricePoint) float64) float64 {
	return velocityWindow(points, window, extract)
}
