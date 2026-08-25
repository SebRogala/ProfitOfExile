// Package mercenary owns the shared pool of mercenary support-icon templates
// (POE-200, epic POE-165).
//
// A support cell in the recruit panel shows an icon (the FAMILY) and a small
// roman numeral (the TIER). The desktop app learns a family's art by hovering
// the cell and reducing the crop to a 24x24 grayscale signature; this package
// is where those signatures are pooled so one device's hover serves every
// device.
//
// The package holds two things HTTP must not: the signature format (and the
// correlation defined over it, which must stay byte-for-byte comparable with
// the desktop's) and the accept decision (dedupe, cap, tombstone). Transport
// lives in internal/server/handlers.
package mercenary

import (
	"encoding/base64"
	"errors"
	"fmt"
	"math"
)

const (
	// SigDim is the signature's side length, mirroring SIG_DIM in
	// desktop/src-tauri/src/mercenary/icons.rs:31.
	SigDim = 24
	// SigBytes is the exact length of a wire signature: SigDim x SigDim
	// grayscale samples, one byte each.
	SigBytes = SigDim * SigDim

	// maskWFrac and maskHFrac are the badge corner excluded from a signature,
	// as fractions of the cell — icons.rs:39-40. The numeral is not part of the
	// family's identity (the same art carries I, II or III), so leaving it in
	// would score one family's tier-1 and tier-3 samples as different families.
	maskWFrac float32 = 0.45
	maskHFrac float32 = 0.35
)

// maskX0 and maskY0 are the top-left corner of the masked region, derived the
// same way icons.rs::masked does: SIG_DIM minus the rounded fraction. For
// SigDim 24 this is (13, 16), so 11x8 = 88 positions are masked and 488 are
// active.
var (
	maskX0 = SigDim - int(math.Round(float64(float32(SigDim)*maskWFrac)))
	maskY0 = SigDim - int(math.Round(float64(float32(SigDim)*maskHFrac)))
)

// Errors returned when a wire signature cannot become a Signature. They are
// distinguished so the upload handler can log WHY a template was rejected;
// every one of them is reported to the client as a rejected count, not as a
// failed request, because one bad template must not discard the good ones sent
// with it.
var (
	// ErrSignatureSize means the decoded payload was not exactly SigBytes.
	ErrSignatureSize = errors.New("mercenary: signature must be exactly 576 bytes")
	// ErrSignatureFlat means the unmasked region carries under one grey level
	// of variance — an empty slot or a flat panel, not an icon. icons.rs:97
	// rejects the same input, and for the same reason: normalising it would
	// divide by ~zero and make every later correlation meaningless.
	ErrSignatureFlat = errors.New("mercenary: signature is flat (no icon)")
)

// masked reports whether a signature position lies in the excluded badge
// corner. Position-based, NOT value-based: an unmasked pixel that happens to be
// pure black is a real sample and counts toward the correlation's divisor.
// icons.rs zeroes the masked bytes when it stores a signature, so a zero byte
// arriving on the wire is ambiguous — the geometry is what resolves it, on both
// sides.
func masked(x, y int) bool {
	return x >= maskX0 && y >= maskY0
}

// Signature is a normalised 24x24 cell signature: zero-mean, unit-stddev over
// its unmasked positions, with the badge corner zeroed.
//
// The zero value is not usable; build one with NewSignature.
type Signature struct {
	// gray is the pre-normalisation grayscale with masked positions zeroed —
	// exactly the bytes that go to and come from the database, so a signature
	// round-trips through storage without storing floats.
	gray []byte
	// norm holds the zero-mean unit-stddev values, exactly 0.0 at masked
	// positions so they contribute nothing to a correlation.
	norm []float32
	// active is how many positions are unmasked: the correlation's divisor.
	active int
}

// NewSignature builds a Signature from a SigBytes grayscale buffer.
//
// This is a port of icons.rs::CellSig::from_gray (icons.rs:76-114) and must
// stay one: the desktop and the server compare the same art with the same
// numbers, so a divergence here silently changes which uploads count as
// duplicates. Accumulation is in float64 and the stored values are float32,
// matching the Rust exactly.
func NewSignature(gray []byte) (Signature, error) {
	if len(gray) != SigBytes {
		return Signature{}, fmt.Errorf("%w (got %d)", ErrSignatureSize, len(gray))
	}

	var sum, sumSq float64
	active := 0
	for y := 0; y < SigDim; y++ {
		for x := 0; x < SigDim; x++ {
			if masked(x, y) {
				continue
			}
			v := float64(gray[y*SigDim+x])
			sum += v
			sumSq += v * v
			active++
		}
	}
	if active == 0 {
		return Signature{}, ErrSignatureFlat
	}

	mean := sum / float64(active)
	variance := sumSq/float64(active) - mean*mean
	if variance < 1.0 {
		return Signature{}, ErrSignatureFlat
	}
	sd := math.Sqrt(variance)

	stored := make([]byte, SigBytes)
	copy(stored, gray)
	norm := make([]float32, SigBytes)
	for y := 0; y < SigDim; y++ {
		for x := 0; x < SigDim; x++ {
			i := y*SigDim + x
			if masked(x, y) {
				stored[i] = 0
				continue
			}
			norm[i] = float32((float64(stored[i]) - mean) / sd)
		}
	}

	return Signature{gray: stored, norm: norm, active: active}, nil
}

// DecodeSignature parses a base64 (standard, padded) wire signature.
func DecodeSignature(b64 string) (Signature, error) {
	raw, err := base64.StdEncoding.DecodeString(b64)
	if err != nil {
		return Signature{}, fmt.Errorf("mercenary: decode signature: %w", err)
	}
	return NewSignature(raw)
}

// EncodeSignature renders stored signature bytes for the wire.
func EncodeSignature(gray []byte) string {
	return base64.StdEncoding.EncodeToString(gray)
}

// Bytes returns a copy of the stored grayscale, masked positions zeroed. This
// is what the database holds and what the serve endpoint hands back.
func (s Signature) Bytes() []byte {
	out := make([]byte, len(s.gray))
	copy(out, s.gray)
	return out
}

// Active reports how many positions take part in a correlation. It is a
// constant for a given SigDim and mask (488 for version 1) and is exposed
// because it is the divisor: a test that pins it pins the mask geometry.
func (s Signature) Active() int { return s.active }

// NCC is the normalised cross-correlation with another signature: 1.0 for
// identical art, ~0.0 for unrelated, negative for inverted.
//
// Port of icons.rs::CellSig::ncc (icons.rs:121-133), including the guard that
// returns 0.0 when the two signatures disagree on their active count — that
// cannot happen inside one format version, and returning 0 rather than
// comparing anyway is what keeps a future mask change from producing a
// confident wrong answer.
func (s Signature) NCC(other Signature) float32 {
	if s.active == 0 || s.active != other.active {
		return 0
	}
	var dot float32
	for i := range s.norm {
		dot += s.norm[i] * other.norm[i]
	}
	return dot / float32(s.active)
}
