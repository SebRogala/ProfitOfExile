package mercenary

import (
	"errors"
	"fmt"
	"strings"
	"time"
)

// SupportedFormatVersion is the ONE signature format this server understands.
//
// Version 2 is: the cell's ALIGNMENT WINDOW — the frame-inset inner rect the
// desktop cuts, shrunk by SHIFT_MAX (3) SCREEN pixels on every side, so 33x33
// at the live 0.974 scale and 34x34 on the 1:1 reference fixture. The margin
// given up is not waste: detected cell rects land 1-3 px off per cell, and that
// margin is the room the matcher slides the window over at match time. It is
// bought here rather than by growing the rect, which would pull the
// neighbouring cell's art in at the extremes. The window is then resized to
// 24x24 RGB (SigDim, SigChannels) with a triangle filter; the positions kept
// are those whose PIXEL CENTRE (x+0.5, y+0.5) lies within 0.36 x SigDim of the
// cell centre AND outside the 0.45 x 0.35 tier-badge corner — 219 positions,
// 657 channel values; then a single zero-mean unit-stddev normalisation applied
// JOINTLY across all 657.
// The stored form is the full 1728 bytes with every masked position zeroed, so
// a reload reproduces the signature byte for byte. Version 1 was 576 grayscale
// bytes with no disc, and is gone: the shared gold frame it kept dominated the
// correlation, so visibly different families scored 0.97-0.99 against each
// other (POE-207).
//
// Anything that changes any of those numbers is version 3, not a tweak to
// version 2: signatures from the two are not comparable, and the whole point of
// carrying the version in the key is that such a change starts an empty pool
// instead of poisoning every device's matcher at once.
//
// Uploads declaring any other version are refused. Reads are NOT gated on it —
// see the serve handler for why a client may legitimately ask for an older
// version.
const SupportedFormatVersion int16 = 2

// DedupeThreshold is the correlation at or above which a candidate is the same
// art as a sample already pooled — under its own key (a duplicate) or under
// another family (a conflict). It is the desktop's default `icon_match`
// threshold (desktop/src-tauri/src/mercenary/mod.rs, `icon_match` field and its
// 0.88 default).
//
// The desktop's copy is overridable from its thresholds JSON; this one is a
// compile-time constant and the server has no way to learn that a client moved
// its own. A device running a lower icon_match calls art a duplicate that the
// server stores, and one running a higher value re-offers what the server keeps
// refusing — in both cases silently, because neither side sees the other's
// number. That is why the served corpus carries `dedupe_threshold`: it is the
// only signal a client gets that its threshold and the pool's disagree.
const DedupeThreshold float32 = 0.88

// MaxSamplesPerKey caps how many live samples one (family, tier) may hold,
// mirroring TemplateStore::MAX_SAMPLES_PER_KEY (icons.rs). More than one sample
// exists because a mistimed hover can save art that later matches nothing and a
// second confirm has to be able to repair it; the cap is what stops a jittery
// hover — or an abusive client — from filling the pool.
//
// From format 2 the cap carries a second, load-bearing job: bounding near-
// duplicates the dedupe rule cannot see. What a device stores and uploads is
// the UNSHIFTED window signature; alignment is a match-time search over the
// 49 shifts, and none of it is baked into what goes on the wire. Two devices
// whose detected rects sit 1-3 px apart therefore upload signatures of the SAME
// art that correlate at only ~0.45-0.70 (measured, POE-207) — far below
// DedupeThreshold — so the pool keeps both. That is by design: an unshifted
// signature is exactly what the shift search needs as its input, and a
// server-side alignment pass would both cost the server work it has no reason
// to do and call art a duplicate that the devices themselves do not. Three
// slots is the bound on how many such per-rect variants one key accumulates.
const MaxSamplesPerKey = 3

