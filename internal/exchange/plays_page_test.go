package exchange

import (
	"math"
	"net/url"
	"reflect"
	"strings"
	"testing"
)

func TestPlaysPageQuery_AbsentGateIsOff(t *testing.T) {
	q, err := ParsePlaysPageQuery(url.Values{"pageSize": {"50"}})
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	if q.Gates != (PlaysGates{}) {
		t.Fatalf("gates = %+v, want every gate 0 (off): the server never defaults a gate", q.Gates)
	}
}

func TestPlaysPageQuery_ParsesEveryContractParam(t *testing.T) {
	q, err := ParsePlaysPageQuery(url.Values{
		"page": {"3"}, "pageSize": {"all"}, "sort": {"fastest"},
		"onlyCategory": {"Currency", "Essences"}, "hideCategory": {"Threads"},
		"onlyItem": {"a"}, "hideItem": {"b", "c"},
		"minItemPrice": {"0.5"}, "minItemPriceDiv": {"0.4"}, "minRoiChaos": {"1"}, "minTurnover": {"2"},
		"maxTickPct": {"3"}, "minEdgeTickRatio": {"4"}, "minRoiPct": {"5"},
		"investMin": {"10"}, "investMax": {"20"}, "unit": {"divine"},
	})
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	ten, twenty := 10.0, 20.0
	want := PlaysPageQuery{
		Page: 3, PageSize: PageSizeAll, Sort: SortFastest,
		OnlyCategories: []string{"Currency", "Essences"}, HideCategories: []string{"Threads"},
		OnlyItems: []string{"a"}, HideItems: []string{"b", "c"},
		Gates:     PlaysGates{MinItemPrice: 0.5, MinItemPriceDiv: 0.4, MinRoiChaos: 1, MinTurnover: 2, MaxTickPct: 3, MinEdgeTickRatio: 4, MinRoiPct: 5},
		InvestMin: &ten, InvestMax: &twenty, Unit: UnitDivine,
	}
	if !reflect.DeepEqual(q, want) {
		t.Fatalf("query = %+v\nwant    %+v", q, want)
	}
}

func TestPlaysPageQuery_DefaultsPageOneSortExpectedUnitChaos(t *testing.T) {
	q, err := ParsePlaysPageQuery(url.Values{"pageSize": {"25"}})
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	if q.Page != 1 || q.PageSize != 25 || q.Sort != SortExpected || q.Unit != UnitChaos || q.InvestMin != nil || q.InvestMax != nil {
		t.Fatalf("query = %+v, want page 1, size 25, sort expected, unit chaos, no bounds", q)
	}
}

func TestPlaysPageQuery_RejectsUnknownPageSize(t *testing.T) {
	for _, size := range []string{"10", "0", "-25", "ALL", ""} {
		_, err := ParsePlaysPageQuery(url.Values{"pageSize": {size}})
		if err == nil || !strings.Contains(err.Error(), "pageSize") {
			t.Errorf("pageSize=%q: err = %v, want an error naming pageSize", size, err)
		}
	}
}

func TestPlaysPageQuery_RejectsUnknownSort(t *testing.T) {
	_, err := ParsePlaysPageQuery(url.Values{"pageSize": {"25"}, "sort": {"pct"}})
	if err == nil || !strings.Contains(err.Error(), "sort") {
		t.Fatalf("err = %v, want an error naming sort", err)
	}
}

func TestPlaysPageQuery_RejectsEveryBadValueNamingTheParam(t *testing.T) {
	for param, bad := range map[string]string{
		"page": "0", "minItemPrice": "abc", "minItemPriceDiv": "NaN", "minRoiChaos": "x",
		"minTurnover": "Inf", "maxTickPct": "1e400", "minEdgeTickRatio": "-", "minRoiPct": "",
		"investMin": "cheap", "investMax": "NaN", "unit": "exalt",
	} {
		_, err := ParsePlaysPageQuery(url.Values{"pageSize": {"25"}, param: {bad}})
		if err == nil || !strings.Contains(err.Error(), param) {
			t.Errorf("%s=%q: err = %v, want an error naming %s", param, bad, err, param)
		}
	}
}

