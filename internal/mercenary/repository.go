package mercenary

import (
	"context"
	"fmt"
	"log/slog"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
)

// Repository persists the shared template pool in merc_icon_templates.
type Repository struct {
	pool *pgxpool.Pool
	// now is the clock behind the retirement window, injectable for tests.
	now func() time.Time
}

// NewRepository creates a Repository backed by the given connection pool.
func NewRepository(pool *pgxpool.Pool) *Repository {
	return &Repository{pool: pool, now: time.Now}
}

// retirementCutoff is the oldest tombstone still in force. Computed here rather
// than as NOW() - INTERVAL in SQL so the window has one definition
// (RetiredMatchWindow) and a test can move the clock.
func (r *Repository) retirementCutoff() time.Time {
	return r.now().Add(-RetiredMatchWindow)
}

const (
	// lockPoolSQL serialises every concurrent upload of ONE format version.
	//
	// It replaced a per-key lock, which could not carry the cross-family
	// conflict rule: two devices uploading the same art under two different
	// families take two different key locks, both read a pool without the
	// other's row, and both store — the exact state the rule exists to
	// prevent. The unit the rule reasons about is the version's whole live
	// view, so that is the unit the lock has to cover. It still carries the
	// cap ("never a fourth live sample"), which read-then-insert cannot
	// promise on its own.
	//
	// Cost of holding it: one upload is at most MaxTemplatesPerUpload candidates
	// against a saturated version. The batch factor is the SERVER's gate, not the
	// desktop's — the desktop sends at most 32 (MAX_TEMPLATES_PER_BATCH), but the
	// handler is what admits a request and a client that is not the desktop owes
	// that number nothing, so the worst request the endpoint accepts is what this
	// has to be sized for.
	//
	// The pool ceiling is DERIVED, never a literal — KnownFamilyCount() families
	// x 3 tiers x MaxSamplesPerKey, i.e. 1377 live rows across 459 keys at the
	// current 153-family vocabulary. (The create migration's 792-over-264 is not
	// an older reading of that derivation, it is simply wrong: 264 keys matches
	// no vocabulary this repository has ever carried — families.go held 154 the
	// day that comment was written, i.e. 462 keys. It stays as written because
	// the migration is applied, and it is the reason nothing here spells the
	// product out.) That is ~152M multiply-adds over 1728-float signatures, plus
	// ~1377 NewSignature decodes — the smaller half.
	//
	// BenchmarkDecide_MaxBatchAgainstFullForeignView measures the correlation
	// half at 97,214,752 ns/op — 97 ms for a full 64-template batch, measured
	// 2026-08-27 on an AMD Ryzen AI 5 340. (The desktop's own 32 costs about
	// half that: 49 ms, same machine and day.) Against a per-device budget of 60
	// writes per 10 minutes (ratelimit.go) that serialisation is affordable; if
	// either bound moves — a bigger batch cap, a bigger vocabulary — re-run the
	// benchmark before assuming it still is.
	//
	// Transaction-scoped (pg_advisory_xact_lock, released at COMMIT) rather
	// than session-scoped, because there is a pgbouncer in front of Postgres
	// running pool_mode = transaction (internal/db/db.go): a session lock would
	// be taken on one backend and released on whichever backend the next
	// statement landed on, i.e. never.
	lockPoolSQL = `SELECT pg_advisory_xact_lock(hashtextextended($1, 0))`

	// poolSQL loads one format version's whole decidable state in ONE read:
	// the rows of the keys a batch touches, and every live row in the version.
	//
	// Not filtered by family any more. The cross-family rule compares a
	// candidate against every live sample under a DIFFERENT family, so the
	// families in the request no longer bound what the decision needs to see.
	// The (format_version, family, tier) index still prefixes this — the
	// version is pinned — and the live half of the result is bounded by the
	// same live-row ceiling the serve path already reads whole (see
	// lockPoolSQL for the derivation).
	//
	// One read, not two: a second query for the foreign view would decode every
	// live signature twice for the families the batch does name.
	//
	// $2 is the retirement cutoff (RetiredMatchWindow ago). A retirement older
	// than that is dropped from the state entirely, so its art stops being
	// refused — the row stays retired and unserved, it just no longer votes.
	//
	// ORDER BY is not cosmetic: the live rows become the foreign view Decide
	// walks, and the FIRST match in that order is the incumbent a conflict is
	// reported against. Without it Postgres may hand back two equally valid
	// orders for the same data, and two identical uploads would name different
	// families as the incumbent the player is told to forget.
	poolSQL = `SELECT family, tier, signature, tombstoned_at IS NOT NULL
	           FROM merc_icon_templates
	           WHERE format_version = $1
	             AND (tombstoned_at IS NULL OR tombstoned_at > $2)
	           ORDER BY family, tier, id`

	insertSQL = `INSERT INTO merc_icon_templates (family, tier, format_version, signature, device_id)
	             VALUES ($1, $2, $3, $4, $5)`

	corpusSQL = `SELECT family, tier, signature
	             FROM merc_icon_templates
	             WHERE format_version = $1 AND tombstoned_at IS NULL
	             ORDER BY family, tier, id`

	// Served tombstones follow the same window as the match targets: a client
	// must not be told to drop a key over a retirement the server itself has
	// stopped enforcing.
	tombstonesSQL = `SELECT DISTINCT family, tier
	                 FROM merc_icon_templates
	                 WHERE format_version = $1 AND tombstoned_at > $2
	                 ORDER BY family, tier`

	tombstoneSQL = `UPDATE merc_icon_templates
	                SET tombstoned_at = NOW()
	                WHERE format_version = $1 AND family = $2 AND tier = $3
	                  AND tombstoned_at IS NULL`
)

