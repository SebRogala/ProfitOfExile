package exchange

import (
	"context"
	"errors"
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

// Messages shared by the parser (syntax) and Validate (constraints), so a
// param fails with one sentence whichever step rejects it.
const (
	msgPageSize = "pageSize must be one of 25, 50, 100, 200, all"
	msgPage     = "page must be an integer >= 1"
	msgSort     = "sort must be one of expected, roi, fastest"
	msgUnit     = "unit must be one of chaos, divine"
)

// ParsePlaysPageQuery parses a paged request's params and validates the result
// (Validate). The error names the offending param and is meant for a 400 body.
func ParsePlaysPageQuery(values url.Values) (PlaysPageQuery, error) {
	q := PlaysPageQuery{Page: 1, Sort: SortExpected, Unit: UnitChaos}

	// The transport enum is the exact spelling: 025 or +25 is not a page size.
	switch size := values.Get("pageSize"); size {
	case "25", "50", "100", "200":
		q.PageSize, _ = strconv.Atoi(size)
	case "all":
		q.PageSize = PageSizeAll
	default:
		return PlaysPageQuery{}, fmt.Errorf(msgPageSize)
	}

	if values.Has("page") {
		page, err := strconv.Atoi(values.Get("page"))
		if err != nil {
			return PlaysPageQuery{}, fmt.Errorf(msgPage)
		}
		q.Page = page
	}

	if values.Has("sort") {
		q.Sort = PlaysSort(values.Get("sort"))
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
		v, err := numberParam(values, g.param)
		if err != nil {
			return PlaysPageQuery{}, err
		}
		// A finite gate at or below 0 is off, as in applyGates; a non-finite one
		// is kept for Validate to reject.
		if v < 0 && !math.IsInf(v, -1) {
			v = 0
		}
		*g.dst = v
	}

	for _, b := range []struct {
		param string
		dst   **float64
	}{{"investMin", &q.InvestMin}, {"investMax", &q.InvestMax}} {
		if !values.Has(b.param) {
			continue
		}
		v, err := numberParam(values, b.param)
		if err != nil {
			return PlaysPageQuery{}, err
		}
		*b.dst = &v
	}

	if values.Has("unit") {
		q.Unit = PlaysUnit(values.Get("unit"))
	}

	q.Search = strings.TrimSpace(values.Get("search"))
	q.SearchItemIDs = ResolveSearch(q.Search)
	if err := q.Validate(); err != nil {
		return PlaysPageQuery{}, err
	}
	return q, nil
}

// Validate checks the typed constraints of the contract: page >= 1, pageSize in
// its enum, sort and unit whitelisted, every gate and bound finite. Mode and
// Horizon are the handler's (validated as on the legacy request).
func (q PlaysPageQuery) Validate() error {
	if q.Page < 1 {
		return fmt.Errorf(msgPage)
	}
	switch q.PageSize {
	case PageSizeAll, 25, 50, 100, 200:
	default:
		return fmt.Errorf(msgPageSize)
	}
	switch q.Sort {
	case SortExpected, SortRoi, SortFastest:
	default:
		return fmt.Errorf(msgSort)
	}
	switch q.Unit {
	case UnitChaos, UnitDivine:
	default:
		return fmt.Errorf(msgUnit)
	}
	numbers := []struct {
		param string
		v     float64
	}{
		{"minItemPrice", q.Gates.MinItemPrice},
		{"minItemPriceDiv", q.Gates.MinItemPriceDiv},
		{"minRoiChaos", q.Gates.MinRoiChaos},
		{"minTurnover", q.Gates.MinTurnover},
		{"maxTickPct", q.Gates.MaxTickPct},
		{"minEdgeTickRatio", q.Gates.MinEdgeTickRatio},
		{"minRoiPct", q.Gates.MinRoiPct},
	}
	if q.InvestMin != nil {
		numbers = append(numbers, struct {
			param string
			v     float64
		}{"investMin", *q.InvestMin})
	}
	if q.InvestMax != nil {
		numbers = append(numbers, struct {
			param string
			v     float64
		}{"investMax", *q.InvestMax})
	}
	for _, n := range numbers {
		if math.IsNaN(n.v) || math.IsInf(n.v, 0) {
			return fmt.Errorf("%s must be a finite number", n.param)
		}
	}
	return nil
}

// numberParam reads a number's syntax. An out-of-range literal (1e400) comes
// back as ±Inf and NaN as NaN, so Validate rejects them as non-finite.
func numberParam(values url.Values, param string) (float64, error) {
	v, err := strconv.ParseFloat(values.Get(param), 64)
	if err != nil && !errors.Is(err, strconv.ErrRange) {
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
