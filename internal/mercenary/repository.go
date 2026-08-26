package mercenary

import (
	"context"
	"fmt"
	"log/slog"
	"sort"
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
	// lockKeySQL serialises concurrent uploads that touch the same key.
	//
	// The cap is "never a fourth live sample", and read-then-insert cannot
	// promise that on its own: two devices uploading different art for a
	// two-sample key would both read 2 and both insert. The lock is
	// transaction-scoped (pg_advisory_xact_lock, released at COMMIT) rather
	// than session-scoped, so it is safe under either pgbouncer pooling mode —
	// a session lock would zombie under transaction pooling.
	lockKeySQL = `SELECT pg_advisory_xact_lock(hashtextextended($1, 0))`

	// stateSQL loads every row for the requested families at one format
	// version. It over-fetches by tier (a family's other tiers come along) and
	// that is deliberate: filtering on families alone keeps the predicate a
	// single array parameter, and a family holds at most 3 tiers x 3 samples.
	//
	// $3 is the retirement cutoff (RetiredMatchWindow ago). A retirement older
	// than that is dropped from the state entirely, so its art stops being
	// refused — the row stays retired and unserved, it just no longer votes.
	stateSQL = `SELECT family, tier, signature, tombstoned_at IS NOT NULL
	            FROM merc_icon_templates
	            WHERE format_version = $1 AND family = ANY($2)
	              AND (tombstoned_at IS NULL OR tombstoned_at > $3)`

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
// The whole batch runs in one transaction holding one advisory lock per
// distinct key, so a concurrent upload of the same key cannot slip past the
// cap. Locks are taken in sorted order, which is what makes two requests
// touching an overlapping set of keys queue instead of deadlock.
//
// Candidates are decided in request order against a state that GROWS as the
// batch is applied: two identical templates in one upload therefore resolve as
// one stored and one duplicate, not as two stored.
func (r *Repository) Accept(ctx context.Context, deviceID string, version int16, candidates []Candidate) (AcceptResult, error) {
	var result AcceptResult
	if len(candidates) == 0 {
		return result, nil
	}

	keys := distinctKeys(candidates)

	tx, err := r.pool.Begin(ctx)
	if err != nil {
		return result, fmt.Errorf("merc templates: begin tx: %w", err)
	}
	defer tx.Rollback(ctx)

	for _, key := range keys {
		lockName := fmt.Sprintf("merc_icon_templates:%d:%s:%d", version, key.Family, key.Tier)
		if _, err := tx.Exec(ctx, lockKeySQL, lockName); err != nil {
			return result, fmt.Errorf("merc templates: lock key %s: %w", key, err)
		}
	}

	state, err := loadState(ctx, tx, version, keys, r.retirementCutoff())
	if err != nil {
		return result, err
	}

	batch := &pgx.Batch{}
	queued := 0
	for _, candidate := range candidates {
		current := state[candidate.Key]
		outcome := Decide(current, candidate.Signature)
		result.Record(outcome)
		if outcome != Stored {
			continue
		}
		current.Live = append(current.Live, candidate.Signature)
		state[candidate.Key] = current
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

// distinctKeys returns the batch's keys, deduplicated and sorted. Sorted
// because the caller locks them in this order.
func distinctKeys(candidates []Candidate) []Key {
	seen := make(map[Key]struct{}, len(candidates))
	keys := make([]Key, 0, len(candidates))
	for _, c := range candidates {
		if _, ok := seen[c.Key]; ok {
			continue
		}
		seen[c.Key] = struct{}{}
		keys = append(keys, c.Key)
	}
	sort.Slice(keys, func(i, j int) bool {
		if keys[i].Family != keys[j].Family {
			return keys[i].Family < keys[j].Family
		}
		return keys[i].Tier < keys[j].Tier
	})
	return keys
}

// loadState reads the pool's current view of the given keys inside the caller's
// transaction.
func loadState(ctx context.Context, tx pgx.Tx, version int16, keys []Key, cutoff time.Time) (map[Key]KeyState, error) {
	families := make([]string, 0, len(keys))
	seen := make(map[string]struct{}, len(keys))
	for _, key := range keys {
		if _, ok := seen[key.Family]; ok {
			continue
		}
		seen[key.Family] = struct{}{}
		families = append(families, key.Family)
	}

	state := make(map[Key]KeyState, len(keys))
	for _, key := range keys {
		state[key] = KeyState{}
	}

	rows, err := tx.Query(ctx, stateSQL, version, families, cutoff)
	if err != nil {
		return nil, fmt.Errorf("merc templates: load key state: %w", err)
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
			return nil, fmt.Errorf("merc templates: scan key state: %w", err)
		}
		key := Key{Family: family, Tier: tier}
		if _, wanted := state[key]; !wanted {
			continue // another tier of a requested family
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
		current := state[key]
		if tombstoned {
			// A retired row is kept, not deleted, precisely so it can be
			// recognised again: this is what refuses a republish of the art
			// somebody threw out.
			current.Retired = append(current.Retired, sig)
		} else {
			current.Live = append(current.Live, sig)
		}
		state[key] = current
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("merc templates: iterate key state: %w", err)
	}
	return state, nil
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
