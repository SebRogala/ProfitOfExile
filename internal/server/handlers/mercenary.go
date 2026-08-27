package handlers

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"log/slog"
	"net/http"
	"strconv"
	"strings"
	"sync"
	"time"

	"profitofexile/internal/mercenary"
	"profitofexile/internal/server/middleware"
)

// mercTemplateBodyLimit caps an upload body, so a device publishing a full
// local store batches instead of sending one giant body.
//
// 128 KB, raised from 32 KB when the signature became format 2 (POE-207). A
// format-2 signature is 1728 bytes, which is 2304 base64 characters, and with
// the family, tier and JSON punctuation around it one wire template runs to
// ~2.4 KB. The desktop's batch is MAX_TEMPLATES_PER_BATCH = 32, so a full batch
// is 32 x ~2.4 KB = ~76 KB. The cap sits above that with room for the longest
// family names rather than exactly on it: a batch that just tipped over would
// answer 413 to a client that had already obeyed its own batch size, and the
// client has no way to see which of the two limits it hit.
const mercTemplateBodyLimit = 128 * 1024

// MercTemplateStore is the slice of the template pool the HTTP layer needs.
// Declared here as an interface rather than taking *mercenary.Repository so the
// transport rules — device identity, body cap, rate limit, response shape — can
// be tested without a database.
type MercTemplateStore interface {
	Accept(ctx context.Context, deviceID string, version int16, candidates []mercenary.Candidate) (mercenary.AcceptResult, error)
	Corpus(ctx context.Context, version int16) (mercenary.Corpus, error)
	Tombstone(ctx context.Context, version int16, key mercenary.Key) (int, error)
}

// mercTemplateItem is one template on the wire, in both directions. The
// signature is base64 of exactly mercenary.SigBytes bytes — the 24x24 RGB disc
// samples, badge corner and frame already zeroed. Nothing else about the
// hovered cell travels: no raw GGG crop, and on the way out no device id.
type mercTemplateItem struct {
	Family       string `json:"family"`
	Tier         int    `json:"tier"`
	SignatureB64 string `json:"signature_b64"`
}

type mercKeyItem struct {
	Family string `json:"family"`
	Tier   int    `json:"tier"`
}

type mercUploadRequest struct {
	FormatVersion int                `json:"format_version"`
	Templates     []mercTemplateItem `json:"templates"`
}

// mercUploadAck is what an upload answers with: what the pool did with the
// batch, by outcome, plus per-sample detail for the one outcome a device cannot
// act on from a count alone.
//
// A struct rather than the map[string]int it used to be, because the payload
// stopped being homogeneous when `conflicts` arrived — and because these key
// names are a wire contract the desktop parses, so they belong somewhere a
// reader can see all of them at once.
//
// Every counter keeps the name it had. The desktop's UploadAck fields are all
// #[serde(default)] with no deny_unknown_fields, so an old build ignores the two
// new keys and a new build tolerates a server that has not shipped them yet.
type mercUploadAck struct {
	Stored     int `json:"stored"`
	Duplicate  int `json:"duplicate"`
	Capped     int `json:"capped"`
	Tombstoned int `json:"tombstoned"`
	// Conflicting counts candidates refused because a LIVE sample of another
	// family already carries that art. Apart from `duplicate` because the two
	// ask opposite things of the device: a duplicate means the pool already
	// serves this sample, a conflict means it never will until somebody retires
	// the incumbent.
	Conflicting           int                `json:"conflicting"`
	Rejected              int                `json:"rejected"`
	RejectedUnknownFamily int                `json:"rejected_unknown_family"`
	Conflicts             []mercConflictItem `json:"conflicts"`
}

// mercConflictItem names one refused template and the family that already owns
// its art, so the log line the player reads says what to forget.
//
// Index is the position in the REQUEST's `templates` array, not in the batch
// the pool decided on — see the decode loop for why the two diverge.
type mercConflictItem struct {
	Index           int    `json:"index"`
	Family          string `json:"family"`
	Tier            int    `json:"tier"`
	IncumbentFamily string `json:"incumbent_family"`
}

