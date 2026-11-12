package handlers

import (
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"profitofexile/internal/lab"
)

// validLabRunBody returns a request body that passes every validation rule;
// tests override one field to reach the rule under test.
func validLabRunBody() map[string]any {
	return map[string]any{
		"difficulty":      "Uber",
		"strategy":        "speed",
		"elapsed_seconds": 300,
		"room_count":      8,
		"rooms":           []map[string]any{},
	}
}

func postLabRun(t *testing.T, body string) (int, string) {
	t.Helper()
	w := httptest.NewRecorder()
	StoreLabRun(nil)(w, httptest.NewRequest(http.MethodPost, "/api/lab/runs", strings.NewReader(body)))
	var resp struct {
		Error string `json:"error"`
	}
	if err := json.Unmarshal(w.Body.Bytes(), &resp); err != nil {
		t.Fatalf("decode error body %q: %v", w.Body.String(), err)
	}
	return w.Code, resp.Error
}

func postLabRunWith(t *testing.T, override map[string]any) (int, string) {
	t.Helper()
	body := validLabRunBody()
	for k, v := range override {
		body[k] = v
	}
	raw, err := json.Marshal(body)
	if err != nil {
		t.Fatal(err)
	}
	return postLabRun(t, string(raw))
}

func assertLabRunError(t *testing.T, code int, msg string, wantCode int, wantMsg string) {
	t.Helper()
	if code != wantCode || msg != wantMsg {
		t.Fatalf("got %d %q, want %d %q", code, msg, wantCode, wantMsg)
	}
}

func rooms(n int) []map[string]any {
	out := make([]map[string]any, n)
	for i := range out {
		out[i] = map[string]any{"room_name": fmt.Sprintf("r%d", i), "room_number": i + 1, "entered_at": "2026-11-12T10:00:00Z"}
	}
	return out
}

func TestStoreLabRun_InvalidJSONReturns400(t *testing.T) {
	code, msg := postLabRun(t, "{not json")
	assertLabRunError(t, code, msg, 400, "invalid JSON body")
}

func TestStoreLabRun_UnknownDifficultyReturns400(t *testing.T) {
	code, msg := postLabRunWith(t, map[string]any{"difficulty": "Bogus"})
	assertLabRunError(t, code, msg, 400, "difficulty must be Normal, Cruel, Merciless, or Uber")
}

func TestStoreLabRun_MissingStrategyReturns400(t *testing.T) {
	code, msg := postLabRunWith(t, map[string]any{"strategy": ""})
	assertLabRunError(t, code, msg, 400, "strategy is required")
}

func TestStoreLabRun_ElapsedSecondsOutOfRangeReturns400(t *testing.T) {
	for _, tc := range []struct {
		elapsed  int
		wantCode int
		wantMsg  string
	}{
		{0, 400, "elapsed_seconds must be 1-86400"},
		{86401, 400, "elapsed_seconds must be 1-86400"},
		{86400, 401, "device identification required"},
	} {
		t.Run(fmt.Sprint(tc.elapsed), func(t *testing.T) {
			code, msg := postLabRunWith(t, map[string]any{"elapsed_seconds": tc.elapsed})
			assertLabRunError(t, code, msg, tc.wantCode, tc.wantMsg)
		})
	}
}

func TestStoreLabRun_KillSecondsAboveElapsedReturns400(t *testing.T) {
	code, msg := postLabRunWith(t, map[string]any{"elapsed_seconds": 300, "kill_seconds": 301})
	assertLabRunError(t, code, msg, 400, "kill_seconds must be 1..elapsed_seconds")
}

func TestStoreLabRun_NonPositiveRoomCountReturns400(t *testing.T) {
	code, msg := postLabRunWith(t, map[string]any{"room_count": 0})
	assertLabRunError(t, code, msg, 400, "room_count must be positive")
}

func TestStoreLabRun_MoreThanFiftyRoomsReturns400(t *testing.T) {
	code, msg := postLabRunWith(t, map[string]any{"rooms": rooms(51)})
	assertLabRunError(t, code, msg, 400, "too many rooms (max 50)")
	code, msg = postLabRunWith(t, map[string]any{"rooms": rooms(50)})
	assertLabRunError(t, code, msg, 401, "device identification required")
}

func TestStoreLabRun_NoDeviceReturns401(t *testing.T) {
	code, msg := postLabRunWith(t, nil)
	assertLabRunError(t, code, msg, 401, "device identification required")
}

func TestListLabRuns_NoDeviceReturns401(t *testing.T) {
	w := httptest.NewRecorder()
	ListLabRuns(nil)(w, httptest.NewRequest(http.MethodGet, "/api/lab/runs", nil))
	if w.Code != 401 || strings.TrimSpace(w.Body.String()) != `{"error":"device identification required"}` {
		t.Fatalf("got %d %s", w.Code, w.Body.String())
	}
}

func TestLabRunStoreErrorMessage_MapsEachStageToItsResponseText(t *testing.T) {
	for stage, want := range map[lab.LabRunStage]string{
		lab.LabRunStageBegin:      "failed to store run",
		lab.LabRunStageInsertRun:  "failed to store run",
		lab.LabRunStageInsertRoom: "failed to store room",
		lab.LabRunStageCommit:     "failed to commit run",
	} {
		if got := labRunStoreErrorMessage(stage); got != want {
			t.Errorf("stage %q: got %q, want %q", stage, got, want)
		}
	}
}
