package handlers

import (
	"context"
	"encoding/json"
	"math"
	"net/http"
	"net/http/httptest"
	"net/url"
	"reflect"
	"strings"
	"testing"
	"time"

	"profitofexile/internal/lab"
	"profitofexile/internal/league"
)

var historyScope = league.Historical("Mirage")

// recordingHistorySource records each bulk read the handler asks for.
type recordingHistorySource struct {
	calls [][]lab.SignalHistoryKey
	limit int
}

// SignalHistoryBulk serves each key a deep series priced by its name and
// variant, cut to the limit as the repository does.
func (s *recordingHistorySource) SignalHistoryBulk(_ context.Context, _ league.Scope, keys []lab.SignalHistoryKey, limit int) (map[lab.SignalHistoryKey][]lab.SignalChange, error) {
	s.calls = append(s.calls, keys)
	s.limit = limit
	out := make(map[lab.SignalHistoryKey][]lab.SignalChange, len(keys))
	for _, k := range keys {
		out[k] = sourceSeries(k)[:limit]
	}
	return out, nil
}

func sourceSeries(k lab.SignalHistoryKey) []lab.SignalChange {
	return historyRing(25, float64(len(k.Name)*100+len(k.Variant)))
}

func historyRing(n int, price float64) []lab.SignalChange {
	ring := make([]lab.SignalChange, n)
	for i := range ring {
		ring[i] = lab.SignalChange{Time: time.Unix(int64(10_000-i*60), 0).UTC(), Signal: "RISING", Window: "W", Price: price + float64(i), Listings: i}
	}
	return ring
}

func warmHistoryCache(byName map[string]map[string][]lab.SignalChange) *lab.Cache {
	c := lab.NewCache(historyScope)
	c.For(historyScope).SetSignalHistoryByName(byName)
	return c
}

func bulkHistoryURL(keys []string, extra string) string {
	q := url.Values{}
	for _, k := range keys {
		q.Add("key", k)
	}
	u := "/api/analysis/history/bulk?" + q.Encode()
	if extra != "" {
		u += "&" + extra
	}
	return u
}

type bulkHistoryEntry struct {
	Name    string             `json:"name"`
	Variant string             `json:"variant"`
	Count   int                `json:"count"`
	History []lab.SignalChange `json:"history"`
	Error   string             `json:"error"`
}

func serveBulkHistory(t *testing.T, h http.HandlerFunc, target string) (int, []bulkHistoryEntry) {
	t.Helper()
	rec := httptest.NewRecorder()
	h(rec, httptest.NewRequest(http.MethodGet, target, nil))
	if rec.Code != http.StatusOK {
		return rec.Code, nil
	}
	var body struct {
		Count int                `json:"count"`
		Data  []bulkHistoryEntry `json:"data"`
	}
	if err := json.Unmarshal(rec.Body.Bytes(), &body); err != nil {
		t.Fatalf("decode %s: %v (%s)", target, err, rec.Body.String())
	}
	if body.Count != len(body.Data) {
		t.Errorf("count %d, want len(data) %d", body.Count, len(body.Data))
	}
	return rec.Code, body.Data
}