func TestPlaysPageQuery_NegativeGateIsOff(t *testing.T) {
	q, err := ParsePlaysPageQuery(url.Values{"pageSize": {"25"}, "minRoiChaos": {"-3"}})
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	if q.Gates.MinRoiChaos != 0 {
		t.Fatalf("minRoiChaos = %v, want 0 (off)", q.Gates.MinRoiChaos)
	}
}

func TestResolveSearch_MatchesDisplayNamesCaseInsensitively(t *testing.T) {
	ids := ResolveSearch("  dIvInE oRb ")
	found := false
	for _, id := range ids {
		if !strings.Contains(strings.ToLower(DisplayName(id)), "divine orb") {
			t.Errorf("%s (%q) does not contain the query", id, DisplayName(id))
		}
		if id == "Metadata/Items/Currency/CurrencyModValues" {
			found = true
		}
	}
	if !found {
		t.Fatalf("ResolveSearch(divine orb) = %v, want it to include the Divine Orb", ids)
	}
}

func TestResolveSearch_BlankQueryMatchesNothing(t *testing.T) {
	if ids := ResolveSearch("   "); len(ids) != 0 {
		t.Fatalf("ResolveSearch(blank) = %d ids, want none", len(ids))
	}
}

// validQuery is a query every Validate case starts from.
func validQuery() PlaysPageQuery {
	return PlaysPageQuery{Page: 1, PageSize: 25, Sort: SortExpected, Unit: UnitChaos}
}

func TestPlaysPageQueryValidate_AcceptsAValidQuery(t *testing.T) {
	q := validQuery()
	q.PageSize = PageSizeAll
	if err := q.Validate(); err != nil {
		t.Fatalf("Validate = %v, want nil", err)
	}
}

func TestPlaysPageQueryValidate_RejectsEach400ClassNamingTheParam(t *testing.T) {
	nan, inf := math.NaN(), math.Inf(1)
	cases := []struct {
		param string
		edit  func(*PlaysPageQuery)
	}{
		{"page", func(q *PlaysPageQuery) { q.Page = 0 }},
		{"pageSize", func(q *PlaysPageQuery) { q.PageSize = 30 }},
		{"sort", func(q *PlaysPageQuery) { q.Sort = "pct" }},
		{"unit", func(q *PlaysPageQuery) { q.Unit = "exalt" }},
		{"minItemPrice", func(q *PlaysPageQuery) { q.Gates.MinItemPrice = nan }},
		{"minItemPriceDiv", func(q *PlaysPageQuery) { q.Gates.MinItemPriceDiv = inf }},
		{"minRoiChaos", func(q *PlaysPageQuery) { q.Gates.MinRoiChaos = math.Inf(-1) }},
		{"minTurnover", func(q *PlaysPageQuery) { q.Gates.MinTurnover = nan }},
		{"maxTickPct", func(q *PlaysPageQuery) { q.Gates.MaxTickPct = inf }},
		{"minEdgeTickRatio", func(q *PlaysPageQuery) { q.Gates.MinEdgeTickRatio = nan }},
		{"minRoiPct", func(q *PlaysPageQuery) { q.Gates.MinRoiPct = inf }},
		{"investMin", func(q *PlaysPageQuery) { q.InvestMin = &nan }},
		{"investMax", func(q *PlaysPageQuery) { q.InvestMax = &inf }},
	}
	for _, c := range cases {
		q := validQuery()
		c.edit(&q)
		if err := q.Validate(); err == nil || !strings.Contains(err.Error(), c.param) {
			t.Errorf("%s: Validate = %v, want an error naming %s", c.param, err, c.param)
		}
	}
}

func TestPlaysPageQuery_RejectsNoncanonicalPageSize(t *testing.T) {
	for _, size := range []string{"025", "+25"} {
		_, err := ParsePlaysPageQuery(url.Values{"pageSize": {size}})
		if err == nil || !strings.Contains(err.Error(), "pageSize") {
			t.Errorf("pageSize=%q: err = %v, want an error naming pageSize", size, err)
		}
	}
}
