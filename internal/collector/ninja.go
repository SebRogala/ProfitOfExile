package collector

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"net/http"
	"strings"
	"time"

	"profitofexile/internal/price/gemcolor"
)

const (
	defaultNinjaBaseURL = "https://poe.ninja/poe1/api"
	ninjaTimeout        = 30 * time.Second
	ninjaUserAgent      = "ProfitOfExile/1.0 (price-collector)"
)

// NinjaFetcher implements Fetcher for the poe.ninja API.
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

// ninjaCurrencyLine represents a single currency entry in the poe.ninja Currency
// item overview response. Fields match the stash/current/item/overview endpoint.
type ninjaCurrencyLine struct {
	CurrencyTypeName string  `json:"currencyTypeName"`
	ChaosEquivalent  float64 `json:"chaosEquivalent"`
	Sparkline        struct {
		TotalChange float64 `json:"totalChange"`
	} `json:"receiveSparkLine"`
}

// ninjaResponse wraps the top-level poe.ninja API response shape.
type ninjaResponse[T any] struct {
	Lines []T `json:"lines"`
}

// FetchGems retrieves all SkillGem prices from poe.ninja, filters out corrupted
// and Heist-exclusive gems, detects transfigured variants, and resolves gem colors.
func (f *NinjaFetcher) FetchGems(ctx context.Context, league string) ([]GemSnapshot, error) {
	url := fmt.Sprintf("%s/economy/stash/current/item/overview?league=%s&type=SkillGem", f.baseURL, league)

	var resp ninjaResponse[ninjaGemLine]
	if err := f.get(ctx, url, &resp); err != nil {
		return nil, fmt.Errorf("ninja: fetch gems: %w", err)
	}

	snapshots := make([]GemSnapshot, 0, len(resp.Lines))
	for _, line := range resp.Lines {
		// Skip corrupted gems — prices are unreliable.
		if line.Corrupted {
			continue
		}

		// Skip Heist-exclusive gems (Trarthus variants).
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
			GemColor:       color,
		})
	}

	// Log unresolved gems so operators can seed gem_colors.
	if f.resolver != nil {
		if unresolved := f.resolver.UnresolvedGems(); len(unresolved) > 0 {
			slog.Warn("unresolved gem colors", "count", len(unresolved), "gems", unresolved)
		}
	}

	slog.Info("ninja: fetched gems", "total_api", len(resp.Lines), "after_filter", len(snapshots))
	return snapshots, nil
}

// FetchCurrency retrieves all Currency prices from poe.ninja.
func (f *NinjaFetcher) FetchCurrency(ctx context.Context, league string) ([]CurrencySnapshot, error) {
	url := fmt.Sprintf("%s/economy/stash/current/item/overview?league=%s&type=Currency", f.baseURL, league)

	var resp ninjaResponse[ninjaCurrencyLine]
	if err := f.get(ctx, url, &resp); err != nil {
		return nil, fmt.Errorf("ninja: fetch currency: %w", err)
	}

	snapshots := make([]CurrencySnapshot, 0, len(resp.Lines))
	for _, line := range resp.Lines {
		snapshots = append(snapshots, CurrencySnapshot{
			CurrencyID:      line.CurrencyTypeName,
			Chaos:           line.ChaosEquivalent,
			SparklineChange: line.Sparkline.TotalChange,
		})
	}

	slog.Info("ninja: fetched currency", "count", len(snapshots))
	return snapshots, nil
}

// get performs an HTTP GET request and JSON-decodes the response body into dst.
func (f *NinjaFetcher) get(ctx context.Context, url string, dst any) error {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return fmt.Errorf("create request: %w", err)
	}
	req.Header.Set("User-Agent", ninjaUserAgent)

	resp, err := f.client.Do(req)
	if err != nil {
		return fmt.Errorf("http get %s: %w", url, err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("http get %s: unexpected status %d", url, resp.StatusCode)
	}

	if err := json.NewDecoder(resp.Body).Decode(dst); err != nil {
		return fmt.Errorf("decode response from %s: %w", url, err)
	}

	return nil
}
