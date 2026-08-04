package lab

import (
	"context"
	"errors"
	"reflect"
	"testing"

	"profitofexile/internal/league"
)

var gemDictScope = league.Historical("Mirage")

// fakeDictionarySource records the seed reads so a test can tell "seeded once"
// from "queried every tick", and answers per pool the way the repository does.
type fakeDictionarySource struct {
	skills       []string
	transfigured []string
	err          error
	calls        int
}

func (f *fakeDictionarySource) GemNameDictionary(_ context.Context, _ league.Scope, transfigured bool) ([]string, error) {
	f.calls++
	if f.err != nil {
		return nil, f.err
	}
	if transfigured {
		return f.transfigured, nil
	}
	return f.skills, nil
}

func dictionaryOf(t *testing.T, cache *Cache, transfigured bool) []string {
	t.Helper()
	names, ok := cache.For(gemDictScope).GemDictionary(transfigured)
	if !ok {
		t.Fatal("dictionary reports cold")
	}
	return names
}

// The union is the point: gem_colors alone collapses to a handful of names at
// league start, and the snapshot half alone loses every gem the market has not
// priced this league.
func TestPopulateGemDictionary_ColdCacheUnionsTheSeedWithTheSnapshot(t *testing.T) {
	cache := NewCache(gemDictScope)
	src := &fakeDictionarySource{
		skills:       []string{"Spark"},
		transfigured: []string{"Spark of Nova"},
	}
	gems := []GemPrice{
		{Name: "Cyclone"},
		{Name: "Cyclone of Tumult", IsTransfigured: true},
	}

	if err := populateGemDictionary(context.Background(), src, cache, gemDictScope, gems); err != nil {
		t.Fatalf("populate: %v", err)
	}

	if got, want := dictionaryOf(t, cache, false), []string{"Cyclone", "Spark"}; !reflect.DeepEqual(got, want) {
		t.Errorf("skill dictionary = %v, want %v", got, want)
	}
	if got, want := dictionaryOf(t, cache, true), []string{"Cyclone of Tumult", "Spark of Nova"}; !reflect.DeepEqual(got, want) {
		t.Errorf("transfigured dictionary = %v, want %v", got, want)
	}
}

// The Font and the Dedication hand out skill gems only, so a support gem is
// never a possible outcome and must not enter the skill pool through the tick —
// the same strip the repository applies to its snapshot half.
func TestPopulateGemDictionary_StripsSupportGemsFromTheSkillPool(t *testing.T) {
	cache := NewCache(gemDictScope)
	gems := []GemPrice{
		{Name: "Cyclone"},
		{Name: "Increased Critical Strikes Support"},
	}

	if err := populateGemDictionary(context.Background(), &fakeDictionarySource{}, cache, gemDictScope, gems); err != nil {
		t.Fatalf("populate: %v", err)
	}

	if got, want := dictionaryOf(t, cache, false), []string{"Cyclone"}; !reflect.DeepEqual(got, want) {
		t.Errorf("skill dictionary = %v, want %v — support gems must not enter the pool", got, want)
	}
}

// The transfigured half is deliberately not support-stripped: is_transfigured
// comes from poe.ninja's alt_ discriminator and is authoritative, so a
// name-shape heuristic layered on it could only subtract.
func TestPopulateGemDictionary_DoesNotStripTheTransfiguredPool(t *testing.T) {
	cache := NewCache(gemDictScope)
	gems := []GemPrice{{Name: "Impending Doom Support", IsTransfigured: true}}

	if err := populateGemDictionary(context.Background(), &fakeDictionarySource{}, cache, gemDictScope, gems); err != nil {
		t.Fatalf("populate: %v", err)
	}

	if got, want := dictionaryOf(t, cache, true), []string{"Impending Doom Support"}; !reflect.DeepEqual(got, want) {
		t.Errorf("transfigured dictionary = %v, want %v — the market's flag is authoritative here", got, want)
	}
}

// Seeded once, then extended for free. Re-reading the seed each tick would put
// the two queries this cache removes back on the tick.
func TestPopulateGemDictionary_WarmCacheDoesNotReadTheSource(t *testing.T) {
	cache := NewCache(gemDictScope)
	src := &fakeDictionarySource{skills: []string{"Spark"}}
	if err := populateGemDictionary(context.Background(), src, cache, gemDictScope, nil); err != nil {
		t.Fatalf("first populate: %v", err)
	}
	seedCalls := src.calls

	if err := populateGemDictionary(context.Background(), src, cache, gemDictScope, []GemPrice{{Name: "Cyclone"}}); err != nil {
		t.Fatalf("second populate: %v", err)
	}

	if src.calls != seedCalls {
		t.Errorf("source read %d times after the seed's %d — the seed must not repeat per tick", src.calls, seedCalls)
	}
	if got, want := dictionaryOf(t, cache, false), []string{"Cyclone", "Spark"}; !reflect.DeepEqual(got, want) {
		t.Errorf("skill dictionary = %v, want %v — the later tick's name must be unioned in", got, want)
	}
}

// The dictionary only grows: a name the market stops listing is still a name the
// OCR matcher has to recognise on a gem the player already holds.
func TestPopulateGemDictionary_KeepsANameAbsentFromTheLatestSnapshot(t *testing.T) {
	cache := NewCache(gemDictScope)
	src := &fakeDictionarySource{}
	if err := populateGemDictionary(context.Background(), src, cache, gemDictScope, []GemPrice{{Name: "Cyclone"}}); err != nil {
		t.Fatalf("first populate: %v", err)
	}

	if err := populateGemDictionary(context.Background(), src, cache, gemDictScope, []GemPrice{{Name: "Spark"}}); err != nil {
		t.Fatalf("second populate: %v", err)
	}

	if got, want := dictionaryOf(t, cache, false), []string{"Cyclone", "Spark"}; !reflect.DeepEqual(got, want) {
		t.Errorf("skill dictionary = %v, want %v", got, want)
	}
}

// Without gem_colors the dictionary is missing every gem the market has not
// priced this league — the league-start blindness the union exists to prevent.
// Storing that as warm would serve it with no fallback.
func TestPopulateGemDictionary_SeedFailureLeavesTheCacheCold(t *testing.T) {
	cache := NewCache(gemDictScope)
	src := &fakeDictionarySource{err: errors.New("boom")}

	if err := populateGemDictionary(context.Background(), src, cache, gemDictScope, []GemPrice{{Name: "Cyclone"}}); err == nil {
		t.Fatal("populate returned no error on a failed seed")
	}
	if _, ok := cache.For(gemDictScope).GemDictionary(false); ok {
		t.Error("cache reports warm after a failed seed; the handler would serve a snapshot-only dictionary")
	}
}

// A league with no gem_colors rows and no snapshots produces an empty
// dictionary, and that is an answer — reading it as cold would query twice per
// request forever.
func TestPopulateGemDictionary_EmptyResultStillMarksTheCacheWarm(t *testing.T) {
	cache := NewCache(gemDictScope)

	if err := populateGemDictionary(context.Background(), &fakeDictionarySource{}, cache, gemDictScope, nil); err != nil {
		t.Fatalf("populate: %v", err)
	}

	names, ok := cache.For(gemDictScope).GemDictionary(false)
	if !ok {
		t.Fatal("cache reports cold after an empty tick")
	}
	if len(names) != 0 {
		t.Errorf("dictionary = %v, want empty", names)
	}
}
