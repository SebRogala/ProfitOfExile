package lab

import (
	"bytes"
	"compress/gzip"
	"crypto/sha256"
	"encoding/csv"
	"fmt"
	"io"
	"math"
	"os"
	"sort"
	"strconv"
	"strings"
	"testing"
	"time"

	"profitofexile/internal/price/evidence"
	"profitofexile/internal/price/tier"
)

type tierReplaySnapshot struct {
	at   time.Time
	gems []GemPrice
}

type tierReplayPoint struct {
	at                time.Time
	raw, trailing     float64
	state, gatedState string
}

type tierReplayHighStats struct {
	previous              float64
	at                    time.Time
	pairs, gaps, lag, run int
	longest               int
	largest, largestGap   float64
}

// keep:critical — silent: flat-market gems receive plausible changed tiers
// from a boundary jump that downstream recommendations trust.
func TestKeepCritical_CovensBWindowReplayMovesHighAtMostTheMarginPerSnapshot(t *testing.T) {
	snapshots := loadTierReplay(t)
	previous := GemClassificationMap(nil)
	previousAt := time.Time{}
	previousBounds := make(map[string]tier.Boundaries)
	highStats := make(map[string]*tierReplayHighStats)
	histories := make(map[GemClassificationKey][]PricePoint)
	measurements := make(map[GemClassificationKey][]tierReplayPoint)
	var priorEvidence []evidencePoint
	start := time.Date(2026, 11, 20, 9, 0, 0, 0, time.UTC)
	end := time.Date(2026, 11, 23, 9, 0, 0, 0, time.UTC)
	logTierReplayRow(t, []string{"time", "name", "variant", "present", "tier", "low_confidence"})
	for _, snapshot := range snapshots {
		var history []GemPriceHistory
		for _, g := range snapshot.gems {
			key := GemClassificationKey{g.Name, g.Variant}
			if isFontOutcome(g) && g.Chaos > labTierCutoffChaos {
				points := append(histories[key], PricePoint{Time: snapshot.at, Chaos: g.Chaos, Listings: g.Listings})
				if len(points) > labTrailingSnapshots {
					points = points[len(points)-labTrailingSnapshots:]
				}
				histories[key] = points
			}
			history = append(history, GemPriceHistory{Name: g.Name, Variant: g.Variant, Points: histories[key]})
		}
		held := deriveEvidenceHolds(snapshot.gems, priorEvidence, snapshot.at)
		inputs := deriveTierInputs(held, history, previous, previousAt, !previousAt.IsZero(), previousBounds, snapshot.at)
		result := ComputeGemClassification(inputs)
		unclamped := append([]GemPrice(nil), inputs...)
		for i := range unclamped {
			unclamped[i].prevBoundaries = tier.Boundaries{}
		}
		rawResult := ComputeGemClassification(unclamped)
		for variant, b := range result.Boundaries {
			high, found := tierReplayHigh(b)
			if !found {
				continue
			}
			stats := highStats[variant]
			if stats == nil {
				stats = &tierReplayHighStats{}
				highStats[variant] = stats
			}
			if stats.previous > 0 {
				step := math.Abs(high/stats.previous - 1)
				stats.pairs++
				stats.largest = math.Max(stats.largest, step)
				if snapshot.at.Sub(stats.at) > labSnapshotGapMax {
					stats.gaps++
					stats.largestGap = math.Max(stats.largestGap, step)
				}
				if step > tier.MaxHighStep+1e-12 {
					t.Errorf("%s %s HIGH %g -> %g after %s: relative step %.12g exceeds MaxHighStep %g", snapshot.at.Format(time.RFC3339Nano), variant, stats.previous, high, snapshot.at.Sub(stats.at), step, tier.MaxHighStep)
				}
			}
			raw, ok := tierReplayHigh(rawResult.Boundaries[variant])
			if ok && math.Abs(high-raw) > 1e-10*math.Max(1, math.Abs(raw)) {
				stats.lag++
				stats.run++
				stats.longest = max(stats.longest, stats.run)
			} else {
				stats.run = 0
			}
			stats.previous, stats.at = high, snapshot.at
			previousBounds[variant] = b
		}
		for variant, stats := range highStats {
			if _, ok := tierReplayHigh(result.Boundaries[variant]); !ok {
				stats.run = 0
			}
		}
		for i, input := range inputs {
			key := GemClassificationKey{input.Name, input.Variant}
			classification, present := result.Gems[key]
			logTierReplayRow(t, []string{snapshot.at.UTC().Format(time.RFC3339Nano), key.Name, key.Variant, strconv.FormatBool(present), classification.Tier, strconv.FormatBool(classification.LowConfidence)})
			if !isFontOutcome(input) || snapshot.at.Before(start) || !snapshot.at.Before(end) || snapshot.gems[i].Chaos <= labTierCutoffChaos {
				continue
			}
			state := classification.Tier
			if !present {
				state = "ABSENT"
			}
			gatedState := state
			if evidence.Grade(gemDepth(input), GemEvidencePolicy()) != evidence.Solid {
				gatedState = "GATED"
			}
			price, _, _ := classificationInput(input)
			measurements[key] = append(measurements[key], tierReplayPoint{snapshot.at, snapshot.gems[i].Chaos, price, state, gatedState})
		}
		// Holds read strictly prior floor-SOLID rows; held values never seed holds.
		retained := priorEvidence[:0]
		for _, point := range priorEvidence {
			if snapshot.at.Sub(point.Time) <= GemEvidencePolicy().HoldWindow {
				retained = append(retained, point)
			}
		}
		priorEvidence = retained
		for _, g := range snapshot.gems {
			priorEvidence = append(priorEvidence, evidencePoint{Name: g.Name, Variant: g.Variant, Time: snapshot.at, Chaos: g.Chaos, Listings: g.Listings, SampleCount: g.SampleCount})
		}
		previous, previousAt = result.Gems, snapshot.at
	}
	for _, variant := range FontTradeVariants() {
		stats := highStats[variant]
		if stats == nil || stats.pairs == 0 {
			t.Errorf("%s: replay produced no consecutive named HIGH values", variant)
			continue
		}
		t.Logf("HIGH variant=%s pairs=%d max_step=%.9f gap_pairs=%d gap_max_step=%.9f clamp_lag_snapshots=%d longest_lag_run=%d", variant, stats.pairs, stats.largest, stats.gaps, stats.largestGap, stats.lag, stats.longest)
	}
	logTierReplayFlips(t, measurements)
}

