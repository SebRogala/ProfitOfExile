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

// TestFontSession_NoDevice_Returns401 asserts that a font-session submission
// without an authenticated device is rejected. The device is taken from the
// request context (set by DeviceMiddleware), never from the body — so a client
// cannot attribute crowd-sourced font data to an arbitrary device id. The body
// below is otherwise valid: it passes every field validation and reaches the
// device-identity check, which is the behavior under test.
//
// Mutation check: removing the `dev == nil` guard in FontSession makes this test
// fail (the handler then dereferences the nil device instead of returning 401).
func TestFontSession_NoDevice_Returns401(t *testing.T) {
	body := `{"lab_type":"Uber","total_crafts":6,"variant":"20/20","rounds":[{"option_chosen":"a","gems_offered":["Spark"],"gem_picked":"Spark"}]}`
	req := httptest.NewRequest(http.MethodPost, "/api/desktop/font-session", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()

	// Pool is nil: the device check must reject before any DB access is attempted.
	FontSession(nil).ServeHTTP(w, req)

	if w.Code != http.StatusUnauthorized {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusUnauthorized, w.Body.String())
	}

	var got map[string]string
	if err := json.NewDecoder(w.Body).Decode(&got); err != nil {
		t.Fatalf("decode error response: %v", err)
	}
	if got["error"] != "device identification required" {
		t.Errorf("error = %q, want %q", got["error"], "device identification required")
	}
}

// fontSessionNoDB posts body to FontSession with no repository and no device:
// validation must answer before the device check, which answers before any DB access.
func fontSessionNoDB(t *testing.T, body string) (int, string) {
	t.Helper()
	req := httptest.NewRequest(http.MethodPost, "/api/desktop/font-session", strings.NewReader(body))
	w := httptest.NewRecorder()
	FontSession(nil).ServeHTTP(w, req)
	var got map[string]string
	if err := json.NewDecoder(w.Body).Decode(&got); err != nil {
		t.Fatalf("decode error response: %v", err)
	}
	return w.Code, got["error"]
}

func assertFontSessionRejected(t *testing.T, body string, wantStatus int, wantMsg string) {
	t.Helper()
	if code, msg := fontSessionNoDB(t, body); code != wantStatus || msg != wantMsg {
		t.Fatalf("got %d %q, want %d %q", code, msg, wantStatus, wantMsg)
	}
}

const fontRoundJSON = `{"option_chosen":"a"}`

func fontSessionBody(totalCrafts int, rounds int) string {
	rs := make([]string, rounds)
	for i := range rs {
		rs[i] = fontRoundJSON
	}
	return fmt.Sprintf(`{"lab_type":"Uber","total_crafts":%d,"rounds":[%s]}`, totalCrafts, strings.Join(rs, ","))
}

func TestFontSession_InvalidJSONReturns400(t *testing.T) {
	assertFontSessionRejected(t, `{"lab_type":`, http.StatusBadRequest, "invalid JSON body")
}

func TestFontSession_MissingLabTypeReturns400(t *testing.T) {
	assertFontSessionRejected(t, `{"total_crafts":6,"rounds":[`+fontRoundJSON+`]}`, http.StatusBadRequest, "lab_type is required")
}

func TestFontSession_TotalCraftsOutOfRangeReturns400(t *testing.T) {
	for _, n := range []int{0, 21} {
		t.Run(fmt.Sprint(n), func(t *testing.T) {
			assertFontSessionRejected(t, fontSessionBody(n, 1), http.StatusBadRequest, "total_crafts must be 1-20")
		})
	}
	t.Run("20 reaches the device check", func(t *testing.T) {
		assertFontSessionRejected(t, fontSessionBody(20, 1), http.StatusUnauthorized, "device identification required")
	})
}

func TestFontSession_EmptyRoundsReturns400(t *testing.T) {
	assertFontSessionRejected(t, fontSessionBody(6, 0), http.StatusBadRequest, "rounds must not be empty")
}

func TestFontSession_MoreThanTwentyRoundsReturns400(t *testing.T) {
	assertFontSessionRejected(t, fontSessionBody(6, 21), http.StatusBadRequest, "too many rounds (max 20)")
}

func TestFontSession_MalformedPairCodeReturns400(t *testing.T) {
	body := `{"lab_type":"Uber","total_crafts":6,"pair_code":"ab-1","rounds":[` + fontRoundJSON + `]}`
	assertFontSessionRejected(t, body, http.StatusBadRequest, "invalid pair_code format")
}

func TestFontSessionStoreErrorMessage_MapsEachStageToItsResponseText(t *testing.T) {
	for stage, want := range map[lab.FontSessionStage]string{
		lab.FontSessionStageBegin:          "failed to store session",
		lab.FontSessionStageInsertSession:  "failed to store session",
		lab.FontSessionStageMarshalOptions: "failed to serialize round options",
		lab.FontSessionStageInsertRound:    "failed to store round",
		lab.FontSessionStageCommit:         "failed to commit session",
	} {
		if got := fontSessionStoreErrorMessage(stage); got != want {
			t.Errorf("stage %q: got %q, want %q", stage, got, want)
		}
	}
}
