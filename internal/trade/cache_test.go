package trade

import (
	"fmt"
	"sync"
	"testing"
	"time"

	"profitofexile/internal/league"
)

func makeResult(gem, variant string) *TradeLookupResult {
	return &TradeLookupResult{
		Gem:       gem,
		Variant:   variant,
		Total:     42,
		FetchedAt: time.Now(),
	}
}

func TestCache_SetGet(t *testing.T) {
	c := NewTradeCache(10, league.Historical("Mirage"))
	r := makeResult("Empower Support", "4/20")

	c.Set("empower|4/20", r)
	got, ok := c.Get("empower|4/20")

	if !ok {
		t.Fatal("expected cache hit, got miss")
	}
	if got.Gem != "Empower Support" {
		t.Errorf("Gem = %q, want %q", got.Gem, "Empower Support")
	}
	if got.Variant != "4/20" {
		t.Errorf("Variant = %q, want %q", got.Variant, "4/20")
	}
	if got.Total != 42 {
		t.Errorf("Total = %d, want 42", got.Total)
	}
}

func TestCache_Miss(t *testing.T) {
	c := NewTradeCache(10, league.Historical("Mirage"))

	got, ok := c.Get("nonexistent")
	if ok {
		t.Fatal("expected cache miss, got hit")
	}
	if got != nil {
		t.Errorf("expected nil result on miss, got %v", got)
	}
}

func TestCache_Eviction(t *testing.T) {
	c := NewTradeCache(3, league.Historical("Mirage"))

	c.Set("a", makeResult("a", "1"))
	c.Set("b", makeResult("b", "1"))
	c.Set("c", makeResult("c", "1"))
	// Cache is full. Inserting "d" should evict "a" (oldest).
	c.Set("d", makeResult("d", "1"))

	if _, ok := c.Get("a"); ok {
		t.Error("expected 'a' to be evicted, but it was found")
	}
	for _, key := range []string{"b", "c", "d"} {
		if _, ok := c.Get(key); !ok {
			t.Errorf("expected %q to be present, got miss", key)
		}
	}
	if c.Len() != 3 {
		t.Errorf("Len() = %d, want 3", c.Len())
	}
}

func TestCache_LRUPromotion(t *testing.T) {
	c := NewTradeCache(3, league.Historical("Mirage"))

	c.Set("a", makeResult("a", "1"))
	c.Set("b", makeResult("b", "1"))
	c.Set("c", makeResult("c", "1"))

	// Access "a" to promote it — now order is [b, c, a].
	c.Get("a")

	// Insert "d" — should evict "b" (now the oldest), not "a".
	c.Set("d", makeResult("d", "1"))

	if _, ok := c.Get("b"); ok {
		t.Error("expected 'b' to be evicted after 'a' was promoted, but it was found")
	}
	if _, ok := c.Get("a"); !ok {
		t.Error("expected 'a' to survive after promotion, got miss")
	}
	for _, key := range []string{"c", "d"} {
		if _, ok := c.Get(key); !ok {
			t.Errorf("expected %q to be present, got miss", key)
		}
	}
}

func TestCache_Update(t *testing.T) {
	c := NewTradeCache(10, league.Historical("Mirage"))

	c.Set("k", makeResult("old", "1"))
	c.Set("k", makeResult("new", "1"))

	got, ok := c.Get("k")
	if !ok {
		t.Fatal("expected cache hit after update, got miss")
	}
	if got.Gem != "new" {
		t.Errorf("Gem = %q, want %q (latest value)", got.Gem, "new")
	}
	if c.Len() != 1 {
		t.Errorf("Len() = %d, want 1 (no duplicate entries)", c.Len())
	}
}

func TestCache_Delete(t *testing.T) {
	c := NewTradeCache(10, league.Historical("Mirage"))

	c.Set("k", makeResult("x", "1"))
	c.Delete("k")

	if _, ok := c.Get("k"); ok {
		t.Error("expected miss after delete, got hit")
	}
	if c.Len() != 0 {
		t.Errorf("Len() = %d, want 0 after delete", c.Len())
	}

	// Delete of nonexistent key should be a no-op.
	c.Delete("nonexistent")
}