// RetiredMatchWindow is how long a retired sample keeps refusing a re-upload of
// the same art.
//
// A tombstone is unauthenticated by design (L4: every device publishes, no role
// gate) and the fingerprint behind it is spoofable, so an unbounded retirement
// would let any client blacklist correct art for a key permanently, with no
// undo short of SQL. The window bounds that: a hostile retirement expires,
// while the case retirement exists for — a device republishing the bad sample
// it still holds until its next pull — is measured in minutes, and even a
// device that stays offline for weeks is covered.
//
// After the window the same art can be offered and stored again. The retired
// row itself never returns to service; it simply stops being a match target,
// and stops being listed as a tombstone.
const RetiredMatchWindow = 30 * 24 * time.Hour

// MaxFamilyLen bounds a family name. The real vocabulary's longest family is
// far under this; the limit exists so an upload cannot store an arbitrarily
// long string.
const MaxFamilyLen = 128

// MaxTemplatesPerUpload bounds how many templates one request may carry. The
// 128 KB body cap already limits this to roughly 55, but a stated ceiling makes
// the bound an intent rather than an accident of the encoding.
const MaxTemplatesPerUpload = 64

// ErrInvalidKey means a family/tier pair is the wrong SHAPE — empty, overlong,
// or a tier outside 1-3.
var ErrInvalidKey = errors.New("mercenary: invalid template key")

// ErrUnknownFamily means the family is well-formed but is not one the shipped
// vocabulary names. Separate from ErrInvalidKey because the two mean different
// things to a client: a malformed key is its bug, an unknown family usually
// means its vocabulary fixture is newer than this server's and the fix is a
// deploy, not a code change.
var ErrUnknownFamily = errors.New("mercenary: family is not in the support vocabulary")

// KnownFamilyCount reports how many icon families the shipped vocabulary
// carries. Exposed so a caller (and a test) can see the key space is finite.
func KnownFamilyCount() int { return len(knownFamilies) }

// Key identifies a pooled icon within one format version: the icon's family and
// the tier its badge showed.
//
// The version is not a field because it is fixed for a whole request; the
// repository takes it alongside the keys. The full storage key is
// (family, tier, format_version).
type Key struct {
	Family string
	Tier   int16
}

func (k Key) String() string { return fmt.Sprintf("%s--%d", k.Family, k.Tier) }

// ParseKey validates a family/tier pair's SHAPE and normalises it, without
// asking whether the family is one this build knows.
//
// This is the form removal uses. A key is orphaned exactly when its family
// leaves the vocabulary — a rename — and that orphan is the case a tombstone
// exists for: the pool still holds art under the old name, no hover will ever
// match it again, and somebody has to be able to retire it. Checking removal
// against the closed set would make the one key that most needs retiring the
// one key that cannot be.
func ParseKey(family string, tier int) (Key, error) {
	family = strings.TrimSpace(family)
	switch {
	case family == "":
		return Key{}, fmt.Errorf("%w: family is empty", ErrInvalidKey)
	case len(family) > MaxFamilyLen:
		return Key{}, fmt.Errorf("%w: family exceeds %d bytes", ErrInvalidKey, MaxFamilyLen)
	case tier < 1 || tier > 3:
		return Key{}, fmt.Errorf("%w: tier %d outside 1-3", ErrInvalidKey, tier)
	}
	return Key{Family: family, Tier: int16(tier)}, nil
}

// NewKey is ParseKey plus the vocabulary gate: the form that ADMITS art to the
// pool.
//
// Free text would make the key space unbounded here. `family` arrives from a
// client that can say anything, and a typo — or a device deliberately inventing
// names — would pool art under keys no hover can ever match, one row of dead
// weight per upload the rate limit allows. The closed set caps the pool at
// KnownFamilyCount x 3 tiers x MaxSamplesPerKey and makes the server's key
// space exactly the desktop's.
//
// The asymmetry with ParseKey is deliberate and one-directional: adding art is
// gated, removing it is not. A gate on removal only ever protects bad art.
func NewKey(family string, tier int) (Key, error) {
	key, err := ParseKey(family, tier)
	if err != nil {
		return Key{}, err
	}
	if _, known := knownFamilies[key.Family]; !known {
		return Key{}, fmt.Errorf("%w: %q", ErrUnknownFamily, key.Family)
	}
	return key, nil
}