func tierReplayHigh(b tier.Boundaries) (float64, bool) {
	for i, name := range b.Names {
		if name == tier.High && i < len(b.Boundaries) && b.Boundaries[i] > 0 {
			return b.Boundaries[i], true
		}
	}
	return 0, false
}

func logTierReplayRow(t *testing.T, row []string) {
	t.Helper()
	var buffer bytes.Buffer
	writer := csv.NewWriter(&buffer)
	if err := writer.Write(row); err != nil {
		t.Fatal(err)
	}
	writer.Flush()
	if err := writer.Error(); err != nil {
		t.Fatal(err)
	}
	t.Logf("POE406_ROW %s", strings.TrimSuffix(buffer.String(), "\n"))
}

func loadTierReplay(t *testing.T) []tierReplaySnapshot {
	t.Helper()
	compressed, err := os.ReadFile("testdata/replay/covens-2026-11-19-to-11-23-b-window.csv.gz")
	if err != nil {
		t.Fatal(err)
	}
	if got := fmt.Sprintf("%x", sha256.Sum256(compressed)); got != "7ce31fd1ac3ad127afc125392712ff1cf353a74504ec2213c18f302de6b05274" {
		t.Fatalf("mature fixture digest changed: %s", got)
	}
	stream, err := gzip.NewReader(bytes.NewReader(compressed))
	if err != nil {
		t.Fatal(err)
	}
	defer stream.Close()
	reader := csv.NewReader(stream)
	header, err := reader.Read()
	if err != nil || strings.Join(header, ",") != "time,name,variant,chaos,listings,is_transfigured,gem_color,is_corrupted,sample_count,sparkline_empty" {
		t.Fatalf("fixture header %v: %v", header, err)
	}
	parseInt := func(value string) int {
		parsed, err := strconv.Atoi(value)
		if err != nil {
			t.Fatalf("fixture integer %q: %v", value, err)
		}
		return parsed
	}
	parseBool := func(value string) bool {
		parsed, err := strconv.ParseBool(value)
		if err != nil {
			t.Fatalf("fixture boolean %q: %v", value, err)
		}
		return parsed
	}
	byTime := make(map[time.Time][]GemPrice)
	seen := make(map[string]bool)
	count := 0
	for {
		row, err := reader.Read()
		if err == io.EOF {
			break
		}
		if err != nil {
			t.Fatal(err)
		}
		at, err := time.Parse("2006-01-02 15:04:05.999999999Z07", row[0])
		if err != nil {
			t.Fatal(err)
		}
		price, err := strconv.ParseFloat(row[3], 64)
		if err != nil {
			t.Fatal(err)
		}
		key := strings.Join(row[:3], "\x00")
		if seen[key] {
			t.Fatalf("duplicate fixture key: %v", row[:3])
		}
		seen[key] = true
		g := GemPrice{Name: row[1], Variant: row[2], Chaos: price, Listings: parseInt(row[4]), IsTransfigured: parseBool(row[5]), GemColor: row[6], IsCorrupted: parseBool(row[7])}
		if row[8] != "" {
			sample := parseInt(row[8])
			g.SampleCount = &sample
		}
		if row[9] != "" {
			empty := parseBool(row[9])
			g.SparklineEmpty = &empty
		}
		byTime[at] = append(byTime[at], g)
		count++
	}
	if count != 154589 || len(byTime) != 193 {
		t.Fatalf("fixture has %d rows/%d snapshots, want 154589/193", count, len(byTime))
	}
	snapshots := make([]tierReplaySnapshot, 0, len(byTime))
	for at, gems := range byTime {
		snapshots = append(snapshots, tierReplaySnapshot{at, gems})
	}
	sort.Slice(snapshots, func(i, j int) bool { return snapshots[i].at.Before(snapshots[j].at) })
	return snapshots
}

