package mercenary

import (
	"errors"
	"math"
	"testing"
)

// The signature format is a port of desktop/src-tauri/src/mercenary/icons.rs.
// These tests pin the three things a port can silently get wrong — which
// positions take part, what the divisor is, and what the correlation of known
// inputs comes to — because a divergence does not fail anywhere: it just makes
// the server disagree with every device about what counts as the same art.

// keptByFormula is the format-2 geometry TRANSCRIBED FROM THE SPEC, not read
// off the production code: a position is kept when its pixel CENTRE lies within
// 0.36 x SigDim of the cell centre AND it is outside the 0.45 x 0.35 tier-badge
// corner. The whole point of spelling it out a second time is that a change to
// masked() — a dropped +0.5, a different radius fraction, a swapped comparison
// — then disagrees with this and fails, instead of silently redefining "the
// same art" for every device at once.
func keptByFormula(x, y int) bool {
	const (
		dim    = 24
		radius = 0.36 * dim
		centre = dim / 2.0
	)
	dx, dy := float64(x)+0.5-centre, float64(y)+0.5-centre
	if math.Sqrt(dx*dx+dy*dy) > radius {
		return false
	}
	// The badge corner: 0.45 of the width and 0.35 of the height, rounded, taken
	// off the bottom-right — (13, 16) for dim 24.
	badgeX0 := dim - int(math.Round(dim*0.45))
	badgeY0 := dim - int(math.Round(dim*0.35))
	return !(x >= badgeX0 && y >= badgeY0)
}

// keptOrdinals lists the float slots that take part, row-major with the three
// channels of a position adjacent. Its length IS the correlation's divisor.
// Built from keptByFormula so every buffer these tests construct is laid out by
// the spec rather than by the code under test.
func keptOrdinals() []int {
	out := make([]int, 0, SigBytes)
	for y := 0; y < SigDim; y++ {
		for x := 0; x < SigDim; x++ {
			if !keptByFormula(x, y) {
				continue
			}
			base := (y*SigDim + x) * SigChannels
			for c := 0; c < SigChannels; c++ {
				out = append(out, base+c)
			}
		}
	}
	return out
}

// patternRGB builds a 1728-byte buffer whose kept slots are set to high or lo
// by `hi`, and whose masked slots are filled with maskFill — a value that must
// never reach the result.
//
// hi receives the slot's ordinal within the kept set (0..656), so a pattern is
// written in the same coordinates the correlation is computed in.
func patternRGB(hi func(ordinal int) bool, lo, high, maskFill byte) []byte {
	rgb := make([]byte, SigBytes)
	for i := range rgb {
		rgb[i] = maskFill
	}
	for ordinal, idx := range keptOrdinals() {
		if hi(ordinal) {
			rgb[idx] = high
		} else {
			rgb[idx] = lo
		}
	}
	return rgb
}

// firstThird puts the first 219 of the 657 kept slots at hi and the rest at lo
// — the base pattern the correlation tests perturb. A third rather than a half
// because 657 is odd: the orthogonal partner below is exact only against a
// third.
func firstThird(ordinal int) bool { return ordinal < 219 }

// shiftedThird is 219 slots again, overlapping firstThird in exactly 73. For a
// two-valued pattern of 219 hi slots out of 657, the correlation is
// (4.5*overlap - 328.5)/657, which is exactly zero at overlap 73 — so these two
// are exactly orthogonal, and "unrelated art scores 0" is arithmetic rather
// than a tolerance.
func shiftedThird(ordinal int) bool { return ordinal >= 146 && ordinal < 365 }

func mustSignature(t *testing.T, rgb []byte) Signature {
	t.Helper()
	sig, err := NewSignature(rgb)
	if err != nil {
		t.Fatalf("NewSignature: %v", err)
	}
	return sig
}

