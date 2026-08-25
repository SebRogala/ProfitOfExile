package handlers

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"sort"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/go-chi/chi/v5"

	"profitofexile/internal/device"
	"profitofexile/internal/mercenary"
	"profitofexile/internal/server/middleware"
)

// --- fixtures ---

const mercTestFingerprint = "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899"

// mercSignatureB64 is a valid wire signature: 576 bytes with enough variance to
// normalise. The handler tests care about transport, not about which art it is.
func mercSignatureB64(seed int) string {
	gray := make([]byte, mercenary.SigBytes)
	for i := range gray {
		gray[i] = byte((i*7 + seed*13) % 251)
	}
	return mercenary.EncodeSignature(gray)
}

type fakeMercStore struct {
	acceptResult mercenary.AcceptResult
	acceptErr    error
	corpus       mercenary.Corpus
	corpusErr    error
	tombstoned   int
	tombstoneErr error

	gotDeviceID   string
	gotVersion    int16
	gotCandidates []mercenary.Candidate
	gotKey        mercenary.Key
	acceptCalls   int
	corpusCalls   int
}

func (f *fakeMercStore) Accept(_ context.Context, deviceID string, version int16, candidates []mercenary.Candidate) (mercenary.AcceptResult, error) {
	f.acceptCalls++
	f.gotDeviceID = deviceID
	f.gotVersion = version
	f.gotCandidates = candidates
	return f.acceptResult, f.acceptErr
}

func (f *fakeMercStore) Corpus(_ context.Context, version int16) (mercenary.Corpus, error) {
	f.corpusCalls++
	f.gotVersion = version
	return f.corpus, f.corpusErr
}

func (f *fakeMercStore) Tombstone(_ context.Context, version int16, key mercenary.Key) (int, error) {
	f.gotVersion = version
	f.gotKey = key
	return f.tombstoned, f.tombstoneErr
}

// mercRouter mirrors the production wiring: device middleware in front, the
// three template routes behind it.
func mercRouter(store MercTemplateStore, limiter *mercenary.RateLimiter, cache *MercCorpusCache) http.Handler {
	upserter := &mockUpserter{
		UpsertFn: func(_ context.Context, fp, _ string) (*device.Device, error) {
			return &device.Device{Fingerprint: fp, Role: "user"}, nil
		},
	}
	r := chi.NewRouter()
	r.Use(middleware.DeviceMiddleware(upserter))
	r.Get("/api/desktop/merc-templates", MercTemplatesServe(store, cache))
	r.Post("/api/desktop/merc-templates", MercTemplatesUpload(store, limiter, cache))
	r.Post("/api/desktop/merc-templates/tombstone", MercTemplatesTombstone(store, limiter, cache))
	return r
}

func mercPost(t *testing.T, router http.Handler, path, body string, withDevice bool) *httptest.ResponseRecorder {
	t.Helper()
	req := httptest.NewRequest(http.MethodPost, path, strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	if withDevice {
		req.Header.Set("X-Device-ID", mercTestFingerprint)
	}
	w := httptest.NewRecorder()
	router.ServeHTTP(w, req)
	return w
}

func decodeCounts(t *testing.T, w *httptest.ResponseRecorder) map[string]int {
	t.Helper()
	var got map[string]int
	if err := json.NewDecoder(w.Body).Decode(&got); err != nil {
		t.Fatalf("decode response: %v (body %q)", err, w.Body.String())
	}
	return got
}

func validUploadBody(templates ...string) string {
	return fmt.Sprintf(`{"format_version":%d,"templates":[%s]}`,
		mercenary.SupportedFormatVersion, strings.Join(templates, ","))
}

func templateJSON(family string, tier int, sigB64 string) string {
	return fmt.Sprintf(`{"family":%q,"tier":%d,"signature_b64":%q}`, family, tier, sigB64)
}

// --- upload ---

// Publishing is open to every device, but it is not open to no device: the
// fingerprint is what attributes a bad sample and what the rate limit is
// counted against.
//
// Mutation check: removing the `dev == nil` guard makes this test fail — the
// handler then reaches the limiter with a nil device instead of answering 401.
func TestMercTemplatesUpload_WithoutDevice_Returns401(t *testing.T) {
	store := &fakeMercStore{}
	w := mercPost(t, mercRouter(store, nil, nil), "/api/desktop/merc-templates",
		validUploadBody(templateJSON("Chain", 1, mercSignatureB64(1))), false)

	if w.Code != http.StatusUnauthorized {
		t.Fatalf("status = %d, want 401; body: %s", w.Code, w.Body.String())
	}
	if store.acceptCalls != 0 {
		t.Errorf("store was called %d times for an unidentified upload", store.acceptCalls)
	}
}

// The uploader is taken from the authenticated context, never from the body, so
// a client cannot attribute its samples to another device.
func TestMercTemplatesUpload_AttributesToTheHeaderDeviceNotTheBody(t *testing.T) {
	store := &fakeMercStore{}
	body := fmt.Sprintf(`{"format_version":%d,"device_id":"someone-else","templates":[%s]}`,
		mercenary.SupportedFormatVersion, templateJSON("Chain", 1, mercSignatureB64(1)))

	w := mercPost(t, mercRouter(store, nil, nil), "/api/desktop/merc-templates", body, true)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200; body: %s", w.Code, w.Body.String())
	}
	if store.gotDeviceID != mercTestFingerprint {
		t.Errorf("stored device = %q, want the header fingerprint %q", store.gotDeviceID, mercTestFingerprint)
	}
}

