package lab

import (
	"testing"
	"time"
)

func sparkAt(now time.Time, ago time.Duration, price float64, listings int) SparklinePoint {
	return SparklinePoint{
		Time:     now.Add(-ago).UTC().Format(time.RFC3339),
		Price:    price,
		Listings: listings,
	}
}

func TestTrimSparkline(t *testing.T) {
	now := time.Now()
	old := sparkAt(now, 20*time.Hour, 60, 40)
	recent := sparkAt(now, time.Hour, 125, 26)

	tests := []struct {
		name  string
		pts   []SparklinePoint
		hours int
		want  []SparklinePoint
	}{
		{"keeps points inside the window", []SparklinePoint{old, recent}, 12, []SparklinePoint{recent}},
		{"drops everything older than the window", []SparklinePoint{old}, 12, nil},
		{"no window keeps the whole series", []SparklinePoint{old, recent}, 0, []SparklinePoint{old, recent}},
		{"unparseable times are skipped, not kept", []SparklinePoint{{Time: "not-a-time", Price: 1}, recent}, 12, []SparklinePoint{recent}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := TrimSparkline(tt.pts, tt.hours)
			if len(got) != len(tt.want) {
				t.Fatalf("got %d points %+v, want %d", len(got), got, len(tt.want))
			}
			for i := range tt.want {
				if got[i] != tt.want[i] {
					t.Errorf("point[%d] = %+v, want %+v", i, got[i], tt.want[i])
				}
			}
		})
	}
}
