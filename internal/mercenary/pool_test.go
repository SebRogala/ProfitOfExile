package mercenary

import (
	"errors"
	"math"
	"testing"
)

// flipped perturbs the splitHalf base pattern by moving k lo positions to hi
// and k hi positions to lo. The result stays balanced (244/244), so its
// z-scores are still exactly ±1 and its correlation against the base is exact:
// 2k of 488 positions disagree, so NCC = 1 - 4k/488.
func flipped(k int) func(int) bool {
	return func(ordinal int) bool {
		switch {
		case ordinal < k:
			return true
		case ordinal >= 244 && ordinal < 244+k:
			return false
		default:
			return splitHalf(ordinal)
		}
	}
}

func perturbed(t *testing.T, k int) Signature {
	t.Helper()
	return mustSignature(t, balancedGray(flipped(k), 1, 255, 200))
}

func baseSignature(t *testing.T) Signature {
	t.Helper()
	return mustSignature(t, balancedGray(splitHalf, 1, 255, 200))
}

// The dedupe threshold is a contract shared with the desktop's `icon_match`.
// If the two drift, a device re-uploads on every sync what the server keeps
// calling a duplicate — or worse, the pool fills with near-copies of one icon.
func TestDedupeThreshold_MatchesTheDesktopIconMatchValue(t *testing.T) {
	if DedupeThreshold != 0.88 {
		t.Fatalf("DedupeThreshold = %v, want 0.88 (mercenary/mod.rs thresholds)", DedupeThreshold)
	}
}

// Just ABOVE the threshold: 28 of 488 positions disagree, NCC ≈ 0.8852. The
// comparison is >=, so this is the same art.
func TestDecide_CorrelationAboveThreshold_ReportsDuplicate(t *testing.T) {
	stored := baseSignature(t)
	candidate := perturbed(t, 14)

	ncc := stored.NCC(candidate)
	if want := float32(1 - 56.0/488.0); math.Abs(float64(ncc-want)) > 1e-6 {
		t.Fatalf("test setup: NCC = %v, want %v", ncc, want)
	}
	if ncc < DedupeThreshold {
		t.Fatalf("test setup: NCC %v is not above the %v threshold", ncc, DedupeThreshold)
	}

	if got := Decide(KeyState{Live: []Signature{stored}}, candidate); got != Duplicate {
		t.Fatalf("Decide at NCC %v = %v, want duplicate", ncc, got)
	}
}

// Just BELOW the threshold: 30 of 488 positions disagree, NCC ≈ 0.8770. Two
// positions further apart than the case above, and the pool must now keep it —
// this is the second sample that repairs a mistimed first hover.
func TestDecide_CorrelationBelowThreshold_ReportsStored(t *testing.T) {
	stored := baseSignature(t)
	candidate := perturbed(t, 15)

	ncc := stored.NCC(candidate)
	if want := float32(1 - 60.0/488.0); math.Abs(float64(ncc-want)) > 1e-6 {
		t.Fatalf("test setup: NCC = %v, want %v", ncc, want)
	}
	if ncc >= DedupeThreshold {
		t.Fatalf("test setup: NCC %v is not below the %v threshold", ncc, DedupeThreshold)
	}

	if got := Decide(KeyState{Live: []Signature{stored}}, candidate); got != Stored {
		t.Fatalf("Decide at NCC %v = %v, want stored", ncc, got)
	}
}

func TestDecide_EmptyKey_ReportsStored(t *testing.T) {
	if got := Decide(KeyState{}, baseSignature(t)); got != Stored {
		t.Fatalf("Decide on an empty key = %v, want stored", got)
	}
}

// A candidate is compared against EVERY live sample, not just the newest: the
// duplicate here sits behind two unrelated samples.
func TestDecide_MatchesAnyLiveSample_ReportsDuplicate(t *testing.T) {
	duplicateOf := baseSignature(t)
	unrelatedA := mustSignature(t, balancedGray(func(o int) bool { return o%2 == 0 }, 1, 255, 200))
	unrelatedB := mustSignature(t, balancedGray(func(o int) bool { return o%3 == 0 }, 1, 255, 200))

	state := KeyState{Live: []Signature{unrelatedA, unrelatedB, duplicateOf}}
	if got := Decide(state, duplicateOf); got != Duplicate {
		t.Fatalf("Decide against a matching third sample = %v, want duplicate", got)
	}
}

// The third sample still fits — the cap is three, not two.
func TestDecide_ThirdNovelSample_ReportsStored(t *testing.T) {
	state := KeyState{Live: []Signature{
		mustSignature(t, balancedGray(func(o int) bool { return o%2 == 0 }, 1, 255, 200)),
		mustSignature(t, balancedGray(func(o int) bool { return o%3 == 0 }, 1, 255, 200)),
	}}

	if got := Decide(state, baseSignature(t)); got != Stored {
		t.Fatalf("Decide on a two-sample key = %v, want stored", got)
	}
}

