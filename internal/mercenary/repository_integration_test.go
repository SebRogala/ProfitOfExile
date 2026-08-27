//go:build integration

package mercenary

import (
	"bytes"
	"context"
	"os"
	"sync"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
)

// The pool's three durable promises live in SQL, not in Decide: a key never
// holds a fourth live sample, a retired key stays retired across requests, and
// a retired sample stops being served. These tests are the only place those are
// exercised end to end; the pure decision rule is covered in pool_test.go.
//
// The helpers patternRGB/firstThird/mustSignature come from signature_test.go,
// which compiles into this build too.

// integrationVersion is the format version these tests write. It tracks
// SupportedFormatVersion rather than pinning a literal: the signature length
// CHECK is version-conditional (POE-207), so a literal here would insert
// 1728-byte signatures under a version whose branch of the CHECK expects 576
// and every write would fail on the constraint rather than on the behaviour
// under test.
const integrationVersion = SupportedFormatVersion

func integrationPool(t *testing.T) *pgxpool.Pool {
	t.Helper()

	dbURL := os.Getenv("DATABASE_URL")
	if dbURL == "" {
		t.Skip("DATABASE_URL not set, skipping integration test")
	}

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	pool, err := pgxpool.New(ctx, dbURL)
	if err != nil {
		t.Fatalf("connect to database: %v", err)
	}
	t.Cleanup(func() { pool.Close() })

	var exists bool
	if err := pool.QueryRow(ctx,
		"SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'merc_icon_templates')").
		Scan(&exists); err != nil {
		t.Fatalf("check merc_icon_templates table: %v", err)
	}
	if !exists {
		t.Skip("merc_icon_templates not found, skipping (migration not applied)")
	}

	clearVersion(t, pool)
	return pool
}

// clearVersionOnce makes the version-wide wipe below run exactly once per suite
// process, on whichever test reaches integrationPool first.
var clearVersionOnce sync.Once

// clearVersion empties the format version these tests write, ONCE per suite.
//
// The per-family clear reserveFamily does is not enough on its own, because the
// accept rule is not per-family any more: poolSQL reads the WHOLE version, and
// every live row in it — under ANY family — is foreign art the cross-family rule
// compares against. So one leftover firstThird row, from a run killed between a
// test's pre- and post-clear, turns every same-art test in this file into a
// Conflicting one, under a family none of them mention.
//
// This is the pre-suite half; reserveFamily's per-family clear stays as the
// post-test half, which is what keeps ONE test's rows out of the NEXT test.
func clearVersion(t *testing.T, pool *pgxpool.Pool) {
	t.Helper()
	var err error
	clearVersionOnce.Do(func() {
		_, err = pool.Exec(context.Background(),
			`DELETE FROM merc_icon_templates WHERE format_version = $1`, integrationVersion)
	})
	if err != nil {
		t.Fatalf("clear format version %d before the suite: %v", integrationVersion, err)
	}
}

// clearFamily deletes every row of one family. `when` names the call site so a
// pre-test wipe, a post-test one and a between-rounds one are told apart in the
// failure.
func clearFamily(t *testing.T, pool *pgxpool.Pool, family, when string) {
	t.Helper()
	if _, err := pool.Exec(context.Background(),
		`DELETE FROM merc_icon_templates WHERE family = $1`, family); err != nil {
		t.Fatalf("%s cleanup for %q: %v", when, family, err)
	}
}

// reserveFamily hands a test one REAL family from the shipped vocabulary and
// makes sure it starts and ends empty.
//
// Synthetic names are not an option any more: NewKey validates against the
// vocabulary, so a made-up family cannot become a key at all. Each test
// therefore takes a family of its own — a test sharing one with another would
// see its neighbour's samples in the state it asserts on.
func reserveFamily(t *testing.T, pool *pgxpool.Pool, family string) string {
	t.Helper()
	if _, err := NewKey(family, 1); err != nil {
		t.Fatalf("test setup: %q is not a shipped family: %v", family, err)
	}
	clearFamily(t, pool, family, "pre-test")
	t.Cleanup(func() { clearFamily(t, pool, family, "post-test") })
	return family
}

// expiredRetirementAge is how far back the tests push a tombstone to make it
// expire. It is a LITERAL, deliberately not RetiredMatchWindow + something:
// deriving it from the constant would make the fixture track the production
// value, and widening the window would then still "pass" while nothing expired.
// Widening RetiredMatchWindow past this instead fails the setup guard below,
// which is the review the policy change deserves.
const expiredRetirementAge = 31 * 24 * time.Hour

// backdateRetirement pushes a key's tombstones past RetiredMatchWindow, which
// is the only way to observe an expired retirement without waiting a month.
func backdateRetirement(t *testing.T, pool *pgxpool.Pool, family string, age time.Duration) {
	t.Helper()
	if age <= RetiredMatchWindow {
		t.Fatalf("test setup: backdating by %v does not clear a %v retirement window",
			age, RetiredMatchWindow)
	}
	tag, err := pool.Exec(context.Background(),
		`UPDATE merc_icon_templates SET tombstoned_at = $2
		 WHERE family = $1 AND tombstoned_at IS NOT NULL`,
		family, time.Now().Add(-age))
	if err != nil {
		t.Fatalf("backdate retirement for %q: %v", family, err)
	}
	if tag.RowsAffected() == 0 {
		t.Fatalf("test setup: %q had no retired rows to backdate", family)
	}
}