// A foreign format version is refused outright rather than stored alongside the
// supported one: signatures from two formats are not comparable, so pooling
// them would make the dedupe threshold meaningless for both.
func TestMercTemplatesUpload_ForeignFormatVersion_Returns400(t *testing.T) {
	store := &fakeMercStore{}
	body := fmt.Sprintf(`{"format_version":%d,"templates":[%s]}`,
		mercenary.SupportedFormatVersion+1, templateJSON("Chain", 1, mercSignatureB64(1)))

	w := mercPost(t, mercRouter(store, nil, nil), "/api/desktop/merc-templates", body, true)

	if w.Code != http.StatusBadRequest {
		t.Fatalf("status = %d, want 400; body: %s", w.Code, w.Body.String())
	}
	if store.acceptCalls != 0 {
		t.Errorf("store was called %d times for a foreign format version", store.acceptCalls)
	}
}

// One unreadable template must not discard the good ones sent with it: it is
// counted and dropped, and the rest reach the pool.
func TestMercTemplatesUpload_UnreadableTemplate_IsRejectedWithoutLosingTheOthers(t *testing.T) {
	store := &fakeMercStore{acceptResult: mercenary.AcceptResult{Stored: 1}}
	body := validUploadBody(
		templateJSON("Chain", 1, "not base64!!"),
		templateJSON("Chain", 2, mercSignatureB64(2)),
	)

	w := mercPost(t, mercRouter(store, nil, nil), "/api/desktop/merc-templates", body, true)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200; body: %s", w.Code, w.Body.String())
	}
	if got := decodeCounts(t, w)["rejected"]; got != 1 {
		t.Errorf("rejected = %d, want 1", got)
	}
	if len(store.gotCandidates) != 1 {
		t.Fatalf("candidates reaching the pool = %d, want 1", len(store.gotCandidates))
	}
	if store.gotCandidates[0].Key.Tier != 2 {
		t.Errorf("surviving candidate tier = %d, want 2 (the readable one)", store.gotCandidates[0].Key.Tier)
	}
}

// A template whose signature is the wrong length is rejected the same way: 576
// bytes is the format, and a short one would change the correlation's divisor
// on every device that pulled it.
func TestMercTemplatesUpload_WrongLengthSignature_IsRejected(t *testing.T) {
	store := &fakeMercStore{}
	short := mercenary.EncodeSignature(make([]byte, mercenary.SigBytes-1))
	body := validUploadBody(templateJSON("Chain", 1, short))

	w := mercPost(t, mercRouter(store, nil, nil), "/api/desktop/merc-templates", body, true)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200; body: %s", w.Code, w.Body.String())
	}
	if got := decodeCounts(t, w)["rejected"]; got != 1 {
		t.Errorf("rejected = %d, want 1", got)
	}
	if store.acceptCalls != 0 {
		t.Errorf("store was called although no template survived validation")
	}
}

// An out-of-range tier never becomes a key. Tiers are the badge's I/II/III;
// anything else is a client bug, and storing it would create a key no hover can
// ever match.
func TestMercTemplatesUpload_OutOfRangeTier_IsRejected(t *testing.T) {
	store := &fakeMercStore{}
	body := validUploadBody(templateJSON("Chain", 4, mercSignatureB64(1)))

	w := mercPost(t, mercRouter(store, nil, nil), "/api/desktop/merc-templates", body, true)

	if got := decodeCounts(t, w)["rejected"]; got != 1 {
		t.Errorf("rejected = %d, want 1 for tier 4", got)
	}
	if store.acceptCalls != 0 {
		t.Errorf("store was called with an out-of-range tier")
	}
}

// The response is the client's instruction sheet: it has to be able to tell
// "we kept it" from "we already had it" from "that key is full" from "that key
// is retired", because it reacts differently to each.
func TestMercTemplatesUpload_ReportsEachOutcomeSeparately(t *testing.T) {
	store := &fakeMercStore{acceptResult: mercenary.AcceptResult{
		Stored: 2, Duplicate: 3, Capped: 4, Tombstoned: 5,
	}}
	body := validUploadBody(templateJSON("Chain", 1, mercSignatureB64(1)))

	w := mercPost(t, mercRouter(store, nil, nil), "/api/desktop/merc-templates", body, true)

	got := decodeCounts(t, w)
	want := map[string]int{"stored": 2, "duplicate": 3, "capped": 4, "tombstoned": 5,
		"rejected": 0, "rejected_unknown_family": 0}
	for key, wantValue := range want {
		if got[key] != wantValue {
			t.Errorf("%s = %d, want %d (full body: %v)", key, got[key], wantValue, got)
		}
	}
}

func TestMercTemplatesUpload_EmptyTemplateList_Returns400(t *testing.T) {
	store := &fakeMercStore{}
	body := fmt.Sprintf(`{"format_version":%d,"templates":[]}`, mercenary.SupportedFormatVersion)

	w := mercPost(t, mercRouter(store, nil, nil), "/api/desktop/merc-templates", body, true)

	if w.Code != http.StatusBadRequest {
		t.Fatalf("status = %d, want 400; body: %s", w.Code, w.Body.String())
	}
}