// Candidate is one signature offered for a key.
type Candidate struct {
	Key       Key
	Signature Signature
}

// Outcome is what the pool did with a candidate.
type Outcome int

const (
	// Stored means the candidate was new art for a key with room.
	Stored Outcome = iota
	// Duplicate means a live sample under the key already carries this art.
	Duplicate
	// Capped means the key already holds MaxSamplesPerKey live samples.
	Capped
	// Tombstoned means this exact art was retired from the key. The key
	// itself stays open — only the sample that was thrown out is refused.
	Tombstoned
	// Conflicting means a LIVE sample of another family already carries this
	// art. One picture belongs to one family, so the pool refuses the second
	// claim on it rather than serving both and letting every device decide
	// which of the two to believe.
	Conflicting
)

func (o Outcome) String() string {
	switch o {
	case Stored:
		return "stored"
	case Duplicate:
		return "duplicate"
	case Capped:
		return "capped"
	case Tombstoned:
		return "tombstoned"
	case Conflicting:
		return "conflicting"
	default:
		return "unknown"
	}
}

// KeyState is the pool's current view of one key: the samples it serves, and
// the samples that were retired from it.
//
// Retired samples are kept rather than deleted because they are the record of
// what was thrown out: a device that published bad art still holds it until its
// next pull, and matching against them is what stops it being republished. They
// do NOT close the key — better art for the same family and tier is still
// welcome, which is how a mistimed hover gets repaired after the bad sample is
// retired.
type KeyState struct {
	Live    []Signature
	Retired []Signature
}

// ForeignSample is one live pooled sample in the form the accept rule needs: a
// key, and a signature already DECODED.
//
// Not Sample, which carries the raw bytes the serve path round-trips: the
// cross-family check compares one candidate against the whole live view, so a
// []byte here would re-decode every sample once per candidate — a full batch's
// 32 candidates times the whole saturated view, instead of the one pass the
// repository pays per request. (The view's ceiling is derived in repository.go's
// lockPoolSQL comment; it moves with the vocabulary, so nothing here names it.)
type ForeignSample struct {
	Key       Key
	Signature Signature
}

// otherFamilies narrows a version's whole live view to the samples that could
// CONFLICT with art offered under `family` — everything under a different
// family name.
//
// Same family, different tier, is deliberately not foreign. The signature masks
// the tier-badge corner (see SupportedFormatVersion), so one family's tier-1 and
// tier-3 art correlate at ~1.0 by construction whenever the picture is the same
// — which it usually is. Treating that as a conflict would refuse the second
// tier of every family in the vocabulary.
//
// The caller supplies a view built from LIVE rows only. Retired art of another
// family is not served to anybody, so it has nothing to be confused with, and
// letting it refuse an upload would break the documented recovery path: the way
// out of a mislabel is to tombstone the wrong key so the right family's art can
// finally be stored.
func otherFamilies(live []ForeignSample, family string) []ForeignSample {
	foreign := make([]ForeignSample, 0, len(live))
	for _, sample := range live {
		if sample.Key.Family != family {
			foreign = append(foreign, sample)
		}
	}
	return foreign
}

