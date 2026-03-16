package trade

import (
	"fmt"
	"sync"
	"testing"
	"time"
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
	c := NewTradeCache(10)
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
	c := NewTradeCache(10)

	got, ok := c.Get("nonexistent")
	if ok {
		t.Fatal("expected cache miss, got hit")
	}
	if got != nil {
		t.Errorf("expected nil result on miss, got %v", got)
	}
}

func TestCache_Eviction(t *testing.T) {
	c := NewTradeCache(3)

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
	c := NewTradeCache(3)

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
	c := NewTradeCache(10)

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
	c := NewTradeCache(10)

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

func TestCache_ConcurrentAccess(t *testing.T) {
	c := NewTradeCache(50)
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