// The body cap bounds how much parsing one request can buy. Without it a device
// could hand the server an unbounded JSON array to decode before any of the
// pool's own limits apply.
//
// 413, not 400: a 400 tells the client its JSON is malformed when the JSON was
// fine and only the batch was too large, which gives it no reason to retry in
// smaller pieces.
func TestMercTemplatesUpload_OversizedBody_Returns413(t *testing.T) {
	store := &fakeMercStore{}
	templates := make([]string, 0, 64)
	for i := 0; i < 64; i++ {
		templates = append(templates, templateJSON("Chain", 1, mercSignatureB64(i)))
	}
	body := validUploadBody(templates...)
	if len(body) <= 32*1024 {
		t.Fatalf("test setup: body is %d bytes, expected it to exceed the 32 KB cap", len(body))
	}

	w := mercPost(t, mercRouter(store, nil, nil), "/api/desktop/merc-templates", body, true)

	if w.Code != http.StatusRequestEntityTooLarge {
		t.Fatalf("status = %d, want 413; body: %s", w.Code, w.Body.String())
	}
	if got := w.Body.String(); !strings.Contains(got, "send fewer templates") {
		t.Errorf("413 body = %s, want it to name the limit and the remedy", got)
	}
	if store.acceptCalls != 0 {
		t.Errorf("store was called for an oversized body")
	}
}

// The fingerprint is spoofable, so the rate limit is not a security boundary —
// it is what stops one identity from spending the server's time without bound
// while the cap and the tombstone deal with the pool's contents.
func TestMercTemplatesUpload_BeyondRateLimit_Returns429(t *testing.T) {
	store := &fakeMercStore{acceptResult: mercenary.AcceptResult{Stored: 1}}
	limiter := mercenary.NewRateLimiter(1, time.Minute, 8)
	router := mercRouter(store, limiter, nil)
	body := validUploadBody(templateJSON("Chain", 1, mercSignatureB64(1)))

	if w := mercPost(t, router, "/api/desktop/merc-templates", body, true); w.Code != http.StatusOK {
		t.Fatalf("first request status = %d, want 200; body: %s", w.Code, w.Body.String())
	}

	w := mercPost(t, router, "/api/desktop/merc-templates", body, true)
	if w.Code != http.StatusTooManyRequests {
		t.Fatalf("second request status = %d, want 429; body: %s", w.Code, w.Body.String())
	}
	if w.Header().Get("Retry-After") == "" {
		t.Error("429 carried no Retry-After header")
	}
	if store.acceptCalls != 1 {
		t.Errorf("store was called %d times, want 1 (the throttled request must not reach it)", store.acceptCalls)
	}
}

// Uploads and tombstones draw on one budget: a device that has spent its
// allowance spamming samples must not get free retirements with what is left.
func TestMercTemplatesTombstone_SharesTheUploadRateLimit(t *testing.T) {
	store := &fakeMercStore{acceptResult: mercenary.AcceptResult{Stored: 1}}
	limiter := mercenary.NewRateLimiter(1, time.Minute, 8)
	router := mercRouter(store, limiter, nil)

	mercPost(t, router, "/api/desktop/merc-templates",
		validUploadBody(templateJSON("Chain", 1, mercSignatureB64(1))), true)

	body := fmt.Sprintf(`{"format_version":%d,"family":"Chain","tier":1}`, mercenary.SupportedFormatVersion)
	w := mercPost(t, router, "/api/desktop/merc-templates/tombstone", body, true)

	if w.Code != http.StatusTooManyRequests {
		t.Fatalf("tombstone status = %d, want 429 after the budget was spent on uploads; body: %s",
			w.Code, w.Body.String())
	}
}

// --- serve ---

// The served corpus is art and keys. A device id in it would put uploader
// fingerprints in front of every other device, which the repository's durable
// rules forbid outright.
func TestMercTemplatesServe_CarriesNoDeviceIdentifiers(t *testing.T) {
	store := &fakeMercStore{corpus: mercenary.Corpus{
		FormatVersion: mercenary.SupportedFormatVersion,
		Templates: []mercenary.Sample{
			{Key: mercenary.Key{Family: "Chain", Tier: 1}, Signature: make([]byte, mercenary.SigBytes)},
		},
		Tombstones: []mercenary.Key{{Family: "Pierce", Tier: 2}},
	}}

	req := httptest.NewRequest(http.MethodGet, "/api/desktop/merc-templates?format_version=1", nil)
	w := httptest.NewRecorder()
	mercRouter(store, nil, nil).ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200; body: %s", w.Code, w.Body.String())
	}

	var envelope map[string]json.RawMessage
	if err := json.Unmarshal(w.Body.Bytes(), &envelope); err != nil {
		t.Fatalf("decode envelope: %v", err)
	}
	assertKeys(t, "envelope", envelope, []string{"dedupe_threshold", "format_version",
		"known_family_count", "templates", "tombstones"})

	var templates []map[string]json.RawMessage
	if err := json.Unmarshal(envelope["templates"], &templates); err != nil {
		t.Fatalf("decode templates: %v", err)
	}
	if len(templates) != 1 {
		t.Fatalf("templates = %d, want 1", len(templates))
	}
	assertKeys(t, "template", templates[0], []string{"family", "signature_b64", "tier"})

	var tombstones []map[string]json.RawMessage
	if err := json.Unmarshal(envelope["tombstones"], &tombstones); err != nil {
		t.Fatalf("decode tombstones: %v", err)
	}
	if len(tombstones) != 1 {
		t.Fatalf("tombstones = %d, want 1", len(tombstones))
	}
	assertKeys(t, "tombstone", tombstones[0], []string{"family", "tier"})
}

