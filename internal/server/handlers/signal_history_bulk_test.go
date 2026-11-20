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

func (s *recordingHistorySource) SignalHistoryBulk(_ context.Context, _ league.Scope, keys []lab.SignalHistoryKey, limit int) (map[lab.SignalHistoryKey][]lab.SignalChange, error) {
	s.calls = append(s.calls, keys)
	s.limit = limit
	return nil, nil
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
// read, as does a limit the single route rejects.
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

// TestKeepHappySignalHistoryBulkLimit: the bulk route applies the single
// route's limit default, clamp and normalized variant to every key, in
// request order.
func TestKeepHappySignalHistoryBulkLimit(t *testing.T) {
	for name, tc := range map[string]struct {
		extra string
		want  int
	}{"default": {"", 4}, "explicit": {"limit=7", 7}, "clamped": {"limit=99", 20}} {
		t.Run(name, func(t *testing.T) {
			src := &recordingHistorySource{}
			_, data := serveBulkHistory(t, SignalHistoryBulk(src, lab.NewCache(historyScope), historyScope), bulkHistoryURL([]string{"Spark|20/0", "Arc|1/20c"}, tc.extra))
			if src.limit != tc.want {
				t.Errorf("read limit %d, want %d", src.limit, tc.want)
			}
			wantKeys := []lab.SignalHistoryKey{{Name: "Spark", Variant: "20"}, {Name: "Arc", Variant: "1/20c"}}
			if len(src.calls) != 1 || !reflect.DeepEqual(src.calls[0], wantKeys) {
				t.Errorf("read keys %v, want %v", src.calls, wantKeys)
			}
			if len(data) != 2 || data[0].Name != "Spark" || data[1].Name != "Arc" {
				t.Errorf("entries %+v, want Spark then Arc", data)
			}
		})
	}
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
	if data[1].Error == "" || data[1].History != nil {
		t.Errorf("broken entry = %+v, want an error and no history", data[1])
	}
	if !reflect.DeepEqual(data[0].History, historyRing(3, 10)) || !reflect.DeepEqual(data[2].History, historyRing(2, 40)) {
		t.Errorf("siblings = %+v / %+v, want their rings", data[0], data[2])
	}
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
	for _, target := range []string{"variant=20/20", "name=Spark", "name=Spark&variant=20/20&limit=0"} {
		rec := httptest.NewRecorder()
		SignalHistory(nil, cache, historyScope)(rec, httptest.NewRequest(http.MethodGet, "/api/analysis/history?"+target, nil))
		if rec.Code != http.StatusBadRequest {
			t.Errorf("%s: status %d, want 400", target, rec.Code)
		}
	}
}