// TestKeepCriticalSignalHistoryBulkAdmission: a malformed key, a missing key
// part, no key or more than twelve keys rejects the whole request before any
// read, as does a limit the single route rejects. Admitting them would let one
// public request put an unbounded key set on the shared 15-backend pool, or
// silently answer a key the caller never meant with another gem's history.
func TestKeepCriticalSignalHistoryBulkAdmission(t *testing.T) {
	thirteen := make([]string, 13)
	for i := range thirteen {
		thirteen[i] = "Gem " + string(rune('A'+i)) + "|20/20"
	}
	cases := map[string]string{
		"no key":          bulkHistoryURL(nil, ""),
		"no separator":    bulkHistoryURL([]string{"Arc|20/20", "Spark 20/20"}, ""),
		"two separators":  bulkHistoryURL([]string{"Arc|20/20|x"}, ""),
		"missing name":    bulkHistoryURL([]string{"Arc|20/20", "|20/20"}, ""),
		"missing variant": bulkHistoryURL([]string{"Arc|20/20", "Spark|"}, ""),
		"blank name":      bulkHistoryURL([]string{"  |20/20"}, ""),
		"over 12 keys":    bulkHistoryURL(thirteen, ""),
		"zero limit":      bulkHistoryURL([]string{"Arc|20/20"}, "limit=0"),
		"text limit":      bulkHistoryURL([]string{"Arc|20/20"}, "limit=four"),
	}
	for name, target := range cases {
		t.Run(name, func(t *testing.T) {
			src := &recordingHistorySource{}
			code, _ := serveBulkHistory(t, SignalHistoryBulk(src, lab.NewCache(historyScope), historyScope), target)
			if code != http.StatusBadRequest {
				t.Errorf("status %d, want 400", code)
			}
			if len(src.calls) != 0 {
				t.Errorf("rejected request read %d times, want none", len(src.calls))
			}
		})
	}
	t.Run("twelve keys are admitted", func(t *testing.T) {
		src := &recordingHistorySource{}
		code, data := serveBulkHistory(t, SignalHistoryBulk(src, lab.NewCache(historyScope), historyScope), bulkHistoryURL(thirteen[:12], ""))
		if code != http.StatusOK || len(data) != 12 || len(src.calls) != 1 {
			t.Errorf("status %d, %d entries, %d reads; want 200, 12, 1", code, len(data), len(src.calls))
		}
	})
}

// TestKeepHappySignalHistoryBulkLimit: the bulk route serves every key at
// the single route's limit default and clamp, under its normalized variant,
// in request order.
func TestKeepHappySignalHistoryBulkLimit(t *testing.T) {
	keys := []string{"Spark|20/0", "Arc|1/20c"}
	for name, tc := range map[string]struct {
		extra string
		want  int
	}{"default": {"", 4}, "explicit": {"limit=7", 7}, "clamped": {"limit=99", 20}} {
		t.Run(name+" depth", func(t *testing.T) {
			code, data := serveBulkHistory(t, SignalHistoryBulk(&recordingHistorySource{}, lab.NewCache(historyScope), historyScope), bulkHistoryURL(keys, tc.extra))
			if code != http.StatusOK || len(data) != len(keys) {
				t.Fatalf("status %d with %d entries, want 200 with %d", code, len(data), len(keys))
			}
			for _, e := range data {
				if want := sourceSeries(lab.SignalHistoryKey{Name: e.Name, Variant: e.Variant})[:tc.want]; !reflect.DeepEqual(e.History, want) || e.Count != tc.want {
					t.Errorf("%s entry = %d rows (count %d), want its %d newest", e.Name, len(e.History), e.Count, tc.want)
				}
			}
		})
	}
	t.Run("normalized variants", func(t *testing.T) {
		_, data := serveBulkHistory(t, SignalHistoryBulk(&recordingHistorySource{}, lab.NewCache(historyScope), historyScope), bulkHistoryURL(keys, ""))
		if len(data) != 2 || data[0].Variant != "20" || data[1].Variant != "1/20c" {
			t.Errorf("entries %+v, want variants 20 and 1/20c", data)
		}
	})
	t.Run("request order", func(t *testing.T) {
		_, data := serveBulkHistory(t, SignalHistoryBulk(&recordingHistorySource{}, lab.NewCache(historyScope), historyScope), bulkHistoryURL([]string{"Arc|1/20c", "Spark|20/0"}, ""))
		if len(data) != 2 || data[0].Name != "Arc" || data[1].Name != "Spark" {
			t.Errorf("entries %+v, want Arc then Spark", data)
		}
	})
}