type mercTombstoneRequest struct {
	FormatVersion int    `json:"format_version"`
	Family        string `json:"family"`
	Tier          int    `json:"tier"`
}

// mercCorpusResponse is the served corpus. A struct rather than a map so the
// field set is visible at a glance — this is the payload that must never grow a
// device identifier.
type mercCorpusResponse struct {
	FormatVersion int `json:"format_version"`
	// DedupeThreshold is the correlation the server calls "the same art". The
	// desktop's own icon_match is overridable from its thresholds JSON and the
	// server cannot see that a client moved it, so publishing this value is the
	// only way a client can notice the two disagree.
	DedupeThreshold float32 `json:"dedupe_threshold"`
	// KnownFamilyCount is the size of the vocabulary this server validates
	// uploads against. A client whose fixture names more families than this is
	// running ahead of the server and its extra families will be refused — see
	// the deploy-order note on rejected_unknown_family.
	KnownFamilyCount int                `json:"known_family_count"`
	Templates        []mercTemplateItem `json:"templates"`
	Tombstones       []mercKeyItem      `json:"tombstones"`
}

// DefaultMercCorpusTTL backstops the corpus cache. Invalidation on write is the
// primary mechanism and is exact within one process; the TTL is what bounds
// staleness if the pool is ever written by a second process, whose stores this
// one's handlers never see.
const DefaultMercCorpusTTL = 5 * time.Minute

// maxMercCorpusVersions bounds the cache. Real deployments hold one or two
// versions, but the version is a client-supplied query parameter, so without a
// bound a caller could mint 32767 entries by asking for each in turn.
const maxMercCorpusVersions = 16

// MercCorpusCache memoizes the rendered corpus per format version.
//
// The serve path was a whole-table read plus a full re-encode on every request,
// for a body that changes only when somebody hovers art nobody has pooled yet.
// Caching the ENCODED body rather than the rows is what makes the ETag cheap:
// the hash is taken once, over exactly the bytes that will be written.
type MercCorpusCache struct {
	ttl time.Duration
	now func() time.Time

	mu      sync.Mutex
	entries map[int16]*mercCorpusEntry
}

type mercCorpusEntry struct {
	body    []byte
	etag    string
	expires time.Time
}

// NewMercCorpusCache builds a corpus cache with the given staleness backstop.
func NewMercCorpusCache(ttl time.Duration) *MercCorpusCache {
	return &MercCorpusCache{ttl: ttl, now: time.Now, entries: make(map[int16]*mercCorpusEntry)}
}

