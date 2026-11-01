package handlers

import (
	"context"
	"net/http"
	"net/http/httptest"
	"profitofexile/internal/device"
	"profitofexile/internal/server/middleware"
	"strings"
	"testing"
)

type bossLootDevice struct{}

func (bossLootDevice) Upsert(_ context.Context, fp, _ string) (*device.Device, error) {
	return &device.Device{Fingerprint: fp}, nil
}
func TestBossLootRequiresIdentityBeforeDatabase(t *testing.T) {
	h := NewBossLoot(nil)
	for _, handler := range []http.HandlerFunc{h.Submit, h.List, h.Edit, h.Delete, h.Stats} {
		w := httptest.NewRecorder()
		handler(w, httptest.NewRequest("POST", "/", strings.NewReader(`{}`)))
		if w.Code != 401 || !strings.Contains(w.Body.String(), "device identification required") {
			t.Fatalf("%d %s", w.Code, w.Body.String())
		}
	}
}
func TestBossLootRejectsInvalidRequestBeforeDatabase(t *testing.T) {
	h := NewBossLoot(nil)
	for _, tc := range []struct {
		handler    http.HandlerFunc
		path, body string
	}{
		{h.Submit, "/", `{"mode":"new","items":{"quantities":{"Eye":-1},"counted":["Eye"]}}`},
		{h.Submit, "/", `{} {}`}, {h.Submit, "/", `null`}, {h.Submit, "/", `{`},
		{h.List, "/?boss=catarina&profileKey=Normal&profileVersion=1&page=0", ``},
		{h.List, "/?boss=catarina&profileKey=Normal&profileVersion=1&pageSize=201", ``},
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