func candidate(t *testing.T, family string, tier int, hi func(int) bool) Candidate {
	t.Helper()
	key, err := NewKey(family, tier)
	if err != nil {
		t.Fatalf("NewKey(%q, %d): %v", family, tier, err)
	}
	return Candidate{Key: key, Signature: mustSignature(t, patternRGB(hi, 1, 255, 200))}
}

// distinctPatterns are four signatures no two of which correlate at the dedupe
// threshold — checked below rather than assumed, so a cap test can never pass
// because its "different" samples were duplicates.
func distinctPatterns() []func(int) bool {
	return []func(int) bool{
		firstThird,
		func(o int) bool { return o%2 == 0 },
		func(o int) bool { return o%3 == 0 },
		func(o int) bool { return o%5 == 0 },
	}
}

func requireDistinct(t *testing.T, family string) {
	t.Helper()
	patterns := distinctPatterns()
	for i := range patterns {
		for j := i + 1; j < len(patterns); j++ {
			a := candidate(t, family, 1, patterns[i]).Signature
			b := candidate(t, family, 1, patterns[j]).Signature
			if ncc := a.NCC(b); ncc >= DedupeThreshold {
				t.Fatalf("test setup: patterns %d and %d correlate at %v, which the pool would call a duplicate", i, j, ncc)
			}
		}
	}
}

func liveCount(t *testing.T, pool *pgxpool.Pool, family string, tier int) int {
	t.Helper()
	var n int
	if err := pool.QueryRow(context.Background(),
		`SELECT count(*) FROM merc_icon_templates
		 WHERE family = $1 AND tier = $2 AND format_version = $3 AND tombstoned_at IS NULL`,
		family, tier, integrationVersion).Scan(&n); err != nil {
		t.Fatalf("count live samples: %v", err)
	}
	return n
}

// One hover anywhere becomes everyone's template; the same hover from a second
// device costs a row of nothing.
func TestRepository_Accept_SecondDeviceUploadingTheSameArt_IsADuplicate(t *testing.T) {
	pool := integrationPool(t)
	repo := NewRepository(pool)
	family := reserveFamily(t, pool, "Chain")
	ctx := context.Background()

	first, err := repo.Accept(ctx, "device-a", integrationVersion,
		[]Candidate{candidate(t, family, 1, firstThird)})
	if err != nil {
		t.Fatalf("first Accept: %v", err)
	}
	if first.Stored != 1 {
		t.Fatalf("first upload = %+v, want 1 stored", first)
	}

	second, err := repo.Accept(ctx, "device-b", integrationVersion,
		[]Candidate{candidate(t, family, 1, firstThird)})
	if err != nil {
		t.Fatalf("second Accept: %v", err)
	}
	if second.Duplicate != 1 || second.Stored != 0 {
		t.Fatalf("second upload of the same art = %+v, want 1 duplicate and 0 stored", second)
	}
	if got := liveCount(t, pool, family, 1); got != 1 {
		t.Errorf("live samples = %d, want 1", got)
	}
}

// The cap is what bounds how much of one key a single device can put in front
// of everyone else.
func TestRepository_Accept_FourthDistinctSample_IsNotStored(t *testing.T) {
	pool := integrationPool(t)
	repo := NewRepository(pool)
	family := reserveFamily(t, pool, "Pierce")
	requireDistinct(t, family)
	ctx := context.Background()

	patterns := distinctPatterns()
	for i, pattern := range patterns[:3] {
		result, err := repo.Accept(ctx, "device-a", integrationVersion,
			[]Candidate{candidate(t, family, 1, pattern)})
		if err != nil {
			t.Fatalf("Accept sample %d: %v", i+1, err)
		}
		if result.Stored != 1 {
			t.Fatalf("sample %d = %+v, want 1 stored", i+1, result)
		}
	}

	fourth, err := repo.Accept(ctx, "device-a", integrationVersion,
		[]Candidate{candidate(t, family, 1, patterns[3])})
	if err != nil {
		t.Fatalf("fourth Accept: %v", err)
	}
	if fourth.Capped != 1 || fourth.Stored != 0 {
		t.Fatalf("fourth distinct sample = %+v, want 1 capped and 0 stored", fourth)
	}
	if got := liveCount(t, pool, family, 1); got != MaxSamplesPerKey {
		t.Errorf("live samples = %d, want %d", got, MaxSamplesPerKey)
	}
}

