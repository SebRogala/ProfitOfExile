package mercenary

import (
	"errors"
	"fmt"
	"math"
	"math/rand"
	"testing"
)

// flipped perturbs the firstThird base pattern by moving k hi slots to lo and k
// lo slots to hi. The hi count stays at 219 of 657, so the mean and stddev are
// unchanged and the correlation against the base is exact arithmetic: 2k of the
// 657 slots disagree, and for a 219/438 split that comes to NCC = 1 - k/146.
func flipped(k int) func(int) bool {
	return func(ordinal int) bool {
		switch {
		case ordinal < k:
			return false
		case ordinal >= 219 && ordinal < 219+k:
			return true
		default:
			return firstThird(ordinal)
		}
	}
}

func perturbed(t *testing.T, k int) Signature {
	t.Helper()
	return mustSignature(t, patternRGB(flipped(k), 1, 255, 200))
}

func baseSignature(t *testing.T) Signature {
	t.Helper()
	return mustSignature(t, patternRGB(firstThird, 1, 255, 200))
}

// The dedupe threshold is a contract shared with the desktop's `icon_match`.
// If the two drift, a device re-uploads on every sync what the server keeps
// calling a duplicate — or worse, the pool fills with near-copies of one icon.
func TestDedupeThreshold_MatchesTheDesktopIconMatchValue(t *testing.T) {
	if DedupeThreshold != 0.88 {
		t.Fatalf("DedupeThreshold = %v, want 0.88 (mercenary/mod.rs thresholds)", DedupeThreshold)
	}
}

// Just ABOVE the threshold: 34 of 657 slots disagree, NCC ≈ 0.8836. The
// comparison is >=, so this is the same art.
func TestDecide_CorrelationAboveThreshold_ReportsDuplicate(t *testing.T) {
	stored := baseSignature(t)
	candidate := perturbed(t, 17)

	ncc := stored.NCC(candidate)
	if want := float32(1 - 17.0/146.0); math.Abs(float64(ncc-want)) > 1e-6 {
		t.Fatalf("test setup: NCC = %v, want %v", ncc, want)
	}
	if ncc < DedupeThreshold {
		t.Fatalf("test setup: NCC %v is not above the %v threshold", ncc, DedupeThreshold)
	}

	if got, _ := Decide(KeyState{Live: []Signature{stored}}, nil, candidate); got != Duplicate {
		t.Fatalf("Decide at NCC %v = %v, want duplicate", ncc, got)
	}
}

// Just BELOW the threshold: 36 of 657 slots disagree, NCC ≈ 0.8767. One slot
// pair further apart than the case above, and the pool must now keep it — this
// is the second sample that repairs a mistimed first hover.
func TestDecide_CorrelationBelowThreshold_ReportsStored(t *testing.T) {
	stored := baseSignature(t)
	candidate := perturbed(t, 18)

	ncc := stored.NCC(candidate)
	if want := float32(1 - 18.0/146.0); math.Abs(float64(ncc-want)) > 1e-6 {
		t.Fatalf("test setup: NCC = %v, want %v", ncc, want)
	}
	if ncc >= DedupeThreshold {
		t.Fatalf("test setup: NCC %v is not below the %v threshold", ncc, DedupeThreshold)
	}

	if got, _ := Decide(KeyState{Live: []Signature{stored}}, nil, candidate); got != Stored {
		t.Fatalf("Decide at NCC %v = %v, want stored", ncc, got)
	}
}

func TestDecide_EmptyKey_ReportsStored(t *testing.T) {
	if got, _ := Decide(KeyState{}, nil, baseSignature(t)); got != Stored {
		t.Fatalf("Decide on an empty key = %v, want stored", got)
	}
}

// A candidate is compared against EVERY live sample, not just the newest: the
// duplicate here sits behind two unrelated samples.
func TestDecide_MatchesAnyLiveSample_ReportsDuplicate(t *testing.T) {
	duplicateOf := baseSignature(t)
	unrelatedA := mustSignature(t, patternRGB(func(o int) bool { return o%2 == 0 }, 1, 255, 200))
	unrelatedB := mustSignature(t, patternRGB(func(o int) bool { return o%3 == 0 }, 1, 255, 200))

	state := KeyState{Live: []Signature{unrelatedA, unrelatedB, duplicateOf}}
	if got, _ := Decide(state, nil, duplicateOf); got != Duplicate {
		t.Fatalf("Decide against a matching third sample = %v, want duplicate", got)
	}
}