// lookup returns a live entry, or nil. A nil cache always misses, which is what
// keeps the handlers usable without one.
func (c *MercCorpusCache) lookup(version int16) *mercCorpusEntry {
	if c == nil {
		return nil
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	entry, ok := c.entries[version]
	if !ok || c.now().After(entry.expires) {
		return nil
	}
	return entry
}

func (c *MercCorpusCache) store(version int16, entry *mercCorpusEntry) {
	if c == nil {
		return
	}
	now := c.now()

	c.mu.Lock()
	defer c.mu.Unlock()

	entry.expires = now.Add(c.ttl)
	if _, replacing := c.entries[version]; !replacing && len(c.entries) >= maxMercCorpusVersions {
		// Nothing here ever expired on its own — lookup treats a stale entry as
		// a miss but leaves it in the map — so a caller could fill all sixteen
		// slots with versions that hold nothing, and the one version anybody
		// reads would then be uncacheable for the life of the process. Sweeping
		// first is what makes the bound a bound on LIVE entries.
		c.sweepExpiredLocked(now)
		if len(c.entries) >= maxMercCorpusVersions {
			// Still full of live entries. Evict the one closest to expiring
			// rather than refuse: refusing would let sixteen junk versions
			// permanently lock out the real one, which is the same failure with
			// a longer fuse.
			c.evictNearestExpiryLocked()
		}
	}
	c.entries[version] = entry
}

// sweepExpiredLocked drops entries past their TTL.
func (c *MercCorpusCache) sweepExpiredLocked(now time.Time) {
	for version, entry := range c.entries {
		if now.After(entry.expires) {
			delete(c.entries, version)
		}
	}
}

// evictNearestExpiryLocked removes the entry with the earliest expiry — the one
// whose loss costs the least remaining cache life.
func (c *MercCorpusCache) evictNearestExpiryLocked() {
	var (
		victim  int16
		soonest time.Time
		found   bool
	)
	for version, entry := range c.entries {
		if !found || entry.expires.Before(soonest) {
			victim, soonest, found = version, entry.expires, true
		}
	}
	if found {
		delete(c.entries, victim)
	}
}

// Invalidate drops a version's cached body. Called by the write paths when they
// actually changed something — a request that stored nothing leaves the corpus
// byte-identical, so evicting on it would throw the cache away on every
// duplicate upload.
func (c *MercCorpusCache) Invalidate(version int16) {
	if c == nil {
		return
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	delete(c.entries, version)
}

// newMercCorpusEntry hashes the body into a strong ETag.
func newMercCorpusEntry(body []byte) *mercCorpusEntry {
	sum := sha256.Sum256(body)
	return &mercCorpusEntry{body: body, etag: `"` + hex.EncodeToString(sum[:]) + `"`}
}

// MercTemplatesUpload handles POST /api/desktop/merc-templates: a device offers
// signatures it has learned, and the pool answers what it did with each.
//
// Every device may publish. The defences are the ones a spoofable fingerprint
// can still support: a body cap, a per-device rate limit, a hard cap of three
// live samples per key, and tombstones as the cleanup path. A role gate was
// considered and rejected for v1 — it would have meant only one person ever
// feeds the pool, which is the problem this endpoint exists to solve.
func MercTemplatesUpload(store MercTemplateStore, limiter *mercenary.RateLimiter, cache *MercCorpusCache) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		// Identity and budget are checked before the body is read: a client
		// that may not write should not get 128 KB of parsing done on its
		// behalf.
		dev := middleware.DeviceFromContext(r.Context())
		if dev == nil {
			jsonError(w, http.StatusUnauthorized, "device identification required")
			return
		}
		if !allowMercWrite(w, limiter, dev.Fingerprint) {
			return
		}

		var body mercUploadRequest
		if !decodeMercBody(w, r, &body, "body exceeds 128 KB — send fewer templates") {
			return
		}

		if body.FormatVersion != int(mercenary.SupportedFormatVersion) {
			jsonError(w, http.StatusBadRequest,
				"unsupported format_version (server supports "+
					strconv.Itoa(int(mercenary.SupportedFormatVersion))+")")
			return
		}
		if len(body.Templates) == 0 {
			jsonError(w, http.StatusBadRequest, "templates must not be empty")
			return
		}
		if len(body.Templates) > mercenary.MaxTemplatesPerUpload {
			jsonError(w, http.StatusBadRequest, "too many templates (max "+
				strconv.Itoa(mercenary.MaxTemplatesPerUpload)+")")
			return
		}

		// A template the server cannot read is counted and dropped, never
		// fatal: one malformed entry must not discard the good ones sent with
		// it, and the count tells the client something is wrong on its side.
		candidates := make([]mercenary.Candidate, 0, len(body.Templates))
		// candidateIndex[i] is the REQUEST position of candidate i. Every
		// `continue` below shifts the two apart, and the conflict report is
		// per-sample: a device settles the offer at `templates[index]`, so
		// handing it a candidate index would make it settle the wrong template —
		// or, after enough drops, an index its batch does not carry at all.
		candidateIndex := make([]int, 0, len(body.Templates))
		rejected, unknownFamily := 0, 0
		for requestIndex, item := range body.Templates {
			key, err := mercenary.NewKey(item.Family, item.Tier)
			if err != nil {
				// An unknown family is counted apart from a malformed one
				// because the two have different causes and different fixes. A
				// malformed key is the client's bug. An unknown family almost
				// always means the client's vocabulary fixture is NEWER than
				// this server's — a league added support links, the desktop
				// build shipped first — and the fix is to deploy the server,
				// not to change either side's code.
				//
				// DEPLOY ORDER: the server must ship no later than a fixture
				// change. Shipping the desktop first is not an outage, but
				// every new family it learns is refused until the server
				// catches up, and without this counter that is invisible.
				if errors.Is(err, mercenary.ErrUnknownFamily) {
					slog.Warn("merc templates: unknown family",
						"device", dev.Fingerprint, "family", item.Family,
						"server_families", mercenary.KnownFamilyCount())
					unknownFamily++
					continue
				}
				slog.Warn("merc templates: rejected key", "device", dev.Fingerprint, "error", err)
				rejected++
				continue
			}
			sig, err := mercenary.DecodeSignature(item.SignatureB64)
			if err != nil {
				slog.Warn("merc templates: rejected signature", "device", dev.Fingerprint,
					"key", key.String(), "error", err)
				rejected++
				continue
			}
			candidates = append(candidates, mercenary.Candidate{Key: key, Signature: sig})
			candidateIndex = append(candidateIndex, requestIndex)
		}

		result := mercenary.AcceptResult{}
		if len(candidates) > 0 {
			var err error
			result, err = store.Accept(r.Context(), dev.Fingerprint,
				mercenary.SupportedFormatVersion, candidates)
			if err != nil {
				slog.Error("merc templates: accept failed", "device", dev.Fingerprint, "error", err)
				jsonError(w, http.StatusInternalServerError, "failed to store templates")
				return
			}
		}

		// Still `Stored > 0`: a conflicting candidate is refused, so the corpus
		// is byte-identical afterwards and evicting on it would throw the cache
		// away for nothing.
		if result.Stored > 0 {
			cache.Invalidate(mercenary.SupportedFormatVersion)
		}

		slog.Info("merc templates: upload",
			"device", dev.Fingerprint,
			"offered", len(body.Templates),
			"stored", result.Stored,
			"duplicate", result.Duplicate,
			"capped", result.Capped,
			"tombstoned", result.Tombstoned,
			"conflicting", result.Conflicting,
			"rejected", rejected,
			"rejected_unknown_family", unknownFamily,
		)

		writeMercJSON(w, mercUploadAck{
			Stored:                result.Stored,
			Duplicate:             result.Duplicate,
			Capped:                result.Capped,
			Tombstoned:            result.Tombstoned,
			Conflicting:           result.Conflicting,
			Rejected:              rejected,
			RejectedUnknownFamily: unknownFamily,
			Conflicts:             mercConflictItems(dev.Fingerprint, result.Conflicts, candidateIndex),
		})
	}
}

