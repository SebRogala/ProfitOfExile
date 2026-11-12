package lab

import (
	"reflect"
	"testing"
)

func transfigureNames(rs []TransfigureResult) []string {
	out := make([]string, 0, len(rs))
	for _, r := range rs {
		out = append(out, r.TransfiguredName+"@"+r.Variant)
	}
	return out
}

func gemSignalNames(ss []GemSignal) []string {
	out := make([]string, 0, len(ss))
	for _, s := range ss {
		out = append(out, s.Name)
	}
	return out
}

func TestFilterTransfigure_KeepsOnlyTheRequestedVariant(t *testing.T) {
	all := []TransfigureResult{
		{TransfiguredName: "A", Variant: "20/20", ROI: 10},
		{TransfiguredName: "C", Variant: "1/20", ROI: 10},
	}
	got := transfigureNames(FilterTransfigure(all, "20/20", 10))
	want := []string{"A@20/20"}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("FilterTransfigure = %v, want %v", got, want)
	}
}

func TestFilterTransfigure_OrdersByROIDescending(t *testing.T) {
	all := []TransfigureResult{
		{TransfiguredName: "A", Variant: "20/20", ROI: 10},
		{TransfiguredName: "B", Variant: "20/20", ROI: 40},
		{TransfiguredName: "D", Variant: "20/20", ROI: 25},
	}
	got := transfigureNames(FilterTransfigure(all, "20/20", 10))
	want := []string{"B@20/20", "D@20/20", "A@20/20"}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("FilterTransfigure = %v, want %v", got, want)
	}
}

func TestFilterTransfigure_CapsResultsAtLimit(t *testing.T) {
	all := []TransfigureResult{
		{TransfiguredName: "B", Variant: "20/20", ROI: 40},
		{TransfiguredName: "D", Variant: "20/20", ROI: 25},
		{TransfiguredName: "A", Variant: "20/20", ROI: 10},
	}
	got := transfigureNames(FilterTransfigure(all, "20/20", 2))
	want := []string{"B@20/20", "D@20/20"}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("FilterTransfigure = %v, want %v", got, want)
	}
}

func TestFilterTransfigure_EmptyVariantKeepsEveryVariant(t *testing.T) {
	all := []TransfigureResult{
		{TransfiguredName: "A", Variant: "20/20", ROI: 10},
		{TransfiguredName: "C", Variant: "1/20", ROI: 99},
		{TransfiguredName: "E", Variant: "20", ROI: 5},
	}
	got := transfigureNames(FilterTransfigure(all, "", 10))
	want := []string{"C@1/20", "A@20/20", "E@20"}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("FilterTransfigure = %v, want %v", got, want)
	}
}

func TestFilterGemSignals_OrdersByConfidenceThenSellability(t *testing.T) {
	all := []GemSignal{
		{Name: "low", Variant: "20/20", Confidence: 50, Sellability: 99},
		{Name: "highLessSellable", Variant: "20/20", Confidence: 80, Sellability: 40},
		{Name: "highMoreSellable", Variant: "20/20", Confidence: 80, Sellability: 70},
	}
	got := gemSignalNames(FilterGemSignals(all, "20/20", "", 10))
	want := []string{"highMoreSellable", "highLessSellable", "low"}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("FilterGemSignals = %v, want %v", got, want)
	}
}

func TestFilterGemSignals_TierFilterExcludesOtherTiers(t *testing.T) {
	all := []GemSignal{
		{Name: "mid", Variant: "20/20", Tier: "MID", Confidence: 50},
		{Name: "top", Variant: "20/20", Tier: "TOP", Confidence: 90},
		{Name: "mid2", Variant: "20/20", Tier: "MID", Confidence: 40},
	}
	got := gemSignalNames(FilterGemSignals(all, "", "MID", 10))
	want := []string{"mid", "mid2"}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("FilterGemSignals = %v, want %v", got, want)
	}
}
