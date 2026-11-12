package exchange

import (
	"context"
	"fmt"
	"math"
	"net/url"
	"sort"
	"strconv"
	"strings"
	"time"
)

// PageSizeAll is the pageSize value meaning "uncapped" (ADR-030 D7/D8 interim
// owner call).
const PageSizeAll = 0

// PlaysSort is a paged request's sort key; every sort is over a Go-computed
// PlayDisplay key, missing keys last, then the served rank.
type PlaysSort string

const (
	SortExpected PlaysSort = "expected"
	SortRoi      PlaysSort = "roi"
	SortFastest  PlaysSort = "fastest"
)

// PlaysUnit is the unit the run-cost bounds are typed in.
type PlaysUnit string

const (
	UnitChaos  PlaysUnit = "chaos"
	UnitDivine PlaysUnit = "divine"
)

// PlaysGates are the seven client gates (desktop filters.ts applyGates). A gate
// of 0 is off, and an absent gate is 0: the server never defaults one (ADR-015,
// ADR-017).
type PlaysGates struct {
	MinItemPrice     float64
	MinItemPriceDiv  float64
	MinRoiChaos      float64
	MinTurnover      float64
	MaxTickPct       float64
	MinEdgeTickRatio float64
	MinRoiPct        float64
}

// PlaysPageQuery is one paged GET /api/currency-exchange/plays request. Mode and
// Horizon are validated and set by the handler, as on the legacy request.
type PlaysPageQuery struct {
	Mode    string
	Horizon Horizon
	Page    int
	// PageSize is 25, 50, 100, 200 or PageSizeAll.
	PageSize       int
	Sort           PlaysSort
	OnlyCategories []string
	HideCategories []string
	OnlyItems      []string
	HideItems      []string
	Gates          PlaysGates
	// InvestMin and InvestMax bound the run investment; nil is no bound.
	InvestMin *float64
	InvestMax *float64
	Unit      PlaysUnit
	// Search is the trimmed query; SearchItemIDs is its resolution through the
	// item asset. A search bypasses the page size.
	Search        string
	SearchItemIDs []string
}

// IsPagedRequest reports whether a request carries pageSize, which selects the
// paged contract; without it the legacy body is served unchanged.
func IsPagedRequest(values url.Values) bool {
	return values.Has("pageSize")
}

// ParsePlaysPageQuery parses and validates a paged request's params. The error
// names the offending param and is meant for a 400 body.
func ParsePlaysPageQuery(values url.Values) (PlaysPageQuery, error) {
	q := PlaysPageQuery{Page: 1, Sort: SortExpected, Unit: UnitChaos}

	switch size := values.Get("pageSize"); size {
	case "25", "50", "100", "200":
		q.PageSize, _ = strconv.Atoi(size)
	case "all":
		q.PageSize = PageSizeAll
	default:
		return PlaysPageQuery{}, fmt.Errorf("pageSize must be one of 25, 50, 100, 200, all")
	}

	if values.Has("page") {
		page, err := strconv.Atoi(values.Get("page"))
		if err != nil || page < 1 {
			return PlaysPageQuery{}, fmt.Errorf("page must be an integer >= 1")
		}
		q.Page = page
	}

	if values.Has("sort") {
		switch s := PlaysSort(values.Get("sort")); s {
		case SortExpected, SortRoi, SortFastest:
			q.Sort = s
		default:
			return PlaysPageQuery{}, fmt.Errorf("sort must be one of expected, roi, fastest")
		}
	}

	q.OnlyCategories = values["onlyCategory"]
	q.HideCategories = values["hideCategory"]
	q.OnlyItems = values["onlyItem"]
	q.HideItems = values["hideItem"]

	gates := []struct {
		param string
		dst   *float64
	}{
		{"minItemPrice", &q.Gates.MinItemPrice},
		{"minItemPriceDiv", &q.Gates.MinItemPriceDiv},
		{"minRoiChaos", &q.Gates.MinRoiChaos},
		{"minTurnover", &q.Gates.MinTurnover},
		{"maxTickPct", &q.Gates.MaxTickPct},
		{"minEdgeTickRatio", &q.Gates.MinEdgeTickRatio},
		{"minRoiPct", &q.Gates.MinRoiPct},
	}
	for _, g := range gates {
		if !values.Has(g.param) {
			continue
		}
		v, err := finiteParam(values, g.param)
		if err != nil {
			return PlaysPageQuery{}, err
		}
		// A gate at or below 0 is off, as in applyGates.
		*g.dst = math.Max(v, 0)
	}

	for _, b := range []struct {
		param string
		dst   **float64
	}{{"investMin", &q.InvestMin}, {"investMax", &q.InvestMax}} {
		if !values.Has(b.param) {
			continue
		}
		v, err := finiteParam(values, b.param)
		if err != nil {
			return PlaysPageQuery{}, err
		}
		*b.dst = &v
	}

	if values.Has("unit") {
		switch u := PlaysUnit(values.Get("unit")); u {
		case UnitChaos, UnitDivine:
			q.Unit = u
		default:
			return PlaysPageQuery{}, fmt.Errorf("unit must be one of chaos, divine")
		}
	}

	q.Search = strings.TrimSpace(values.Get("search"))
	q.SearchItemIDs = ResolveSearch(q.Search)
	return q, nil
}

func finiteParam(values url.Values, param string) (float64, error) {
	v, err := strconv.ParseFloat(values.Get(param), 64)
	if err != nil || math.IsNaN(v) || math.IsInf(v, 0) {
		return 0, fmt.Errorf("%s must be a finite number", param)
	}
	return v, nil
}

// ResolveSearch returns, sorted, the ids of every item in the item asset whose
// display name contains query, trimmed and case-insensitively (the desktop's
// matchesSearch). A blank query matches nothing.
func ResolveSearch(query string) []string {
	needle := strings.ToLower(strings.TrimSpace(query))
	if needle == "" {
		return nil
	}
	var ids []string
	for id := range itemsByID {
		if strings.Contains(strings.ToLower(DisplayName(id)), needle) {
			ids = append(ids, id)
		}
	}
	sort.Strings(ids)
	return ids
}

// UniverseItem is one item of the served-after-mode plays, with the number of
// plays naming it on any side. Names, icons and categories are the transport's.
type UniverseItem struct {
	ID        string
	PlayCount int
}

// PlaysPage is one page of the stored plays and the counts around it.
type PlaysPage struct {
	// Warm is false while nothing has been stored for the horizon.
	Warm            bool
	League          string
	Horizon         Horizon
	Hours           int
	DivineChaosRate float64
	From, To        time.Time
	// LastUpdated is the newest feed hour; zero when the set covers none.
	LastUpdated time.Time
	// Plays is the page, decorated, in the requested order.
	Plays []Play
	// Total counts the served plays after the mode filter; AfterRules,
	// AfterGates and Matched count each later stage (desktop rowCounts).
	Total, AfterRules, AfterGates, Matched int
	// ShowConvert is whether any matched play has a convert step.
	ShowConvert  bool
	ItemUniverse []UniverseItem
}

// PlaysPager serves one page of the stored plays.
type PlaysPager interface {
	PlaysPage(ctx context.Context, q PlaysPageQuery) (PlaysPage, error)
}
