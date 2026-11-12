package handlers

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"profitofexile/internal/lab"
	"profitofexile/internal/league"
)

// fakeCollectivePager stands in for the SQL ranker. Its sparkline read panics
// like the nil repository the sparkline tests use, so "served without a
// gem_snapshots query" stays observable through it.
type fakeCollectivePager struct {
	page        lab.CollectivePage
	err         error
	queries     []lab.CollectiveQuery
	basePrices  map[string]float64
	baseLookups [][]string
}

func (f *fakeCollectivePager) CollectivePage(_ context.Context, _ league.Scope, q lab.CollectiveQuery) (lab.CollectivePage, error) {
	f.queries = append(f.queries, q)
	return f.page, f.err
}

func (f *fakeCollectivePager) CollectiveBasePrices(_ context.Context, _ league.Scope, baseNames []string) (map[string]float64, error) {
	f.baseLookups = append(f.baseLookups, baseNames)
	return f.basePrices, nil
}

func (f *fakeCollectivePager) SparklineData(context.Context, league.Scope, []string, string, int) (map[string][]lab.SparklinePoint, error) {
	panic("fake pager: sparkline query reached the database")
}

// rankedOverRepository serves a fixed ranked page while every decoration read
// (sparklines, GCP bases) goes to the real repository.
type rankedOverRepository struct {
	*lab.Repository
	page lab.CollectivePage
}

func (r rankedOverRepository) CollectivePage(context.Context, league.Scope, lab.CollectiveQuery) (lab.CollectivePage, error) {
	return r.page, nil
}

func servePaged(t *testing.T, pager *fakeCollectivePager, target string) *httptest.ResponseRecorder {
	t.Helper()
	cache := lab.NewCache(sparklineScope)
	warmSparklines(t, cache, nil, nil)
	w := httptest.NewRecorder()
	collectiveAnalysis(nil, pager, cache, sparklineScope).ServeHTTP(w, httptest.NewRequest(http.MethodGet, target, nil))
	return w
}

func TestCollectiveAnalysis_PagedRequestPassesParamsToThePager(t *testing.T) {
	pager := &fakeCollectivePager{}
	w := servePaged(t, pager, "/api/analysis/collective?pageSize=50&page=3&sort=roiPct&variant=20/0&color=RED&budget=120&hideLowConfidence=true")
	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, body %s", w.Code, w.Body.String())
	}
	want := lab.CollectiveQuery{
		Variant: "20", SparkVariant: "20/0", Color: "RED", Budget: 120, HideLowConfidence: true,
		Sort: lab.CollectiveSortROIPct, Page: 3, PageSize: 50,
	}
	if len(pager.queries) != 1 || pager.queries[0] != want {
		t.Errorf("pager queries = %+v, want exactly %+v", pager.queries, want)
	}
}

func TestCollectiveAnalysis_PagedRequestDefaultsToPageOneSortedByPrice(t *testing.T) {
	pager := &fakeCollectivePager{}
	servePaged(t, pager, "/api/analysis/collective?pageSize=20&color=ALL")
	want := lab.CollectiveQuery{Sort: lab.CollectiveSortPrice, Page: 1, PageSize: 20}
	if len(pager.queries) != 1 || pager.queries[0] != want {
		t.Errorf("pager queries = %+v, want exactly %+v", pager.queries, want)
	}
}

func TestCollectiveAnalysis_PagedResponseCarriesTotalPagePageSize(t *testing.T) {
	pager := &fakeCollectivePager{page: lab.CollectivePage{
		Rows:  []lab.CollectiveResult{{TransfiguredName: "Spark of Nova", Variant: "1/20"}},
		Total: 37,
	}}
	w := servePaged(t, pager, "/api/analysis/collective?pageSize=10&page=2")
	var resp struct {
		Count    int               `json:"count"`
		Total    int               `json:"total"`
		Page     int               `json:"page"`
		PageSize int               `json:"pageSize"`
		Data     []json.RawMessage `json:"data"`
	}
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if resp.Count != 1 || resp.Total != 37 || resp.Page != 2 || resp.PageSize != 10 || len(resp.Data) != 1 {
		t.Errorf("envelope = count %d total %d page %d pageSize %d rows %d; want 1, 37, 2, 10, 1",
			resp.Count, resp.Total, resp.Page, resp.PageSize, len(resp.Data))
	}
}

func TestCollectiveAnalysis_PagedRequestRejectsBadParams(t *testing.T) {
	for _, tt := range []struct {
		query, param string
	}{
		{"pageSize=0", "pageSize"},
		{"pageSize=101", "pageSize"},
		{"pageSize=ten", "pageSize"},
		{"pageSize=10&page=0", "page"},
		{"pageSize=10&sort=pct", "sort"},
		{"pageSize=10&pool=skill", "pool"},
		{"pageSize=10&color=PURPLE", "color"},
		{"pageSize=10&budget=-1", "budget"},
		{"pageSize=10&hideLowConfidence=maybe", "hideLowConfidence"},
		{"pageSize=10&variant=21/23", "variant"},
	} {
		t.Run(tt.query, func(t *testing.T) {
			pager := &fakeCollectivePager{}
			w := servePaged(t, pager, "/api/analysis/collective?"+tt.query)
			if w.Code != http.StatusBadRequest {
				t.Fatalf("status = %d, want 400", w.Code)
			}
			var body map[string]string
			if err := json.NewDecoder(w.Body).Decode(&body); err != nil {
				t.Fatalf("decode error body: %v", err)
			}
			if !strings.Contains(body["error"], tt.param) {
				t.Errorf("error = %q, want it to name %q", body["error"], tt.param)
			}
			if len(pager.queries) != 0 {
				t.Errorf("pager was queried for a rejected request: %+v", pager.queries)
			}
		})
	}
}

