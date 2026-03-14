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

// ninjaResponse wraps the top-level poe.ninja API response shape.
type ninjaResponse[T any] struct {
	Lines []T `json:"lines"`
}

// httpResult holds the HTTP response metadata and body from a cache-aware
// request. Callers must close Body when non-nil.
type httpResult struct {
	StatusCode  int
	ETag        string
	Age         int // seconds since origin server generated the response
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
		GemData: snapshots,
		ETag:    hr.ETag,
		Age:     hr.Age,
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
	}
	if err := result.Validate(); err != nil {
		return nil, fmt.Errorf("ninja: fragment result invalid: %w", err)
	}
	return result, nil
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
	if ageStr := resp.Header.Get("Age"); ageStr != "" {
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
		Body:       resp.Body,
	}, nil
}

