package exchange

import (
	"context"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// emptyHourBody is a minimal valid 200 body for tests that assert on the
// request rather than on the decoded payload.
const emptyHourBody = `{"next_change_id":1787119200,"markets":[]}`

// readTestdata returns the bytes of a file under testdata/.
func readTestdata(t *testing.T, name string) []byte {
	t.Helper()
	data, err := os.ReadFile(filepath.Join("testdata", name))
	if err != nil {
		t.Fatalf("read testdata/%s: %v", name, err)
	}
	return data
}

// testClient builds a Client exactly as NewClient does, then points it at the
// test server through the unexported baseURL.
func testClient(t *testing.T, server *httptest.Server) *Client {
	t.Helper()
	c := NewClient()
	c.baseURL = server.URL
	return c
}

// serveBytes starts a test server answering every request with status and body.
func serveBytes(t *testing.T, status int, body []byte) *httptest.Server {
	t.Helper()
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(status)
		w.Write(body)
	}))
	t.Cleanup(server.Close)
	return server
}

func TestFetchHour_publishedHour_decodesMarkets(t *testing.T) {
	server := serveBytes(t, http.StatusOK, readTestdata(t, "hour_allflame_sample.json"))

	payload, err := testClient(t, server).FetchHour(context.Background(), 1787119200, RealmPC)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if payload.NextChangeID != 1787122800 {
		t.Errorf("NextChangeID = %d, want 1787122800", payload.NextChangeID)
	}
	if len(payload.Markets) != 25 {
		t.Fatalf("got %d markets, want 25", len(payload.Markets))
	}

	first := payload.Markets[0]
	if first.League != "Allflame" {
		t.Errorf("Markets[0].League = %q, want %q", first.League, "Allflame")
	}
	wantID := "Metadata/Items/Currency/CurrencyRerollRare|Metadata/Items/Currency/CurrencyModValues"
	if first.MarketID != wantID {
		t.Errorf("Markets[0].MarketID = %q, want %q", first.MarketID, wantID)
	}
	wantPair := []string{
		"Metadata/Items/Currency/CurrencyRerollRare",
		"Metadata/Items/Currency/CurrencyModValues",
	}
	if len(first.MarketPair) != 2 || first.MarketPair[0] != wantPair[0] || first.MarketPair[1] != wantPair[1] {
		t.Errorf("Markets[0].MarketPair = %q, want %q", first.MarketPair, wantPair)
	}
	if got := first.LowestRatio[wantPair[0]]; got != 196 {
		t.Errorf("Markets[0].LowestRatio[chaos] = %d, want 196", got)
	}
	if got := first.LowestRatio[wantPair[1]]; got != 1 {
		t.Errorf("Markets[0].LowestRatio[divine] = %d, want 1", got)
	}
	if got := first.VolumeTraded[wantPair[0]]; got != 13001051 {
		t.Errorf("Markets[0].VolumeTraded[chaos] = %d, want 13001051", got)
	}
}

func TestFetchHour_setsUserAgentHeader(t *testing.T) {
	var gotAgent string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		gotAgent = r.Header.Get("User-Agent")
		w.Write([]byte(emptyHourBody))
	}))
	defer server.Close()

	if _, err := testClient(t, server).FetchHour(context.Background(), 1787119200, RealmPC); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if gotAgent != "ProfitOfExile/1.0 (currency-exchange)" {
		t.Errorf("User-Agent = %q, want %q", gotAgent, "ProfitOfExile/1.0 (currency-exchange)")
	}
}

