package lab

import (
	"context"
	"errors"
	"testing"

	"github.com/jackc/pgx/v5"

	"profitofexile/internal/league"
)

// fakeOfferingQuerier answers the ten offering statements: a current price for
// every QueryRow, no rows for every Query, and failErr at statement failAt.
type fakeOfferingQuerier struct {
	calls   int
	failAt  int
	failErr error
	iterErr bool // fail statement failAt through rows.Err instead
}

type fakePriceRow struct{ err error }

func (r fakePriceRow) Scan(dest ...any) error {
	if r.err != nil {
		return r.err
	}
	price := 41.6
	*dest[0].(**float64) = &price
	return nil
}

type fakeOfferingRows struct {
	pgx.Rows
	err error
}

func (r *fakeOfferingRows) Next() bool { return false }
func (r *fakeOfferingRows) Close()     {}
func (r *fakeOfferingRows) Err() error { return r.err }

func (q *fakeOfferingQuerier) QueryRow(context.Context, string, ...any) pgx.Row {
	n := q.calls
	q.calls++
	if n == q.failAt {
		return fakePriceRow{err: q.failErr}
	}
	return fakePriceRow{}
}

func (q *fakeOfferingQuerier) Query(context.Context, string, ...any) (pgx.Rows, error) {
	n := q.calls
	q.calls++
	if n == q.failAt {
		if q.iterErr {
			return &fakeOfferingRows{err: q.failErr}, nil
		}
		return nil, q.failErr
	}
	return &fakeOfferingRows{}, nil
}

func TestKeepHappyOfferingTimingsAllStatementsSucceedReturnsNoError(t *testing.T) {
	q := &fakeOfferingQuerier{failAt: -1}
	got, err := offeringTimings(context.Background(), q, league.Historical("Mirage"))
	if err != nil {
		t.Fatalf("offeringTimings error = %v, want nil when every statement succeeds", err)
	}
	if q.calls != 10 || len(got) != 2 || got[0].CurrentPrice != 42 || got[1].CurrentPrice != 42 {
		t.Errorf("calls %d, offerings %+v; want 10 statements and both offerings at 42", q.calls, got)
	}
}

// Every one of the ten statements failing — by query or by iteration — is
// reported, and the partial result the others produced is kept as today.
func TestKeepCriticalOfferingTimingsEachStatementFailureIsReportedWithThePartialResult(t *testing.T) {
	boom := errors.New("boom")
	for failAt := 0; failAt < 10; failAt++ {
		for _, iter := range []bool{false, true} {
			currentPrice := failAt == 0 || failAt == 5
			if iter && currentPrice {
				continue // QueryRow has no iteration phase
			}
			q := &fakeOfferingQuerier{failAt: failAt, failErr: boom, iterErr: iter}
			got, err := offeringTimings(context.Background(), q, league.Historical("Mirage"))
			if !errors.Is(err, boom) {
				t.Errorf("statement %d (iteration %v): error = %v, want it to wrap the failure", failAt, iter, err)
			}
			wantOfferings, wantCalls := 2, 10
			if currentPrice {
				wantOfferings, wantCalls = 1, 6
			}
			if len(got) != wantOfferings || q.calls != wantCalls {
				t.Errorf("statement %d (iteration %v): %d offerings after %d statements, want %d after %d", failAt, iter, len(got), q.calls, wantOfferings, wantCalls)
			}
		}
	}
}

// An offering without snapshot rows is dropped, not a failure: a league with
// none is a complete empty answer.
func TestKeepHappyOfferingTimingsNoSnapshotRowsIsACompleteEmptyAnswer(t *testing.T) {
	q := &fakeOfferingQuerier{failAt: 0, failErr: pgx.ErrNoRows}
	got, err := offeringTimings(context.Background(), q, league.Historical("Mirage"))
	if err != nil || len(got) != 1 || got[0].FragmentID != "offer-dedication" {
		t.Errorf("offerings %+v, error %v; want only offer-dedication and no error", got, err)
	}
}