func assertKeys(t *testing.T, what string, object map[string]json.RawMessage, want []string) {
	t.Helper()
	got := make([]string, 0, len(object))
	for key := range object {
		got = append(got, key)
	}
	sort.Strings(got)
	if strings.Join(got, ",") != strings.Join(want, ",") {
		t.Errorf("%s fields = %v, want exactly %v", what, got, want)
	}
}

// The desktop pulls with reqwest, which has no HTTP cache, so this header does
// nothing for the caller it was written for — it is here for any browser or
// proxy in front of the server. Pinned because dropping it is invisible until
// something in front starts re-fetching a corpus that changes a few times a
// league.
func TestMercTemplatesServe_SetsAPublicCacheWindow(t *testing.T) {
	store := &fakeMercStore{corpus: mercenary.Corpus{FormatVersion: 1}}

	req := httptest.NewRequest(http.MethodGet, "/api/desktop/merc-templates", nil)
	w := httptest.NewRecorder()
	mercRouter(store, nil, nil).ServeHTTP(w, req)

	if got := w.Header().Get("Cache-Control"); got != "public, max-age=300" {
		t.Errorf("Cache-Control = %q, want %q", got, "public, max-age=300")
	}
}

// Reading the corpus needs no identity: it is the fail-soft path the desktop
// runs at module start, and requiring a device would only make that pull
// harder without protecting anything the upload path does not already.
func TestMercTemplatesServe_WithoutDevice_Returns200(t *testing.T) {
	store := &fakeMercStore{corpus: mercenary.Corpus{FormatVersion: 1}}

	req := httptest.NewRequest(http.MethodGet, "/api/desktop/merc-templates", nil)
	w := httptest.NewRecorder()
	mercRouter(store, nil, nil).ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200; body: %s", w.Code, w.Body.String())
	}
}

// An omitted version means "the version this server publishes". A client that
// does state one gets exactly that version, including an older one it is still
// running — which is what keeps a format bump from breaking clients that have
// not updated yet.
func TestMercTemplatesServe_ServesTheRequestedVersion(t *testing.T) {
	store := &fakeMercStore{corpus: mercenary.Corpus{FormatVersion: 1}}
	router := mercRouter(store, nil, nil)

	req := httptest.NewRequest(http.MethodGet, "/api/desktop/merc-templates", nil)
	router.ServeHTTP(httptest.NewRecorder(), req)
	if store.gotVersion != mercenary.SupportedFormatVersion {
		t.Errorf("version without a query param = %d, want %d",
			store.gotVersion, mercenary.SupportedFormatVersion)
	}

	req = httptest.NewRequest(http.MethodGet, "/api/desktop/merc-templates?format_version=7", nil)
	router.ServeHTTP(httptest.NewRecorder(), req)
	if store.gotVersion != 7 {
		t.Errorf("version for format_version=7 = %d, want 7", store.gotVersion)
	}
}

func TestMercTemplatesServe_UnparseableFormatVersion_Returns400(t *testing.T) {
	store := &fakeMercStore{}
	router := mercRouter(store, nil, nil)

	for _, raw := range []string{"abc", "0", "-3", "99999"} {
		req := httptest.NewRequest(http.MethodGet, "/api/desktop/merc-templates?format_version="+raw, nil)
		w := httptest.NewRecorder()
		router.ServeHTTP(w, req)
		if w.Code != http.StatusBadRequest {
			t.Errorf("format_version=%s status = %d, want 400", raw, w.Code)
		}
	}
	if store.corpusCalls != 0 {
		t.Errorf("store was queried %d times for unparseable versions", store.corpusCalls)
	}
}

// --- tombstone ---

func TestMercTemplatesTombstone_WithoutDevice_Returns401(t *testing.T) {
	store := &fakeMercStore{}
	body := fmt.Sprintf(`{"format_version":%d,"family":"Chain","tier":1}`, mercenary.SupportedFormatVersion)

	w := mercPost(t, mercRouter(store, nil, nil), "/api/desktop/merc-templates/tombstone", body, false)

	if w.Code != http.StatusUnauthorized {
		t.Fatalf("status = %d, want 401; body: %s", w.Code, w.Body.String())
	}
}

// The count is how the device knows the retirement actually removed something:
// zero means the pool never held that key, which is a different situation from
// "retired three samples".
func TestMercTemplatesTombstone_ReportsHowManySamplesWereRetired(t *testing.T) {
	store := &fakeMercStore{tombstoned: 3}
	body := fmt.Sprintf(`{"format_version":%d,"family":"Chain","tier":2}`, mercenary.SupportedFormatVersion)

	w := mercPost(t, mercRouter(store, nil, nil), "/api/desktop/merc-templates/tombstone", body, true)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200; body: %s", w.Code, w.Body.String())
	}
	if got := decodeCounts(t, w)["tombstoned"]; got != 3 {
		t.Errorf("tombstoned = %d, want 3", got)
	}
	wantKey := mercenary.Key{Family: "Chain", Tier: 2}
	if store.gotKey != wantKey {
		t.Errorf("retired key = %+v, want %+v", store.gotKey, wantKey)
	}
}