func TestFetchHour_requestPath(t *testing.T) {
	tests := []struct {
		name       string
		baseSuffix string
		hour       int64
		realm      string
		wantPath   string
	}{
		{name: "pc realm omits the realm segment", hour: 1787119200, realm: RealmPC, wantPath: "/1787119200"},
		{name: "xbox realm adds a realm segment", hour: 1787119200, realm: RealmXbox, wantPath: "/xbox/1787119200"},
		{name: "sony realm adds a realm segment", hour: 1787119200, realm: RealmSony, wantPath: "/sony/1787119200"},
		{name: "hour zero omits the hour segment", hour: 0, realm: RealmPC, wantPath: "/"},
		{name: "hour zero keeps the realm segment", hour: 0, realm: RealmXbox, wantPath: "/xbox"},
		{name: "trailing slash on the base url is trimmed", baseSuffix: "/", hour: 1787119200, realm: RealmPC, wantPath: "/1787119200"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var gotPath string
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				gotPath = r.URL.Path
				w.Write([]byte(emptyHourBody))
			}))
			defer server.Close()

			c := testClient(t, server)
			c.baseURL += tt.baseSuffix

			if _, err := c.FetchHour(context.Background(), tt.hour, tt.realm); err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if gotPath != tt.wantPath {
				t.Errorf("request path = %q, want %q", gotPath, tt.wantPath)
			}
		})
	}
}

func TestFetchHour_unpublishedHour_returnsErrNotPublishedWithFeedCursor(t *testing.T) {
	server := serveBytes(t, http.StatusNotFound, readTestdata(t, "not_published.json"))

	payload, err := testClient(t, server).FetchHour(context.Background(), 1787126400, RealmPC)
	if payload != nil {
		t.Errorf("payload = %+v, want nil", payload)
	}

	var notPublished *ErrNotPublished
	if !errors.As(err, &notPublished) {
		t.Fatalf("error = %v, want *ErrNotPublished", err)
	}
	if notPublished.NextChangeID != 1787126400 {
		t.Errorf("NextChangeID = %d, want 1787126400", notPublished.NextChangeID)
	}
}

func TestErrNotPublished_ErrorNamesTheNextChangeID(t *testing.T) {
	err := &ErrNotPublished{NextChangeID: 1787126400}

	want := "exchange: hour not published yet (next_change_id=1787126400)"
	if got := err.Error(); got != want {
		t.Errorf("Error() = %q, want %q", got, want)
	}
}

func TestFetchHour_notFoundThatIsNotTheFeedShape_returnsStatusError(t *testing.T) {
	tests := []struct {
		name     string
		body     string
		wantBody string
	}{
		{
			name:     "html error page",
			body:     "<html><body>404 Not Found</body></html>",
			wantBody: "404 Not Found",
		},
		{
			name:     "json 404 that still carries markets",
			body:     `{"next_change_id":1787126400,"markets":[{"league":"Allflame"}]}`,
			wantBody: "Allflame",
		},
		// Hour 0 is FetchHour's "oldest retained hour" sentinel, so a 404 body
		// without a positive cursor carries no usable retry target and must not
		// come back as ErrNotPublished.
		{
			name:     "empty json object carries no cursor",
			body:     `{}`,
			wantBody: `{}`,
		},
		{
			name:     "null markets with no cursor",
			body:     `{"markets":null}`,
			wantBody: `{"markets":null}`,
		},
		{
			name:     "explicit zero cursor",
			body:     `{"next_change_id":0,"markets":[]}`,
			wantBody: `{"next_change_id":0,"markets":[]}`,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			server := serveBytes(t, http.StatusNotFound, []byte(tt.body))

			_, err := testClient(t, server).FetchHour(context.Background(), 1787119200, RealmPC)
			if err == nil {
				t.Fatal("error = nil, want a status error")
			}

			var notPublished *ErrNotPublished
			if errors.As(err, &notPublished) {
				t.Fatalf("error = %v, want a plain status error, not *ErrNotPublished", err)
			}
			if !strings.Contains(err.Error(), "status 404") {
				t.Errorf("error = %q, want it to contain %q", err.Error(), "status 404")
			}
			if !strings.Contains(err.Error(), tt.wantBody) {
				t.Errorf("error = %q, want it to quote %q", err.Error(), tt.wantBody)
			}
		})
	}
}