// Decide is the accept rule, pure and independent of storage.
//
// `state` is the candidate's own key; `foreign` is every LIVE sample of the
// same format version under a DIFFERENT family (otherFamilies builds it). The
// second return is the incumbent that refused a Conflicting candidate, and nil
// for every other outcome — returned rather than an index so the caller can
// name the family without carrying a parallel array. It is the FIRST foreign
// match in pool order, so the order `foreign` arrives in decides which of
// several matching families gets named: the repository's poolSQL orders by
// (family, tier, id) precisely so two identical uploads name the same one.
//
// A retired sample is refused by the SAME correlation that catches a duplicate:
// what was thrown out is recognised again and stays out, while art the pool has
// never held is accepted normally. The key is never closed — a rename orphans a
// key, and the answer to that is tombstone-then-relearn, which only works if
// the key keeps accepting new art.
//
// The cap counts live samples alone. Retired ones occupy no slot; if they did,
// three retirements would close a key by exhaustion and reintroduce the block
// this rule exists to avoid.
//
// The order — own-key retired, own-key duplicate, cross-family conflict, cap,
// store — is the whole rule, and every step of it is load-bearing:
//
//   - Duplicate before the cap: a full key offered art it already has is
//     reported as a duplicate rather than as capped, because "we already have
//     this" is the answer the uploading device can act on (stop offering it)
//     while "full" invites a retry once a slot frees.
//   - Own-key duplicate before the conflict: if the pool already carries this
//     art under BOTH families it is already inconsistent, and the client's own
//     merge rule empties such a cluster. Reporting the conflict instead would
//     tell a device to settle a sample the pool is in fact still serving to it.
//   - Conflict before the cap: "another family owns this picture" is a
//     permanent refusal the player can fix (forget the wrong family), while
//     "full" tells the device to keep retrying art that will never be stored.
func Decide(state KeyState, foreign []ForeignSample, candidate Signature) (Outcome, *ForeignSample) {
	for _, retired := range state.Retired {
		if retired.NCC(candidate) >= DedupeThreshold {
			return Tombstoned, nil
		}
	}
	for _, live := range state.Live {
		if live.NCC(candidate) >= DedupeThreshold {
			return Duplicate, nil
		}
	}
	for _, other := range foreign {
		if other.Signature.NCC(candidate) >= DedupeThreshold {
			incumbent := other
			return Conflicting, &incumbent
		}
	}
	if len(state.Live) >= MaxSamplesPerKey {
		return Capped, nil
	}
	return Stored, nil
}

// Conflict names one refused candidate and the art that refused it.
//
// Index is the candidate's position in the batch handed to Accept — NOT its
// position in the request that produced the batch. The handler drops templates
// it cannot decode before building candidates, so the two differ whenever
// anything was dropped, and translating between them is the handler's job.
type Conflict struct {
	Index           int
	Key             Key
	IncumbentFamily string
}

// AcceptResult tallies one upload by outcome. The counts sum to the number of
// candidates that reached the pool; templates the handler could not decode are
// counted separately as rejected and never become candidates.
//
// Conflicts carries one entry per Conflicting candidate. It is detail on top of
// the count, not a replacement for it: a device settles per SAMPLE, so it needs
// to know WHICH of the templates it offered was refused and by whom.
type AcceptResult struct {
	Stored      int
	Duplicate   int
	Capped      int
	Tombstoned  int
	Conflicting int
	Conflicts   []Conflict
}

// Record folds one outcome into the tally. The Conflicts detail is appended by
// the caller, which is the only party that knows the candidate's index.
func (r *AcceptResult) Record(o Outcome) {
	switch o {
	case Stored:
		r.Stored++
	case Duplicate:
		r.Duplicate++
	case Capped:
		r.Capped++
	case Tombstoned:
		r.Tombstoned++
	case Conflicting:
		r.Conflicting++
	}
}

// Sample is one stored template as the serve path returns it: art and key, and
// nothing that identifies who uploaded it.
type Sample struct {
	Key       Key
	Signature []byte
}

// Corpus is everything a client needs to rebuild its local store for one
// format version: the live samples, and the keys something was retired from.
//
// A tombstoned key may still carry live samples — retiring bad art does not
// close the key — so the tombstone list means "this key had a sample thrown
// out", not "this key is gone". A client merges by replacing its copies of a
// listed key with the served ones, not by deleting the key.
type Corpus struct {
	FormatVersion int16
	Templates     []Sample
	Tombstones    []Key
}

// Retired samples older than RetiredMatchWindow are neither served nor listed:
// see the constant for why a retirement expires.
