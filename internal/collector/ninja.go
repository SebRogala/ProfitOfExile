package collector

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"time"

	"profitofexile/internal/price/gemcolor"
)

const (
	defaultNinjaBaseURL = "https://poe.ninja/poe1/api"
	ninjaTimeout        = 30 * time.Second
	ninjaUserAgent      = "ProfitOfExile/1.0 (price-collector)"
)

// NinjaFetcher fetches price data from the poe.ninja API.
type NinjaFetcher struct {
	client   *http.Client
	baseURL  string
	resolver *gemcolor.Resolver
}

// NewNinjaFetcher creates a fetcher for the poe.ninja API.
// The resolver is used to map gem names to colors; pass nil if color
// resolution is not needed (colors will be empty strings).
func NewNinjaFetcher(resolver *gemcolor.Resolver) *NinjaFetcher {
	return &NinjaFetcher{
		client:   &http.Client{Timeout: ninjaTimeout},
		baseURL:  defaultNinjaBaseURL,
		resolver: resolver,
	}
}

// ninjaGemLine represents a single gem entry in the poe.ninja SkillGem response.
type ninjaGemLine struct {
	Name         string  `json:"name"`
	Variant      string  `json:"variant"`
	ChaosValue   float64 `json:"chaosValue"`
	ListingCount int     `json:"listingCount"`
	Corrupted    bool    `json:"corrupted"`
	Icon         string  `json:"icon"`
	TradeFilter  struct {
		Query struct {
			Type struct {
				Discriminator string `json:"discriminator"`
			} `json:"type"`
		} `json:"query"`
	} `json:"tradeFilter"`
}

// ninjaCurrencyLine represents a single currency entry from the poe.ninja
// economy/exchange/current/overview endpoint.
type ninjaCurrencyLine struct {
	ID           string  `json:"id"`
	PrimaryValue float64 `json:"primaryValue"`
	Sparkline    struct {
		TotalChange float64 `json:"totalChange"`
	} `json:"sparkline"`
}

// ninjaItemLine represents a single entry in a poe.ninja
// economy/stash/current/item/overview response. It is the shape shared by every
// category in ItemEndpoints (IncursionTemple, Vial and the five unique slots).
//
// Variant and Links arrive as JSON null on categories and lines that do not
// carry them (no IncursionTemple, Vial or UniqueFlask line had either on the
// 2026-09-06 payload). Unmarshalling null into a non-pointer leaves the Go zero
// value untouched, which is exactly the "" / 0 the storage layer wants, so
// these stay plain types rather than pointers. DivineValue is null on lines
// cheaper than a divine and reads back as 0 the same way, and ItemType,
// LevelRequired and StackSize are simply absent on the categories that do not
// carry them (StackSize is a Vial-only key; IncursionTemple sends none of the
// three). ExaltedValue, by contrast, is present on every line of every category
// measured on 2026-09-06 — it is a third price axis, not an optional one.
//
// The sparkline key is `sparkLine` on this endpoint, not the `sparkline` the
// exchange overview uses for currency and fragments.
type ninjaItemLine struct {
	ID            int64   `json:"id"`
	Name          string  `json:"name"`
	DetailsID     string  `json:"detailsId"`
	Variant       string  `json:"variant"`
	Links         int     `json:"links"`
	ChaosValue    float64 `json:"chaosValue"`
	DivineValue   float64 `json:"divineValue"`
	ExaltedValue  float64 `json:"exaltedValue"`
	ListingCount  int     `json:"listingCount"`
	Count         int     `json:"count"`
	StackSize     int     `json:"stackSize"`
	Icon          string  `json:"icon"`
	ItemClass     int     `json:"itemClass"`
	ItemType      string  `json:"itemType"`
	BaseType      string  `json:"baseType"`
	LevelRequired int     `json:"levelRequired"`
	SparkLine     struct {
		TotalChange float64 `json:"totalChange"`
	} `json:"sparkLine"`
}

// ninjaResponse wraps the top-level poe.ninja API response shape.
type ninjaResponse[T any] struct {
	Lines []T `json:"lines"`
}

// httpResult holds the HTTP response metadata and body from a cache-aware
// request. Callers must close Body when non-nil.
type httpResult struct {
	StatusCode  int
	ETag        string
	Age         int  // seconds since origin server generated the response
	AgePresent  bool // true when the Age header was present in the response (any value, including 0)
	Body        io.ReadCloser
	NotModified bool
}