func TestFetchHour_serverError_quotesStatusAndBody(t *testing.T) {
	server := serveBytes(t, http.StatusInternalServerError, []byte("boom"))

	_, err := testClient(t, server).FetchHour(context.Background(), 1787119200, RealmPC)
	if err == nil {
		t.Fatal("error = nil, want a status error")
	}
	if !strings.Contains(err.Error(), "status 500") {
		t.Errorf("error = %q, want it to contain %q", err.Error(), "status 500")
	}
	if !strings.Contains(err.Error(), "boom") {
		t.Errorf("error = %q, want it to quote the body %q", err.Error(), "boom")
	}
}

// The 404 branch reads up to 1 KiB so it can recognise the feed's
// not-published body, so it is the branch where the 512 byte quote cap in the
// rendered error actually has to do the trimming.
func TestFetchHour_quotesAtMost512BytesOfTheErrorBody(t *testing.T) {
	server := serveBytes(t, http.StatusNotFound, []byte(strings.Repeat("x", 900)+"TAIL"))

	_, err := testClient(t, server).FetchHour(context.Background(), 1787119200, RealmPC)
	if err == nil {
		t.Fatal("error = nil, want a status error")
	}

	const marker = "status 404: "
	idx := strings.Index(err.Error(), marker)
	if idx < 0 {
		t.Fatalf("error = %q, want it to contain %q", err.Error(), marker)
	}
	if quoted := err.Error()[idx+len(marker):]; len(quoted) != 512 {
		t.Errorf("error quotes %d body bytes, want 512", len(quoted))
	}
}

// A response whose Content-Length promises more than the server delivers fails
// mid-read, so the status error has to name the read failure alongside the
// partial body it did get.
func TestFetchHour_truncatedErrorBody_namesTheReadFailure(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		conn, _, err := w.(http.Hijacker).Hijack()
		if err != nil {
			t.Errorf("hijack: %v", err)
			return
		}
		defer conn.Close()
		io.WriteString(conn, "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 100\r\n\r\nboom")
	}))
	defer server.Close()

	_, err := testClient(t, server).FetchHour(context.Background(), 1787119200, RealmPC)
	if err == nil {
		t.Fatal("error = nil, want a status error")
	}
	if !strings.Contains(err.Error(), "status 500") {
		t.Errorf("error = %q, want it to contain %q", err.Error(), "status 500")
	}
	if !strings.Contains(err.Error(), "boom") {
		t.Errorf("error = %q, want it to quote the partial body %q", err.Error(), "boom")
	}
	if !errors.Is(err, io.ErrUnexpectedEOF) {
		t.Errorf("error = %v, want it to wrap the body read failure", err)
	}
}

func TestFetchHour_malformedSuccessBody_returnsDecodeError(t *testing.T) {
	server := serveBytes(t, http.StatusOK, []byte(`{"next_change_id":`))

	payload, err := testClient(t, server).FetchHour(context.Background(), 1787119200, RealmPC)
	if payload != nil {
		t.Errorf("payload = %+v, want nil", payload)
	}
	if err == nil {
		t.Fatal("error = nil, want a decode error")
	}
	if !strings.Contains(err.Error(), "decode hour 1787119200") {
		t.Errorf("error = %q, want it to name the decode action and hour", err.Error())
	}
}

func TestFetchHour_cancelledContext_wrapsContextCanceled(t *testing.T) {
	server := serveBytes(t, http.StatusOK, []byte(emptyHourBody))

	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	_, err := testClient(t, server).FetchHour(ctx, 1787119200, RealmPC)
	if err == nil {
		t.Fatal("error = nil, want context.Canceled")
	}
	if !errors.Is(err, context.Canceled) {
		t.Errorf("error = %v, want it to wrap context.Canceled", err)
	}
}