// The disc plus the badge corner keep 219 of 576 positions, which is 657 of the
// 1728 stored bytes. Pinning the number pins the geometry, and the geometry is
// what decides whether two devices can compare signatures at all: NCC returns
// 0.0 — indistinguishable from "unrelated art" — the moment two signatures
// disagree on their active count.
func TestNewSignature_DiscAndBadgeMaskKeep219Positions(t *testing.T) {
	positions := 0
	for y := 0; y < SigDim; y++ {
		for x := 0; x < SigDim; x++ {
			if keptByFormula(x, y) {
				positions++
			}
		}
	}
	if positions != 219 {
		t.Fatalf("kept positions = %d, want 219", positions)
	}
	if got := len(keptOrdinals()); got != 657 {
		t.Fatalf("active floats = %d, want 657", got)
	}

	sig := mustSignature(t, patternRGB(firstThird, 1, 255, 200))
	if sig.Active() != 657 {
		t.Errorf("Signature.Active() = %d, want 657", sig.Active())
	}
}

// The implementation's mask must agree with the spec at every one of the 576
// positions, not merely on the total. Two errors that cancel — a position let
// in at the disc edge and another wrongly excluded in the corner — would keep
// the count at 219 while comparing different art.
func TestMasked_AgreesWithTheSpecAtEveryPosition(t *testing.T) {
	for y := 0; y < SigDim; y++ {
		for x := 0; x < SigDim; x++ {
			if want, got := !keptByFormula(x, y), masked(x, y); got != want {
				t.Fatalf("masked(%d,%d) = %v, want %v", x, y, got, want)
			}
		}
	}
}

// A pure-black pixel inside the disc is a real sample, not a masked one.
// Treating the zero BYTE as the mask (rather than the position) would drop 657
// slots here and change the divisor — and would diverge from icons.rs, which
// zeroes masked bytes on the way out but never reads that zero back as
// "excluded".
func TestNewSignature_KeptBlackPixelsStayActive(t *testing.T) {
	sig := mustSignature(t, patternRGB(firstThird, 0, 255, 200))

	if sig.Active() != 657 {
		t.Fatalf("Active() = %d with 438 black kept slots, want 657", sig.Active())
	}
}

// The stored bytes are the wire and database representation, so the mask has to
// be applied to them too: a device that pulls this signature must get the same
// bytes it would have produced locally.
func TestNewSignature_ZeroesEveryMaskedByteInStoredBytes(t *testing.T) {
	sig := mustSignature(t, patternRGB(firstThird, 1, 255, 200))
	stored := sig.Bytes()

	if len(stored) != 1728 {
		t.Fatalf("stored length = %d, want 1728", len(stored))
	}
	for y := 0; y < SigDim; y++ {
		for x := 0; x < SigDim; x++ {
			base := (y*SigDim + x) * SigChannels
			for c := 0; c < SigChannels; c++ {
				switch {
				case !keptByFormula(x, y) && stored[base+c] != 0:
					t.Fatalf("masked position (%d,%d) channel %d = %d, want 0", x, y, c, stored[base+c])
				case keptByFormula(x, y) && stored[base+c] == 0:
					t.Fatalf("kept position (%d,%d) channel %d was zeroed", x, y, c)
				}
			}
		}
	}
}

// Two crops that differ only inside the badge corner are the same art. This is
// the whole reason the corner is masked, and it is the property that lets a
// tier-1 confirmation recognise the same family at tier 3.
func TestNCC_IgnoresDifferencesInsideTheBadgeCorner(t *testing.T) {
	a := mustSignature(t, patternRGB(firstThird, 1, 255, 200))
	b := mustSignature(t, patternRGB(firstThird, 1, 255, 17))

	if got := a.NCC(b); math.Abs(float64(got)-1.0) > 1e-6 {
		t.Fatalf("NCC of crops differing only in the masked corner = %v, want 1.0", got)
	}
}

// The frame is what version 1 got wrong: every support cell draws the same gold
// border, so two different families agreed almost everywhere once it was in the
// signature. Version 2 keeps only the disc, and this is that promise stated as
// a test — the two crops below are unrelated inside the disc and identical
// everywhere outside it, and they must still score 0.
func TestNCC_IgnoresEverythingOutsideTheDisc(t *testing.T) {
	a := patternRGB(firstThird, 1, 255, 200)
	b := patternRGB(shiftedThird, 1, 255, 200)
	// Paint an identical "frame" over every position the disc excludes, so the
	// only thing left to disagree about is the art.
	for y := 0; y < SigDim; y++ {
		for x := 0; x < SigDim; x++ {
			if keptByFormula(x, y) {
				continue
			}
			base := (y*SigDim + x) * SigChannels
			for c := 0; c < SigChannels; c++ {
				a[base+c], b[base+c] = 240, 240
			}
		}
	}

	got := mustSignature(t, a).NCC(mustSignature(t, b))
	if math.Abs(float64(got)) > 1e-6 {
		t.Fatalf("NCC of unrelated discs behind an identical frame = %v, want 0.0", got)
	}
}