// A batch is decided against a state that grows as it is applied, so a device
// sending the same art twice in one request cannot buy two slots with it.
func TestRepository_Accept_RepeatedArtWithinOneBatch_TakesOneSlot(t *testing.T) {
	pool := integrationPool(t)
	repo := NewRepository(pool)
	family := reserveFamily(t, pool, "Brutality")
	ctx := context.Background()

	result, err := repo.Accept(ctx, "device-a", integrationVersion, []Candidate{
		candidate(t, family, 1, firstThird),
		candidate(t, family, 1, firstThird),
	})
	if err != nil {
		t.Fatalf("Accept: %v", err)
	}
	if result.Stored != 1 || result.Duplicate != 1 {
		t.Fatalf("batch of two identical templates = %+v, want 1 stored and 1 duplicate", result)
	}
	if got := liveCount(t, pool, family, 1); got != 1 {
		t.Errorf("live samples = %d, want 1", got)
	}
}

// The two tiers of one family are separate keys: 58 of 153 families span more
// than one tier and the art may differ, so a tier-1 sample must never fill a
// tier-3 slot.
func TestRepository_Accept_TiersOfOneFamilyAreSeparateKeys(t *testing.T) {
	pool := integrationPool(t)
	repo := NewRepository(pool)
	family := reserveFamily(t, pool, "Combustion")
	ctx := context.Background()

	if _, err := repo.Accept(ctx, "device-a", integrationVersion,
		[]Candidate{candidate(t, family, 1, firstThird)}); err != nil {
		t.Fatalf("Accept tier 1: %v", err)
	}

	result, err := repo.Accept(ctx, "device-a", integrationVersion,
		[]Candidate{candidate(t, family, 3, firstThird)})
	if err != nil {
		t.Fatalf("Accept tier 3: %v", err)
	}
	if result.Stored != 1 {
		t.Fatalf("same art at another tier = %+v, want 1 stored", result)
	}
	if got := liveCount(t, pool, family, 3); got != 1 {
		t.Errorf("tier 3 live samples = %d, want 1", got)
	}
}

// Retiring a key drops its art from what every device pulls.
func TestRepository_Tombstone_RetiredSamplesLeaveTheServedCorpus(t *testing.T) {
	pool := integrationPool(t)
	repo := NewRepository(pool)
	family := reserveFamily(t, pool, "Consecration")
	requireDistinct(t, family)
	ctx := context.Background()

	patterns := distinctPatterns()
	if _, err := repo.Accept(ctx, "device-a", integrationVersion, []Candidate{
		candidate(t, family, 1, patterns[0]),
		candidate(t, family, 1, patterns[1]),
	}); err != nil {
		t.Fatalf("Accept: %v", err)
	}

	key, _ := NewKey(family, 1)
	marked, err := repo.Tombstone(ctx, integrationVersion, key)
	if err != nil {
		t.Fatalf("Tombstone: %v", err)
	}
	if marked != 2 {
		t.Fatalf("tombstoned = %d, want 2", marked)
	}

	corpus, err := repo.Corpus(ctx, integrationVersion)
	if err != nil {
		t.Fatalf("Corpus: %v", err)
	}
	for _, sample := range corpus.Templates {
		if sample.Key.Family == family {
			t.Fatalf("retired family %q is still served", family)
		}
	}
	found := false
	for _, tombstone := range corpus.Tombstones {
		if tombstone == key {
			found = true
		}
	}
	if !found {
		t.Errorf("retired key %v is not listed in the corpus tombstones", key)
	}
}

// The tombstone has to survive the device that published the bad art: until it
// pulls, that device still holds the sample and will offer it again.
func TestRepository_Tombstone_BlocksAReUploadOfTheRetiredKey(t *testing.T) {
	pool := integrationPool(t)
	repo := NewRepository(pool)
	family := reserveFamily(t, pool, "Ash")
	ctx := context.Background()

	if _, err := repo.Accept(ctx, "device-a", integrationVersion,
		[]Candidate{candidate(t, family, 1, firstThird)}); err != nil {
		t.Fatalf("Accept: %v", err)
	}
	key, _ := NewKey(family, 1)
	if _, err := repo.Tombstone(ctx, integrationVersion, key); err != nil {
		t.Fatalf("Tombstone: %v", err)
	}

	again, err := repo.Accept(ctx, "device-a", integrationVersion,
		[]Candidate{candidate(t, family, 1, firstThird)})
	if err != nil {
		t.Fatalf("re-upload Accept: %v", err)
	}
	if again.Tombstoned != 1 || again.Stored != 0 {
		t.Fatalf("re-upload of a retired key = %+v, want 1 tombstoned and 0 stored", again)
	}
	if got := liveCount(t, pool, family, 1); got != 0 {
		t.Errorf("live samples after a re-upload of a retired key = %d, want 0", got)
	}
}

