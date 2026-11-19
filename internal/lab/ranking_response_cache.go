package lab

import (
	"context"
)

// RankingInput names the tables a ranking answer reads; a write to one
// invalidates every answer that reads it.
type RankingInput uint8

const (
	RankingInputTransfigure  RankingInput = 1 << iota // transfigure_results
	RankingInputGemSignals                            // gem_signals
	RankingInputGemFeatures                           // gem_features
	RankingInputGemSnapshots                          // gem_snapshots
	RankingInputDedication                            // dedication_snapshots
)

const (
	rankingAnswerMaxEntries = 256
	rankingAnswerMaxBytes   = 16 << 20
)

// RankingAnswers is the ranked-answer cache (skeleton).
type RankingAnswers struct {
	maxEntries int
	maxBytes   int64
}

func newRankingAnswers(maxEntries int, maxBytes int64) *RankingAnswers {
	return &RankingAnswers{maxEntries: maxEntries, maxBytes: maxBytes}
}

func (rc *RankingAnswers) NormalPage(ctx context.Context, q CollectiveQuery, load func(context.Context) (CollectivePage, error)) (CollectivePage, error) {
	return load(ctx)
}

func (rc *RankingAnswers) DedicationPage(ctx context.Context, q DedicationCollectiveQuery, load func(context.Context) (CollectivePage, error)) (CollectivePage, error) {
	return load(ctx)
}

func (rc *RankingAnswers) BasePrices(ctx context.Context, bases []string, load func(context.Context, []string) (map[string]float64, error)) (map[string]float64, error) {
	return load(ctx, bases)
}

func (rc *RankingAnswers) Commit(inputs RankingInput, write func() error) error {
	return write()
}

func (rc *RankingAnswers) Publish(inputs RankingInput) {}

// waiting reports how many callers wait on in-flight loads whose key starts
// with prefix.
func (rc *RankingAnswers) waiting(prefix string) int { return 0 }

const basePriceKeyPrefix = "b|"

func rankingPageBytes(p CollectivePage) int64 { return 0 }

const (
	normalKeyPrefix     = "n|"
	dedicationKeyPrefix = "d|"
)