// Accept applies the pool rules to a batch of candidates and stores what
// survives them, attributing every stored row to deviceID.
//
// The whole batch runs in one transaction holding ONE advisory lock covering
// the format version, so neither a concurrent upload of the same key nor a
// concurrent upload of the same art under another family can slip past the cap
// or the conflict rule. One lock rather than one per key is also why there is
// no lock ordering left to get wrong: two requests can no longer hold half of
// each other's key set.
//
// Candidates are decided in request order against a state that GROWS as the
// batch is applied — two identical templates in one upload resolve as one
// stored and one duplicate, not as two stored — and a stored candidate joins
// the live view every LATER candidate of another family is checked against, so
// one batch claiming the same art for two families keeps the first and refuses
// the second whichever order they arrive in.
func (r *Repository) Accept(ctx context.Context, deviceID string, version int16, candidates []Candidate) (AcceptResult, error) {
	var result AcceptResult
	if len(candidates) == 0 {
		return result, nil
	}

	tx, err := r.pool.Begin(ctx)
	if err != nil {
		return result, fmt.Errorf("merc templates: begin tx: %w", err)
	}
	defer tx.Rollback(ctx)

	lockName := fmt.Sprintf("merc_icon_templates:%d", version)
	if _, err := tx.Exec(ctx, lockPoolSQL, lockName); err != nil {
		return result, fmt.Errorf("merc templates: lock pool %s: %w", lockName, err)
	}

	state, live, err := loadPool(ctx, tx, version, batchKeys(candidates), r.retirementCutoff())
	if err != nil {
		return result, err
	}

	batch := &pgx.Batch{}
	queued := 0
	// otherFamilies copies the whole live view, so calling it per candidate is
	// 32 copies of up to 1377 samples inside the lock. Build one view per
	// DISTINCT family in the batch instead — a batch names far fewer families
	// than it has candidates.
	//
	// Invalidated WHOLESALE on any store, which is the simplest form that
	// cannot serve a stale view: a stored candidate has to become visible to
	// every later candidate of every other family, and dropping the map says
	// that without a per-entry rule to get wrong. Stores are the rare case —
	// the batch a saturated pool costs most is the one where nothing is stored
	// and nothing is rebuilt.
	foreignByFamily := make(map[string][]ForeignSample)
	for index, candidate := range candidates {
		current := state[candidate.Key]
		foreign, cached := foreignByFamily[candidate.Key.Family]
		if !cached {
			foreign = otherFamilies(live, candidate.Key.Family)
			foreignByFamily[candidate.Key.Family] = foreign
		}
		outcome, incumbent := Decide(current, foreign, candidate.Signature)
		result.Record(outcome)
		if outcome == Conflicting {
			result.Conflicts = append(result.Conflicts, Conflict{
				Index:           index,
				Key:             candidate.Key,
				IncumbentFamily: incumbent.Key.Family,
			})
			continue
		}
		if outcome != Stored {
			continue
		}
		current.Live = append(current.Live, candidate.Signature)
		state[candidate.Key] = current
		live = append(live, ForeignSample{Key: candidate.Key, Signature: candidate.Signature})
		clear(foreignByFamily)
		batch.Queue(insertSQL, candidate.Key.Family, candidate.Key.Tier, version,
			candidate.Signature.Bytes(), deviceID)
		queued++
	}

	if queued > 0 {
		br := tx.SendBatch(ctx, batch)
		for i := 0; i < queued; i++ {
			if _, execErr := br.Exec(); execErr != nil {
				br.Close()
				return AcceptResult{}, fmt.Errorf("merc templates: insert sample: %w", execErr)
			}
		}
		if err := br.Close(); err != nil {
			return AcceptResult{}, fmt.Errorf("merc templates: close insert batch: %w", err)
		}
	}

	if err := tx.Commit(ctx); err != nil {
		return AcceptResult{}, fmt.Errorf("merc templates: commit: %w", err)
	}
	return result, nil
}

