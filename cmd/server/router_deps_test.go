package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"reflect"
	"strings"
	"testing"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/league"
	"profitofexile/internal/mercenary"
	"profitofexile/internal/mercure"
	"profitofexile/internal/server"
	"profitofexile/internal/server/handlers"
)

// productionRouter builds the router the way main.go does, minus everything a
// device-less request never reaches. The pool is never dialled: each request
// stops at the device check before persistence.
func productionRouter(t *testing.T) http.Handler {
	t.Helper()
	var cfg server.RouterConfig
	newRouterDeps(&pgxpool.Pool{}, league.Scope{}, "", "", nil).applyTo(&cfg)
	return server.NewRouter(handlers.NopPinger{}, nil, cfg)
}

// assertDeviceRequired fails on anything but the handler's 401, so a route
// that is not registered (chi's JSON 404) fails the test.
func assertDeviceRequired(t *testing.T, router http.Handler, method, path, body string) {
	t.Helper()
	w := httptest.NewRecorder()
	router.ServeHTTP(w, httptest.NewRequest(method, path, strings.NewReader(body)))
	if w.Code != http.StatusUnauthorized {
		t.Fatalf("%s %s: status = %d, want 401 (body: %s)", method, path, w.Code, w.Body.String())
	}
	var got map[string]string
	if err := json.Unmarshal(w.Body.Bytes(), &got); err != nil {
		t.Fatalf("%s %s: decode body: %v (body: %s)", method, path, err, w.Body.String())
	}
	if want := map[string]string{"error": "device identification required"}; !reflect.DeepEqual(got, want) {
		t.Errorf("%s %s: body = %#v, want %#v", method, path, got, want)
	}
}

func TestNewRouterDeps_RegistersEveryBossLootRouteBehindTheDeviceCheck(t *testing.T) {
	router := productionRouter(t)
	for _, tc := range []struct{ method, path string }{
		{http.MethodPost, "/api/boss-loot/runs"},
		{http.MethodGet, "/api/boss-loot/runs"},
		{http.MethodPatch, "/api/boss-loot/runs/1"},
		{http.MethodPost, "/api/boss-loot/runs/1/delete"},
		{http.MethodGet, "/api/boss-loot/stats"},
	} {
		assertDeviceRequired(t, router, tc.method, tc.path, `{}`)
	}
}

func TestNewRouterDeps_RegistersLabRunRoutesBehindTheDeviceCheck(t *testing.T) {
	router := productionRouter(t)
	validRun := `{"difficulty":"Uber","strategy":"speed","elapsed_seconds":300,"room_count":8}`
	assertDeviceRequired(t, router, http.MethodPost, "/api/lab/runs", validRun)
	assertDeviceRequired(t, router, http.MethodGet, "/api/lab/runs", "")
}

func TestNewRouterDeps_RegistersFontSessionRouteBehindTheDeviceCheck(t *testing.T) {
	router := productionRouter(t)
	validSession := `{"lab_type":"Uber","total_crafts":3,"rounds":[{"option_chosen":"a"}]}`
	assertDeviceRequired(t, router, http.MethodPost, "/api/desktop/font-session", validSession)
}

func TestNewRouterDeps_RegistersMercWriteRoutesBehindTheDeviceCheck(t *testing.T) {
	router := productionRouter(t)
	assertDeviceRequired(t, router, http.MethodPost, "/api/desktop/merc-templates", `{}`)
	assertDeviceRequired(t, router, http.MethodPost, "/api/desktop/merc-templates/tombstone", `{}`)
}

func TestLayoutPublisher_IsUntypedNilWithoutAHubURL(t *testing.T) {
	if pub := layoutPublisher("", "secret"); pub != nil {
		t.Fatalf("layoutPublisher(\"\", secret) = %#v, want untyped nil", pub)
	}
}

func TestLayoutPublisher_IsUntypedNilWithoutASecret(t *testing.T) {
	if pub := layoutPublisher("http://hub/.well-known/mercure", ""); pub != nil {
		t.Fatalf("layoutPublisher(url, \"\") = %#v, want untyped nil", pub)
	}
}

func TestLayoutPublisher_CarriesTheConfiguredHub(t *testing.T) {
	pub := layoutPublisher("http://hub/.well-known/mercure", "secret")
	want := &mercure.HubPublisher{URL: "http://hub/.well-known/mercure", Secret: "secret"}
	if !reflect.DeepEqual(pub, mercure.Publisher(want)) {
		t.Fatalf("layoutPublisher = %#v, want %#v", pub, want)
	}
}

func TestKnownGemFunc_IsNilForANilRepository(t *testing.T) {
	if fn := knownGemFunc(nil); fn != nil {
		t.Fatal("knownGemFunc(nil) returned a func, want nil")
	}
}

func TestNewRouterDeps_MercLimiterUsesTheMercenaryUploadBudget(t *testing.T) {
	deps := newRouterDeps(&pgxpool.Pool{}, league.Scope{}, "", "", nil)
	for i := 0; i < mercenary.DefaultUploadBudget; i++ {
		if ok, _ := deps.MercLimiter.Allow("fp"); !ok {
			t.Fatalf("request %d refused, want %d allowed", i+1, mercenary.DefaultUploadBudget)
		}
	}
	if ok, _ := deps.MercLimiter.Allow("fp"); ok {
		t.Fatalf("request %d allowed, want refused after the budget", mercenary.DefaultUploadBudget+1)
	}
}
