//go:build integration

package handlers

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/lab"
	"profitofexile/internal/server/middleware"
)

func labRunsPool(t *testing.T) *pgxpool.Pool {
	t.Helper()
	url := os.Getenv("DATABASE_URL")
	if url == "" {
		t.Skip("DATABASE_URL required")
	}
	pool, err := pgxpool.New(context.Background(), url)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(pool.Close)
	return pool
}

// labRunsDevice returns a fingerprint unique to this test and deletes its runs
// (rooms cascade) when the test ends.
func labRunsDevice(t *testing.T, pool *pgxpool.Pool) string {
	t.Helper()
	dev := fmt.Sprintf("%x", sha256.Sum256([]byte(fmt.Sprintf("poe344-labruns-%s-%d", t.Name(), time.Now().UnixNano()))))
	t.Cleanup(func() {
		if _, err := pool.Exec(context.Background(), `DELETE FROM lab_runs WHERE device_id = $1`, dev); err != nil {
			t.Error(err)
		}
	})
	return dev
}

type labRunsHTTP struct {
	store, list http.Handler
}

func newLabRunsHTTP(pool *pgxpool.Pool) labRunsHTTP {
	mw := middleware.DeviceMiddleware(bossLootDevice{})
	return labRunsHTTP{store: mw(StoreLabRun(lab.NewLabRunRepository(pool))), list: mw(ListLabRuns(lab.NewLabRunRepository(pool)))}
}

func (h labRunsHTTP) do(t *testing.T, ctx context.Context, handler http.Handler, method, target, dev string, body any) *httptest.ResponseRecorder {
	t.Helper()
	var data []byte
	if body != nil {
		var err error
		if data, err = json.Marshal(body); err != nil {
			t.Fatal(err)
		}
	}
	req := httptest.NewRequest(method, target, bytes.NewReader(data)).WithContext(ctx)
	req.Header.Set("X-Device-ID", dev)
	w := httptest.NewRecorder()
	handler.ServeHTTP(w, req)
	return w
}

func labRunBody(difficulty string, elapsed int, startedAt time.Time) map[string]any {
	return map[string]any{
		"difficulty": difficulty, "strategy": "speed", "elapsed_seconds": elapsed,
		"room_count": 8, "started_at": startedAt.Format(time.RFC3339), "rooms": []map[string]any{},
	}
}

func (h labRunsHTTP) mustStore(t *testing.T, dev string, body map[string]any) {
	t.Helper()
	if w := h.do(t, context.Background(), h.store, http.MethodPost, "/api/lab/runs", dev, body); w.Code != 201 {
		t.Fatalf("store: %d %s", w.Code, w.Body.String())
	}
}

type labRunsList struct {
	Runs  []labRunResponse `json:"runs"`
	Stats labRunStats      `json:"stats"`
}

func (h labRunsHTTP) mustList(t *testing.T, dev, query string) labRunsList {
	t.Helper()
	w := h.do(t, context.Background(), h.list, http.MethodGet, "/api/lab/runs"+query, dev, nil)
	if w.Code != 200 {
		t.Fatalf("list: %d %s", w.Code, w.Body.String())
	}
	var out labRunsList
	if err := json.Unmarshal(w.Body.Bytes(), &out); err != nil {
		t.Fatal(err)
	}
	return out
}

func assertErrorBody(t *testing.T, w *httptest.ResponseRecorder, want string) {
	t.Helper()
	wantBody := fmt.Sprintf(`{"error":%q}`, want)
	if w.Code != 500 || strings.TrimSpace(w.Body.String()) != wantBody {
		t.Fatalf("got %d %s, want 500 %s", w.Code, w.Body.String(), wantBody)
	}
}

func cancelledContext() context.Context {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	return ctx
}

func TestStoreLabRun_StoredRunIsListedWithItsFields(t *testing.T) {
	pool := labRunsPool(t)
	dev := labRunsDevice(t, pool)
	h := newLabRunsHTTP(pool)
	body := labRunBody("Merciless", 420, time.Date(2026, 11, 12, 10, 0, 0, 0, time.UTC))
	body["strategy"] = "golden-route"
	body["kill_seconds"] = 390
	body["has_golden_door"] = true
	body["rooms"] = []map[string]any{
		{"room_name": "Estate Path", "room_number": 1, "entered_at": "2026-11-12T10:00:00Z"},
		{"room_name": "Basilica Halls", "room_number": 2, "entered_at": "2026-11-12T10:01:00Z"},
	}

	w := h.do(t, context.Background(), h.store, http.MethodPost, "/api/lab/runs", dev, body)
	var created struct {
		RunID int64 `json:"run_id"`
		Rooms int   `json:"rooms"`
	}
	if w.Code != 201 || json.Unmarshal(w.Body.Bytes(), &created) != nil || created.RunID <= 0 || created.Rooms != 2 {
		t.Fatalf("store: %d %s", w.Code, w.Body.String())
	}

	got := h.mustList(t, dev, "").Runs
	if len(got) != 1 {
		t.Fatalf("runs = %+v, want one", got)
	}
	run := got[0]
	if run.ID != created.RunID || run.Difficulty != "Merciless" || run.Strategy != "golden-route" ||
		run.ElapsedSeconds != 420 || run.KillSeconds == nil || *run.KillSeconds != 390 || !run.HasGoldenDoor ||
		run.RoomCount != 8 || !run.StartedAt.Equal(time.Date(2026, 11, 12, 10, 0, 0, 0, time.UTC)) {
		t.Fatalf("listed run = %+v (kill=%v), want posted fields with id %d", run, run.KillSeconds, created.RunID)
	}
}