// FetchGemsEndpoint is a FetchFunc-compatible method that fetches SkillGem data
// with conditional request support. When etag is non-empty, it sends an
// If-None-Match header; a 304 response returns FetchResult{NotModified: true}.
func (f *NinjaFetcher) FetchGemsEndpoint(ctx context.Context, league string, etag string) (*FetchResult, error) {
	endpoint := fmt.Sprintf("%s/economy/stash/current/item/overview?league=%s&type=SkillGem", f.baseURL, url.QueryEscape(league))

	hr, err := f.getWithCache(ctx, endpoint, etag)
	if err != nil {
		return nil, fmt.Errorf("ninja: fetch gems: %w", err)
	}

	if hr.NotModified {
		return &FetchResult{
			NotModified: true,
			ETag:        hr.ETag,
			Age:         hr.Age,
			AgePresent:  hr.AgePresent,
		}, nil
	}
	defer hr.Body.Close()

	var resp ninjaResponse[ninjaGemLine]
	if err := json.NewDecoder(hr.Body).Decode(&resp); err != nil {
		return nil, fmt.Errorf("ninja: decode gems response: %w", err)
	}

	snapshots := f.convertGemLines(resp.Lines)

	slog.Info("ninja: fetched gems", "total_api", len(resp.Lines), "after_filter", len(snapshots), "age", hr.Age, "etag", hr.ETag)
	result := &FetchResult{
		GemData:    snapshots,
		ETag:       hr.ETag,
		Age:        hr.Age,
		AgePresent: hr.AgePresent,
	}
	if err := result.Validate(); err != nil {
		return nil, fmt.Errorf("ninja: gems result invalid: %w", err)
	}
	return result, nil
}

// FetchCurrencyEndpoint is a FetchFunc-compatible method that fetches Currency
// data with conditional request support.
func (f *NinjaFetcher) FetchCurrencyEndpoint(ctx context.Context, league string, etag string) (*FetchResult, error) {
	endpoint := fmt.Sprintf("%s/economy/exchange/current/overview?league=%s&type=Currency", f.baseURL, url.QueryEscape(league))

	hr, err := f.getWithCache(ctx, endpoint, etag)
	if err != nil {
		return nil, fmt.Errorf("ninja: fetch currency: %w", err)
	}

	if hr.NotModified {
		return &FetchResult{
			NotModified: true,
			ETag:        hr.ETag,
			Age:         hr.Age,
			AgePresent:  hr.AgePresent,
		}, nil
	}
	defer hr.Body.Close()

	var resp ninjaResponse[ninjaCurrencyLine]
	if err := json.NewDecoder(hr.Body).Decode(&resp); err != nil {
		return nil, fmt.Errorf("ninja: decode currency response: %w", err)
	}

	snapshots := convertCurrencyLines(resp.Lines)

	slog.Info("ninja: fetched currency", "count", len(snapshots), "age", hr.Age, "etag", hr.ETag)
	result := &FetchResult{
		CurrencyData: snapshots,
		ETag:         hr.ETag,
		Age:          hr.Age,
		AgePresent:   hr.AgePresent,
	}
	if err := result.Validate(); err != nil {
		return nil, fmt.Errorf("ninja: currency result invalid: %w", err)
	}
	return result, nil
}

// FetchFragmentEndpoint is a FetchFunc-compatible method that fetches Fragment
// data with conditional request support. Same exchange endpoint as currency but
// with type=Fragment.
func (f *NinjaFetcher) FetchFragmentEndpoint(ctx context.Context, league string, etag string) (*FetchResult, error) {
	endpoint := fmt.Sprintf("%s/economy/exchange/current/overview?league=%s&type=Fragment", f.baseURL, url.QueryEscape(league))

	hr, err := f.getWithCache(ctx, endpoint, etag)
	if err != nil {
		return nil, fmt.Errorf("ninja: fetch fragments: %w", err)
	}

	if hr.NotModified {
		return &FetchResult{
			NotModified: true,
			ETag:        hr.ETag,
			Age:         hr.Age,
			AgePresent:  hr.AgePresent,
		}, nil
	}
	defer hr.Body.Close()

	var resp ninjaResponse[ninjaCurrencyLine]
	if err := json.NewDecoder(hr.Body).Decode(&resp); err != nil {
		return nil, fmt.Errorf("ninja: decode fragment response: %w", err)
	}

	snapshots := convertFragmentLines(resp.Lines)

	slog.Info("ninja: fetched fragments", "count", len(snapshots), "age", hr.Age, "etag", hr.ETag)
	result := &FetchResult{
		FragmentData: snapshots,
		ETag:         hr.ETag,
		Age:          hr.Age,
		AgePresent:   hr.AgePresent,
	}
	if err := result.Validate(); err != nil {
		return nil, fmt.Errorf("ninja: fragment result invalid: %w", err)
	}
	return result, nil
}