func TestCollectiveAnalysis_LegacyRequestKeepsCountDataShapeAndLimitCap(t *testing.T) {
	pager := &fakeCollectivePager{page: lab.CollectivePage{
		Rows:  []lab.CollectiveResult{{TransfiguredName: "Spark of Nova", Variant: "1/20"}},
		Total: 12,
	}}
	w := servePaged(t, pager, "/api/analysis/collective?limit=500&budget=30")
	var resp map[string]json.RawMessage
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if _, ok := resp["page"]; ok {
		t.Error("legacy response carries page, want it omitted")
	}
	if _, ok := resp["pageSize"]; ok {
		t.Error("legacy response carries pageSize, want it omitted")
	}
	if string(resp["count"]) != "1" || string(resp["total"]) != "12" {
		t.Errorf("count = %s, total = %s; want 1 and 12", resp["count"], resp["total"])
	}
	want := lab.CollectiveQuery{Budget: 30, Sort: lab.CollectiveSortLegacyWeightedPct, Page: 1, PageSize: 100, Legacy: true}
	if len(pager.queries) != 1 || pager.queries[0] != want {
		t.Errorf("pager queries = %+v, want exactly %+v (limit capped at 100, budget-aware sort)", pager.queries, want)
	}
}

func TestCollectiveAnalysis_LegacyRequestDefaultsToTwentyRowsByWeightedChaos(t *testing.T) {
	pager := &fakeCollectivePager{}
	servePaged(t, pager, "/api/analysis/collective?variant=20/20&search=Spark")
	want := lab.CollectiveQuery{Variant: "20/20", SparkVariant: "20/20", Search: "Spark",
		Sort: lab.CollectiveSortLegacyWeightedChaos, Page: 1, PageSize: 20, Legacy: true}
	if len(pager.queries) != 1 || pager.queries[0] != want {
		t.Errorf("pager queries = %+v, want exactly %+v", pager.queries, want)
	}
}

func TestCollectiveAnalysis_GCPRecipeDecoratesTwentyTwentyPageRowsFromOneLookup(t *testing.T) {
	pager := &fakeCollectivePager{
		page: lab.CollectivePage{Rows: []lab.CollectiveResult{
			{TransfiguredName: "Spark of Nova", BaseName: "Spark", Variant: "20/20", BasePrice: 100},
			{TransfiguredName: "Cleave of Rage", BaseName: "Cleave", Variant: "20/20", BasePrice: 80},
			{TransfiguredName: "Ice Nova of Frostbolts", BaseName: "Ice Nova", Variant: "1/20", BasePrice: 5},
		}, Total: 3},
		basePrices: map[string]float64{"Spark": 10, "Cleave": 20, "Ice Nova": 1},
	}
	w := servePaged(t, pager, "/api/analysis/collective?pageSize=10&variant=20/20")
	var resp struct {
		Data []struct {
			Name  string  `json:"transfiguredName"`
			Cost  float64 `json:"gcpRecipeCost"`
			Base  float64 `json:"gcpRecipeBase"`
			Saves float64 `json:"gcpRecipeSaves"`
		} `json:"data"`
	}
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if len(pager.baseLookups) != 1 || strings.Join(pager.baseLookups[0], ",") != "Spark,Cleave" {
		t.Errorf("base lookups = %v, want one lookup for the 20/20 rows' bases [Spark Cleave]", pager.baseLookups)
	}
	// GCP price is uncached here, so the 4c fallback applies: base + 20 x 4.
	want := map[string][3]float64{
		"Spark of Nova":          {90, 10, 10},
		"Cleave of Rage":         {100, 20, -20},
		"Ice Nova of Frostbolts": {0, 0, 0},
	}
	for _, row := range resp.Data {
		if got := [3]float64{row.Cost, row.Base, row.Saves}; got != want[row.Name] {
			t.Errorf("%s GCP (cost, base, saves) = %v, want %v", row.Name, got, want[row.Name])
		}
	}
}

func TestCollectiveAnalysis_PagerFailureReturnsQueryFailed(t *testing.T) {
	pager := &fakeCollectivePager{err: context.DeadlineExceeded}
	w := servePaged(t, pager, "/api/analysis/collective?pageSize=10")
	if w.Code != http.StatusInternalServerError || w.Body.String() != "{\"error\":\"query failed\"}\n" {
		t.Errorf("status = %d, body = %q; want 500 query failed", w.Code, w.Body.String())
	}
}