func TestCache_GetSnapshot(t *testing.T) {
	c := NewTradeCache(10, league.Historical("Mirage"))

	r1 := makeResult("Spark of Nova", "20/20")
	r2 := makeResult("Cleave of Rage", "20/20")
	r3 := makeResult("Ice Shot of Frost", "1/20")

	c.Set(CacheKey("Spark of Nova", "20/20"), r1)
	c.Set(CacheKey("Cleave of Rage", "20/20"), r2)
	c.Set(CacheKey("Ice Shot of Frost", "1/20"), r3)

	snap := c.GetSnapshot()

	// Snapshot should contain all 3 entries.
	if len(snap) != 3 {
		t.Fatalf("GetSnapshot() returned %d entries, want 3", len(snap))
	}

	// Verify specific entries are accessible by key.
	if got, ok := snap[CacheKey("Spark of Nova", "20/20")]; !ok {
		t.Error("snapshot missing Spark of Nova")
	} else if got.Gem != "Spark of Nova" {
		t.Errorf("Spark Gem = %q, want %q", got.Gem, "Spark of Nova")
	}

	if got, ok := snap[CacheKey("Cleave of Rage", "20/20")]; !ok {
		t.Error("snapshot missing Cleave of Rage")
	} else if got.Gem != "Cleave of Rage" {
		t.Errorf("Cleave Gem = %q, want %q", got.Gem, "Cleave of Rage")
	}

	if got, ok := snap[CacheKey("Ice Shot of Frost", "1/20")]; !ok {
		t.Error("snapshot missing Ice Shot of Frost")
	} else if got.Variant != "1/20" {
		t.Errorf("Ice Shot Variant = %q, want %q", got.Variant, "1/20")
	}
}

func TestCache_GetSnapshot_Empty(t *testing.T) {
	c := NewTradeCache(10, league.Historical("Mirage"))

	snap := c.GetSnapshot()

	if snap == nil {
		t.Fatal("GetSnapshot() returned nil, want non-nil empty map")
	}
	if len(snap) != 0 {
		t.Errorf("GetSnapshot() returned %d entries, want 0", len(snap))
	}
}

func TestCache_GetSnapshot_DoesNotPromote(t *testing.T) {
	c := NewTradeCache(3, league.Historical("Mirage"))

	c.Set("a", makeResult("a", "1"))
	c.Set("b", makeResult("b", "1"))
	c.Set("c", makeResult("c", "1"))

	// Take a snapshot — this should NOT promote "a".
	_ = c.GetSnapshot()

	// Insert "d" — should evict "a" (still the oldest since snapshot didn't promote).
	c.Set("d", makeResult("d", "1"))

	if _, ok := c.Get("a"); ok {
		t.Error("expected 'a' to be evicted (GetSnapshot should not promote), but it was found")
	}
	// "b", "c", "d" should survive.
	for _, key := range []string{"b", "c", "d"} {
		if _, ok := c.Get(key); !ok {
			t.Errorf("expected %q to be present, got miss", key)
		}
	}
}

func TestCache_ConcurrentAccess(t *testing.T) {
	c := NewTradeCache(50, league.Historical("Mirage"))
	var wg sync.WaitGroup

	// Spawn writers.
	for i := 0; i < 20; i++ {
		wg.Add(1)
		go func(n int) {
			defer wg.Done()
			key := fmt.Sprintf("gem-%d", n)
			c.Set(key, makeResult(key, "1"))
		}(i)
	}

	// Spawn readers.
	for i := 0; i < 20; i++ {
		wg.Add(1)
		go func(n int) {
			defer wg.Done()
			key := fmt.Sprintf("gem-%d", n)
			c.Get(key)
		}(i)
	}

	// Spawn deleters.
	for i := 0; i < 10; i++ {
		wg.Add(1)
		go func(n int) {
			defer wg.Done()
			key := fmt.Sprintf("gem-%d", n)
			c.Delete(key)
		}(i)
	}

	wg.Wait()

	// If we got here without a race detector complaint, concurrency is safe.
	if c.Len() > 50 {
		t.Errorf("Len() = %d, exceeds maxSize 50", c.Len())
	}
}

// requirePanic fails the test unless fn panics — used to assert the cache
// rejects access under a foreign league.
func requirePanic(t *testing.T, fn func()) {
	t.Helper()
	defer func() {
		if r := recover(); r == nil {
			t.Fatalf("expected a panic (foreign-league access), got none")
		}
	}()
	fn()
}

// Reading the cache under its owning league returns the stored lookup — For is
// transparent for the league the cache is bound to.
func TestCache_For_OwningLeagueRoundTrips(t *testing.T) {
	scope := league.Historical("LeagueA")
	c := NewTradeCache(10, scope)

	key := CacheKey("Spark of Nova", "20/20")
	c.For(scope).Set(key, makeResult("Spark of Nova", "20/20"))

	got, ok := c.For(scope).Get(key)
	if !ok || got == nil || got.Gem != "Spark of Nova" {
		t.Fatalf("owning-league read: got %+v ok=%v, want the stored lookup", got, ok)
	}
}

// The tenancy gate: a trade cache warmed under one league must not serve those
// lookups under another league's scope. Removing the scope check in For makes
// this Get return LeagueA's entry instead of panicking, so this test fails.
func TestCache_For_RejectsForeignLeague(t *testing.T) {
	owner := league.Historical("LeagueA")
	other := league.Historical("LeagueB")

	c := NewTradeCache(10, owner)
	key := CacheKey("Spark of Nova", "20/20")
	c.For(owner).Set(key, makeResult("Spark of Nova", "20/20"))

	requirePanic(t, func() {
		_, _ = c.For(other).Get(key)
	})
}
