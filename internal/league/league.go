// Package league owns the database-backed league scope used by scoped data.
package league

import (
	"context"
	"errors"
	"fmt"
	"hash/fnv"

	"github.com/jackc/pgx/v5"
)

// ErrUnscoped reports a scope that never selected a league. Callers reach it
// through the zero Scope, which would otherwise match no rows instead of
// failing.
var ErrUnscoped = errors.New("league scope is not set")

// Scope identifies a league and the configuration revision that selected it.
// Its fields are private so callers can obtain a scope only through Resolve or
// Historical.
type Scope struct {
	id       string
	revision int64
}

// Validate rejects a scope that carries no league identity. Repositories call
// it before using a scope in a predicate or an inserted row.
func (s Scope) Validate() error {
	if s.id == "" {
		return ErrUnscoped
	}
	return nil
}

// ID returns the exact upstream league identifier.
func (s Scope) ID() string { return s.id }

// Revision returns the runtime configuration revision. Historical scopes are
// revision-neutral and therefore return zero.
func (s Scope) Revision() int64 { return s.revision }

// Resolve reads the default league scope from the runtime SSOT.
func Resolve(ctx context.Context, db interface {
	QueryRow(context.Context, string, ...any) pgx.Row
}) (Scope, error) {
	var scope Scope
	err := db.QueryRow(ctx, `SELECT active_league, revision FROM runtime_config WHERE singleton = TRUE`).Scan(&scope.id, &scope.revision)
	if errors.Is(err, pgx.ErrNoRows) {
		return Scope{}, errors.New("league runtime configuration is not initialized")
	}
	if err != nil {
		return Scope{}, fmt.Errorf("resolve league scope: %w", err)
	}
	return scope, nil
}

// Historical explicitly selects a non-runtime league for research. Its
// revision is zero because no revision contract has been defined for archived
// selections. An empty id yields the zero Scope, which Validate rejects.
func Historical(id string) Scope {
	return Scope{id: id}
}

// RuntimeLockKey serializes process ownership of the runtime configuration.
func RuntimeLockKey() int64 { return lockKey("runtime") }

// DataLockKey serializes work against one scoped dataset.
func DataLockKey(scope Scope) int64 { return lockKey("data", scope.id) }

// AdministrationLockKey serializes league administration operations.
func AdministrationLockKey() int64 { return lockKey("administration") }

func lockKey(parts ...string) int64 {
	h := fnv.New64a()
	_, _ = h.Write([]byte("profitofexile/league/"))
	for _, part := range parts {
		_, _ = h.Write([]byte{0})
		_, _ = h.Write([]byte(part))
	}
	return int64(h.Sum64() & (1<<63 - 1))
}
