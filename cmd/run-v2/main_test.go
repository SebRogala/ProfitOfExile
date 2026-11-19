package main

import (
	"context"
	"errors"
	"fmt"
	"reflect"
	"sort"
	"strings"
	"testing"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"

	"profitofexile/internal/lab"
	"profitofexile/internal/league"
)

// fakeRows scripts a pgx.Rows: each row is a value slice or a scan error.
type fakeRows struct {
	rows    [][]any
	scanErr map[int]error
	iterErr error
	idx     int
}

func (r *fakeRows) Close()                                       {}
func (r *fakeRows) Err() error                                   { return r.iterErr }
func (r *fakeRows) CommandTag() pgconn.CommandTag                { return pgconn.CommandTag{} }
func (r *fakeRows) FieldDescriptions() []pgconn.FieldDescription { return nil }
func (r *fakeRows) Values() ([]any, error)                       { return r.rows[r.idx-1], nil }
func (r *fakeRows) RawValues() [][]byte                          { return nil }
func (r *fakeRows) Conn() *pgx.Conn                              { return nil }

func (r *fakeRows) Next() bool {
	if r.idx >= len(r.rows) {
		return false
	}
	r.idx++
	return true
}

func (r *fakeRows) Scan(dest ...any) error {
	if err := r.scanErr[r.idx-1]; err != nil {
		return err
	}
	row := r.rows[r.idx-1]
	if len(dest) != len(row) {
		return fmt.Errorf("fakeRows: %d dest for %d values", len(dest), len(row))
	}
	for i, v := range row {
		reflect.ValueOf(dest[i]).Elem().Set(reflect.ValueOf(v))
	}
	return nil
}

type fakeQuerier struct{ rows *fakeRows }

func (q fakeQuerier) Query(context.Context, string, ...any) (pgx.Rows, error) {
	return q.rows, nil
}

var (
	t0 = time.Date(2026, 11, 1, 12, 0, 0, 0, time.UTC)
	t1 = t0.Add(time.Hour)
)

type loaderCase struct {
	name string
	rows [][]any
	run  func(q querier) (any, error)
	want any
}

func loaderCases() []loaderCase {
	scope := league.Scope{}
	return []loaderCase{
		{
			name: "loadGemsAtTime",
			rows: [][]any{
				{"Spark of Nova", "20/20", 120.5, 7, true, "blue", false},
				{"Cleave", "1/20", 3.0, 40, false, "red", true},
			},
			run: func(q querier) (any, error) { return loadGemsAtTime(context.Background(), q, scope, t0) },
			want: []lab.GemPrice{
				{Name: "Spark of Nova", Variant: "20/20", Chaos: 120.5, Listings: 7, IsTransfigured: true, GemColor: "blue"},
				{Name: "Cleave", Variant: "1/20", Chaos: 3.0, Listings: 40, GemColor: "red", IsCorrupted: true},
			},
		},
		{
			name: "loadHistoryUpTo",
			rows: [][]any{
				{"Spark of Nova", "20/20", "blue", t0, 100.0, 5},
				{"Spark of Nova", "20/20", "blue", t1, 110.0, 6},
				{"Cleave of Rage", "1/20", "red", t0, 9.0, 30},
			},
			run: func(q querier) (any, error) { return loadHistoryUpTo(context.Background(), q, scope, t1, 168) },
			want: []lab.GemPriceHistory{
				{Name: "Cleave of Rage", Variant: "1/20", GemColor: "red", Points: []lab.PricePoint{{Time: t0, Chaos: 9.0, Listings: 30}}},
				{Name: "Spark of Nova", Variant: "20/20", GemColor: "blue", Points: []lab.PricePoint{
					{Time: t0, Chaos: 100.0, Listings: 5},
					{Time: t1, Chaos: 110.0, Listings: 6},
				}},
			},
		},
	}
}

func sortHistory(v any) any {
	if h, ok := v.([]lab.GemPriceHistory); ok {
		sort.Slice(h, func(i, j int) bool { return h[i].Name < h[j].Name })
	}
	return v
}

func TestKeepRegression_RunV2LoadersReturnRowsErr(t *testing.T) {
	for _, c := range loaderCases() {
		t.Run(c.name, func(t *testing.T) {
			sentinel := errors.New("connection reset mid-iteration")
			rows := &fakeRows{rows: c.rows[:1], iterErr: sentinel}
			_, err := c.run(fakeQuerier{rows})
			if !errors.Is(err, sentinel) {
				t.Fatalf("err = %v, want wrapping %v", err, sentinel)
			}
		})
	}
}

func TestKeepCritical_RunV2LoadersFailOnScanError(t *testing.T) {
	for _, c := range loaderCases() {
		t.Run(c.name, func(t *testing.T) {
			sentinel := errors.New("cannot scan row")
			rows := &fakeRows{rows: c.rows, scanErr: map[int]error{1: sentinel}}
			_, err := c.run(fakeQuerier{rows})
			if !errors.Is(err, sentinel) {
				t.Fatalf("err = %v, want wrapping %v", err, sentinel)
			}
			if !strings.Contains(err.Error(), c.name) {
				t.Fatalf("err = %q, want it to name %s", err, c.name)
			}
		})
	}
}

func TestKeepHappy_RunV2LoadersReturnAllRows(t *testing.T) {
	for _, c := range loaderCases() {
		t.Run(c.name, func(t *testing.T) {
			got, err := c.run(fakeQuerier{&fakeRows{rows: c.rows}})
			if err != nil {
				t.Fatalf("err = %v, want nil", err)
			}
			if got := sortHistory(got); !reflect.DeepEqual(got, c.want) {
				t.Fatalf("got %+v\nwant %+v", got, c.want)
			}
		})
	}
}