// The third sample still fits — the cap is three, not two.
func TestDecide_ThirdNovelSample_ReportsStored(t *testing.T) {
	state := KeyState{Live: []Signature{
		mustSignature(t, patternRGB(func(o int) bool { return o%2 == 0 }, 1, 255, 200)),
		mustSignature(t, patternRGB(func(o int) bool { return o%3 == 0 }, 1, 255, 200)),
	}}

	if got, _ := Decide(state, nil, baseSignature(t)); got != Stored {
		t.Fatalf("Decide on a two-sample key = %v, want stored", got)
	}
}

// The fourth never does. This is the bound on how much one abusive device can
// put in front of everyone else for a single key.
func TestDecide_FourthNovelSample_ReportsCapped(t *testing.T) {
	state := KeyState{Live: []Signature{
		mustSignature(t, patternRGB(func(o int) bool { return o%2 == 0 }, 1, 255, 200)),
		mustSignature(t, patternRGB(func(o int) bool { return o%3 == 0 }, 1, 255, 200)),
		mustSignature(t, patternRGB(func(o int) bool { return o%5 == 0 }, 1, 255, 200)),
	}}

	if got, _ := Decide(state, nil, baseSignature(t)); got != Capped {
		t.Fatalf("Decide on a full key = %v, want capped", got)
	}
}

// A full key offered art it already holds reports the duplicate, not the cap.
// The device can act on "we have this" by dropping the sample; "full" would
// invite it to retry forever.
func TestDecide_DuplicateOnAFullKey_ReportsDuplicate(t *testing.T) {
	known := baseSignature(t)
	state := KeyState{Live: []Signature{
		mustSignature(t, patternRGB(func(o int) bool { return o%2 == 0 }, 1, 255, 200)),
		mustSignature(t, patternRGB(func(o int) bool { return o%3 == 0 }, 1, 255, 200)),
		known,
	}}

	if got, _ := Decide(state, nil, known); got != Duplicate {
		t.Fatalf("Decide on a full key holding this art = %v, want duplicate", got)
	}
}

// Art that was thrown out is recognised and stays out. Without this the device
// that published the bad sample simply republishes it before its next pull, and
// the tombstone never sticks.
func TestDecide_MatchesRetiredArt_ReportsTombstoned(t *testing.T) {
	retired := baseSignature(t)
	state := KeyState{Retired: []Signature{retired}}

	if got, _ := Decide(state, nil, retired); got != Tombstoned {
		t.Fatalf("Decide on art identical to a retired sample = %v, want tombstoned", got)
	}
}

// Retirement is per-sample, not per-key: the key stays open to better art for
// the same family and tier. This is what makes tombstone-then-relearn work
// after a rename orphans a key.
func TestDecide_NovelArtUnderARetiredKey_ReportsStored(t *testing.T) {
	retired := baseSignature(t)
	novel := mustSignature(t, patternRGB(func(o int) bool { return o%2 == 0 }, 1, 255, 200))
	if ncc := retired.NCC(novel); ncc >= DedupeThreshold {
		t.Fatalf("test setup: the 'novel' sample correlates at %v with the retired one", ncc)
	}

	state := KeyState{Retired: []Signature{retired}}
	if got, _ := Decide(state, nil, novel); got != Stored {
		t.Fatalf("Decide on new art under a retired key = %v, want stored", got)
	}
}

// The same threshold governs retirement matching as governs dedupe: 36 of 657
// slots apart is a different picture, so it is stored rather than read as a
// republish of the retired one.
func TestDecide_CorrelationWithRetiredArtBelowThreshold_ReportsStored(t *testing.T) {
	retired := baseSignature(t)
	candidate := perturbed(t, 18)

	ncc := retired.NCC(candidate)
	if ncc >= DedupeThreshold {
		t.Fatalf("test setup: NCC %v is not below the %v threshold", ncc, DedupeThreshold)
	}

	if got, _ := Decide(KeyState{Retired: []Signature{retired}}, nil, candidate); got != Stored {
		t.Fatalf("Decide at NCC %v against retired art = %v, want stored", ncc, got)
	}
}