// The key survives its retirement: art the pool has never held is stored under
// it and served, which is how a family whose art was thrown out is relearned.
func TestRepository_Tombstone_AcceptsNewArtUnderTheRetiredKey(t *testing.T) {
	pool := integrationPool(t)
	repo := NewRepository(pool)
	family := reserveFamily(t, pool, "Blasting")
	requireDistinct(t, family)
	ctx := context.Background()

	patterns := distinctPatterns()
	if _, err := repo.Accept(ctx, "device-a", integrationVersion,
		[]Candidate{candidate(t, family, 1, patterns[0])}); err != nil {
		t.Fatalf("Accept: %v", err)
	}
	key, _ := NewKey(family, 1)
	if _, err := repo.Tombstone(ctx, integrationVersion, key); err != nil {
		t.Fatalf("Tombstone: %v", err)
	}

	result, err := repo.Accept(ctx, "device-b", integrationVersion,
		[]Candidate{candidate(t, family, 1, patterns[1])})
	if err != nil {
		t.Fatalf("Accept new art: %v", err)
	}
	if result.Stored != 1 || result.Tombstoned != 0 {
		t.Fatalf("new art under a retired key = %+v, want 1 stored and 0 tombstoned", result)
	}
	if got := liveCount(t, pool, family, 1); got != 1 {
		t.Fatalf("live samples after relearning a retired key = %d, want 1", got)
	}

	corpus, err := repo.Corpus(ctx, integrationVersion)
	if err != nil {
		t.Fatalf("Corpus: %v", err)
	}
	served := 0
	for _, sample := range corpus.Templates {
		if sample.Key.Family == family {
			served++
		}
	}
	if served != 1 {
		t.Fatalf("corpus serves %d samples of the relearned key, want 1", served)
	}
}

// Retiring a key the pool never held is a no-op. Pre-emptive blocking would let
// one device deny a family to everyone before anyone had a chance to learn it.
func TestRepository_Tombstone_UnknownKey_RetiresNothing(t *testing.T) {
	pool := integrationPool(t)
	repo := NewRepository(pool)
	family := reserveFamily(t, pool, "Archers")
	ctx := context.Background()

	key, _ := NewKey(family, 2)
	marked, err := repo.Tombstone(ctx, integrationVersion, key)
	if err != nil {
		t.Fatalf("Tombstone: %v", err)
	}
	if marked != 0 {
		t.Fatalf("tombstoned = %d for a key nobody uploaded, want 0", marked)
	}

	result, err := repo.Accept(ctx, "device-a", integrationVersion,
		[]Candidate{candidate(t, family, 2, firstThird)})
	if err != nil {
		t.Fatalf("Accept: %v", err)
	}
	if result.Stored != 1 {
		t.Fatalf("upload after a no-op tombstone = %+v, want 1 stored", result)
	}
}

// The format version partitions the pool. Signatures from two formats are not
// comparable, so a version-2 sample must neither dedupe against nor be served
// alongside a version-1 one.
func TestRepository_Corpus_KeepsFormatVersionsApart(t *testing.T) {
	pool := integrationPool(t)
	repo := NewRepository(pool)
	family := reserveFamily(t, pool, "Brittle Chance")
	ctx := context.Background()

	// A legacy format-1 row, planted raw: Accept cannot make one any more —
	// NewSignature refuses 576 bytes — and this is the real post-POE-207 state,
	// a pool holding both formats at once for the same key.
	const legacyVersion int16 = 1
	if err := insertRawLength(t, pool, family, legacyVersion, 576); err != nil {
		t.Fatalf("plant a format-1 row: %v", err)
	}

	stored, err := repo.Accept(ctx, "device-a", integrationVersion,
		[]Candidate{candidate(t, family, 1, firstThird)})
	if err != nil {
		t.Fatalf("Accept v2: %v", err)
	}
	if stored.Stored != 1 {
		t.Fatalf("a v2 upload over an occupied v1 key = %+v, want 1 stored — the v1 sample "+
			"must not dedupe against it or count toward its cap", stored)
	}

	countFamily := func(version int16) int {
		t.Helper()
		corpus, err := repo.Corpus(ctx, version)
		if err != nil {
			t.Fatalf("Corpus v%d: %v", version, err)
		}
		seen := 0
		for _, sample := range corpus.Templates {
			if sample.Key.Family == family {
				seen++
			}
		}
		return seen
	}
	if got := countFamily(integrationVersion); got != 1 {
		t.Errorf("version-%d corpus holds %d samples of %q, want 1 (its own)",
			integrationVersion, got, family)
	}
	if got := countFamily(legacyVersion); got != 1 {
		t.Errorf("version-1 corpus holds %d samples of %q, want 1 (the legacy row, still served)",
			got, family)
	}
}

// What a device pulls is byte-for-byte what it would have produced locally: the
// corpus round-trips the stored signature, not a re-encoding of it.
func TestRepository_Corpus_ServesTheStoredSignatureBytes(t *testing.T) {
	pool := integrationPool(t)
	repo := NewRepository(pool)
	family := reserveFamily(t, pool, "Cascade Count")
	ctx := context.Background()

	uploaded := candidate(t, family, 1, firstThird)
	if _, err := repo.Accept(ctx, "device-a", integrationVersion, []Candidate{uploaded}); err != nil {
		t.Fatalf("Accept: %v", err)
	}

	corpus, err := repo.Corpus(ctx, integrationVersion)
	if err != nil {
		t.Fatalf("Corpus: %v", err)
	}
	for _, sample := range corpus.Templates {
		if sample.Key.Family != family {
			continue
		}
		// bytes.Equal, not a correlation: the promise this test makes is
		// byte-for-byte, and a correlation cannot see the difference. Two
		// buffers that differ by one grey level everywhere still correlate at
		// 1.0, so an NCC check would pass a round trip that rescaled or
		// re-encoded the stored bytes — exactly the regression the promise
		// exists to catch.
		if !bytes.Equal(sample.Signature, uploaded.Signature.Bytes()) {
			t.Fatalf("served signature differs from the uploaded bytes (%d vs %d bytes)",
				len(sample.Signature), len(uploaded.Signature.Bytes()))
		}
		// It is also still a decodable signature, not just a matching blob.
		if _, err := NewSignature(sample.Signature); err != nil {
			t.Fatalf("served signature is unreadable: %v", err)
		}
		return
	}
	t.Fatalf("uploaded family %q is missing from the served corpus", family)
}

