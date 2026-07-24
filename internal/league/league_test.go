package league

import (
	"context"
	"errors"
	"testing"

	"github.com/jackc/pgx/v5"
)

type scopeRow struct {
	scan func(...any) error
}

func (r scopeRow) Scan(dest ...any) error {
	return r.scan(dest...)
}

type scopeDB struct {
	row pgx.Row
}

func (db scopeDB) QueryRow(context.Context, string, ...any) pgx.Row {
	return db.row
}

func TestResolveReturnsRuntimeScope(t *testing.T) {
	db := scopeDB{row: scopeRow{scan: func(dest ...any) error {
		*dest[0].(*string) = "Mirage"
		*dest[1].(*int64) = 7
		return nil
	}}}

	scope, err := Resolve(context.Background(), db)
	if err != nil {
		t.Fatalf("Resolve: %v", err)
	}
	if got, want := scope.ID(), "Mirage"; got != want {
		t.Errorf("scope ID = %q, want %q", got, want)
	}
	if got, want := scope.Revision(), int64(7); got != want {
		t.Errorf("scope revision = %d, want %d", got, want)
	}
}

func TestResolveRejectsUninitializedRuntimeConfiguration(t *testing.T) {
	db := scopeDB{row: scopeRow{scan: func(...any) error {
		return pgx.ErrNoRows
	}}}

	_, err := Resolve(context.Background(), db)
	if err == nil {
		t.Fatal("Resolve returned nil error for missing runtime configuration")
	}
	if got, want := err.Error(), "league runtime configuration is not initialized"; got != want {
		t.Errorf("Resolve error = %q, want %q", got, want)
	}
}

func TestResolveWrapsDatabaseFailure(t *testing.T) {
	db := scopeDB{row: scopeRow{scan: func(...any) error {
		return errors.New("database unavailable")
	}}}

	_, err := Resolve(context.Background(), db)
	if err == nil {
		t.Fatal("Resolve returned nil error for database failure")
	}
	if got, want := err.Error(), "resolve league scope: database unavailable"; got != want {
		t.Errorf("Resolve error = %q, want %q", got, want)
	}
}

func TestHistoricalKeepsOnlyExplicitIdentity(t *testing.T) {
	scope := Historical("Settlers")

	if got, want := scope.ID(), "Settlers"; got != want {
		t.Errorf("scope ID = %q, want %q", got, want)
	}
}

func TestValidateRejectsTheZeroScope(t *testing.T) {
	var scope Scope

	if err := scope.Validate(); !errors.Is(err, ErrUnscoped) {
		t.Errorf("Validate error = %v, want %v", err, ErrUnscoped)
	}
}

func TestValidateRejectsAnEmptyHistoricalIdentity(t *testing.T) {
	if err := Historical("").Validate(); !errors.Is(err, ErrUnscoped) {
		t.Errorf("Validate error = %v, want %v", err, ErrUnscoped)
	}
}

func TestValidateAcceptsAResolvedScope(t *testing.T) {
	db := scopeDB{row: scopeRow{scan: func(dest ...any) error {
		*dest[0].(*string) = "Mirage"
		*dest[1].(*int64) = 7
		return nil
	}}}

	scope, err := Resolve(context.Background(), db)
	if err != nil {
		t.Fatalf("Resolve: %v", err)
	}
	if err := scope.Validate(); err != nil {
		t.Errorf("Validate error = %v, want nil", err)
	}
}

func TestLockKeysAreStableAndPurposeSeparated(t *testing.T) {
	scope := Historical("Mirage")

	keys := map[string]int64{
		"runtime":        RuntimeLockKey(),
		"server":         ServerLockKey(),
		"data":           DataLockKey(scope),
		"administration": AdministrationLockKey(),
	}
	if got, want := RuntimeLockKey(), keys["runtime"]; got != want {
		t.Errorf("runtime lock key changed between calls: got %d, want %d", got, want)
	}
	// ServerLockKey and RuntimeLockKey fence a co-located server and collector
	// independently; if ServerLockKey aliased RuntimeLockKey (or any other key)
	// they would deadlock on a shared key at boot. Assert stable and distinct.
	if got, want := ServerLockKey(), keys["server"]; got != want {
		t.Errorf("server lock key changed between calls: got %d, want %d", got, want)
	}
	if got, want := DataLockKey(scope), keys["data"]; got != want {
		t.Errorf("data lock key changed between calls: got %d, want %d", got, want)
	}

	for leftName, left := range keys {
		for rightName, right := range keys {
			if leftName < rightName && left == right {
				t.Errorf("%s and %s use the same lock key %d", leftName, rightName, left)
			}
		}
	}
}

func TestDataLockKeySeparatesLeagues(t *testing.T) {
	if mirage, settlers := DataLockKey(Historical("Mirage")), DataLockKey(Historical("Settlers")); mirage == settlers {
		t.Errorf("data lock keys collide for distinct leagues: %d", mirage)
	}
}
