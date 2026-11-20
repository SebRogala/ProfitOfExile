package lab

import (
	"testing"

	"profitofexile/internal/league"
)

// A flight shared across a different league, cache, SparkVariant, window or
// key set would silently serve one request another's series as its own
// sparklines.
func TestKeepCriticalSparklineFallbackFlightKey_EqualRequestsShareAndDistinctOnesDoNot(t *testing.T) {
	scope := league.Historical("Mirage")
	cache := NewCache(scope).For(scope)
	keys := []CollectiveSparklineKey{{"Spark of Nova", "20/20"}, {"Arc of Surging", "1"}}
	base := sparklineFallbackFlightKey(cache, scope, "", keys, SparklineWindowHours)

	t.Run("canonical keys share", func(t *testing.T) {
		reordered := []CollectiveSparklineKey{{"Arc of Surging", "1"}, {"Spark of Nova", "20/20"}, {"Arc of Surging", "1"}}
		if got := sparklineFallbackFlightKey(cache, scope, "", reordered, SparklineWindowHours); got != base {
			t.Errorf("reordered, duplicated keys give flight %q, want the canonical %q", got, base)
		}
	})
	other := league.Historical("Settlers")
	for name, got := range map[string]string{
		"league":       sparklineFallbackFlightKey(cache, other, "", keys, SparklineWindowHours),
		"cache":        sparklineFallbackFlightKey(NewCache(scope).For(scope), scope, "", keys, SparklineWindowHours),
		"SparkVariant": sparklineFallbackFlightKey(cache, scope, "20/20", keys, SparklineWindowHours),
		"window":       sparklineFallbackFlightKey(cache, scope, "", keys, 24),
		"keys":         sparklineFallbackFlightKey(cache, scope, "", keys[:1], SparklineWindowHours),
		"variant":      sparklineFallbackFlightKey(cache, scope, "", []CollectiveSparklineKey{{"Spark of Nova", "20"}, {"Arc of Surging", "1"}}, SparklineWindowHours),
	} {
		t.Run("different "+name+" does not share", func(t *testing.T) {
			if got == base {
				t.Errorf("a different %s shares the flight %q", name, base)
			}
		})
	}
}
