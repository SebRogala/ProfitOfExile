package exchange

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"time"
)

// DefaultBaseURL is the public currency-exchange feed root.
const DefaultBaseURL = "https://web.poecdn.com/api/currency-exchange"

const (
	userAgent      = "ProfitOfExile/1.0 (currency-exchange)"
	requestTimeout = 30 * time.Second

	// errorBodyLimit caps how much of a failing response body statusError quotes
	// back in the error message.
	errorBodyLimit = 512

	// bodyReadLimit caps every non-200 body read. The observed not-published
	// body is 42 bytes, so 1 KiB leaves room for the feed's shape to grow while
	// keeping a stray HTML error page out of memory.
	bodyReadLimit = 1024
)

// Realm path segments. RealmPC is the empty string because the PC realm has no
// segment in the feed URL.
const (
	RealmPC   = ""
	RealmXbox = "xbox"
	RealmSony = "sony"
	RealmPoE2 = "poe2"
)

// Client fetches hours from the currency-exchange feed.
//
// It has no retries and no backoff on purpose: the collector loop that drives it
// owns retry policy. baseURL stays unexported so tests can point the client at
// an httptest server.
type Client struct {
	http    *http.Client
	baseURL string
}

// NewClient returns a Client for the public feed with a 30 second timeout.
func NewClient() *Client {
	return &Client{
		http:    &http.Client{Timeout: requestTimeout},
		baseURL: DefaultBaseURL,
	}
}

// FetchHour fetches one hour of the feed.
//
// hour is the unix hour to request; hour == 0 omits the id segment, which the
// feed answers with the oldest hour it still retains. realm == RealmPC omits the
// realm segment.
//
// An hour that is not published yet answers 404, which FetchHour reports as
// *ErrNotPublished carrying the feed's own next_change_id — on that body the
// cursor is the current unpublished hour, so it is the hour to retry. The error
// is only ever a pointer; detect it with:
//
//	var notPublished *exchange.ErrNotPublished
//	if errors.As(err, &notPublished) { ... }
func (c *Client) FetchHour(ctx context.Context, hour int64, realm string) (*HourPayload, error) {
	rawURL := c.hourURL(hour, realm)

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, rawURL, nil)
	if err != nil {
		return nil, fmt.Errorf("exchange: create request %s: %w", rawURL, err)
	}
	req.Header.Set("User-Agent", userAgent)

	resp, err := c.http.Do(req)
	if err != nil {
		return nil, fmt.Errorf("exchange: fetch hour %d: %w", hour, err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		body, readErr := io.ReadAll(io.LimitReader(resp.Body, bodyReadLimit))

		// A 404 is the feed's normal answer for an hour that has not been
		// published yet, and its body is the same JSON shape with an empty
		// markets array and the hour to retry in next_change_id. All three
		// conditions have to hold before the 404 is reported as a retry signal:
		// hour 0 is FetchHour's "oldest retained hour" sentinel, so a zero
		// cursor must never be handed back as a retry target. Anything else —
		// an HTML error page, a 404 that still carries markets, a body like
		// {} or {"markets":null} — is a plain status error.
		if resp.StatusCode == http.StatusNotFound {
			var payload HourPayload
			decodeErr := json.Unmarshal(body, &payload)
			if decodeErr == nil && len(payload.Markets) == 0 && payload.NextChangeID > 0 {
				return nil, &ErrNotPublished{NextChangeID: payload.NextChangeID}
			}
		}

		return nil, statusError(hour, resp.StatusCode, body, readErr)
	}

	var payload HourPayload
	if err := json.NewDecoder(resp.Body).Decode(&payload); err != nil {
		return nil, fmt.Errorf("exchange: decode hour %d: %w", hour, err)
	}
	return &payload, nil
}

// hourURL builds baseURL[/realm][/hour].
func (c *Client) hourURL(hour int64, realm string) string {
	rawURL := strings.TrimSuffix(c.baseURL, "/")
	if realm != RealmPC {
		rawURL += "/" + url.PathEscape(realm)
	}
	if hour != 0 {
		rawURL += "/" + strconv.FormatInt(hour, 10)
	}
	return rawURL
}

// statusError renders a non-200 response. It owns the quote cap: at most
// errorBodyLimit bytes of body reach the message, whatever the caller read.
// A readErr means the quoted body is short or empty, so it is named rather than
// dropped.
func statusError(hour int64, status int, body []byte, readErr error) error {
	if len(body) > errorBodyLimit {
		body = body[:errorBodyLimit]
	}
	if readErr != nil {
		return fmt.Errorf("exchange: fetch hour %d: status %d: %s: read body: %w", hour, status, string(body), readErr)
	}
	return fmt.Errorf("exchange: fetch hour %d: status %d: %s", hour, status, string(body))
}

// ErrNotPublished reports that the requested hour is not published yet.
//
// NextChangeID is the cursor the feed named in its 404 body: the current
// unpublished hour, which is the hour to retry. Only *ErrNotPublished implements
// error, so callers detect it with errors.As on a pointer.
type ErrNotPublished struct {
	NextChangeID int64
}

// Error implements error on the pointer so a bare ErrNotPublished value cannot
// satisfy the interface and slip past an errors.As check.
func (e *ErrNotPublished) Error() string {
	return fmt.Sprintf("exchange: hour not published yet (next_change_id=%d)", e.NextChangeID)
}