func TestListLabRuns_NoRunsReturnsEmptyArrayNotNull(t *testing.T) {
	pool := labRunsPool(t)
	dev := labRunsDevice(t, pool)
	h := newLabRunsHTTP(pool)
	w := h.do(t, context.Background(), h.list, http.MethodGet, "/api/lab/runs", dev, nil)
	if w.Code != 200 || !strings.Contains(w.Body.String(), `"runs":[]`) {
		t.Fatalf("got %d %s, want runs:[]", w.Code, w.Body.String())
	}
}

func TestListLabRuns_StatsRoundAveragesToOneDecimal(t *testing.T) {
	pool := labRunsPool(t)
	dev := labRunsDevice(t, pool)
	h := newLabRunsHTTP(pool)
	start := time.Now().Add(-time.Hour)
	for i, elapsed := range []int{100, 101, 101} {
		h.mustStore(t, dev, labRunBody("Uber", elapsed, start.Add(time.Duration(i)*time.Minute)))
	}
	if got := h.mustList(t, dev, "").Stats.AvgSeconds; got != 100.7 {
		t.Fatalf("avg_seconds = %v, want 100.7", got)
	}
}

func TestListLabRuns_UnknownDifficultyFilterIsIgnored(t *testing.T) {
	pool := labRunsPool(t)
	dev := labRunsDevice(t, pool)
	h := newLabRunsHTTP(pool)
	start := time.Now().Add(-time.Hour)
	h.mustStore(t, dev, labRunBody("Normal", 100, start))
	h.mustStore(t, dev, labRunBody("Uber", 200, start.Add(time.Minute)))
	got := h.mustList(t, dev, "?difficulty=Bogus")
	if len(got.Runs) != 2 || got.Stats.TotalRuns != 2 {
		t.Fatalf("runs=%d total=%d, want both difficulties", len(got.Runs), got.Stats.TotalRuns)
	}
}

func TestListLabRuns_StatsIgnoreLimit(t *testing.T) {
	pool := labRunsPool(t)
	dev := labRunsDevice(t, pool)
	h := newLabRunsHTTP(pool)
	start := time.Now().Add(-time.Hour)
	for i := 0; i < 3; i++ {
		h.mustStore(t, dev, labRunBody("Uber", 100+i, start.Add(time.Duration(i)*time.Minute)))
	}
	got := h.mustList(t, dev, "?limit=1")
	if len(got.Runs) != 1 || got.Stats.TotalRuns != 3 {
		t.Fatalf("runs=%d total=%d, want 1 and 3", len(got.Runs), got.Stats.TotalRuns)
	}
}

func TestListLabRuns_DifficultyFilterScopesRunsAndStats(t *testing.T) {
	pool := labRunsPool(t)
	dev := labRunsDevice(t, pool)
	h := newLabRunsHTTP(pool)
	start := time.Now().Add(-time.Hour)
	h.mustStore(t, dev, labRunBody("Normal", 100, start))
	h.mustStore(t, dev, labRunBody("Uber", 200, start.Add(time.Minute)))
	h.mustStore(t, dev, labRunBody("Uber", 300, start.Add(2*time.Minute)))
	got := h.mustList(t, dev, "?difficulty=Uber")
	for _, run := range got.Runs {
		if run.Difficulty != "Uber" {
			t.Fatalf("run %+v leaked past the Uber filter", run)
		}
	}
	if len(got.Runs) != 2 || got.Stats.TotalRuns != 2 || got.Stats.BestSeconds != 200 || got.Stats.AvgSeconds != 250 {
		t.Fatalf("runs=%d stats=%+v, want 2 Uber runs, best 200, avg 250", len(got.Runs), got.Stats)
	}
}

func TestStoreLabRun_BeginFailureReturnsFailedToStoreRun(t *testing.T) {
	pool := labRunsPool(t)
	dev := labRunsDevice(t, pool)
	h := newLabRunsHTTP(pool)
	w := h.do(t, cancelledContext(), h.store, http.MethodPost, "/api/lab/runs", dev, labRunBody("Uber", 100, time.Now()))
	assertErrorBody(t, w, "failed to store run")
}

func TestStoreLabRun_RoomInsertFailureReturnsFailedToStoreRoom(t *testing.T) {
	pool := labRunsPool(t)
	dev := labRunsDevice(t, pool)
	h := newLabRunsHTTP(pool)
	body := labRunBody("Uber", 100, time.Now())
	body["rooms"] = []map[string]any{
		{"room_name": "ok", "room_number": 1, "entered_at": "2026-11-12T10:00:00Z"},
		{"room_name": "bad\x00name", "room_number": 2, "entered_at": "2026-11-12T10:01:00Z"},
	}
	w := h.do(t, context.Background(), h.store, http.MethodPost, "/api/lab/runs", dev, body)
	assertErrorBody(t, w, "failed to store room")
	var n int
	if err := pool.QueryRow(context.Background(), `SELECT count(*) FROM lab_runs WHERE device_id = $1`, dev).Scan(&n); err != nil || n != 0 {
		t.Fatalf("lab_runs rows for device = %d (err %v), want 0 after rollback", n, err)
	}
}

func TestListLabRuns_QueryFailureReturnsFailedToQueryRuns(t *testing.T) {
	pool := labRunsPool(t)
	dev := labRunsDevice(t, pool)
	h := newLabRunsHTTP(pool)
	w := h.do(t, cancelledContext(), h.list, http.MethodGet, "/api/lab/runs", dev, nil)
	assertErrorBody(t, w, "failed to query runs")
}
