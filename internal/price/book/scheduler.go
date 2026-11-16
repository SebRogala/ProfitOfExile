package book

import (
	"time"

	"profitofexile/internal/price/resolve"
)

// Demand describes a current snapshot market and its potential Font exposure.
type Demand struct {
	Key                   resolve.Key
	SnapshotTime          time.Time
	Ninja                 resolve.Observation
	PotentialContribution float64
	PickProbability       float64
}
