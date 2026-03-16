package trade

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strconv"
	"strings"
	"time"
)

const (
	defaultTradeBaseURL = "https://www.pathofexile.com"
	tradeClientTimeout  = 10 * time.Second
)

// ErrRateLimited is returned when the GGG trade API responds with 429.
var ErrRateLimited = errors.New("trade: rate limited (429)")

// Client is an HTTP client for the GGG Path of Exile trade API.
type Client struct {
	httpClient *http.Client
	baseURL    string
	userAgent  string
	leagueName string
}

// NewClient creates a trade API client from the given configuration.
func NewClient(cfg TradeConfig) *Client {
	return &Client{
		httpClient: &http.Client{Timeout: tradeClientTimeout},
		baseURL:    defaultTradeBaseURL,
		userAgent:  cfg.UserAgent,
		leagueName: cfg.LeagueName,
	}
}

// SetBaseURL overrides the base URL for testing (e.g., httptest.Server.URL).
func (c *Client) SetBaseURL(url string) {
	c.baseURL = url
}

// tradeSearchResp is the GGG trade search API response shape.
type tradeSearchResp struct {
	ID     string   `json:"id"`
	Result []string `json:"result"`
	Total  int      `json:"total"`
}

// tradeFetchResp is the GGG trade fetch API response shape.
type tradeFetchResp struct {
	Result []tradeFetchEntry `json:"result"`
}

type tradeFetchEntry struct {
	Listing tradeFetchListing `json:"listing"`
	Item    tradeFetchItem    `json:"item"`
}

type tradeFetchListing struct {
	Indexed time.Time          `json:"indexed"`
	Account tradeFetchAccount  `json:"account"`
	Price   tradeFetchPrice    `json:"price"`
}

type tradeFetchAccount struct {
	Name string `json:"name"`
}

type tradeFetchPrice struct {
	Amount   float64 `json:"amount"`
	Currency string  `json:"currency"`
}

type tradeFetchItem struct {
	Corrupted  bool                   `json:"corrupted"`
	Properties []tradeFetchProperty   `json:"properties"`
}

type tradeFetchProperty struct {
	Name   string          `json:"name"`
	Values [][]interface{} `json:"values"`
}

// Search performs a trade search for the given gem name and variant.
// Variant format is "level/quality" (e.g., "20/20") or just "level" (e.g., "20").
// It returns the parsed search response, the raw HTTP response headers
// (for rate limit synchronisation), and any error.
func (c *Client) Search(ctx context.Context, gem, variant string) (*SearchResponse, http.Header, error) {
	gemLevel, gemQuality := parseVariant(variant)
	body := buildSearchQuery(gem, gemLevel, gemQuality)

	url := fmt.Sprintf("%s/api/trade/search/%s", c.baseURL, c.leagueName)

	req, err := http.NewRequestWithContext(ctx, http.MethodPost, url, bytes.NewReader(body))
	if err != nil {
		return nil, nil, fmt.Errorf("trade search: create request: %w", err)
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("User-Agent", c.userAgent)

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, nil, fmt.Errorf("trade search: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode == http.StatusTooManyRequests {
		return nil, resp.Header, ErrRateLimited
	}

	if resp.StatusCode != http.StatusOK {
		excerpt, _ := io.ReadAll(io.LimitReader(resp.Body, 512))
		return nil, resp.Header, fmt.Errorf("trade search: status %d: %s", resp.StatusCode, string(excerpt))
	}

	var parsed tradeSearchResp
	if err := json.NewDecoder(resp.Body).Decode(&parsed); err != nil {
		return nil, resp.Header, fmt.Errorf("trade search: decode response: %w", err)
	}

	return &SearchResponse{
		QueryID: parsed.ID,
		IDs:     parsed.Result,
		Total:   parsed.Total,
	}, resp.Header, nil
}

// Fetch retrieves listing details for the given result IDs from a prior search.
// At most 10 IDs are fetched (the GGG API limit per request).
// Returns parsed listing details, raw HTTP headers, and any error.
func (c *Client) Fetch(ctx context.Context, queryID string, ids []string) ([]TradeListingDetail, http.Header, error) {
	if len(ids) > 10 {
		ids = ids[:10]
	}

	url := fmt.Sprintf("%s/api/trade/fetch/%s?query=%s", c.baseURL, strings.Join(ids, ","), queryID)

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return nil, nil, fmt.Errorf("trade fetch: create request: %w", err)
	}
	req.Header.Set("User-Agent", c.userAgent)

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, nil, fmt.Errorf("trade fetch: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode == http.StatusTooManyRequests {
		return nil, resp.Header, ErrRateLimited
	}

	if resp.StatusCode != http.StatusOK {
		excerpt, _ := io.ReadAll(io.LimitReader(resp.Body, 512))
		return nil, resp.Header, fmt.Errorf("trade fetch: status %d: %s", resp.StatusCode, string(excerpt))
	}

	var parsed tradeFetchResp
	if err := json.NewDecoder(resp.Body).Decode(&parsed); err != nil {
		return nil, resp.Header, fmt.Errorf("trade fetch: decode response: %w", err)
	}

	listings := make([]TradeListingDetail, 0, len(parsed.Result))
	for _, entry := range parsed.Result {
		detail := TradeListingDetail{
			Price:     entry.Listing.Price.Amount,
			Currency:  entry.Listing.Price.Currency,
			Account:   entry.Listing.Account.Name,
			IndexedAt: entry.Listing.Indexed,
			Corrupted: entry.Item.Corrupted,
		}

		// Extract gem level and quality from item properties.
		for _, prop := range entry.Item.Properties {
			val := extractPropertyValue(prop)
			switch prop.Name {
			case "Level":
				detail.GemLevel = val
			case "Quality":
				detail.GemQuality = val
			}
		}

		listings = append(listings, detail)
	}

	return listings, resp.Header, nil
}