// mercConflictItems renders the pool's conflict detail for the wire, mapping
// each entry's CANDIDATE index back to the index of the template the client
// actually sent.
//
// A never-nil slice, so the field marshals as `[]` and not as `null`: the
// desktop parses this into a Vec, and serde's `default` covers a MISSING field,
// not an explicit null.
//
// An index outside the batch can only be a bug in this server's own bookkeeping
// — Accept indexes the slice it was handed — so it is logged and sent as -1
// rather than silently mapped onto some other template. The client
// bounds-checks and ignores it; the count still says a sample was refused.
func mercConflictItems(fingerprint string, conflicts []mercenary.Conflict, candidateIndex []int) []mercConflictItem {
	items := make([]mercConflictItem, 0, len(conflicts))
	for _, conflict := range conflicts {
		requestIndex := -1
		if conflict.Index >= 0 && conflict.Index < len(candidateIndex) {
			requestIndex = candidateIndex[conflict.Index]
		} else {
			slog.Error("merc templates: conflict index outside the batch",
				"device", fingerprint, "candidate_index", conflict.Index,
				"candidates", len(candidateIndex))
		}
		items = append(items, mercConflictItem{
			Index:           requestIndex,
			Family:          conflict.Key.Family,
			Tier:            int(conflict.Key.Tier),
			IncumbentFamily: conflict.IncumbentFamily,
		})
	}
	return items
}