// A retirement is not a permanent blacklist. Tombstones are unauthenticated by
// design and the fingerprint behind one is spoofable, so an expiry is what
// stops any device from taking correct art off the pool forever with no undo
// short of SQL.
func TestRepository_Accept_RetirementOlderThanTheWindow_NoLongerRefusesTheArt(t *testing.T) {
	pool := integrationPool(t)
	repo := NewRepository(pool)
	family := reserveFamily(t, pool, "Ailment Effect")
	ctx := context.Background()

	if _, err := repo.Accept(ctx, "device-a", integrationVersion,
		[]Candidate{candidate(t, family, 1, firstThird)}); err != nil {
		t.Fatalf("Accept: %v", err)
	}
	key, _ := NewKey(family, 1)
	if _, err := repo.Tombstone(ctx, integrationVersion, key); err != nil {
		t.Fatalf("Tombstone: %v", err)
	}

	// While the retirement is in force the art is refused — the control that
	// makes the expiry below mean something.
	blocked, err := repo.Accept(ctx, "device-a", integrationVersion,
		[]Candidate{candidate(t, family, 1, firstThird)})
	if err != nil {
		t.Fatalf("Accept while retired: %v", err)
	}
	if blocked.Tombstoned != 1 {
		t.Fatalf("re-upload inside the window = %+v, want 1 tombstoned", blocked)
	}

	backdateRetirement(t, pool, family, expiredRetirementAge)

	after, err := repo.Accept(ctx, "device-a", integrationVersion,
		[]Candidate{candidate(t, family, 1, firstThird)})
	if err != nil {
		t.Fatalf("Accept after the window: %v", err)
	}
	if after.Stored != 1 || after.Tombstoned != 0 {
		t.Fatalf("re-upload after the window = %+v, want 1 stored and 0 tombstoned", after)
	}
	if got := liveCount(t, pool, family, 1); got != 1 {
		t.Errorf("live samples = %d, want 1", got)
	}
}

// The served tombstone list follows the same window: a client must not be told
// to drop a key over a retirement the server has stopped enforcing.
func TestRepository_Corpus_DropsTombstonesOlderThanTheWindow(t *testing.T) {
	pool := integrationPool(t)
	repo := NewRepository(pool)
	family := reserveFamily(t, pool, "Ailment Damage")
	ctx := context.Background()

	if _, err := repo.Accept(ctx, "device-a", integrationVersion,
		[]Candidate{candidate(t, family, 1, firstThird)}); err != nil {
		t.Fatalf("Accept: %v", err)
	}
	key, _ := NewKey(family, 1)
	if _, err := repo.Tombstone(ctx, integrationVersion, key); err != nil {
		t.Fatalf("Tombstone: %v", err)
	}

	if !listsTombstone(t, repo, key) {
		t.Fatal("a fresh retirement is not listed in the corpus tombstones")
	}

	backdateRetirement(t, pool, family, expiredRetirementAge)

	if listsTombstone(t, repo, key) {
		t.Fatalf("retirement of %v is still listed after the window elapsed", key)
	}
}

func listsTombstone(t *testing.T, repo *Repository, key Key) bool {
	t.Helper()
	corpus, err := repo.Corpus(context.Background(), integrationVersion)
	if err != nil {
		t.Fatalf("Corpus: %v", err)
	}
	for _, listed := range corpus.Tombstones {
		if listed == key {
			return true
		}
	}
	return false
}

