package mercenary

import (
	"bytes"
	"image"
	"image/png"
	"math"
	"os"
	"path/filepath"
	"testing"
)

// The parity golden is the ONE artefact both sides of the format-2 signature
// are checked against.
//
// testdata/merc-sig-v2/crop.png is a real GGG cell crop taken from Sebastian's
// store on 2026-08-26 (the "Multistrike" support at tier 3): the 39x39 INNER
// rect the desktop cuts at `cell_inset` 2. signature.bin is the 1728 bytes that
// crop must reduce to.
//
// THE RUST SIDE IS THE AUTHORITATIVE WRITER. icons.rs owns the derivation; its
// own parity test regenerates this file behind an update flag (see the parity
// test in desktop/src-tauri/src/mercenary/icons.rs for the exact command). Go
// only READS and compares. If the two ever disagree, the Go helper below is
// what gets corrected — it is a reconstruction of the crate's resampler, while
// the Rust path is the code that actually runs on a device.
const (
	parityDir      = "testdata/merc-sig-v2"
	parityCropFile = "crop.png"
	parityGolden   = "signature.bin"

	// shiftMax mirrors SHIFT_MAX in icons.rs. The signature is NOT built from
	// the whole inner crop: the inner crop is shrunk by this many SCREEN pixels
	// per side first, and the margin given up is the room the matcher slides
	// the cell over at match time. Detected cell rects land 1-3 px off per cell
	// (POE-207), which is what the margin is sized for.
	//
	// It also means the window is not a fixed 33x33 — that is only what a 39x39
	// inner crop at the live 0.974 scale comes to. The window is always derived
	// from the crop's own size, never written as a literal, because a fixture
	// captured at another scale would then silently be cropped wrong.
	shiftMax = 3
)

// TestParity_TheCommittedCropReducesToTheCommittedSignature is the format's
// end-to-end pin: crop in, shift window out, 1728 bytes, byte for byte.
//
// It catches what the synthetic tests cannot — that the shift window, the disc,
// the badge corner and the RGB byte order are applied to REAL art in the same
// places the desktop applies them. A one-pixel change to the disc radius, a
// window taken from the wrong offset, a swapped R and B, a row-major slip: each
// changes these bytes and none of them changes any other test in this package.
func TestParity_TheCommittedCropReducesToTheCommittedSignature(t *testing.T) {
	sig := signatureFromParityCrop(t)
	got := sig.Bytes()

	if len(got) != SigBytes {
		t.Fatalf("derived signature is %d bytes, want %d", len(got), SigBytes)
	}
	// Folded in here rather than given its own test: a signature whose divisor
	// is not 657 correlates to exactly 0.0 against every peer, so the golden
	// carrying the wrong one would be a silent non-match everywhere.
	if sig.Active() != 657 {
		t.Fatalf("derived signature Active() = %d, want 657", sig.Active())
	}

	goldenPath := filepath.Join(parityDir, parityGolden)
	want, err := os.ReadFile(goldenPath)
	if err != nil {
		t.Fatalf("read golden %s (the Rust parity test in icons.rs writes it): %v", goldenPath, err)
	}
	if len(want) != SigBytes {
		t.Fatalf("golden %s is %d bytes, want %d — it is not a format-2 signature",
			goldenPath, len(want), SigBytes)
	}
	if !bytes.Equal(got, want) {
		first := -1
		for i := range got {
			if got[i] != want[i] {
				first = i
				break
			}
		}
		position, channel := first/SigChannels, first%SigChannels
		x, y := position%SigDim, position/SigDim
		t.Fatalf("derived signature differs from %s: first difference at byte %d "+
			"(position (%d,%d), channel %d, kept=%v): got %d, want %d",
			goldenPath, first, x, y, channel, keptByFormula(x, y), got[first], want[first])
	}
}

// signatureFromParityCrop runs the crop through the whole derivation: decode,
// take the shift window, resize to SigDim, build the signature.
func signatureFromParityCrop(t *testing.T) Signature {
	t.Helper()
	window := shiftWindow(t, readParityCrop(t))
	sig, err := NewSignature(resizeTriangleToSigDim(t, window))
	if err != nil {
		t.Fatalf("NewSignature from %s: %v", parityCropFile, err)
	}
	return sig
}

// rgbImage is a plain 8-bit RGB buffer: width x height positions, three bytes
// each, row-major — the same layout a signature uses.
type rgbImage struct {
	w, h int
	px   []byte
}

func (i rgbImage) at(x, y, c int) byte { return i.px[(y*i.w+x)*3+c] }

// shiftWindow is icons.rs::shift_window: the inner crop shrunk by shiftMax on
// every side. The result's size is derived from the input's, so a fixture
// recaptured at another display scale still yields the right window.
func shiftWindow(t *testing.T, src rgbImage) rgbImage {
	t.Helper()
	w, h := src.w-2*shiftMax, src.h-2*shiftMax
	if w < SigDim || h < SigDim {
		t.Fatalf("crop %dx%d leaves no room for the +/-%d px alignment window", src.w, src.h, shiftMax)
	}

	out := rgbImage{w: w, h: h, px: make([]byte, w*h*3)}
	for y := 0; y < h; y++ {
		for x := 0; x < w; x++ {
			for c := 0; c < 3; c++ {
				out.px[(y*w+x)*3+c] = src.at(x+shiftMax, y+shiftMax, c)
			}
		}
	}
	return out
}