func TestNCC_IdenticalArtCorrelatesToOne(t *testing.T) {
	rgb := patternRGB(firstThird, 1, 255, 200)
	a := mustSignature(t, rgb)
	b := mustSignature(t, rgb)

	if got := a.NCC(b); math.Abs(float64(got)-1.0) > 1e-6 {
		t.Fatalf("NCC(a, a) = %v, want 1.0", got)
	}
}

// An inverted crop is maximally UNlike the original, not maximally like it. A
// correlation that dropped the zero-mean step would score this near +1 and let
// a negative image pass as a duplicate.
func TestNCC_InvertedArtCorrelatesToMinusOne(t *testing.T) {
	a := mustSignature(t, patternRGB(firstThird, 1, 255, 200))
	b := mustSignature(t, patternRGB(func(o int) bool { return !firstThird(o) }, 1, 255, 200))

	if got := a.NCC(b); math.Abs(float64(got)+1.0) > 1e-6 {
		t.Fatalf("NCC(a, inverted a) = %v, want -1.0", got)
	}
}

// Unrelated art scores zero: the two patterns below are constructed to be
// exactly orthogonal (see shiftedThird).
func TestNCC_UnrelatedArtCorrelatesToZero(t *testing.T) {
	a := mustSignature(t, patternRGB(firstThird, 1, 255, 200))
	b := mustSignature(t, patternRGB(shiftedThird, 1, 255, 200))

	if got := a.NCC(b); math.Abs(float64(got)) > 1e-6 {
		t.Fatalf("NCC of orthogonal patterns = %v, want 0.0", got)
	}
}

// Colour is what version 2 added, and it only buys anything if the three
// channels are normalised TOGETHER.
//
// The two crops below are the same SHAPE under a different colour cast: one
// draws its art at full contrast in every channel, the other at full contrast
// in red and at a fifth of that in green and blue. Normalising each channel
// against its own mean and stddev divides the cast straight out and scores them
// 1.0 — a tinted copy of one family's art would then be pooled as a duplicate
// of another's. One joint normalisation over all 657 values keeps the cast, and
// the two score 0.68: related, not the same.
func TestNCC_SameShapeUnderADifferentColourCastIsNotADuplicate(t *testing.T) {
	// byPosition paints whole positions, so the three channels of a kept pixel
	// are set together and the colour cast is the only variable.
	byPosition := func(ink func(position int) [SigChannels]byte) []byte {
		rgb := make([]byte, SigBytes)
		position := 0
		for y := 0; y < SigDim; y++ {
			for x := 0; x < SigDim; x++ {
				if !keptByFormula(x, y) {
					continue
				}
				base := (y*SigDim + x) * SigChannels
				channels := ink(position)
				copy(rgb[base:base+SigChannels], channels[:])
				position++
			}
		}
		return rgb
	}
	// The same third of the disc is "ink" in both crops.
	isInk := func(position int) bool { return position < 73 }

	neutral := mustSignature(t, byPosition(func(p int) [SigChannels]byte {
		if isInk(p) {
			return [SigChannels]byte{220, 220, 220}
		}
		return [SigChannels]byte{20, 20, 20}
	}))
	redCast := mustSignature(t, byPosition(func(p int) [SigChannels]byte {
		if isInk(p) {
			return [SigChannels]byte{220, 120, 120}
		}
		return [SigChannels]byte{20, 100, 100}
	}))

	got := neutral.NCC(redCast)
	if got >= DedupeThreshold {
		t.Fatalf("NCC(art, the same art under a red cast) = %v, want under the dedupe threshold %v "+
			"(1.0 would mean the channels were normalised separately)", got, DedupeThreshold)
	}
	if math.Abs(float64(got)-0.6761) > 1e-3 {
		t.Fatalf("NCC(art, the same art under a red cast) = %v, want 0.6761 — the cast is being "+
			"weighted differently than a single joint normalisation would weight it", got)
	}
}