// The fourth never does. This is the bound on how much one abusive device can
// put in front of everyone else for a single key.
func TestDecide_FourthNovelSample_ReportsCapped(t *testing.T) {
	state := KeyState{Live: []Signature{
		mustSignature(t, balancedGray(func(o int) bool { return o%2 == 0 }, 1, 255, 200)),
		mustSignature(t, balancedGray(func(o int) bool { return o%3 == 0 }, 1, 255, 200)),
		mustSignature(t, balancedGray(func(o int) bool { return o%5 == 0 }, 1, 255, 200)),
	}}

	if got := Decide(state, baseSignature(t)); got != Capped {
		t.Fatalf("Decide on a full key = %v, want capped", got)
	}
}

// A full key offered art it already holds reports the duplicate, not the cap.
// The device can act on "we have this" by dropping the sample; "full" would
// invite it to retry forever.
func TestDecide_DuplicateOnAFullKey_ReportsDuplicate(t *testing.T) {
	known := baseSignature(t)
	state := KeyState{Live: []Signature{
		mustSignature(t, balancedGray(func(o int) bool { return o%2 == 0 }, 1, 255, 200)),
		mustSignature(t, balancedGray(func(o int) bool { return o%3 == 0 }, 1, 255, 200)),
		known,
	}}

	if got := Decide(state, known); got != Duplicate {
		t.Fatalf("Decide on a full key holding this art = %v, want duplicate", got)
	}
}

// Art that was thrown out is recognised and stays out. Without this the device
// that published the bad sample simply republishes it before its next pull, and
// the tombstone never sticks.
func TestDecide_MatchesRetiredArt_ReportsTombstoned(t *testing.T) {
	retired := baseSignature(t)
	state := KeyState{Retired: []Signature{retired}}

	if got := Decide(state, retired); got != Tombstoned {
		t.Fatalf("Decide on art identical to a retired sample = %v, want tombstoned", got)
	}
}

// Retirement is per-sample, not per-key: the key stays open to better art for
// the same family and tier. This is what makes tombstone-then-relearn work
// after a rename orphans a key.
func TestDecide_NovelArtUnderARetiredKey_ReportsStored(t *testing.T) {
	retired := baseSignature(t)
	novel := mustSignature(t, balancedGray(func(o int) bool { return o%2 == 0 }, 1, 255, 200))
	if ncc := retired.NCC(novel); ncc >= DedupeThreshold {
		t.Fatalf("test setup: the 'novel' sample correlates at %v with the retired one", ncc)
	}

	state := KeyState{Retired: []Signature{retired}}
	if got := Decide(state, novel); got != Stored {
		t.Fatalf("Decide on new art under a retired key = %v, want stored", got)
	}
}

// The same threshold governs retirement matching as governs dedupe: 30 of 488
// positions apart is a different picture, so it is stored rather than read as a
// republish of the retired one.
func TestDecide_CorrelationWithRetiredArtBelowThreshold_ReportsStored(t *testing.T) {
	retired := baseSignature(t)
	candidate := perturbed(t, 15)

	ncc := retired.NCC(candidate)
	if ncc >= DedupeThreshold {
		t.Fatalf("test setup: NCC %v is not below the %v threshold", ncc, DedupeThreshold)
	}

	if got := Decide(KeyState{Retired: []Signature{retired}}, candidate); got != Stored {
		t.Fatalf("Decide at NCC %v against retired art = %v, want stored", ncc, got)
	}
}

// Retired samples occupy no slot. If they counted toward the cap, three
// retirements would close a key by exhaustion — the block that per-sample
// retirement exists to avoid.
func TestDecide_CapCountsLiveSamplesOnly(t *testing.T) {
	state := KeyState{Retired: []Signature{
		mustSignature(t, balancedGray(func(o int) bool { return o%2 == 0 }, 1, 255, 200)),
		mustSignature(t, balancedGray(func(o int) bool { return o%3 == 0 }, 1, 255, 200)),
		mustSignature(t, balancedGray(func(o int) bool { return o%5 == 0 }, 1, 255, 200)),
	}}

	if got := Decide(state, baseSignature(t)); got != Stored {
		t.Fatalf("Decide on a key holding %d retired and 0 live samples = %v, want stored",
			len(state.Retired), got)
	}
}

func TestNewKey_TrimsAndAcceptsEveryTier(t *testing.T) {
	for tier := 1; tier <= 3; tier++ {
		key, err := NewKey("  Chain  ", tier)
		if err != nil {
			t.Fatalf("NewKey(Chain, %d): %v", tier, err)
		}
		if key.Family != "Chain" || key.Tier != int16(tier) {
			t.Errorf("NewKey(Chain, %d) = %+v, want {Chain %d}", tier, key, tier)
		}
	}
}