// batchKeys returns the set of keys a batch touches. A set, not a sorted list:
// nothing is locked per key any more, so the only thing the caller asks of it
// is membership — "does this row belong to a key this request decides on".
func batchKeys(candidates []Candidate) map[Key]struct{} {
	keys := make(map[Key]struct{}, len(candidates))
	for _, c := range candidates {
		keys[c.Key] = struct{}{}
	}
	return keys
}

// loadPool reads, inside the caller's transaction, both halves of what the
// accept rule needs for one format version: the per-key state of the keys this
// batch touches, and every live sample in the version.
//
// The live view is the cross-family conflict rule's input, so it spans families
// nobody in this request named — that is the point. Retired rows are decoded
// only for the batch's own keys: retired art of another family blocks nothing
// (it is not served, and refusing over it would break tombstone-then-relearn),
// so decoding it would be work with no reader.
func loadPool(ctx context.Context, tx pgx.Tx, version int16, keys map[Key]struct{}, cutoff time.Time) (map[Key]KeyState, []ForeignSample, error) {
	state := make(map[Key]KeyState, len(keys))
	// Capacity is the saturated live ceiling, derived rather than spelled out:
	// every family, at every one of the three tiers, holding MaxSamplesPerKey.
	live := make([]ForeignSample, 0, MaxSamplesPerKey*3*KnownFamilyCount())

	rows, err := tx.Query(ctx, poolSQL, version, cutoff)
	if err != nil {
		return nil, nil, fmt.Errorf("merc templates: load pool state: %w", err)
	}
	defer rows.Close()

	for rows.Next() {
		var (
			family     string
			tier       int16
			raw        []byte
			tombstoned bool
		)
		if err := rows.Scan(&family, &tier, &raw, &tombstoned); err != nil {
			return nil, nil, fmt.Errorf("merc templates: scan pool state: %w", err)
		}
		key := Key{Family: family, Tier: tier}
		_, inBatch := keys[key]
		// Skip the decode for a row nothing will look at. A retired row is only
		// ever read on its OWN key, so one belonging to a key this batch does
		// not name has no reader, and NewSignature below is the expensive part
		// of this loop. Pure optimisation: dropping this guard changes no
		// outcome, only the work done to reach it.
		if tombstoned && !inBatch {
			continue
		}
		sig, err := NewSignature(raw)
		if err != nil {
			// From POE-207 the column's length CHECK is version-CONDITIONAL
			// (576 for format 1, 1728 for format 2), so a stored row being
			// well-formed no longer means it is well-formed FOR THIS decode.
			// The realistic cause is a format-1 row reaching a format-2 read —
			// which the `WHERE format_version = $1` filter above is supposed to
			// make impossible, so reaching here means that filter was dropped
			// or the version was mis-plumbed, not that bytes rotted on disk.
			// Skipping a live sample costs a redundant store; skipping a
			// retired one lets the art it recorded back in. Both are worse the
			// other way round: failing the upload would cost every device the
			// whole key.
			slog.Error("merc templates: stored signature unreadable", "key", key.String(), "error", err)
			continue
		}
		if tombstoned {
			// A retired row is kept, not deleted, precisely so it can be
			// recognised again: this is what refuses a republish of the art
			// somebody threw out.
			//
			// The `continue` here is what enforces "retired art never joins the
			// live view", and with it the rest of the rule: retired art is not
			// served, so nothing out there can be confused with it, and letting
			// it refuse a cross-family upload would leave the first writer
			// owning a picture forever — tombstoning the mislabel is the
			// documented way to hand it back.
			current := state[key]
			current.Retired = append(current.Retired, sig)
			state[key] = current
			continue
		}
		live = append(live, ForeignSample{Key: key, Signature: sig})
		if inBatch {
			current := state[key]
			current.Live = append(current.Live, sig)
			state[key] = current
		}
	}
	if err := rows.Err(); err != nil {
		return nil, nil, fmt.Errorf("merc templates: iterate pool state: %w", err)
	}
	return state, live, nil
}

