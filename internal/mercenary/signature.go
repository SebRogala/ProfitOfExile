// Package mercenary owns the shared pool of mercenary support-icon templates
// (POE-200, epic POE-165).
//
// A support cell in the recruit panel shows an icon (the FAMILY) and a small
// roman numeral (the TIER). The desktop app learns a family's art by hovering
// the cell and reducing the crop to a 24x24 RGB signature over the icon's disc;
// this package is where those signatures are pooled so one device's hover
// serves every device.
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
	// desktop/src-tauri/src/mercenary/icons.rs.
	SigDim = 24
	// SigChannels is how many bytes one position carries. Version 2 is RGB, in
	// that order: the gold frame every cell shares is what made a luma
	// signature correlate 0.97-0.99 across visibly different families
	// (POE-207), and colour is what separates them again.
	SigChannels = 3
	// SigBytes is the exact length of a wire signature: SigDim x SigDim
	// positions, SigChannels bytes each, row-major.
	SigBytes = SigDim * SigDim * SigChannels

	// maskWFrac and maskHFrac are the badge corner excluded from a signature,
	// as fractions of the cell — icons.rs. The numeral is not part of the
	// family's identity (the same art carries I, II or III), so leaving it in
	// would score one family's tier-1 and tier-3 samples as different families.
	maskWFrac float32 = 0.45
	maskHFrac float32 = 0.35

	// discRadiusFrac is the icon disc's radius as a fraction of the cell. The
	// art lives inside the disc; everything outside it is the shared gold
	// frame, which is identical on every cell and therefore carries no family
	// information at all.
	discRadiusFrac = 0.36
)

// maskX0 and maskY0 are the top-left corner of the masked badge region, derived
// the same way icons.rs::masked does: SIG_DIM minus the rounded fraction. For
// SigDim 24 this is (13, 16).
//
// discRadius and discCentre define the disc in the PIXEL-CENTRE convention: a
// position (x, y) is measured at (x+0.5, y+0.5) against a centre of SigDim/2.
// The convention is load-bearing — measuring from the integer corner instead
// shifts the disc half a pixel up and left and changes which positions are
// kept, and a signature whose kept set differs from its peer's correlates to
// exactly 0.0 (NCC's active-count guard), which reads as "unrelated art"
// rather than as the version mismatch it is.
var (
	maskX0     = SigDim - int(math.Round(float64(float32(SigDim)*maskWFrac)))
	maskY0     = SigDim - int(math.Round(float64(float32(SigDim)*maskHFrac)))
	discRadius = discRadiusFrac * float64(SigDim)
	discCentre = float64(SigDim) / 2
)

// Errors returned when a wire signature cannot become a Signature. They are
// distinguished so the upload handler can log WHY a template was rejected;
// every one of them is reported to the client as a rejected count, not as a
// failed request, because one bad template must not discard the good ones sent
// with it.
var (
	// ErrSignatureSize means the decoded payload was not exactly SigBytes.
	ErrSignatureSize = errors.New("mercenary: signature must be exactly 1728 bytes")
	// ErrSignatureFlat means the kept region carries under one level of
	// variance — an empty slot or a flat panel, not an icon. icons.rs rejects
	// the same input, and for the same reason: normalising it would divide by
	// ~zero and make every later correlation meaningless.
	ErrSignatureFlat = errors.New("mercenary: signature is flat (no icon)")
)

// inBadgeCorner reports whether a position lies in the tier-badge corner.
func inBadgeCorner(x, y int) bool {
	return x >= maskX0 && y >= maskY0
}

// insideDisc reports whether a position's CENTRE lies within the icon disc.
func insideDisc(x, y int) bool {
	return math.Hypot(float64(x)+0.5-discCentre, float64(y)+0.5-discCentre) <= discRadius
}

// masked reports whether a signature position is excluded from the
// correlation: outside the icon disc, or inside the badge corner.
//
// Position-based, NOT value-based: a kept pixel that happens to be pure black
// is a real sample and counts toward the correlation's divisor. icons.rs zeroes
// the masked bytes when it stores a signature, so a zero byte arriving on the
// wire is ambiguous — the geometry is what resolves it, on both sides.
func masked(x, y int) bool {
	return !insideDisc(x, y) || inBadgeCorner(x, y)
}

// Signature is a normalised 24x24 RGB cell signature: zero-mean, unit-stddev
// JOINTLY over the channel values at its kept positions, with everything else
// zeroed.
//
// Joint, not per-channel: normalising each channel on its own would rescale a
// colour cast away and make a red icon and a blue one of the same shape score
// as the same art — which is the failure version 1 had in grayscale.
//
// The zero value is not usable; build one with NewSignature.
type Signature struct {
	// rgb is the pre-normalisation RGB with masked positions zeroed — exactly
	// the bytes that go to and come from the database, so a signature
	// round-trips through storage without storing floats.
	rgb []byte
	// norm holds the zero-mean unit-stddev values, exactly 0.0 at masked
	// positions so they contribute nothing to a correlation.
	norm []float32
	// active is how many FLOATS take part: 3 per kept position, and the
	// correlation's divisor.
	active int
}

// NewSignature builds a Signature from a SigBytes RGB buffer, row-major, three
// bytes per position.
//
// This is a port of icons.rs::CellSig::from_rgb and must stay one: the desktop
// and the server compare the same art with the same numbers, so a divergence
// here silently changes which uploads count as duplicates. Accumulation is in
// float64 and the stored values are float32, matching the Rust exactly.
func NewSignature(rgb []byte) (Signature, error) {
	if len(rgb) != SigBytes {
		return Signature{}, fmt.Errorf("%w (got %d)", ErrSignatureSize, len(rgb))
	}

	var sum, sumSq float64
	active := 0
	for y := 0; y < SigDim; y++ {
		for x := 0; x < SigDim; x++ {
			if masked(x, y) {
				continue
			}
			base := (y*SigDim + x) * SigChannels
			for c := 0; c < SigChannels; c++ {
				v := float64(rgb[base+c])
				sum += v
				sumSq += v * v
				active++
			}
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
	copy(stored, rgb)
	norm := make([]float32, SigBytes)
	for y := 0; y < SigDim; y++ {
		for x := 0; x < SigDim; x++ {
			base := (y*SigDim + x) * SigChannels
			for c := 0; c < SigChannels; c++ {
				if masked(x, y) {
					stored[base+c] = 0
					continue
				}
				norm[base+c] = float32((float64(stored[base+c]) - mean) / sd)
			}
		}
	}

	return Signature{rgb: stored, norm: norm, active: active}, nil
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
func EncodeSignature(rgb []byte) string {
	return base64.StdEncoding.EncodeToString(rgb)
}

// Bytes returns a copy of the stored RGB, masked positions zeroed. This is what
// the database holds and what the serve endpoint hands back.
func (s Signature) Bytes() []byte {
	out := make([]byte, len(s.rgb))
	copy(out, s.rgb)
	return out
}

// Active reports how many floats take part in a correlation. It is a constant
// for a given SigDim, disc and mask (657 = 219 kept positions x 3 channels for
// version 2) and is exposed because it is the divisor: a test that pins it pins
// the geometry.
func (s Signature) Active() int { return s.active }

// NCC is the normalised cross-correlation with another signature: 1.0 for
// identical art, ~0.0 for unrelated, negative for inverted.
//
// Port of icons.rs::CellSig::ncc, including the guard that returns 0.0 when the
// two signatures disagree on their active count — that cannot happen inside one
// format version, and returning 0 rather than comparing anyway is what keeps a
// future mask change from producing a confident wrong answer.
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
