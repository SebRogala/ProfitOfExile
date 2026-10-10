package lab

import (
	"reflect"
	"testing"
)

// The Font pipeline (detectLowConfidence, detectTops, computeCleanTierBoundaries)
// and the filtered pipeline run with isFontOutcome produce the same result on
// every corpus case, so the two can collapse into one. This file goes away with
// classifyWithFilter when they do.
func TestFontPipelineEqualsFilteredPipeline(t *testing.T) {
	for _, c := range loadClassificationCorpus(t) {
		font := ComputeGemClassification(c.Gems)
		filtered := classifyWithFilter(c.Gems, isFontOutcome)
		if !reflect.DeepEqual(font, filtered) {
			t.Errorf("case %q: Font pipeline differs from classifyWithFilter(isFontOutcome)\n font: %+v\nfiltered: %+v",
				c.Case, font, filtered)
		}
	}
}