// ItemEndpointFetcher returns a FetchFunc bound to one poe.ninja item-overview
// category. Every category in ItemEndpoints shares this one implementation and
// this one response shape; what differs per category is the `type` query value,
// and — because each returned FetchFunc becomes its own EndpointConfig — the
// ETag state, staleness read and Mercure topic the scheduler keeps for it.
//
// A category poe.ninja does not serve for a league answers 404. getWithCache
// turns that into an error naming the status, the scheduler logs it as a failed
// fetch for that endpoint alone, and the remaining categories are untouched.
func (f *NinjaFetcher) ItemEndpointFetcher(category string) FetchFunc {
	return func(ctx context.Context, league string, etag string) (*FetchResult, error) {
		return f.fetchItems(ctx, category, league, etag)
	}
}

// fetchItems performs one conditional GET against the item-overview endpoint
// for a single category and converts the payload into ItemSnapshots.
func (f *NinjaFetcher) fetchItems(ctx context.Context, category string, league string, etag string) (*FetchResult, error) {
	endpoint := fmt.Sprintf("%s/economy/stash/current/item/overview?league=%s&type=%s",
		f.baseURL, url.QueryEscape(league), url.QueryEscape(category))

	hr, err := f.getWithCache(ctx, endpoint, etag)
	if err != nil {
		return nil, fmt.Errorf("ninja: fetch items %s: %w", category, err)
	}

	if hr.NotModified {
		return &FetchResult{
			NotModified: true,
			ETag:        hr.ETag,
			Age:         hr.Age,
			AgePresent:  hr.AgePresent,
		}, nil
	}
	defer hr.Body.Close()

	var resp ninjaResponse[ninjaItemLine]
	if err := json.NewDecoder(hr.Body).Decode(&resp); err != nil {
		return nil, fmt.Errorf("ninja: decode items %s response: %w", category, err)
	}

	snapshots := convertItemLines(category, resp.Lines)

	slog.Info("ninja: fetched items", "category", category, "total_api", len(resp.Lines), "stored", len(snapshots), "age", hr.Age, "etag", hr.ETag)
	result := &FetchResult{
		ItemData:   snapshots,
		ETag:       hr.ETag,
		Age:        hr.Age,
		AgePresent: hr.AgePresent,
	}
	if err := result.Validate(); err != nil {
		return nil, fmt.Errorf("ninja: items %s result invalid: %w", category, err)
	}
	return result, nil
}

// convertItemLines transforms raw item-overview lines into ItemSnapshots,
// stamping each with the category it was fetched under.
//
// The only line it drops is one whose numeric id is absent or zero: that id is
// the table's identity, so an id-less line cannot be keyed, and a batch of them
// would silently collapse to a single row under ON CONFLICT DO NOTHING. Dropped
// lines are counted in a warning rather than passing unnoticed. This is an
// identity precondition, not market filtering — no price, listing-count or name
// rule removes a line here.
func convertItemLines(category string, lines []ninjaItemLine) []ItemSnapshot {
	snapshots := make([]ItemSnapshot, 0, len(lines))
	unidentified := 0
	for _, line := range lines {
		if line.ID == 0 {
			unidentified++
			continue
		}
		snapshots = append(snapshots, ItemSnapshot{
			Category:        category,
			NinjaID:         line.ID,
			DetailsID:       line.DetailsID,
			Name:            line.Name,
			Variant:         line.Variant,
			Links:           line.Links,
			Chaos:           line.ChaosValue,
			Divine:          line.DivineValue,
			Exalted:         line.ExaltedValue,
			Listings:        line.ListingCount,
			SampleCount:     line.Count,
			StackSize:       line.StackSize,
			Icon:            line.Icon,
			ItemClass:       line.ItemClass,
			ItemType:        line.ItemType,
			BaseType:        line.BaseType,
			LevelRequired:   line.LevelRequired,
			SparklineChange: line.SparkLine.TotalChange,
		})
	}
	if unidentified > 0 {
		slog.Warn("ninja: item lines without an id, cannot be keyed",
			"category", category,
			"dropped", unidentified,
			"kept", len(snapshots),
		)
	}
	return snapshots
}