// The cap survives concurrency, which read-then-insert alone cannot promise:
// two uploads racing for the last slot would both read two live samples and
// both insert. Repeated because a lock bug is a race — one pass proves nothing.
func TestRepository_Accept_ConcurrentUploadsForTheLastSlot_StoreExactlyOne(t *testing.T) {
	pool := integrationPool(t)
	repo := NewRepository(pool)
	family := reserveFamily(t, pool, "Charged Traps")
	requireDistinct(t, family)
	patterns := distinctPatterns()
	ctx := context.Background()

	const rounds = 20
	for round := 0; round < rounds; round++ {
		if _, err := pool.Exec(ctx, `DELETE FROM merc_icon_templates WHERE family = $1`, family); err != nil {
			t.Fatalf("round %d: clear key: %v", round, err)
		}
		if _, err := repo.Accept(ctx, "seed", integrationVersion, []Candidate{
			candidate(t, family, 1, patterns[0]),
			candidate(t, family, 1, patterns[1]),
		}); err != nil {
			t.Fatalf("round %d: seed two samples: %v", round, err)
		}

		results := make([]AcceptResult, 2)
		errs := make([]error, 2)
		var wg sync.WaitGroup
		for i := 0; i < 2; i++ {
			wg.Add(1)
			go func(i int) {
				defer wg.Done()
				results[i], errs[i] = repo.Accept(ctx, "racer", integrationVersion,
					[]Candidate{candidate(t, family, 1, patterns[2+i])})
			}(i)
		}
		wg.Wait()

		for i, err := range errs {
			if err != nil {
				t.Fatalf("round %d: racer %d: %v", round, i, err)
			}
		}
		stored := results[0].Stored + results[1].Stored
		capped := results[0].Capped + results[1].Capped
		if stored != 1 || capped != 1 {
			t.Fatalf("round %d: two racers for one slot = %d stored, %d capped; want 1 and 1 (%+v, %+v)",
				round, stored, capped, results[0], results[1])
		}
		if got := liveCount(t, pool, family, 1); got != MaxSamplesPerKey {
			t.Fatalf("round %d: live samples = %d, want %d", round, got, MaxSamplesPerKey)
		}
	}
}

// Two multi-key uploads landing at once both complete. They queue on the one
// pool-wide advisory lock instead of taking a lock per key, which is what
// removed the lock-ordering problem the old per-key locks had to sort around:
// there is no longer a second lock for a transaction to hold while it waits.
//
// Each family carries its OWN art here. The same picture under two families is
// now a conflict by design, so building both batches from one pattern — which
// the per-key version of this test did — would assert the refusal rule instead
// of the lock.
func TestRepository_Accept_ConcurrentMultiKeyUploads_BothSucceedUnderThePoolLock(t *testing.T) {
	pool := integrationPool(t)
	repo := NewRepository(pool)
	first := reserveFamily(t, pool, "Cold Penetration")
	second := reserveFamily(t, pool, "Critical Chance")
	requireDistinct(t, first)
	patterns := distinctPatterns()

	// A lock that never released would otherwise hang the test until the whole
	// suite times out; with a deadline it surfaces as a failure naming this test.
	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancel()

	forward := []Candidate{
		candidate(t, first, 1, patterns[0]),
		candidate(t, second, 1, patterns[1]),
	}
	backward := []Candidate{
		candidate(t, second, 2, patterns[1]),
		candidate(t, first, 2, patterns[0]),
	}

	results := make([]AcceptResult, 2)
	errs := make([]error, 2)
	var wg sync.WaitGroup
	for i, batch := range [][]Candidate{forward, backward} {
		wg.Add(1)
		go func(i int, batch []Candidate) {
			defer wg.Done()
			results[i], errs[i] = repo.Accept(ctx, "racer", integrationVersion, batch)
		}(i, batch)
	}
	wg.Wait()

	for i, err := range errs {
		if err != nil {
			t.Fatalf("batch %d failed (a lock that never released looks like this): %v", i, err)
		}
	}
	if results[0].Stored != 2 || results[1].Stored != 2 {
		t.Fatalf("stored = %d and %d, want 2 each: %+v %+v",
			results[0].Stored, results[1].Stored, results[0], results[1])
	}
}

// clearFamilies wipes several families between rounds of a repeated test — the
// same delete reserveFamily brackets a test with, needed mid-test here.
func clearFamilies(t *testing.T, pool *pgxpool.Pool, families ...string) {
	t.Helper()
	for _, family := range families {
		clearFamily(t, pool, family, "between-rounds")
	}
}

// One picture belongs to one family. A device that hovered the wrong cell can
// offer the same art under two families in a single batch, and the pool keeps
// the first claim and refuses the second — whichever order they arrive in,
// because a stored candidate joins the live view the rest of the batch is
// checked against.
//
// Without that within-batch growth the two would be decided against a snapshot
// that contains neither, and both would be stored.
func TestRepository_Accept_SameArtUnderTwoFamiliesInOneBatch_StoresTheFirstAndRefusesTheSecond(t *testing.T) {
	pool := integrationPool(t)
	repo := NewRepository(pool)
	alpha := reserveFamily(t, pool, "Added Chaos")
	beta := reserveFamily(t, pool, "Added Cold")
	ctx := context.Background()

	for _, order := range [][2]string{{alpha, beta}, {beta, alpha}} {
		winner, loser := order[0], order[1]
		t.Run(winner+"-first", func(t *testing.T) {
			clearFamilies(t, pool, alpha, beta)

			result, err := repo.Accept(ctx, "device-a", integrationVersion, []Candidate{
				candidate(t, winner, 1, firstThird),
				candidate(t, loser, 1, firstThird),
			})
			if err != nil {
				t.Fatalf("Accept: %v", err)
			}

			if result.Stored != 1 || result.Conflicting != 1 {
				t.Fatalf("one art claimed by two families = %+v, want 1 stored and 1 conflicting", result)
			}
			if len(result.Conflicts) != 1 {
				t.Fatalf("conflicts = %+v, want 1 entry", result.Conflicts)
			}
			got := result.Conflicts[0]
			if got.Index != 1 {
				t.Errorf("conflict index = %d, want 1 (the second candidate is the refused one)", got.Index)
			}
			if got.Key.Family != loser {
				t.Errorf("refused family = %q, want %q", got.Key.Family, loser)
			}
			if got.IncumbentFamily != winner {
				t.Errorf("incumbent = %q, want %q — the log line the player acts on names it",
					got.IncumbentFamily, winner)
			}
			if live := liveCount(t, pool, winner, 1); live != 1 {
				t.Errorf("%q live samples = %d, want 1", winner, live)
			}
			if live := liveCount(t, pool, loser, 1); live != 0 {
				t.Errorf("%q live samples = %d, want 0", loser, live)
			}
		})
	}
}