// readParityCrop decodes crop.png into RGB.
//
// The desktop calls `.to_rgb8()` on the cropped window BEFORE resizing, so
// alpha is dropped, never premultiplied — dropping it here matches. That is
// only lossless while the fixture is fully opaque, which is asserted rather
// than assumed: a crop carrying real transparency would mean the two sides are
// reducing different pixels, and this is where that surfaces.
func readParityCrop(t *testing.T) rgbImage {
	t.Helper()
	f, err := os.Open(filepath.Join(parityDir, parityCropFile))
	if err != nil {
		t.Fatalf("open crop: %v", err)
	}
	defer f.Close()

	decoded, err := png.Decode(f)
	if err != nil {
		t.Fatalf("decode crop: %v", err)
	}
	nrgba, ok := decoded.(*image.NRGBA)
	if !ok {
		t.Fatalf("crop decoded as %T, want *image.NRGBA (an 8-bit RGBA PNG)", decoded)
	}

	b := nrgba.Bounds()
	if b.Dx() != 39 || b.Dy() != 39 {
		t.Fatalf("crop is %dx%d, want 39x39 (the inner rect at cell_inset 2)", b.Dx(), b.Dy())
	}

	out := rgbImage{w: b.Dx(), h: b.Dy(), px: make([]byte, b.Dx()*b.Dy()*3)}
	for y := 0; y < out.h; y++ {
		for x := 0; x < out.w; x++ {
			src := nrgba.PixOffset(b.Min.X+x, b.Min.Y+y)
			if a := nrgba.Pix[src+3]; a != 255 {
				t.Fatalf("crop pixel (%d,%d) has alpha %d — the fixture must be fully opaque", x, y, a)
			}
			copy(out.px[(y*out.w+x)*3:], nrgba.Pix[src:src+3])
		}
	}
	return out
}

// resizeTriangleToSigDim reduces the shift window to SigDim x SigDim RGB.
//
// This reproduces `image::imageops::resize(.., FilterType::Triangle)` — the
// resampler icons.rs uses — rather than inventing one: a separable tent filter
// whose support is scaled by the downsampling ratio, applied vertically into a
// float intermediate and then horizontally with a round-and-clamp back to u8.
// A plain four-tap bilinear would NOT match: at this downsampling ratio each
// output sample draws on about three input samples per axis, and a two-tap
// filter would throw most of them away.
//
// It lives in the test, not in the package: the server never resizes anything.
// It exists only so the committed golden can be checked against the same
// arithmetic the desktop performs, and golang.org/x/image/draw — whose
// ApproxBiLinear would have been the obvious shortcut — is not a dependency of
// this module and is not worth becoming one for a test helper.
func resizeTriangleToSigDim(t *testing.T, src rgbImage) []byte {
	t.Helper()

	// Vertical pass: rows collapse to SigDim, kept in float64.
	tmp := make([]float64, src.w*SigDim*3)
	for out := 0; out < SigDim; out++ {
		taps := triangleTaps(src.h, SigDim, out)
		for x := 0; x < src.w; x++ {
			for c := 0; c < 3; c++ {
				var acc float64
				for _, tap := range taps {
					acc += tap.weight * float64(src.at(x, tap.index, c))
				}
				tmp[(out*src.w+x)*3+c] = acc
			}
		}
	}

	// Horizontal pass: columns collapse to SigDim, back to u8.
	out := make([]byte, SigBytes)
	for ox := 0; ox < SigDim; ox++ {
		taps := triangleTaps(src.w, SigDim, ox)
		for y := 0; y < SigDim; y++ {
			for c := 0; c < 3; c++ {
				var acc float64
				for _, tap := range taps {
					acc += tap.weight * tmp[(y*src.w+tap.index)*3+c]
				}
				out[(y*SigDim+ox)*3+c] = clampRound(acc)
			}
		}
	}
	return out
}

type triangleTap struct {
	index  int
	weight float64
}

// triangleTaps is the crate's per-output-sample weight computation: the source
// centre is (out + 0.5) * ratio, the support is the tent's radius scaled by the
// ratio when downsampling, and the weights are normalised to sum to 1 so a flat
// input stays flat.
func triangleTaps(oldLen, newLen, out int) []triangleTap {
	ratio := float64(oldLen) / float64(newLen)
	sratio := math.Max(ratio, 1.0)
	support := sratio // the triangle filter's support is 1.0, scaled

	centre := (float64(out) + 0.5) * ratio
	left := clampInt(int(math.Floor(centre-support)), 0, oldLen-1)
	right := clampInt(int(math.Ceil(centre+support)), left+1, oldLen)
	centre -= 0.5

	taps := make([]triangleTap, 0, right-left)
	sum := 0.0
	for i := left; i < right; i++ {
		w := triangleKernel((float64(i) - centre) / sratio)
		taps = append(taps, triangleTap{index: i, weight: w})
		sum += w
	}
	for i := range taps {
		taps[i].weight /= sum
	}
	return taps
}

func triangleKernel(x float64) float64 {
	if x = math.Abs(x); x < 1.0 {
		return 1.0 - x
	}
	return 0.0
}

func clampInt(v, lo, hi int) int {
	switch {
	case v < lo:
		return lo
	case v > hi:
		return hi
	default:
		return v
	}
}

// clampRound is the crate's FloatNearest conversion: clamp into range, then
// round to nearest (half away from zero), not truncate.
func clampRound(v float64) byte {
	switch {
	case v <= 0:
		return 0
	case v >= 255:
		return 255
	default:
		return byte(math.Round(v))
	}
}