// Retired samples occupy no slot. If they counted toward the cap, three
// retirements would close a key by exhaustion — the block that per-sample
// retirement exists to avoid.
func TestDecide_CapCountsLiveSamplesOnly(t *testing.T) {
	state := KeyState{Retired: []Signature{
		mustSignature(t, patternRGB(func(o int) bool { return o%2 == 0 }, 1, 255, 200)),
		mustSignature(t, patternRGB(func(o int) bool { return o%3 == 0 }, 1, 255, 200)),
		mustSignature(t, patternRGB(func(o int) bool { return o%5 == 0 }, 1, 255, 200)),
	}}

	if got, _ := Decide(state, nil, baseSignature(t)); got != Stored {
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
// sample, so each outcome has to land in its own column. Conflicting is counted
// apart from Duplicate in particular: a duplicate means the pool already serves
// this art, a conflict means it never will until somebody retires the incumbent.
func TestAcceptResult_RecordsEachOutcomeSeparately(t *testing.T) {
	var got AcceptResult
	for _, outcome := range []Outcome{
		Stored, Stored, Duplicate, Capped, Capped, Capped, Tombstoned, Conflicting, Conflicting,
	} {
		got.Record(outcome)
	}

	want := AcceptResult{Stored: 2, Duplicate: 1, Capped: 3, Tombstoned: 1, Conflicting: 2}
	// Field by field rather than ==: AcceptResult carries the Conflicts slice
	// now, which makes the struct uncomparable. Conflicts is checked too — the
	// index a conflict entry carries is the CALLER's, so Record growing one
	// would be filling in a number it does not have.
	if got.Stored != want.Stored || got.Duplicate != want.Duplicate ||
		got.Capped != want.Capped || got.Tombstoned != want.Tombstoned ||
		got.Conflicting != want.Conflicting || len(got.Conflicts) != 0 {
		t.Fatalf("AcceptResult = %+v, want %+v with no Conflicts detail", got, want)
	}
}

// --- cross-family conflict ---

// One picture belongs to one family. A mistimed hover on one device claims art
// another family is already pooled for, and serving both would leave every
// device matching that picture against two answers — which is the state the
// client's own merge rule reacts to by emptying BOTH keys.
func TestDecide_ArtAlreadyPooledUnderAnotherFamily_ReportsConflicting(t *testing.T) {
	candidate := perturbed(t, 17) // NCC ≈ 0.8836 against the base: just above the threshold
	foreign := []ForeignSample{{Key: Key{Family: "Chain", Tier: 1}, Signature: baseSignature(t)}}

	if got, _ := Decide(KeyState{}, foreign, candidate); got != Conflicting {
		t.Fatalf("Decide against another family's live art = %v, want conflicting", got)
	}
}

// The refusal is only actionable if the player is told what to forget, so the
// conflict names the sample that refused it — the MATCHING one, not simply the
// first family in the view.
func TestDecide_Conflict_NamesTheMatchingFamilyAsTheIncumbent(t *testing.T) {
	candidate := perturbed(t, 17)
	unrelated := mustSignature(t, patternRGB(func(o int) bool { return o%2 == 0 }, 1, 255, 200))
	if ncc := unrelated.NCC(candidate); ncc >= DedupeThreshold {
		t.Fatalf("test setup: the decoy sample correlates at %v with the candidate", ncc)
	}
	// The decoy sits FIRST, so returning the head of the view — or the last
	// sample walked — names the wrong family.
	foreign := []ForeignSample{
		{Key: Key{Family: "Pierce", Tier: 1}, Signature: unrelated},
		{Key: Key{Family: "Chain", Tier: 1}, Signature: baseSignature(t)},
		{Key: Key{Family: "Brutality", Tier: 3}, Signature: unrelated},
	}

	_, incumbent := Decide(KeyState{}, foreign, candidate)

	if incumbent == nil {
		t.Fatal("Decide reported a conflict without naming the incumbent")
	}
	if incumbent.Key.Family != "Chain" || incumbent.Key.Tier != 1 {
		t.Errorf("incumbent = %v, want Chain--1 — the sample that actually matched", incumbent.Key)
	}
}

// The conflict runs on the SAME correlation as dedupe. One slot pair further
// apart is a different picture, and two families are allowed to own two
// different pictures.
func TestDecide_ForeignArtBelowTheThreshold_IsNotAConflict(t *testing.T) {
	candidate := perturbed(t, 18) // NCC ≈ 0.8767: just below
	foreign := []ForeignSample{{Key: Key{Family: "Chain", Tier: 1}, Signature: baseSignature(t)}}

	got, incumbent := Decide(KeyState{}, foreign, candidate)

	if got != Stored {
		t.Fatalf("Decide against another family's art at NCC 0.8767 = %v, want stored", got)
	}
	if incumbent != nil {
		t.Errorf("Decide named %q as an incumbent for a candidate it stored", incumbent.Key.Family)
	}
}

// A full key offered another family's art is told about the conflict, not about
// the cap. "Full" invites the device to retry once a slot frees; this art will
// never be stored under this key until the incumbent is retired, and only the
// conflict says so.
func TestDecide_ConflictOnAFullKey_ReportsConflictingNotCapped(t *testing.T) {
	art := baseSignature(t)
	full := KeyState{Live: []Signature{
		mustSignature(t, patternRGB(func(o int) bool { return o%2 == 0 }, 1, 255, 200)),
		mustSignature(t, patternRGB(func(o int) bool { return o%3 == 0 }, 1, 255, 200)),
		mustSignature(t, patternRGB(func(o int) bool { return o%5 == 0 }, 1, 255, 200)),
	}}
	foreign := []ForeignSample{{Key: Key{Family: "Chain", Tier: 1}, Signature: art}}

	got, incumbent := Decide(full, foreign, art)

	if got != Conflicting {
		t.Fatalf("Decide on a full key against another family's art = %v, want conflicting", got)
	}
	if incumbent == nil || incumbent.Key.Family != "Chain" {
		t.Errorf("incumbent = %+v, want Chain", incumbent)
	}
}

// The other half of the conflict-before-cap ordering: a full key offered art no
// other family holds is still capped. A non-empty foreign view is not by itself
// a refusal — only a MATCH in it is — so the cap has to survive one.
func TestDecide_FullKeyAgainstANonMatchingForeignView_ReportsCapped(t *testing.T) {
	candidate := baseSignature(t)
	full := KeyState{Live: []Signature{
		mustSignature(t, patternRGB(func(o int) bool { return o%2 == 0 }, 1, 255, 200)),
		mustSignature(t, patternRGB(func(o int) bool { return o%3 == 0 }, 1, 255, 200)),
		mustSignature(t, patternRGB(func(o int) bool { return o%5 == 0 }, 1, 255, 200)),
	}}
	foreign := []ForeignSample{
		{Key: Key{Family: "Chain", Tier: 1}, Signature: full.Live[0]},
		{Key: Key{Family: "Pierce", Tier: 2}, Signature: full.Live[1]},
	}
	for _, other := range foreign {
		if ncc := other.Signature.NCC(candidate); ncc >= DedupeThreshold {
			t.Fatalf("test setup: %v correlates at %v with the candidate, so this would be a conflict",
				other.Key, ncc)
		}
	}

	got, incumbent := Decide(full, foreign, candidate)

	if got != Capped {
		t.Fatalf("Decide on a full key against art no other family holds = %v, want capped", got)
	}
	if incumbent != nil {
		t.Errorf("Decide named %q as an incumbent for a capped candidate", incumbent.Key.Family)
	}
}

// The pool already carries this art under both families, so it is already
// inconsistent and the client's merge rule is what cleans that up. Reporting a
// conflict here would tell the device to settle a sample the pool is in fact
// still serving it.
func TestDecide_ArtOnItsOwnKeyAndOnAnothers_ReportsDuplicate(t *testing.T) {
	art := baseSignature(t)
	foreign := []ForeignSample{{Key: Key{Family: "Chain", Tier: 1}, Signature: art}}

	got, incumbent := Decide(KeyState{Live: []Signature{art}}, foreign, art)

	if got != Duplicate {
		t.Fatalf("Decide on art this key already holds AND another family holds = %v, want duplicate", got)
	}
	if incumbent != nil {
		t.Errorf("Decide named %q as an incumbent for a duplicate", incumbent.Key.Family)
	}
}

// Retirement of this key's own art outranks the conflict. The two ask different
// things of the device — a tombstone says "stop holding this", a conflict says
// "this belongs to somebody else" — and the sample the device is being told
// about was thrown out of THIS key.
func TestDecide_OwnKeyRetirementOutranksACrossFamilyConflict(t *testing.T) {
	art := baseSignature(t)
	foreign := []ForeignSample{{Key: Key{Family: "Chain", Tier: 1}, Signature: art}}

	got, _ := Decide(KeyState{Retired: []Signature{art}}, foreign, art)

	if got != Tombstoned {
		t.Fatalf("Decide on art retired from this key and live under another = %v, want tombstoned", got)
	}
}

// The signature masks the tier badge, so one family's tier-1 and tier-3 art are
// the same picture by construction whenever the game draws them the same. If
// its own other tier counted as foreign, the second tier of every such family
// would be refused.
func TestOtherFamilies_SameFamilyAtAnotherTierIsNotForeign(t *testing.T) {
	art := baseSignature(t)
	live := []ForeignSample{{Key: Key{Family: "Chain", Tier: 1}, Signature: art}}

	foreign := otherFamilies(live, "Chain")

	if len(foreign) != 0 {
		t.Fatalf("foreign view for Chain = %+v, want empty: its own tier-1 sample is not another family's art",
			foreign)
	}
	if got, _ := Decide(KeyState{}, foreign, art); got != Stored {
		t.Fatalf("Chain tier 3 offered the art Chain tier 1 holds = %v, want stored", got)
	}
}

// Every other family stays in the view, at every tier. Narrowing it to one
// family, or to one tier, would let a conflict through unnoticed.
func TestOtherFamilies_KeepsEveryOtherFamilysArt(t *testing.T) {
	art := baseSignature(t)
	live := []ForeignSample{
		{Key: Key{Family: "Chain", Tier: 1}, Signature: art},
		{Key: Key{Family: "Pierce", Tier: 1}, Signature: art},
		{Key: Key{Family: "Brutality", Tier: 3}, Signature: art},
	}

	foreign := otherFamilies(live, "Chain")

	seen := map[string]int16{}
	for _, sample := range foreign {
		seen[sample.Key.Family] = sample.Key.Tier
	}
	if len(seen) != 2 || seen["Pierce"] != 1 || seen["Brutality"] != 3 {
		t.Fatalf("foreign view for Chain = %v, want {Pierce:1, Brutality:3}", seen)
	}
}

// The pool-wide advisory lock in repository.go is held for exactly this shape of
// work — the largest batch the SERVER will accept, decided against a SATURATED
// pool — and its comment quotes the number this benchmark produces.
//
// The batch factor is MaxTemplatesPerUpload, not the desktop's
// MAX_TEMPLATES_PER_BATCH (32): the upload handler is what admits a request, and
// a client that is not the desktop owes that constant nothing. What the lock has
// to survive is the worst request the endpoint accepts, so that is what this
// measures.
//
// Decide alone, not Decide plus otherFamilies: the filter is a slice copy next
// to a full-batch-times-full-view sweep of correlations over 1728 floats each.
func BenchmarkDecide_MaxBatchAgainstFullForeignView(b *testing.B) {
	// What the upload handler admits (internal/server/handlers/mercenary.go).
	batchSize := MaxTemplatesPerUpload
	// The saturated key space, DERIVED: every family at all three tiers. A
	// literal here would pin the benchmark — and the lock comment quoting it —
	// to whatever the vocabulary held the day it was written, which is exactly
	// how the create migration's 264 outlived the 88-family vocabulary that
	// produced it.
	foreignKeys := 3 * KnownFamilyCount()

	rng := rand.New(rand.NewSource(1))
	randomSignature := func() Signature {
		rgb := make([]byte, SigBytes)
		for i := range rgb {
			rgb[i] = byte(rng.Intn(256))
		}
		sig, err := NewSignature(rgb)
		if err != nil {
			b.Fatalf("NewSignature: %v", err)
		}
		return sig
	}

	foreign := make([]ForeignSample, 0, foreignKeys*MaxSamplesPerKey)
	for key := 0; key < foreignKeys; key++ {
		for sample := 0; sample < MaxSamplesPerKey; sample++ {
			foreign = append(foreign, ForeignSample{
				Key:       Key{Family: fmt.Sprintf("family-%d", key), Tier: 1},
				Signature: randomSignature(),
			})
		}
	}
	batch := make([]Signature, batchSize)
	for i := range batch {
		batch[i] = randomSignature()
	}

	// Nothing matches, so every candidate walks the whole view. A setup that
	// short-circuited would report a fraction of what the lock really holds for.
	for _, candidate := range batch {
		if outcome, _ := Decide(KeyState{}, foreign, candidate); outcome != Stored {
			b.Fatalf("benchmark setup: a candidate decided %v, so the scan exits early", outcome)
		}
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		for _, candidate := range batch {
			Decide(KeyState{}, foreign, candidate)
		}
	}
}
