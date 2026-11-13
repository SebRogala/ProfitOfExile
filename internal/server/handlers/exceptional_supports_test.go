package handlers

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"testing"
	"time"

	"profitofexile/internal/lab"
	"profitofexile/internal/league"
)

type fakeExceptionalPricer struct {
	snap     lab.ExceptionalSupportSnapshot
	err      error
	gotScope league.Scope
}

func (f *fakeExceptionalPricer) ExceptionalSupportPrices(_ context.Context, scope league.Scope) (lab.ExceptionalSupportSnapshot, error) {
	f.gotScope = scope
	return f.snap, f.err
}

var exceptionalTestScope = league.Historical("Mirage")

func serveExceptionalSupports(t *testing.T, fake *fakeExceptionalPricer, cache *lab.Cache) *httptest.ResponseRecorder {
	t.Helper()
	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/api/analysis/exceptional-supports", nil)
	ExceptionalSupportPrices(fake, cache, exceptionalTestScope)(rec, req)
	return rec
}

func readExceptionalGolden(t *testing.T, file string) string {
	t.Helper()
	b, err := os.ReadFile("../../../desktop/src/lib/compass/__fixtures__/" + file)
	if err != nil {
		t.Fatalf("read golden %s: %v", file, err)
	}
	return strings.TrimSpace(string(b))
}

func cacheWithDivineRate(rate float64) *lab.Cache {
	c := lab.NewCache(exceptionalTestScope)
	c.SetDivineRate(rate)
	return c
}

var fixtureAsOf = time.Date(2026, 9, 9, 0, 33, 0, 0, time.UTC)

func decodeExceptionalFields(t *testing.T, rec *httptest.ResponseRecorder) map[string]json.RawMessage {
	t.Helper()
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(rec.Body.Bytes(), &fields); err != nil {
		t.Fatalf("decode body %q: %v", rec.Body.String(), err)
	}
	return fields
}

func TestExceptionalSupportPrices_servesTheContractBody(t *testing.T) {
	fake := &fakeExceptionalPricer{snap: lab.ExceptionalSupportSnapshot{
		AsOf:  fixtureAsOf,
		Chaos: map[string]float64{"Empower Support": 43.2, "Enhance Support": 9, "Enlighten Support": 185},
	}}

	rec := serveExceptionalSupports(t, fake, cacheWithDivineRate(230))

	if got, want := strings.TrimSpace(rec.Body.String()), readExceptionalGolden(t, "exceptional-supports-priced.json"); got != want {
		t.Errorf("body\n got %s\nwant %s", got, want)
	}
}

func TestExceptionalSupportPrices_missingGemServesNull(t *testing.T) {
	fake := &fakeExceptionalPricer{snap: lab.ExceptionalSupportSnapshot{
		AsOf:  fixtureAsOf,
		Chaos: map[string]float64{"Empower Support": 43.2, "Enhance Support": 9},
	}}

	rec := serveExceptionalSupports(t, fake, cacheWithDivineRate(230))

	if got, want := strings.TrimSpace(rec.Body.String()), readExceptionalGolden(t, "exceptional-supports-missing.json"); got != want {
		t.Errorf("body\n got %s\nwant %s", got, want)
	}
}

func TestExceptionalSupportPrices_noSnapshotServesNullAsOf(t *testing.T) {
	rec := serveExceptionalSupports(t, &fakeExceptionalPricer{}, cacheWithDivineRate(230))

	asOf, ok := decodeExceptionalFields(t, rec)["asOf"]
	if !ok || string(asOf) != "null" {
		t.Errorf("asOf = %q (present %v), want null", asOf, ok)
	}
}

func TestExceptionalSupportPrices_nilCacheServesZeroDivineRate(t *testing.T) {
	fake := &fakeExceptionalPricer{snap: lab.ExceptionalSupportSnapshot{AsOf: fixtureAsOf}}

	rec := serveExceptionalSupports(t, fake, nil)

	if got := string(decodeExceptionalFields(t, rec)["divineRate"]); got != "0" {
		t.Errorf("divineRate = %q, want 0", got)
	}
}

func TestExceptionalSupportPrices_repositoryErrorIs500(t *testing.T) {
	fake := &fakeExceptionalPricer{err: errors.New("db down")}

	rec := serveExceptionalSupports(t, fake, cacheWithDivineRate(230))

	if rec.Code != http.StatusInternalServerError {
		t.Errorf("status = %d, want 500", rec.Code)
	}
	if got := strings.TrimSpace(rec.Body.String()); got != `{"error":"query failed"}` {
		t.Errorf("body = %q, want {\"error\":\"query failed\"}", got)
	}
}

func TestExceptionalSupportPrices_readsTheProcessLeague(t *testing.T) {
	fake := &fakeExceptionalPricer{}

	serveExceptionalSupports(t, fake, nil)

	if got := fake.gotScope.ID(); got != "Mirage" {
		t.Errorf("repository read league %q, want Mirage", got)
	}
}
