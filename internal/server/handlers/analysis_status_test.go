package handlers

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"profitofexile/internal/exchange"
	"profitofexile/internal/lab"
	"profitofexile/internal/league"
)

var statusScope = league.Historical("Mirage")

// getAnalysisStatus serves one status request and decodes the body as a key map,
// so a test can tell an omitted key from a zero one.
func getAnalysisStatus(t *testing.T, divineRate *exchange.DivineRateReader) map[string]any {
	t.Helper()
	w := httptest.NewRecorder()
	AnalysisStatus(lab.NewCache(statusScope), divineRate, statusScope).
		ServeHTTP(w, httptest.NewRequest(http.MethodGet, "/api/analysis/status", nil))
	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200 (body: %s)", w.Code, w.Body.String())
	}
	var fields map[string]any
	if err := json.Unmarshal(w.Body.Bytes(), &fields); err != nil {
		t.Fatalf("decode body %q: %v", w.Body.String(), err)
	}
	return fields
}

func TestAnalysisStatus_servesTheAppWideRateAsDivinePrice(t *testing.T) {
	fields := getAnalysisStatus(t, readerWithDivineRate(199))

	if got, ok := fields["divinePrice"]; !ok || got != float64(199) {
		t.Errorf("divinePrice = %v (present %v), want 199", got, ok)
	}
}

func TestAnalysisStatus_noRateOmitsDivinePrice(t *testing.T) {
	fields := getAnalysisStatus(t, exchange.NewDivineRateReader(exchange.NewCache(), nil))

	if got, ok := fields["divinePrice"]; ok {
		t.Errorf("divinePrice = %v, want the key absent while there is no rate", got)
	}
}