// The batch shape that tells a correct hoisted foreign view from a stale one.
//
// Accept builds the cross-family view once per DISTINCT family and drops it on
// every store, and only a batch that RETURNS to a family after storing under
// another one can see the difference: candidate 3 is decided for a family whose
// view was built before candidate 2 put art in the pool. Without the
// invalidation it is checked against a view missing that art and stored, and the
// pool ends up serving one picture under two families — the state the conflict
// rule exists to prevent. A two-candidate batch cannot catch this: its second
// family has no cached view yet.
func TestRepository_Accept_FamilyRevisitedAfterAnotherFamilyStored_StillSeesTheNewArt(t *testing.T) {
	pool := integrationPool(t)
	repo := NewRepository(pool)
	revisited := reserveFamily(t, pool, "Additional Duration")
	other := reserveFamily(t, pool, "Additional Leech")
	requireDistinct(t, revisited)
	patterns := distinctPatterns()
	ctx := context.Background()

	result, err := repo.Accept(ctx, "device-a", integrationVersion, []Candidate{
		candidate(t, revisited, 1, patterns[0]),
		candidate(t, other, 1, patterns[1]),
		// Same art as the candidate above, offered back under the family whose
		// view was built first.
		candidate(t, revisited, 3, patterns[1]),
	})
	if err != nil {
		t.Fatalf("Accept: %v", err)
	}

	if result.Stored != 2 || result.Conflicting != 1 {
		t.Fatalf("batch revisiting a family after a foreign store = %+v, want 2 stored and 1 conflicting",
			result)
	}
	if len(result.Conflicts) != 1 {
		t.Fatalf("conflicts = %+v, want 1 entry", result.Conflicts)
	}
	got := result.Conflicts[0]
	if got.Index != 2 || got.Key.Family != revisited || got.Key.Tier != 3 {
		t.Errorf("conflict = index %d on %v, want index 2 on %s--3", got.Index, got.Key, revisited)
	}
	if got.IncumbentFamily != other {
		t.Errorf("incumbent = %q, want %q — the art stored earlier in this same batch",
			got.IncumbentFamily, other)
	}
	if live := liveCount(t, pool, revisited, 3); live != 0 {
		t.Errorf("%q tier-3 live samples = %d, want 0: the conflicting art must not be stored",
			revisited, live)
	}
}

// The refusal has to survive concurrency, which is the whole reason the lock
// moved from per-key to per-version: two devices claiming one picture for two
// families take two DIFFERENT key locks, each reads a pool without the other's
// row, and both store. Repeated because a lock bug is a race — one pass proves
// nothing.
func TestRepository_Accept_ConcurrentUploadsOfOneArtUnderTwoFamilies_StoreExactlyOne(t *testing.T) {
	pool := integrationPool(t)
	repo := NewRepository(pool)
	families := [2]string{
		reserveFamily(t, pool, "Added Fire"),
		reserveFamily(t, pool, "Added Lightning"),
	}
	ctx := context.Background()

	const rounds = 20
	for round := 0; round < rounds; round++ {
		clearFamilies(t, pool, families[0], families[1])

		results := make([]AcceptResult, 2)
		errs := make([]error, 2)
		var wg sync.WaitGroup
		for i := 0; i < 2; i++ {
			wg.Add(1)
			go func(i int) {
				defer wg.Done()
				results[i], errs[i] = repo.Accept(ctx, "racer", integrationVersion,
					[]Candidate{candidate(t, families[i], 1, firstThird)})
			}(i)
		}
		wg.Wait()

		for i, err := range errs {
			if err != nil {
				t.Fatalf("round %d: racer %d: %v", round, i, err)
			}
		}
		stored := results[0].Stored + results[1].Stored
		conflicting := results[0].Conflicting + results[1].Conflicting
		if stored != 1 || conflicting != 1 {
			t.Fatalf("round %d: two families racing for one picture = %d stored, %d conflicting; "+
				"want 1 and 1 (%+v, %+v)", round, stored, conflicting, results[0], results[1])
		}
		live := liveCount(t, pool, families[0], 1) + liveCount(t, pool, families[1], 1)
		if live != 1 {
			t.Fatalf("round %d: live samples across both families = %d, want 1", round, live)
		}
	}
}