func TestMercTemplatesTombstone_InvalidKey_Returns400(t *testing.T) {
	store := &fakeMercStore{tombstoned: 1}
	body := fmt.Sprintf(`{"format_version":%d,"family":"","tier":9}`, mercenary.SupportedFormatVersion)

	w := mercPost(t, mercRouter(store, nil, nil), "/api/desktop/merc-templates/tombstone", body, true)

	if w.Code != http.StatusBadRequest {
		t.Fatalf("status = %d, want 400; body: %s", w.Code, w.Body.String())
	}
	if store.gotKey != (mercenary.Key{}) {
		t.Errorf("store was asked to retire %+v from an invalid request", store.gotKey)
	}
}

// Unlike upload, tombstone is NOT gated on the server's own format version: a
// client still running an older format must be able to retire art it published
// there, otherwise a format bump strands every bad sample in the old pool.
func TestMercTemplatesTombstone_AcceptsAVersionOtherThanTheServersOwn(t *testing.T) {
	store := &fakeMercStore{tombstoned: 1}
	other := mercenary.SupportedFormatVersion + 1
	body := fmt.Sprintf(`{"format_version":%d,"family":"Chain","tier":1}`, other)

	w := mercPost(t, mercRouter(store, nil, nil), "/api/desktop/merc-templates/tombstone", body, true)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200; body: %s", w.Code, w.Body.String())
	}
	if store.gotVersion != other {
		t.Errorf("retired version = %d, want %d", store.gotVersion, other)
	}
}

// --- vocabulary gate ---

// `family` is free text on the wire and the key space has to stay finite:
// without the vocabulary check one device can pool art under names no hover
// will ever match, one dead row per upload its rate limit allows.
func TestMercTemplatesUpload_FamilyOutsideTheVocabulary_IsRejected(t *testing.T) {
	store := &fakeMercStore{}
	body := validUploadBody(templateJSON("Definitely Not A Support", 1, mercSignatureB64(1)))

	w := mercPost(t, mercRouter(store, nil, nil), "/api/desktop/merc-templates", body, true)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200; body: %s", w.Code, w.Body.String())
	}
	counts := decodeCounts(t, w)
	if counts["rejected_unknown_family"] != 1 {
		t.Errorf("rejected_unknown_family = %d, want 1", counts["rejected_unknown_family"])
	}
	if counts["rejected"] != 0 {
		t.Errorf("rejected = %d, want 0 — an unknown family is not a malformed one", counts["rejected"])
	}
	if store.acceptCalls != 0 {
		t.Errorf("store was called with an invented family")
	}
}

// The two causes are reported apart because a client reacts to them
// differently: a malformed template is its bug, an unknown family usually means
// its vocabulary fixture is newer than the server's and the fix is a deploy.
func TestMercTemplatesUpload_CountsUnknownFamiliesApartFromMalformedTemplates(t *testing.T) {
	store := &fakeMercStore{acceptResult: mercenary.AcceptResult{Stored: 1}}
	body := validUploadBody(
		templateJSON("Definitely Not A Support", 1, mercSignatureB64(1)),
		templateJSON("Chain", 1, "not base64!!"),
		templateJSON("Pierce", 2, mercSignatureB64(2)),
	)

	w := mercPost(t, mercRouter(store, nil, nil), "/api/desktop/merc-templates", body, true)

	counts := decodeCounts(t, w)
	if counts["rejected_unknown_family"] != 1 {
		t.Errorf("rejected_unknown_family = %d, want 1", counts["rejected_unknown_family"])
	}
	if counts["rejected"] != 1 {
		t.Errorf("rejected = %d, want 1", counts["rejected"])
	}
	if len(store.gotCandidates) != 1 {
		t.Fatalf("candidates reaching the pool = %d, want 1", len(store.gotCandidates))
	}
	if store.gotCandidates[0].Key.Family != "Pierce" {
		t.Errorf("surviving candidate = %q, want Pierce", store.gotCandidates[0].Key.Family)
	}
}

// Retiring is NOT gated on the vocabulary, and the asymmetry with upload is the
// point: a key is orphaned exactly when its family leaves the fixture, so the
// key that most needs retiring is the one a vocabulary gate would refuse.
func TestMercTemplatesTombstone_FamilyOutsideTheVocabulary_IsAccepted(t *testing.T) {
	store := &fakeMercStore{tombstoned: 2}
	const renamedAway = "Formerly A Support"
	body := fmt.Sprintf(`{"format_version":%d,"family":%q,"tier":1}`,
		mercenary.SupportedFormatVersion, renamedAway)

	w := mercPost(t, mercRouter(store, nil, nil), "/api/desktop/merc-templates/tombstone", body, true)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200; body: %s", w.Code, w.Body.String())
	}
	if got := decodeCounts(t, w)["tombstoned"]; got != 2 {
		t.Errorf("tombstoned = %d, want 2", got)
	}
	if store.gotKey.Family != renamedAway {
		t.Errorf("retired family = %q, want %q", store.gotKey.Family, renamedAway)
	}
}

