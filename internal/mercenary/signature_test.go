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

// activeIndices lists the unmasked positions, row-major. Its length IS the
// correlation's divisor.
func activeIndices() []int {
	out := make([]int, 0, SigBytes)
	for y := 0; y < SigDim; y++ {
		for x := 0; x < SigDim; x++ {
			if !masked(x, y) {
				out = append(out, y*SigDim+x)
			}
		}
	}
	return out
}

// balancedGray builds a 576-byte buffer whose unmasked half is exactly half lo
// and half hi, so its z-scores are exactly -1 and +1 and every correlation
// below is exact arithmetic rather than a float approximation. Masked positions
// are filled with maskFill, which must never reach the result.
//
// assign receives the position's ordinal within the active set and returns
// true for the hi value.
func balancedGray(hi func(ordinal int) bool, lo, high, maskFill byte) []byte {
	gray := make([]byte, SigBytes)
	for i := range gray {
		gray[i] = maskFill
	}
	for ordinal, idx := range activeIndices() {
		if hi(ordinal) {
			gray[idx] = high
		} else {
			gray[idx] = lo
		}
	}
	return gray
}

// splitHalf puts the first half of the active positions at lo and the rest at
// hi — the base pattern the correlation tests perturb.
func splitHalf(ordinal int) bool { return ordinal >= 244 }

func mustSignature(t *testing.T, gray []byte) Signature {
	t.Helper()
	sig, err := NewSignature(gray)
	if err != nil {
		t.Fatalf("NewSignature: %v", err)
	}
	return sig
}

// The mask is geometric: 11 columns by 8 rows in the badge corner, leaving 488
// of 576 positions active. Pinning the number pins the corner, and the corner
// is what lets one family's tier-1 and tier-3 art score as the same family.
func TestNewSignature_MaskLeaves488ActivePositions(t *testing.T) {
	if got := len(activeIndices()); got != 488 {
		t.Fatalf("active positions = %d, want 488 (mask origin (%d,%d))", got, maskX0, maskY0)
	}

	sig := mustSignature(t, balancedGray(splitHalf, 1, 255, 200))
	if sig.Active() != 488 {
		t.Errorf("Signature.Active() = %d, want 488", sig.Active())
	}
}

// A pure-black pixel outside the badge corner is a real sample, not a masked
// one. Treating the zero BYTE as the mask (rather than the position) would
// drop 244 positions here and change the divisor — and would diverge from
// icons.rs, which zeroes masked bytes on the way out but never reads that zero
// back as "excluded".
func TestNewSignature_UnmaskedBlackPixelsStayActive(t *testing.T) {
	sig := mustSignature(t, balancedGray(splitHalf, 0, 255, 200))

	if sig.Active() != 488 {
		t.Fatalf("Active() = %d with 244 black active positions, want 488", sig.Active())
	}
}

// The stored bytes are the wire and database representation, so the mask has to
// be applied to them too: a device that pulls this signature must get the same
// bytes it would have produced locally.
func TestNewSignature_ZeroesTheMaskedCornerInStoredBytes(t *testing.T) {
	sig := mustSignature(t, balancedGray(splitHalf, 1, 255, 200))
	stored := sig.Bytes()

	for y := 0; y < SigDim; y++ {
		for x := 0; x < SigDim; x++ {
			i := y*SigDim + x
			if masked(x, y) && stored[i] != 0 {
				t.Fatalf("masked position (%d,%d) = %d, want 0", x, y, stored[i])
			}
			if !masked(x, y) && stored[i] == 0 {
				t.Fatalf("active position (%d,%d) was zeroed", x, y)
			}
		}
	}
}

// Two crops that differ only inside the badge corner are the same art. This is
// the whole reason the corner is masked, and it is the property that lets a
// tier-1 confirmation recognise the same family at tier 3.
func TestNCC_IgnoresDifferencesInsideTheBadgeCorner(t *testing.T) {
	a := mustSignature(t, balancedGray(splitHalf, 1, 255, 200))
	b := mustSignature(t, balancedGray(splitHalf, 1, 255, 17))

	if got := a.NCC(b); math.Abs(float64(got)-1.0) > 1e-6 {
		t.Fatalf("NCC of crops differing only in the masked corner = %v, want 1.0", got)
	}
}

func TestNCC_IdenticalArtCorrelatesToOne(t *testing.T) {
	gray := balancedGray(splitHalf, 1, 255, 200)
	a := mustSignature(t, gray)
	b := mustSignature(t, gray)

	if got := a.NCC(b); math.Abs(float64(got)-1.0) > 1e-6 {
		t.Fatalf("NCC(a, a) = %v, want 1.0", got)
	}
}

// An inverted crop is maximally UNlike the original, not maximally like it. A
// correlation that dropped the zero-mean step would score this near +1 and let
// a negative image pass as a duplicate.
func TestNCC_InvertedArtCorrelatesToMinusOne(t *testing.T) {
	a := mustSignature(t, balancedGray(splitHalf, 1, 255, 200))
	b := mustSignature(t, balancedGray(func(o int) bool { return !splitHalf(o) }, 1, 255, 200))

	if got := a.NCC(b); math.Abs(float64(got)+1.0) > 1e-6 {
		t.Fatalf("NCC(a, inverted a) = %v, want -1.0", got)
	}
}

// Unrelated art scores near zero: the two patterns below agree on exactly half
// the active positions.
func TestNCC_UnrelatedArtCorrelatesToZero(t *testing.T) {
	a := mustSignature(t, balancedGray(splitHalf, 1, 255, 200))
	b := mustSignature(t, balancedGray(func(o int) bool { return o%2 == 0 }, 1, 255, 200))

	if got := a.NCC(b); math.Abs(float64(got)) > 1e-6 {
		t.Fatalf("NCC of half-agreeing patterns = %v, want 0.0", got)
	}
}

func TestNewSignature_WrongLengthIsRejected(t *testing.T) {
	for _, size := range []int{0, SigBytes - 1, SigBytes + 1} {
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

// Just under one grey level of variation is still flat. The boundary matters:
// it is what separates "an icon drawn in near-uniform colours" from "the panel
// behind an empty slot".
func TestNewSignature_VarianceJustBelowOneIsRejected(t *testing.T) {
	gray := make([]byte, SigBytes)
	for i := range gray {
		gray[i] = 128
	}
	// One active position raised by 1: variance is (487/488)*(1/488) ≈ 0.002.
	gray[activeIndices()[0]] = 129
	if _, err := NewSignature(gray); !errors.Is(err, ErrSignatureFlat) {
		t.Fatalf("NewSignature(variance < 1) error = %v, want ErrSignatureFlat", err)
	}
}

// A signature survives the wire and the database unchanged: what a device
// uploads is byte-for-byte what every other device pulls back.
func TestDecodeSignature_RoundTripsStoredBytes(t *testing.T) {
	original := mustSignature(t, balancedGray(splitHalf, 1, 255, 200))

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

func TestDecodeSignature_NonBase64IsRejected(t *testing.T) {
	if _, err := DecodeSignature("not base64!!"); err == nil {
		t.Fatal("DecodeSignature accepted a non-base64 payload")
	}
}