// Corpus returns every live sample for a format version plus the keys retired
// under it. Neither carries a device id: attribution stays in the table.
func (r *Repository) Corpus(ctx context.Context, version int16) (Corpus, error) {
	corpus := Corpus{FormatVersion: version, Templates: []Sample{}, Tombstones: []Key{}}

	rows, err := r.pool.Query(ctx, corpusSQL, version)
	if err != nil {
		return Corpus{}, fmt.Errorf("merc templates: query corpus: %w", err)
	}
	defer rows.Close()
	for rows.Next() {
		var sample Sample
		if err := rows.Scan(&sample.Key.Family, &sample.Key.Tier, &sample.Signature); err != nil {
			return Corpus{}, fmt.Errorf("merc templates: scan corpus row: %w", err)
		}
		corpus.Templates = append(corpus.Templates, sample)
	}
	if err := rows.Err(); err != nil {
		return Corpus{}, fmt.Errorf("merc templates: iterate corpus: %w", err)
	}

	tombRows, err := r.pool.Query(ctx, tombstonesSQL, version, r.retirementCutoff())
	if err != nil {
		return Corpus{}, fmt.Errorf("merc templates: query tombstones: %w", err)
	}
	defer tombRows.Close()
	for tombRows.Next() {
		var key Key
		if err := tombRows.Scan(&key.Family, &key.Tier); err != nil {
			return Corpus{}, fmt.Errorf("merc templates: scan tombstone: %w", err)
		}
		corpus.Tombstones = append(corpus.Tombstones, key)
	}
	if err := tombRows.Err(); err != nil {
		return Corpus{}, fmt.Errorf("merc templates: iterate tombstones: %w", err)
	}

	return corpus, nil
}

// Tombstone retires every live sample of a key and returns how many it marked.
//
// What is retired is the ART, not the key: the marked rows stop being served
// and are matched against later uploads so the sample cannot be republished,
// while the key stays open to better art for the same family and tier. That is
// what makes tombstone-then-relearn work after a rename orphans a key.
//
// Zero means the key held nothing live — either it was already retired, or
// nobody ever pooled it. Nothing is recorded in that case, which is the
// intended no-op: a tombstone removes art that is in the pool, and there is no
// art here to refuse later.
func (r *Repository) Tombstone(ctx context.Context, version int16, key Key) (int, error) {
	tag, err := r.pool.Exec(ctx, tombstoneSQL, version, key.Family, key.Tier)
	if err != nil {
		return 0, fmt.Errorf("merc templates: tombstone %s: %w", key, err)
	}
	return int(tag.RowsAffected()), nil
}