// TestKeepHappySignalHistoryBulkMatchesSingle: each bulk entry is the single
// route's body for that key, served from the warm rings without a query.
func TestKeepHappySignalHistoryBulkMatchesSingle(t *testing.T) {
	cache := warmHistoryCache(map[string]map[string][]lab.SignalChange{
		"Spark": {"20/20": historyRing(6, 10), "1/20": historyRing(2, 500)},
		"Arc":   {"20": historyRing(3, 90)},
	})
	keys := []string{"Spark|20/20", "Arc|20/0", "Spark|1/20", "Missing|20/20"}
	_, data := serveBulkHistory(t, SignalHistoryBulk((*lab.Repository)(nil), cache, historyScope), bulkHistoryURL(keys, "limit=5"))
	if len(data) != len(keys) {
		t.Fatalf("%d entries, want %d", len(data), len(keys))
	}
	for i, k := range keys {
		name, variant, _ := strings.Cut(k, "|")
		rec := httptest.NewRecorder()
		SignalHistory(nil, cache, historyScope)(rec, httptest.NewRequest(http.MethodGet,
			"/api/analysis/history?"+url.Values{"name": {name}, "variant": {variant}, "limit": {"5"}}.Encode(), nil))
		var single bulkHistoryEntry
		if err := json.Unmarshal(rec.Body.Bytes(), &single); err != nil {
			t.Fatalf("decode single %s: %v", k, err)
		}
		if !reflect.DeepEqual(data[i], single) {
			t.Errorf("bulk entry %s = %+v, want the single route %+v", k, data[i], single)
		}
	}
}

// TestKeepHappySignalHistoryBulkEntryError: a key whose history cannot be
// encoded answers an error entry while its siblings keep their history.
func TestKeepHappySignalHistoryBulkEntryError(t *testing.T) {
	broken := historyRing(3, 1)
	broken[1].PriceVel = math.NaN()
	cache := warmHistoryCache(map[string]map[string][]lab.SignalChange{
		"Spark":  {"20/20": historyRing(3, 10)},
		"Broken": {"20/20": broken},
		"Arc":    {"20/20": historyRing(2, 40)},
	})
	code, data := serveBulkHistory(t, SignalHistoryBulk((*lab.Repository)(nil), cache, historyScope), bulkHistoryURL([]string{"Spark|20/20", "Broken|20/20", "Arc|20/20"}, ""))
	if code != http.StatusOK || len(data) != 3 {
		t.Fatalf("status %d with %d entries, want 200 with 3", code, len(data))
	}
	t.Run("broken entry carries an error", func(t *testing.T) {
		if data[1].Error == "" || data[1].History != nil {
			t.Errorf("broken entry = %+v, want an error and no history", data[1])
		}
	})
	t.Run("siblings keep their history", func(t *testing.T) {
		if !reflect.DeepEqual(data[0].History, historyRing(3, 10)) || !reflect.DeepEqual(data[2].History, historyRing(2, 40)) {
			t.Errorf("siblings = %+v / %+v, want their rings", data[0], data[2])
		}
	})
}

// TestKeepHappySignalHistoryLegacyRoute: the single-key route keeps its
// limit default, clamp and validation.
func TestKeepHappySignalHistoryLegacyRoute(t *testing.T) {
	cache := warmHistoryCache(map[string]map[string][]lab.SignalChange{"Spark": {"20/20": historyRing(20, 1)}})
	for target, want := range map[string]int{
		"name=Spark&variant=20/20":          4,
		"name=Spark&variant=20/20&limit=2":  2,
		"name=Spark&variant=20/20&limit=99": 20,
	} {
		rec := httptest.NewRecorder()
		SignalHistory(nil, cache, historyScope)(rec, httptest.NewRequest(http.MethodGet, "/api/analysis/history?"+target, nil))
		var body bulkHistoryEntry
		if err := json.Unmarshal(rec.Body.Bytes(), &body); err != nil || body.Count != want || len(body.History) != want {
			t.Errorf("%s: count %d, %d rows (err %v), want %d", target, body.Count, len(body.History), err, want)
		}
	}
	for target, wantErr := range map[string]string{
		"variant=20/20":                    "name and variant are required",
		"name=Spark":                       "name and variant are required",
		"name=Spark&variant=20/20&limit=0": "limit must be a positive integer (max 20)",
	} {
		rec := httptest.NewRecorder()
		SignalHistory(nil, cache, historyScope)(rec, httptest.NewRequest(http.MethodGet, "/api/analysis/history?"+target, nil))
		var body map[string]string
		if err := json.Unmarshal(rec.Body.Bytes(), &body); rec.Code != http.StatusBadRequest || err != nil || body["error"] != wantErr {
			t.Errorf("%s: status %d, body %s; want 400 with error %q", target, rec.Code, rec.Body.String(), wantErr)
		}
	}
}