// The shape checks still apply to removal — dropping the vocabulary gate is not
// dropping validation.
func TestMercTemplatesTombstone_MalformedKey_StillReturns400(t *testing.T) {
	store := &fakeMercStore{tombstoned: 1}
	body := fmt.Sprintf(`{"format_version":%d,"family":"Chain","tier":9}`,
		mercenary.SupportedFormatVersion)

	w := mercPost(t, mercRouter(store, nil, nil), "/api/desktop/merc-templates/tombstone", body, true)

	if w.Code != http.StatusBadRequest {
		t.Fatalf("status = %d, want 400; body: %s", w.Code, w.Body.String())
	}
	if store.gotKey != (mercenary.Key{}) {
		t.Errorf("store was asked to retire the malformed key %+v", store.gotKey)
	}
}

// --- corpus cache and ETag ---

func mercGet(t *testing.T, router http.Handler, path, ifNoneMatch string) *httptest.ResponseRecorder {
	t.Helper()
	req := httptest.NewRequest(http.MethodGet, path, nil)
	if ifNoneMatch != "" {
		req.Header.Set("If-None-Match", ifNoneMatch)
	}
	w := httptest.NewRecorder()
	router.ServeHTTP(w, req)
	return w
}

func corpusOf(family string, tier int) mercenary.Corpus {
	return mercenary.Corpus{
		FormatVersion: mercenary.SupportedFormatVersion,
		Templates: []mercenary.Sample{
			{Key: mercenary.Key{Family: family, Tier: int16(tier)}, Signature: make([]byte, mercenary.SigBytes)},
		},
	}
}

// The serve path was a whole-table read plus a full re-encode per request, for
// a body that changes only when someone hovers art nobody has pooled yet.
func TestMercTemplatesServe_SecondRequestSkipsTheStore(t *testing.T) {
	store := &fakeMercStore{corpus: corpusOf("Chain", 1)}
	router := mercRouter(store, nil, NewMercCorpusCache(time.Minute))

	first := mercGet(t, router, "/api/desktop/merc-templates", "")
	second := mercGet(t, router, "/api/desktop/merc-templates", "")

	if store.corpusCalls != 1 {
		t.Fatalf("store was queried %d times for two identical GETs, want 1", store.corpusCalls)
	}
	if first.Body.String() != second.Body.String() {
		t.Errorf("cached body differs from the first response")
	}
	if first.Header().Get("ETag") != second.Header().Get("ETag") {
		t.Errorf("cached ETag differs from the first response")
	}
}

// An expired entry must be re-read, or a pool written by another process would
// never reach this one's clients.
func TestMercTemplatesServe_ExpiredCacheEntryIsRefetched(t *testing.T) {
	store := &fakeMercStore{corpus: corpusOf("Chain", 1)}
	cache := NewMercCorpusCache(time.Minute)
	at := time.Date(2026, 8, 25, 12, 0, 0, 0, time.UTC)
	cache.now = func() time.Time { return at }
	router := mercRouter(store, nil, cache)

	mercGet(t, router, "/api/desktop/merc-templates", "")
	at = at.Add(2 * time.Minute)
	mercGet(t, router, "/api/desktop/merc-templates", "")

	if store.corpusCalls != 2 {
		t.Fatalf("store was queried %d times across the TTL boundary, want 2", store.corpusCalls)
	}
}

// The desktop pulls the corpus at every module start. Once its copy is current
// the answer should be a header, not the whole pool.
func TestMercTemplatesServe_MatchingIfNoneMatch_Returns304WithNoBody(t *testing.T) {
	store := &fakeMercStore{corpus: corpusOf("Chain", 1)}
	router := mercRouter(store, nil, NewMercCorpusCache(time.Minute))

	first := mercGet(t, router, "/api/desktop/merc-templates", "")
	etag := first.Header().Get("ETag")
	if etag == "" {
		t.Fatal("the 200 carried no ETag")
	}

	second := mercGet(t, router, "/api/desktop/merc-templates", etag)

	if second.Code != http.StatusNotModified {
		t.Fatalf("status = %d, want 304; body: %s", second.Code, second.Body.String())
	}
	if second.Body.Len() != 0 {
		t.Errorf("304 carried %d bytes of body", second.Body.Len())
	}
	if second.Header().Get("ETag") != etag {
		t.Errorf("304 ETag = %q, want %q", second.Header().Get("ETag"), etag)
	}
}

// A stale validator must not win: the client is holding a different corpus and
// has to be given the current one.
func TestMercTemplatesServe_StaleIfNoneMatch_Returns200(t *testing.T) {
	store := &fakeMercStore{corpus: corpusOf("Chain", 1)}
	router := mercRouter(store, nil, NewMercCorpusCache(time.Minute))

	w := mercGet(t, router, "/api/desktop/merc-templates", `"an-older-corpus"`)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200", w.Code)
	}
	if w.Body.Len() == 0 {
		t.Error("200 carried no body")
	}
}

// Two corpora that differ must not share a validator, or a client holding the
// old one is told it is current forever.
func TestMercTemplatesServe_DifferentCorporaGetDifferentETags(t *testing.T) {
	one := &fakeMercStore{corpus: corpusOf("Chain", 1)}
	other := &fakeMercStore{corpus: corpusOf("Pierce", 2)}

	first := mercGet(t, mercRouter(one, nil, nil), "/api/desktop/merc-templates", "")
	second := mercGet(t, mercRouter(other, nil, nil), "/api/desktop/merc-templates", "")

	if first.Header().Get("ETag") == second.Header().Get("ETag") {
		t.Fatalf("two different corpora share the ETag %q", first.Header().Get("ETag"))
	}
}

