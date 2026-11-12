//go:build integration

package handlers

import (
	"context"
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"net/http"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/lab"
	"profitofexile/internal/server/middleware"
)

// fontSessionDevice returns a fingerprint unique to this test and deletes its
// sessions (rounds cascade) when the test ends.
func fontSessionDevice(t *testing.T, pool *pgxpool.Pool) string {
	t.Helper()
	dev := fmt.Sprintf("%x", sha256.Sum256([]byte(fmt.Sprintf("poe344-font-%s-%d", t.Name(), time.Now().UnixNano()))))
	t.Cleanup(func() {
		if _, err := pool.Exec(context.Background(), `DELETE FROM font_sessions WHERE device_id = $1`, dev); err != nil {
			t.Error(err)
		}
	})
	return dev
}

func newFontSessionHTTP(pool *pgxpool.Pool) http.Handler {
	return middleware.DeviceMiddleware(bossLootDevice{})(FontSession(lab.NewFontSessionRepository(pool)))
}

func postFontSession(t *testing.T, pool *pgxpool.Pool, ctx context.Context, dev string, body map[string]any) (int, []byte) {
	t.Helper()
	w := labRunsHTTP{}.do(t, ctx, newFontSessionHTTP(pool), http.MethodPost, "/api/desktop/font-session", dev, body)
	return w.Code, w.Body.Bytes()
}

func fontBody(rounds ...map[string]any) map[string]any {
	return map[string]any{"lab_type": "Uber", "total_crafts": 6, "rounds": rounds}
}

// mustStoreFontSession posts body and returns the stored session id.
func mustStoreFontSession(t *testing.T, pool *pgxpool.Pool, dev string, body map[string]any) int64 {
	t.Helper()
	code, raw := postFontSession(t, pool, context.Background(), dev, body)
	if code != http.StatusOK {
		t.Fatalf("store: %d %s", code, raw)
	}
	var out struct {
		SessionID int64 `json:"session_id"`
		Rounds    int   `json:"rounds"`
	}
	if err := json.Unmarshal(raw, &out); err != nil || out.SessionID == 0 {
		t.Fatalf("store response %s (err %v)", raw, err)
	}
	return out.SessionID
}

func TestFontSession_EmptyVariantIsStoredAsTwentyTwenty(t *testing.T) {
	pool := labRunsPool(t)
	dev := fontSessionDevice(t, pool)
	id := mustStoreFontSession(t, pool, dev, fontBody(map[string]any{"option_chosen": "a"}))

	var variant string
	if err := pool.QueryRow(context.Background(), `SELECT variant FROM font_sessions WHERE id = $1`, id).Scan(&variant); err != nil {
		t.Fatal(err)
	}
	if variant != "20/20" {
		t.Fatalf("variant = %q, want 20/20", variant)
	}
}

func TestFontSession_RoundsAreNumberedFromOneInSubmissionOrder(t *testing.T) {
	pool := labRunsPool(t)
	dev := fontSessionDevice(t, pool)
	id := mustStoreFontSession(t, pool, dev, fontBody(
		map[string]any{"option_chosen": "first"},
		map[string]any{"option_chosen": "second"},
		map[string]any{"option_chosen": "third"},
	))

	rows, err := pool.Query(context.Background(), `SELECT round_number, option_chosen FROM font_rounds WHERE session_id = $1 ORDER BY round_number`, id)
	if err != nil {
		t.Fatal(err)
	}
	defer rows.Close()
	var got []string
	for rows.Next() {
		var n int
		var opt string
		if err := rows.Scan(&n, &opt); err != nil {
			t.Fatal(err)
		}
		got = append(got, fmt.Sprintf("%d:%s", n, opt))
	}
	if want := "[1:first 2:second 3:third]"; fmt.Sprint(got) != want {
		t.Fatalf("rounds = %v, want %s", got, want)
	}
}