func TestNewSignature_WrongLengthIsRejected(t *testing.T) {
	// 576 is the format-1 length: a v1 client's upload must be refused by size,
	// not decoded as a truncated v2 signature.
	for _, size := range []int{0, 576, SigBytes - 1, SigBytes + 1} {
		if _, err := NewSignature(make([]byte, size)); !errors.Is(err, ErrSignatureSize) {
			t.Errorf("NewSignature(%d bytes) error = %v, want ErrSignatureSize", size, err)
		}
	}
}

// An empty support slot has no gradient. Normalising it would divide by ~zero
// and make every later correlation against it meaningless, so it never becomes
// a signature at all — the same guard icons.rs applies at var < 1.
func TestNewSignature_FlatCropIsRejected(t *testing.T) {
	flat := make([]byte, SigBytes)
	for i := range flat {
		flat[i] = 128
	}
	if _, err := NewSignature(flat); !errors.Is(err, ErrSignatureFlat) {
		t.Fatalf("NewSignature(flat) error = %v, want ErrSignatureFlat", err)
	}
}

// Just under one level of variation is still flat. The boundary matters: it is
// what separates "an icon drawn in near-uniform colours" from "the panel behind
// an empty slot".
func TestNewSignature_VarianceJustBelowOneIsRejected(t *testing.T) {
	rgb := make([]byte, SigBytes)
	for i := range rgb {
		rgb[i] = 128
	}
	// One kept slot raised by 1: variance is (656/657)*(1/657) ~= 0.0015.
	rgb[keptOrdinals()[0]] = 129
	if _, err := NewSignature(rgb); !errors.Is(err, ErrSignatureFlat) {
		t.Fatalf("NewSignature(variance < 1) error = %v, want ErrSignatureFlat", err)
	}
}

// A signature survives the wire and the database unchanged: what a device
// uploads is byte-for-byte what every other device pulls back.
func TestDecodeSignature_RoundTripsStoredBytes(t *testing.T) {
	original := mustSignature(t, patternRGB(firstThird, 1, 255, 200))

	decoded, err := DecodeSignature(EncodeSignature(original.Bytes()))
	if err != nil {
		t.Fatalf("DecodeSignature: %v", err)
	}
	if got := original.NCC(decoded); math.Abs(float64(got)-1.0) > 1e-6 {
		t.Fatalf("round-tripped signature NCC = %v, want 1.0", got)
	}
	for i, b := range decoded.Bytes() {
		if b != original.Bytes()[i] {
			t.Fatalf("byte %d = %d after round trip, want %d", i, b, original.Bytes()[i])
		}
	}
}

// Bytes() hands out a COPY. The slice it returns goes straight into a pgx
// insert and into the served corpus payload, so a caller that reused or
// rewrote the buffer would otherwise corrupt the signature every later
// correlation on this server is computed against — silently, and only for as
// long as the process lives.
func TestBytes_ReturnsACopyTheCallerCannotWriteThrough(t *testing.T) {
	sig := mustSignature(t, patternRGB(firstThird, 1, 255, 200))
	first := sig.Bytes()
	original := first[keptOrdinals()[0]]

	first[keptOrdinals()[0]] = ^original

	if got := sig.Bytes()[keptOrdinals()[0]]; got != original {
		t.Fatalf("a write through the returned slice changed the signature: byte = %d, want %d",
			got, original)
	}
	if got := sig.NCC(sig); math.Abs(float64(got)-1.0) > 1e-6 {
		t.Fatalf("NCC(sig, sig) = %v after a caller wrote through Bytes(), want 1.0", got)
	}
}

func TestDecodeSignature_NonBase64IsRejected(t *testing.T) {
	if _, err := DecodeSignature("not base64!!"); err == nil {
		t.Fatal("DecodeSignature accepted a non-base64 payload")
	}
}