// MercTemplatesServe handles GET /api/desktop/merc-templates?format_version=N.
//
// Public, like the shared lab layouts: the corpus is art the pool already
// decided to serve, and requiring identity to read it would only make the
// desktop's fail-soft pull harder without protecting anything.
//
// The version is NOT checked against the server's supported one. The table can
// only ever hold rows the upload path accepted, so an unknown version answers
// with an empty corpus by itself — and after a format bump that is exactly what
// a not-yet-updated client asking for the older version should still be able to
// read.
func MercTemplatesServe(store MercTemplateStore, cache *MercCorpusCache) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		version := int(mercenary.SupportedFormatVersion)
		if raw := r.URL.Query().Get("format_version"); raw != "" {
			parsed, err := strconv.Atoi(raw)
			if err != nil || parsed < 1 || parsed > 32767 {
				jsonError(w, http.StatusBadRequest, "invalid format_version")
				return
			}
			version = parsed
		}

		if entry := cache.lookup(int16(version)); entry != nil {
			writeMercCorpus(w, r, entry)
			return
		}

		corpus, err := store.Corpus(r.Context(), int16(version))
		if err != nil {
			slog.Error("merc templates: corpus query failed", "version", version, "error", err)
			jsonError(w, http.StatusInternalServerError, "failed to read templates")
			return
		}

		payload := mercCorpusResponse{
			FormatVersion:    version,
			DedupeThreshold:  mercenary.DedupeThreshold,
			KnownFamilyCount: mercenary.KnownFamilyCount(),
			Templates:        make([]mercTemplateItem, 0, len(corpus.Templates)),
			Tombstones:       make([]mercKeyItem, 0, len(corpus.Tombstones)),
		}
		for _, sample := range corpus.Templates {
			payload.Templates = append(payload.Templates, mercTemplateItem{
				Family:       sample.Key.Family,
				Tier:         int(sample.Key.Tier),
				SignatureB64: mercenary.EncodeSignature(sample.Signature),
			})
		}
		for _, key := range corpus.Tombstones {
			payload.Tombstones = append(payload.Tombstones, mercKeyItem{Family: key.Family, Tier: int(key.Tier)})
		}

		body, err := json.Marshal(payload)
		if err != nil {
			slog.Error("merc templates: encode corpus", "version", version, "error", err)
			jsonError(w, http.StatusInternalServerError, "failed to read templates")
			return
		}

		entry := newMercCorpusEntry(body)
		cache.store(int16(version), entry)
		writeMercCorpus(w, r, entry)
	}
}