// An upload that pooled something changes the corpus, so the cached body has to
// go — otherwise a new family stays invisible for the whole TTL.
func TestMercTemplatesServe_UploadThatStored_InvalidatesTheCache(t *testing.T) {
	store := &fakeMercStore{corpus: corpusOf("Chain", 1), acceptResult: mercenary.AcceptResult{Stored: 1}}
	router := mercRouter(store, nil, NewMercCorpusCache(time.Minute))

	mercGet(t, router, "/api/desktop/merc-templates", "")
	mercPost(t, router, "/api/desktop/merc-templates",
		validUploadBody(templateJSON("Chain", 1, mercSignatureB64(1))), true)
	mercGet(t, router, "/api/desktop/merc-templates", "")

	if store.corpusCalls != 2 {
		t.Fatalf("store was queried %d times around an upload that stored, want 2", store.corpusCalls)
	}
}

// An upload that stored nothing leaves the corpus byte-identical. Evicting on
// it would throw the cache away on every duplicate — which is most uploads,
// once the pool is warm.
func TestMercTemplatesServe_UploadThatStoredNothing_KeepsTheCache(t *testing.T) {
	store := &fakeMercStore{corpus: corpusOf("Chain", 1), acceptResult: mercenary.AcceptResult{Duplicate: 1}}
	router := mercRouter(store, nil, NewMercCorpusCache(time.Minute))

	mercGet(t, router, "/api/desktop/merc-templates", "")
	mercPost(t, router, "/api/desktop/merc-templates",
		validUploadBody(templateJSON("Chain", 1, mercSignatureB64(1))), true)
	mercGet(t, router, "/api/desktop/merc-templates", "")

	if store.corpusCalls != 1 {
		t.Fatalf("store was queried %d times around an upload that stored nothing, want 1", store.corpusCalls)
	}
}

// A retirement removes art from the corpus, so it must evict too.
func TestMercTemplatesServe_TombstoneThatRetired_InvalidatesTheCache(t *testing.T) {
	store := &fakeMercStore{corpus: corpusOf("Chain", 1), tombstoned: 2}
	router := mercRouter(store, nil, NewMercCorpusCache(time.Minute))

	mercGet(t, router, "/api/desktop/merc-templates", "")
	body := fmt.Sprintf(`{"format_version":%d,"family":"Chain","tier":1}`, mercenary.SupportedFormatVersion)
	mercPost(t, router, "/api/desktop/merc-templates/tombstone", body, true)
	mercGet(t, router, "/api/desktop/merc-templates", "")

	if store.corpusCalls != 2 {
		t.Fatalf("store was queried %d times around a tombstone that retired rows, want 2", store.corpusCalls)
	}
}

// The desktop's icon_match is overridable from its thresholds JSON and the
// server cannot see that a client moved it. Publishing the server's value is
// the only signal a client gets that the two disagree.
func TestMercTemplatesServe_PublishesTheServerDedupeThreshold(t *testing.T) {
	store := &fakeMercStore{corpus: corpusOf("Chain", 1)}

	w := mercGet(t, mercRouter(store, nil, nil), "/api/desktop/merc-templates", "")

	var envelope struct {
		DedupeThreshold float32 `json:"dedupe_threshold"`
	}
	if err := json.Unmarshal(w.Body.Bytes(), &envelope); err != nil {
		t.Fatalf("decode envelope: %v", err)
	}
	if envelope.DedupeThreshold != mercenary.DedupeThreshold {
		t.Fatalf("dedupe_threshold = %v, want %v", envelope.DedupeThreshold, mercenary.DedupeThreshold)
	}
}

// --- store failures ---

// A database error is the server's problem to describe, not the client's to
// read: an error string can carry a query, a column list, or a connection
// target, and this endpoint answers unauthenticated readers and every device.
func TestMercTemplatesServe_StoreFailure_Returns500WithoutTheErrorText(t *testing.T) {
	store := &fakeMercStore{corpusErr: errors.New("pgx: connection to 10.0.0.5 refused")}

	w := mercGet(t, mercRouter(store, nil, nil), "/api/desktop/merc-templates", "")

	if w.Code != http.StatusInternalServerError {
		t.Fatalf("status = %d, want 500; body: %s", w.Code, w.Body.String())
	}
	if strings.Contains(w.Body.String(), "10.0.0.5") {
		t.Errorf("500 body leaked the store's error: %s", w.Body.String())
	}
}

func TestMercTemplatesUpload_StoreFailure_Returns500WithoutTheErrorText(t *testing.T) {
	store := &fakeMercStore{acceptErr: errors.New("pgx: relation merc_icon_templates does not exist")}
	body := validUploadBody(templateJSON("Chain", 1, mercSignatureB64(1)))

	w := mercPost(t, mercRouter(store, nil, nil), "/api/desktop/merc-templates", body, true)

	if w.Code != http.StatusInternalServerError {
		t.Fatalf("status = %d, want 500; body: %s", w.Code, w.Body.String())
	}
	if strings.Contains(w.Body.String(), "merc_icon_templates") {
		t.Errorf("500 body leaked the store's error: %s", w.Body.String())
	}
}