// convertFragmentLines transforms raw API fragment lines into FragmentSnapshots.
func convertFragmentLines(lines []ninjaCurrencyLine) []FragmentSnapshot {
	snapshots := make([]FragmentSnapshot, 0, len(lines))
	for _, line := range lines {
		snapshots = append(snapshots, FragmentSnapshot{
			FragmentID:      line.ID,
			Chaos:           line.PrimaryValue,
			SparklineChange: line.Sparkline.TotalChange,
		})
	}
	return snapshots
}

// convertGemLines filters and transforms raw API gem lines into GemSnapshots.
func (f *NinjaFetcher) convertGemLines(lines []ninjaGemLine) []GemSnapshot {
	snapshots := make([]GemSnapshot, 0, len(lines))
	for _, line := range lines {
		// Skip Heist-exclusive gems (identified by "Trarthus" in name) -- not obtainable in standard league play.
		if strings.Contains(line.Name, "Trarthus") {
			continue
		}

		isTransfigured := strings.HasPrefix(line.TradeFilter.Query.Type.Discriminator, "alt_")

		var color string
		if f.resolver != nil {
			if c, ok := f.resolver.Resolve(line.Name); ok {
				color = c.String()
			}
		}

		variant := line.Variant
		if variant == "" {
			variant = "default"
		}

		snapshots = append(snapshots, GemSnapshot{
			Name:           line.Name,
			Variant:        variant,
			Chaos:          line.ChaosValue,
			Listings:       line.ListingCount,
			IsTransfigured: isTransfigured,
			IsCorrupted:    line.Corrupted,
			GemColor:       color,
		})
	}

	// Log unresolved gems so operators can seed gem_colors.
	if f.resolver != nil {
		if unresolved := f.resolver.UnresolvedGems(); len(unresolved) > 0 {
			slog.Warn("unresolved gem colors", "count", len(unresolved), "gems", unresolved)
		}
	}

	return snapshots
}

// convertCurrencyLines transforms raw API currency lines into CurrencySnapshots.
func convertCurrencyLines(lines []ninjaCurrencyLine) []CurrencySnapshot {
	snapshots := make([]CurrencySnapshot, 0, len(lines))
	for _, line := range lines {
		snapshots = append(snapshots, CurrencySnapshot{
			CurrencyID:      line.ID,
			Chaos:           line.PrimaryValue,
			SparklineChange: line.Sparkline.TotalChange,
		})
	}
	return snapshots
}

// getWithCache performs an HTTP GET request with conditional request support.
// When etag is non-empty, it sends an If-None-Match header. On a 304 response,
// it returns an httpResult with NotModified=true and a nil Body. On a 200
// response, it returns the response body (caller must close) along with parsed
// ETag and Age headers.
func (f *NinjaFetcher) getWithCache(ctx context.Context, rawURL string, etag string) (*httpResult, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, rawURL, nil)
	if err != nil {
		return nil, fmt.Errorf("create request: %w", err)
	}
	req.Header.Set("User-Agent", ninjaUserAgent)

	if etag != "" {
		req.Header.Set("If-None-Match", etag)
	}

	resp, err := f.client.Do(req)
	if err != nil {
		return nil, fmt.Errorf("http get %s: %w", rawURL, err)
	}

	// Parse Age header (seconds since origin generated the response).
	age := 0
	ageStr := resp.Header.Get("Age")
	agePresent := ageStr != ""
	if agePresent {
		parsed, parseErr := strconv.Atoi(ageStr)
		if parseErr != nil {
			slog.Warn("invalid Age header, defaulting to 0",
				"raw", ageStr, "url", rawURL, "error", parseErr)
		} else if parsed < 0 {
			slog.Warn("negative Age header, defaulting to 0",
				"raw", ageStr, "url", rawURL)
		} else {
			age = parsed
		}
	}

	// Parse ETag from response.
	respETag := resp.Header.Get("ETag")

	// Handle 304 Not Modified.
	if resp.StatusCode == http.StatusNotModified {
		resp.Body.Close()
		return &httpResult{
			StatusCode:  resp.StatusCode,
			ETag:        respETag,
			Age:         age,
			AgePresent:  agePresent,
			NotModified: true,
		}, nil
	}

	if resp.StatusCode != http.StatusOK {
		body, _ := io.ReadAll(io.LimitReader(resp.Body, 512))
		resp.Body.Close()
		return nil, fmt.Errorf("http get %s: status %d: %s", rawURL, resp.StatusCode, string(body))
	}

	return &httpResult{
		StatusCode: resp.StatusCode,
		ETag:       respETag,
		Age:        age,
		AgePresent: agePresent,
		Body:       resp.Body,
	}, nil
}