// parseVariant splits a variant string like "20/20" into level and quality.
// Returns (level, quality). If quality is missing, returns (level, -1).
func parseVariant(variant string) (int, int) {
	parts := strings.SplitN(variant, "/", 2)
	level, _ := strconv.Atoi(parts[0])
	quality := -1
	if len(parts) == 2 {
		quality, _ = strconv.Atoi(parts[1])
	}
	return level, quality
}

// buildSearchQuery constructs the JSON body for a GGG trade search request.
// Filters by gem category, exact level, exact quality, not corrupted, priced
// listings only, collapsed by account.
func buildSearchQuery(gem string, gemLevel, gemQuality int) []byte {
	miscFilters := map[string]interface{}{
		"corrupted": map[string]interface{}{"option": "false"},
	}
	if gemLevel > 0 {
		miscFilters["gem_level"] = map[string]interface{}{"min": gemLevel, "max": gemLevel}
	}
	if gemQuality >= 0 {
		miscFilters["quality"] = map[string]interface{}{"min": gemQuality, "max": gemQuality}
	}

	query := map[string]interface{}{
		"query": map[string]interface{}{
			"type": gem,
			"stats": []map[string]interface{}{
				{"type": "and", "filters": []interface{}{}},
			},
			"filters": map[string]interface{}{
				"type_filters": map[string]interface{}{
					"filters": map[string]interface{}{
						"category": map[string]string{"option": "gem"},
					},
				},
				"misc_filters": map[string]interface{}{
					"filters": miscFilters,
				},
				"trade_filters": map[string]interface{}{
					"filters": map[string]interface{}{
						"sale_type": map[string]string{"option": "priced"},
						"collapse":  map[string]string{"option": "true"},
					},
				},
			},
			"status": map[string]string{"option": "any"},
		},
		"sort": map[string]string{"price": "asc"},
	}

	data, _ := json.Marshal(query)
	return data
}

// extractPropertyValue parses a numeric value from a GGG item property.
// Properties come as [["20", 0]] where the first element of the inner array
// is the display string (which may include "+" prefix or "%" suffix).
func extractPropertyValue(prop tradeFetchProperty) int {
	if len(prop.Values) == 0 || len(prop.Values[0]) == 0 {
		return 0
	}

	raw, ok := prop.Values[0][0].(string)
	if !ok {
		return 0
	}

	// Strip common prefixes/suffixes: "+20%", "20", etc.
	raw = strings.TrimPrefix(raw, "+")
	raw = strings.TrimSuffix(raw, "%")

	var val int
	fmt.Sscanf(raw, "%d", &val)
	return val
}