func TestFontSession_EmptyOptionGemAndOfferedAreStoredAsNull(t *testing.T) {
	pool := labRunsPool(t)
	dev := fontSessionDevice(t, pool)
	id := mustStoreFontSession(t, pool, dev, fontBody(map[string]any{"option_chosen": "", "gem_picked": "", "gems_offered": []string{}}))

	var optNull, gemNull, offeredNull bool
	if err := pool.QueryRow(context.Background(),
		`SELECT option_chosen IS NULL, gem_picked IS NULL, gems_offered IS NULL FROM font_rounds WHERE session_id = $1`, id,
	).Scan(&optNull, &gemNull, &offeredNull); err != nil {
		t.Fatal(err)
	}
	if !optNull || !gemNull || !offeredNull {
		t.Fatalf("NULL option_chosen=%v gem_picked=%v gems_offered=%v, want all true", optNull, gemNull, offeredNull)
	}
}

func TestFontSession_CraftOptionsAreStoredAsSubmittedJSON(t *testing.T) {
	pool := labRunsPool(t)
	dev := fontSessionDevice(t, pool)
	id := mustStoreFontSession(t, pool, dev, fontBody(map[string]any{
		"option_chosen": "a",
		"craft_options": []map[string]any{
			{"type": "quality", "text": "+3% quality", "value": 3},
			{"type": "exp", "text": "gain experience"},
		},
	}))

	var match bool
	if err := pool.QueryRow(context.Background(),
		`SELECT craft_options = '[{"type":"quality","text":"+3% quality","value":3},{"type":"exp","text":"gain experience"}]'::jsonb FROM font_rounds WHERE session_id = $1`, id,
	).Scan(&match); err != nil {
		t.Fatal(err)
	}
	if !match {
		var stored string
		_ = pool.QueryRow(context.Background(), `SELECT craft_options::text FROM font_rounds WHERE session_id = $1`, id).Scan(&stored)
		t.Fatalf("craft_options = %s, want the submitted options with value omitted when absent", stored)
	}
}

func TestFontSession_DeviceIsTakenFromTheRequestContext(t *testing.T) {
	pool := labRunsPool(t)
	dev := fontSessionDevice(t, pool)
	body := fontBody(map[string]any{"option_chosen": "a"})
	body["device_id"] = "spoofed-device"
	id := mustStoreFontSession(t, pool, dev, body)

	var got string
	if err := pool.QueryRow(context.Background(), `SELECT device_id FROM font_sessions WHERE id = $1`, id).Scan(&got); err != nil {
		t.Fatal(err)
	}
	if got != dev {
		t.Fatalf("device_id = %q, want the middleware fingerprint %q", got, dev)
	}
}

func TestFontSession_BeginFailureReturnsFailedToStoreSession(t *testing.T) {
	pool := labRunsPool(t)
	dev := fontSessionDevice(t, pool)
	w := labRunsHTTP{}.do(t, cancelledContext(), newFontSessionHTTP(pool), http.MethodPost, "/api/desktop/font-session", dev, fontBody(map[string]any{"option_chosen": "a"}))
	assertErrorBody(t, w, "failed to store session")
}

func TestFontSession_RoundInsertFailureReturnsFailedToStoreRound(t *testing.T) {
	pool := labRunsPool(t)
	dev := fontSessionDevice(t, pool)
	w := labRunsHTTP{}.do(t, context.Background(), newFontSessionHTTP(pool), http.MethodPost, "/api/desktop/font-session", dev,
		fontBody(map[string]any{"option_chosen": "ok"}, map[string]any{"option_chosen": "bad\x00"}))
	assertErrorBody(t, w, "failed to store round")

	var n int
	if err := pool.QueryRow(context.Background(), `SELECT count(*) FROM font_sessions WHERE device_id = $1`, dev).Scan(&n); err != nil || n != 0 {
		t.Fatalf("font_sessions rows = %d (err %v), want 0 after rollback", n, err)
	}
}