func TestMercTemplatesTombstone_StoreFailure_Returns500WithoutTheErrorText(t *testing.T) {
	store := &fakeMercStore{tombstoneErr: errors.New("pgx: deadlock detected on merc_icon_templates")}
	body := fmt.Sprintf(`{"format_version":%d,"family":"Chain","tier":1}`, mercenary.SupportedFormatVersion)

	w := mercPost(t, mercRouter(store, nil, nil), "/api/desktop/merc-templates/tombstone", body, true)

	if w.Code != http.StatusInternalServerError {
		t.Fatalf("status = %d, want 500; body: %s", w.Code, w.Body.String())
	}
	if strings.Contains(w.Body.String(), "deadlock") {
		t.Errorf("500 body leaked the store's error: %s", w.Body.String())
	}
}

// A failed read must not be cached: the next request has to try the store
// again, not serve an error-shaped body for the whole TTL.
func TestMercTemplatesServe_StoreFailureIsNotCached(t *testing.T) {
	store := &fakeMercStore{corpusErr: errors.New("transient")}
	router := mercRouter(store, nil, NewMercCorpusCache(time.Minute))

	mercGet(t, router, "/api/desktop/merc-templates", "")
	mercGet(t, router, "/api/desktop/merc-templates", "")

	if store.corpusCalls != 2 {
		t.Fatalf("store was queried %d times after a failure, want 2", store.corpusCalls)
	}
}

// The vocabulary size is published so a desktop running a newer fixture than
// the server can see that it is ahead, rather than inferring it from refusals.
func TestMercTemplatesServe_PublishesTheKnownFamilyCount(t *testing.T) {
	store := &fakeMercStore{corpus: corpusOf("Chain", 1)}

	w := mercGet(t, mercRouter(store, nil, nil), "/api/desktop/merc-templates", "")

	var envelope struct {
		KnownFamilyCount int `json:"known_family_count"`
	}
	if err := json.Unmarshal(w.Body.Bytes(), &envelope); err != nil {
		t.Fatalf("decode envelope: %v", err)
	}
	if envelope.KnownFamilyCount != mercenary.KnownFamilyCount() {
		t.Fatalf("known_family_count = %d, want %d",
			envelope.KnownFamilyCount, mercenary.KnownFamilyCount())
	}
	if envelope.KnownFamilyCount == 0 {
		t.Fatal("known_family_count is 0; the vocabulary is empty")
	}
}

// A stale entry is a miss for readers but stayed in the map forever, so a
// caller asking for sixteen versions that hold nothing could permanently lock
// the one version anybody reads out of the cache. Storing must sweep first.
func TestMercCorpusCache_ExpiredEntriesDoNotHoldTheBound(t *testing.T) {
	store := &fakeMercStore{corpus: corpusOf("Chain", 1)}
	cache := NewMercCorpusCache(time.Minute)
	at := time.Date(2026, 8, 25, 12, 0, 0, 0, time.UTC)
	cache.now = func() time.Time { return at }
	router := mercRouter(store, nil, cache)

	for version := 100; version < 100+maxMercCorpusVersions; version++ {
		mercGet(t, router, "/api/desktop/merc-templates?format_version="+strconv.Itoa(version), "")
	}
	if got := store.corpusCalls; got != maxMercCorpusVersions {
		t.Fatalf("test setup: %d store calls filling the cache, want %d", got, maxMercCorpusVersions)
	}

	at = at.Add(2 * time.Minute) // every filler entry is now stale

	mercGet(t, router, "/api/desktop/merc-templates", "")
	before := store.corpusCalls
	mercGet(t, router, "/api/desktop/merc-templates", "")

	if store.corpusCalls != before {
		t.Fatalf("the real version was re-queried (%d then %d store calls): it never got cached",
			before, store.corpusCalls)
	}
}

// Even with every slot live, a newcomer is cached — one live entry is evicted
// instead. Refusing would let sixteen junk versions lock out the real one for
// the life of the process, which is the same failure with a longer fuse.
func TestMercCorpusCache_FullOfLiveEntries_EvictsRatherThanRefuses(t *testing.T) {
	store := &fakeMercStore{corpus: corpusOf("Chain", 1)}
	cache := NewMercCorpusCache(time.Minute)
	at := time.Date(2026, 8, 25, 12, 0, 0, 0, time.UTC)
	cache.now = func() time.Time { return at }
	router := mercRouter(store, nil, cache)

	for version := 100; version < 100+maxMercCorpusVersions; version++ {
		mercGet(t, router, "/api/desktop/merc-templates?format_version="+strconv.Itoa(version), "")
		at = at.Add(time.Second) // staggered expiries, so "nearest" is well defined
	}

	mercGet(t, router, "/api/desktop/merc-templates", "")
	before := store.corpusCalls
	mercGet(t, router, "/api/desktop/merc-templates", "")

	if store.corpusCalls != before {
		t.Fatalf("the newcomer was not cached (%d then %d store calls)", before, store.corpusCalls)
	}
	if len(cache.entries) > maxMercCorpusVersions {
		t.Errorf("cache holds %d entries, want at most %d", len(cache.entries), maxMercCorpusVersions)
	}
}