// writeMercCorpus answers with the cached body, or with 304 when the client
// already holds it.
//
// The ETag is the whole point of the conditional: the desktop pulls the corpus
// at every module start, and once its copy is current the answer is 33 bytes of
// header instead of the entire pool. Strong, because the body is byte-exact —
// it is the hash of what would have been written, not a weak validator over
// equivalent renderings.
func writeMercCorpus(w http.ResponseWriter, r *http.Request, entry *mercCorpusEntry) {
	w.Header().Set("ETag", entry.etag)
	// The corpus changes only when someone hovers art nobody has pooled yet, so
	// five minutes stale is a few missing families, never wrong art. Inert for
	// the desktop, which fetches with reqwest and has no HTTP cache; kept for
	// any browser or proxy in front of this.
	w.Header().Set("Cache-Control", "public, max-age=300")

	if etagMatches(r.Header.Get("If-None-Match"), entry.etag) {
		w.WriteHeader(http.StatusNotModified)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	if _, err := w.Write(entry.body); err != nil {
		slog.Error("merc templates: write corpus", "error", err)
	}
}

// etagMatches implements the If-None-Match comparison: a comma-separated list,
// "*" matching anything, and the weak marker ignored on the client's side (the
// server only ever issues strong tags).
func etagMatches(header, etag string) bool {
	if header == "" {
		return false
	}
	for _, candidate := range strings.Split(header, ",") {
		candidate = strings.TrimSpace(candidate)
		if candidate == "*" {
			return true
		}
		if strings.TrimPrefix(candidate, "W/") == etag {
			return true
		}
	}
	return false
}

// MercTemplatesTombstone handles POST /api/desktop/merc-templates/tombstone: a
// device that forgot bad art locally makes that removal stick for everyone.
//
// Without this a local forget is undone by the next pull. The retired samples
// are kept and matched against later uploads, which is what prevents the device
// that published the bad art from simply sending it again before it has synced.
// The key itself stays open: better art for the same family and tier is
// accepted normally, so a key orphaned by a rename is retired and relearned
// rather than lost for the whole format version.
//
// Any positive version is accepted, not just the server's current one: a client
// still on an older format must be able to retire art it published there.
func MercTemplatesTombstone(store MercTemplateStore, limiter *mercenary.RateLimiter, cache *MercCorpusCache) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		dev := middleware.DeviceFromContext(r.Context())
		if dev == nil {
			jsonError(w, http.StatusUnauthorized, "device identification required")
			return
		}
		if !allowMercWrite(w, limiter, dev.Fingerprint) {
			return
		}

		var body mercTombstoneRequest
		if !decodeMercBody(w, r, &body, "body exceeds 128 KB") {
			return
		}
		if body.FormatVersion < 1 || body.FormatVersion > 32767 {
			jsonError(w, http.StatusBadRequest, "invalid format_version")
			return
		}
		// ParseKey, not NewKey: a key is orphaned exactly when its family
		// leaves the vocabulary, and that orphan is what a tombstone is for.
		// Gating removal on the closed set would make the one key that most
		// needs retiring the one key that cannot be.
		key, err := mercenary.ParseKey(body.Family, body.Tier)
		if err != nil {
			jsonError(w, http.StatusBadRequest, err.Error())
			return
		}

		marked, err := store.Tombstone(r.Context(), int16(body.FormatVersion), key)
		if err != nil {
			slog.Error("merc templates: tombstone failed", "device", dev.Fingerprint,
				"key", key.String(), "error", err)
			jsonError(w, http.StatusInternalServerError, "failed to tombstone key")
			return
		}

		if marked > 0 {
			cache.Invalidate(int16(body.FormatVersion))
		}

		slog.Info("merc templates: tombstone", "device", dev.Fingerprint,
			"key", key.String(), "version", body.FormatVersion, "tombstoned", marked)

		writeMercJSON(w, map[string]int{"tombstoned": marked})
	}
}

// allowMercWrite spends one write token, answering 429 when the device is out.
// A nil limiter means unlimited, which keeps tests and any future
// limiter-less wiring honest rather than silently unbounded.
func allowMercWrite(w http.ResponseWriter, limiter *mercenary.RateLimiter, fingerprint string) bool {
	if limiter == nil {
		return true
	}
	ok, retryAfter := limiter.Allow(fingerprint)
	if ok {
		return true
	}
	seconds := int(retryAfter / time.Second)
	if seconds < 1 {
		seconds = 1
	}
	w.Header().Set("Retry-After", strconv.Itoa(seconds))
	jsonError(w, http.StatusTooManyRequests, "upload rate limit exceeded")
	return false
}

// writeMercJSON writes a 200 JSON body, logging an encode failure rather than
// swallowing it. Named for its subsystem because the log line names one and
// this package is shared by every handler in the server.
func writeMercJSON(w http.ResponseWriter, payload any) {
	w.Header().Set("Content-Type", "application/json")
	if err := json.NewEncoder(w).Encode(payload); err != nil {
		slog.Error("merc templates: encode response", "error", err)
	}
}

// decodeMercBody caps and decodes a request body, distinguishing "too big" from
// "malformed".
//
// A body over the cap used to answer 400, which tells a client its JSON is
// wrong when the JSON was fine and only the batch was too large — the client
// then has no reason to retry in smaller pieces. 413 with the limit named is
// the answer it can act on.
func decodeMercBody(w http.ResponseWriter, r *http.Request, dst any, oversizeMsg string) bool {
	r.Body = http.MaxBytesReader(w, r.Body, mercTemplateBodyLimit)
	if err := json.NewDecoder(r.Body).Decode(dst); err != nil {
		var tooLarge *http.MaxBytesError
		if errors.As(err, &tooLarge) {
			jsonError(w, http.StatusRequestEntityTooLarge, oversizeMsg)
			return false
		}
		jsonError(w, http.StatusBadRequest, "invalid JSON body")
		return false
	}
	return true
}