// The way out of a mislabel: retire the wrong key, and the family whose art it
// really is can finally pool it. Retired art of another family is not served to
// anybody, so it has nothing to be confused with and must not go on refusing
// the correct upload — otherwise the first writer owns the picture forever and
// the conflict rule has no undo short of SQL.
func TestRepository_Accept_RetiringTheIncumbentUnblocksTheConflictingArt(t *testing.T) {
	pool := integrationPool(t)
	repo := NewRepository(pool)
	mislabel := reserveFamily(t, pool, "Arcane Traps")
	correct := reserveFamily(t, pool, "Arrow Nova")
	ctx := context.Background()

	if _, err := repo.Accept(ctx, "device-a", integrationVersion,
		[]Candidate{candidate(t, mislabel, 1, firstThird)}); err != nil {
		t.Fatalf("Accept the mislabel: %v", err)
	}

	// While the mislabel is live the correct family is refused — the control
	// that makes the retirement below mean something.
	blocked, err := repo.Accept(ctx, "device-b", integrationVersion,
		[]Candidate{candidate(t, correct, 1, firstThird)})
	if err != nil {
		t.Fatalf("Accept while the mislabel is live: %v", err)
	}
	if blocked.Conflicting != 1 || blocked.Stored != 0 {
		t.Fatalf("upload against a live incumbent = %+v, want 1 conflicting and 0 stored", blocked)
	}

	key, _ := NewKey(mislabel, 1)
	if _, err := repo.Tombstone(ctx, integrationVersion, key); err != nil {
		t.Fatalf("Tombstone the mislabel: %v", err)
	}

	after, err := repo.Accept(ctx, "device-b", integrationVersion,
		[]Candidate{candidate(t, correct, 1, firstThird)})
	if err != nil {
		t.Fatalf("Accept after retiring the mislabel: %v", err)
	}
	if after.Stored != 1 || after.Conflicting != 0 {
		t.Fatalf("upload after retiring the incumbent = %+v, want 1 stored and 0 conflicting", after)
	}
	if live := liveCount(t, pool, correct, 1); live != 1 {
		t.Errorf("%q live samples = %d, want 1", correct, live)
	}
}

// reserveOrphanFamily is reserveFamily for a name the vocabulary does NOT
// carry — the shape a family takes after a rename orphans its key.
func reserveOrphanFamily(t *testing.T, pool *pgxpool.Pool, family string) string {
	t.Helper()
	if _, known := knownFamilies[family]; known {
		t.Fatalf("test setup: %q is in the vocabulary, so it is not an orphan", family)
	}
	clearFamily(t, pool, family, "pre-test")
	t.Cleanup(func() { clearFamily(t, pool, family, "post-test") })
	return family
}

// insertRawSample writes a row straight to the table, bypassing Accept.
//
// Accept cannot create this row: it validates the family against the shipped
// vocabulary. That is precisely the state under test — art pooled while the
// family WAS in the vocabulary, still sitting there after a rename removed it.
func insertRawSample(t *testing.T, pool *pgxpool.Pool, family string, tier int, sig Signature) {
	t.Helper()
	if _, err := pool.Exec(context.Background(),
		`INSERT INTO merc_icon_templates (family, tier, format_version, signature, device_id)
		 VALUES ($1, $2, $3, $4, $5)`,
		family, tier, integrationVersion, sig.Bytes(), "device-legacy"); err != nil {
		t.Fatalf("insert legacy sample for %q: %v", family, err)
	}
}

// A rename orphans a key: the pool still holds art under the old name and no
// hover will ever match it again. Retiring that art is the whole reason
// tombstones exist, so removal must not be gated on the vocabulary the way
// admission is.
func TestRepository_Tombstone_RetiresAFamilyTheVocabularyNoLongerCarries(t *testing.T) {
	pool := integrationPool(t)
	repo := NewRepository(pool)
	family := reserveOrphanFamily(t, pool, "Formerly A Support")
	ctx := context.Background()

	insertRawSample(t, pool, family, 1, mustSignature(t, patternRGB(firstThird, 1, 255, 200)))
	insertRawSample(t, pool, family, 1,
		mustSignature(t, patternRGB(func(o int) bool { return o%2 == 0 }, 1, 255, 200)))

	key, err := ParseKey(family, 1)
	if err != nil {
		t.Fatalf("ParseKey rejected an orphaned family: %v", err)
	}

	marked, err := repo.Tombstone(ctx, integrationVersion, key)
	if err != nil {
		t.Fatalf("Tombstone: %v", err)
	}
	if marked != 2 {
		t.Fatalf("tombstoned = %d, want 2", marked)
	}
	if got := liveCount(t, pool, family, 1); got != 0 {
		t.Errorf("live samples after retiring the orphan = %d, want 0", got)
	}

	corpus, err := repo.Corpus(ctx, integrationVersion)
	if err != nil {
		t.Fatalf("Corpus: %v", err)
	}
	for _, sample := range corpus.Templates {
		if sample.Key.Family == family {
			t.Fatalf("orphaned family %q is still served after retirement", family)
		}
	}
}
