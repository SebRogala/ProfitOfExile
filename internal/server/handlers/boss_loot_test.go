package handlers

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"profitofexile/internal/device"
	"profitofexile/internal/ratelimit"
	"profitofexile/internal/server/middleware"
	"strconv"
	"strings"
	"testing"
	"time"
)

type bossLootDevice struct{}

func (bossLootDevice) Upsert(_ context.Context, fp, _ string) (*device.Device, error) {
	return &device.Device{Fingerprint: fp}, nil
}
func TestBossLootRequiresIdentityBeforeDatabase(t *testing.T) {
	h := NewBossLoot(nil, ratelimit.NewRateLimiter(3, time.Hour, 8))
	for _, handler := range []http.HandlerFunc{h.Submit, h.List, h.Edit, h.Delete, h.Stats} {
		w := httptest.NewRecorder()
		handler(w, httptest.NewRequest("POST", "/", strings.NewReader(`{}`)))
		if w.Code != 401 || !strings.Contains(w.Body.String(), "device identification required") {
			t.Fatalf("%d %s", w.Code, w.Body.String())
		}
	}
}
func TestBossLootRejectsInvalidRequestBeforeDatabase(t *testing.T) {
	h := NewBossLoot(nil, nil)
	for _, tc := range []struct {
		handler    http.HandlerFunc
		path, body string
	}{
		{h.Submit, "/", `{"mode":"new","items":{"quantities":{"Eye":-1},"counted":["Eye"]}}`},
		{h.Submit, "/", `{} {}`}, {h.Submit, "/", `null`}, {h.Submit, "/", `{`},
		{h.List, "/?boss=catarina&profileKey=Normal&profileVersion=1&page=0", ``},
		{h.List, "/?boss=catarina&profileKey=Normal&profileVersion=1&pageSize=201", ``},
		{h.List, "/?boss=catarina&profileKey=Normal&profileVersion=1&page=52&pageSize=200", ``},
		{h.Stats, "/?boss=catarina&profileKey=Normal&profileVersion=1&scope=other", ``},
	} {
		req := httptest.NewRequest("POST", tc.path, strings.NewReader(tc.body))
		req.Header.Set("X-Device-ID", strings.Repeat("a", 64))
		w := httptest.NewRecorder()
		middleware.DeviceMiddleware(bossLootDevice{})(tc.handler).ServeHTTP(w, req)
		if w.Code != 400 || !strings.Contains(w.Body.String(), `"error"`) {
			t.Fatalf("%s: %d %s", tc.path, w.Code, w.Body.String())
		}
	}
}

func TestBossLootWritesShareDeviceBudget(t *testing.T) {
	h := NewBossLoot(nil, ratelimit.NewRateLimiter(3, time.Hour, 8))
	writes := []http.HandlerFunc{h.Submit, h.Edit, h.Delete}
	request := func(handler http.HandlerFunc, fp string) *httptest.ResponseRecorder {
		t.Helper()
		req := httptest.NewRequest("POST", "/", strings.NewReader(`{`))
		req.Header.Set("X-Device-ID", strings.Repeat(fp, 64))
		w := httptest.NewRecorder()
		middleware.DeviceMiddleware(bossLootDevice{})(handler).ServeHTTP(w, req)
		return w
	}
	for i := 0; i < 3; i++ {
		w := request(writes[i%len(writes)], "a")
		if w.Code != 400 || !strings.Contains(w.Body.String(), "invalid JSON body") {
			t.Fatalf("write %d: %d %s", i, w.Code, w.Body.String())
		}
	}
	for _, handler := range writes {
		w := request(handler, "a")
		var body map[string]string
		err := json.Unmarshal(w.Body.Bytes(), &body)
		retry, retryErr := strconv.Atoi(w.Header().Get("Retry-After"))
		if w.Code != 429 || err != nil || len(body) != 1 || body["error"] != "upload rate limit exceeded" || retryErr != nil || retry < 1 || w.Header().Get("Content-Type") != "application/json" {
			t.Fatalf("refusal: %d %s headers=%v", w.Code, w.Body.String(), w.Header())
		}
	}
	for _, handler := range writes {
		w := request(handler, "b")
		if w.Code != 400 || !strings.Contains(w.Body.String(), "invalid JSON body") {
			t.Fatalf("independent device: %d %s", w.Code, w.Body.String())
		}
	}
}