func TestNewKey_RejectsOutOfRangeTier(t *testing.T) {
	for _, tier := range []int{-1, 0, 4, 255} {
		if _, err := NewKey("Chain", tier); err == nil {
			t.Errorf("NewKey(Chain, %d) was accepted; tiers are 1-3", tier)
		}
	}
}

func TestNewKey_RejectsEmptyFamily(t *testing.T) {
	for _, family := range []string{"", "   ", "\t"} {
		if _, err := NewKey(family, 1); err == nil {
			t.Errorf("NewKey(%q, 1) was accepted", family)
		}
	}
}

// The family space is closed. Free text would let one device pool art under
// keys no hover can ever match, one dead row per upload the rate limit allows.
func TestNewKey_RejectsAFamilyOutsideTheVocabulary(t *testing.T) {
	for _, family := range []string{"Chian", "Chain Support", "chain", "Definitely Not A Support"} {
		if _, err := NewKey(family, 1); err == nil {
			t.Errorf("NewKey(%q, 1) was accepted; it is not in the support vocabulary", family)
		}
	}
}

// The vocabulary's own names pass unchanged — the check must reject invention,
// not the real key space.
func TestNewKey_AcceptsEveryFamilyInTheVocabulary(t *testing.T) {
	for family := range knownFamilies {
		if _, err := NewKey(family, 1); err != nil {
			t.Fatalf("NewKey(%q, 1) rejected a shipped family: %v", family, err)
		}
	}
}

// Removal is not gated on the vocabulary. A key is orphaned exactly when its
// family leaves the fixture, and that orphan is the case a tombstone exists
// for: art pooled under the old name that no hover will ever match again.
func TestParseKey_AcceptsAFamilyOutsideTheVocabulary(t *testing.T) {
	const renamedAway = "Formerly A Support"
	if _, known := knownFamilies[renamedAway]; known {
		t.Fatalf("test setup: %q is in the vocabulary", renamedAway)
	}

	key, err := ParseKey(renamedAway, 2)
	if err != nil {
		t.Fatalf("ParseKey rejected an orphaned family: %v", err)
	}
	if key.Family != renamedAway || key.Tier != 2 {
		t.Errorf("ParseKey = %+v, want {%s 2}", key, renamedAway)
	}
}

// Dropping the vocabulary check does not drop the shape checks: a tombstone
// still names a key, and a key is still non-empty with a tier of 1-3.
func TestParseKey_StillEnforcesShape(t *testing.T) {
	if _, err := ParseKey("", 1); err == nil {
		t.Error("ParseKey accepted an empty family")
	}
	if _, err := ParseKey("Chain", 4); err == nil {
		t.Error("ParseKey accepted tier 4")
	}
	long := make([]byte, MaxFamilyLen+1)
	for i := range long {
		long[i] = 'a'
	}
	if _, err := ParseKey(string(long), 1); err == nil {
		t.Error("ParseKey accepted an overlong family")
	}
}

// The two rejection causes are distinguishable, because upload reports them
// separately and a client reacts to them differently.
func TestNewKey_UnknownFamilyIsDistinguishableFromAMalformedKey(t *testing.T) {
	_, unknownErr := NewKey("Definitely Not A Support", 1)
	if !errors.Is(unknownErr, ErrUnknownFamily) {
		t.Errorf("unknown family error = %v, want ErrUnknownFamily", unknownErr)
	}
	if errors.Is(unknownErr, ErrInvalidKey) {
		t.Errorf("unknown family error also matches ErrInvalidKey; the two causes must be separable")
	}

	_, shapeErr := NewKey("Chain", 9)
	if !errors.Is(shapeErr, ErrInvalidKey) {
		t.Errorf("malformed key error = %v, want ErrInvalidKey", shapeErr)
	}
	if errors.Is(shapeErr, ErrUnknownFamily) {
		t.Errorf("malformed key error also matches ErrUnknownFamily")
	}
}

func TestNewKey_RejectsOverlongFamily(t *testing.T) {
	long := make([]byte, MaxFamilyLen+1)
	for i := range long {
		long[i] = 'a'
	}
	if _, err := NewKey(string(long), 1); err == nil {
		t.Fatalf("NewKey accepted a %d-byte family; the limit is %d", len(long), MaxFamilyLen)
	}
}

// The tally is what the client reads to decide whether to keep offering a
// sample, so each outcome has to land in its own column.
func TestAcceptResult_RecordsEachOutcomeSeparately(t *testing.T) {
	var got AcceptResult
	for _, outcome := range []Outcome{Stored, Stored, Duplicate, Capped, Capped, Capped, Tombstoned} {
		got.Record(outcome)
	}

	want := AcceptResult{Stored: 2, Duplicate: 1, Capped: 3, Tombstoned: 1}
	if got != want {
		t.Fatalf("AcceptResult = %+v, want %+v", got, want)
	}
}