func logTierReplayFlips(t *testing.T, measurements map[GemClassificationKey][]tierReplayPoint) {
	t.Helper()
	type stats struct {
		rates, withoutCutoff [2][]float64
		flips, cutoff        [2]int
		stable, changed      int
	}
	byVariant := make(map[string]*stats)
	for key, points := range measurements {
		if len(points) < 24 {
			continue
		}
		s := byVariant[key.Variant]
		if s == nil {
			s = &stats{}
			byVariant[key.Variant] = s
		}
		var flips, cutoff [2]int
		for i := 1; i < len(points); i++ {
			before, after := points[i-1], points[i]
			if after.at.Sub(before.at) > labSnapshotGapMax {
				continue
			}
			if math.Abs(after.raw/before.raw-1) < .05 {
				s.stable++
				if before.state != after.state {
					s.changed++
				}
			}
			for mode, changed := range []bool{before.state != after.state, before.gatedState != after.gatedState} {
				if changed {
					flips[mode]++
					if before.trailing <= labTierCutoffChaos || after.trailing <= labTierCutoffChaos {
						cutoff[mode]++
					}
				}
			}
		}
		days := float64(len(points)) / 48
		for mode := range flips {
			s.rates[mode] = append(s.rates[mode], float64(flips[mode])/days)
			s.withoutCutoff[mode] = append(s.withoutCutoff[mode], float64(flips[mode]-cutoff[mode])/days)
			s.flips[mode] += flips[mode]
			s.cutoff[mode] += cutoff[mode]
		}
	}
	for _, variant := range FontTradeVariants() {
		s := byVariant[variant]
		if s == nil || len(s.rates[0]) == 0 {
			t.Errorf("%s: no keys with at least 24 evaluated >5c points", variant)
			continue
		}
		for mode, name := range []string{"tier", "tier+gate"} {
			t.Logf("FLIPS variant=%s mode=%s keys=%d p95=%.6f p95_without_cutoff=%.6f flips=%d cutoff_flips=%d", variant, name, len(s.rates[mode]), tierReplayP95(s.rates[mode]), tierReplayP95(s.withoutCutoff[mode]), s.flips[mode], s.cutoff[mode])
		}
		t.Logf("STABLE variant=%s changed=%d pairs=%d share=%.6f", variant, s.changed, s.stable, float64(s.changed)/float64(max(1, s.stable)))
	}
}

func tierReplayP95(values []float64) float64 {
	sorted := append([]float64(nil), values...)
	sort.Float64s(sorted)
	index := .95 * float64(len(sorted)-1)
	lo := int(math.Floor(index))
	hi := int(math.Ceil(index))
	return sorted[lo] + (index-float64(lo))*(sorted[hi]-sorted[lo])
}
