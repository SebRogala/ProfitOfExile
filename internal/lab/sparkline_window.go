package lab

import (
	"time"

	"profitofexile/internal/league"
)

// TrimSparkline returns the suffix of pts newer than hours ago. Points are
// stored ascending by time, so the first point inside the window starts the
// suffix. hours <= 0 means "no trim" (the caller serves the full cached series).
func TrimSparkline(pts []SparklinePoint, hours int) []SparklinePoint {
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

// CachedSparklines collects the cached non-corrupted series for names at a
// single variant, keyed by name so the result is a drop-in for what
// Repository.SparklineData returns. Names with no cached points are omitted:
// on a warm cache that means the gem genuinely has no recent points, which is
// the same empty series the query would have produced.
func CachedSparklines(cache *Cache, scope league.Scope, names []string, variant string, hours int) map[string][]SparklinePoint {
	c := cache.For(scope)
	out := make(map[string][]SparklinePoint, len(names))
	for _, n := range names {
		if pts := TrimSparkline(c.Sparklines(n, variant), hours); len(pts) > 0 {
			out[n] = pts
		}
	}
	return out
}
