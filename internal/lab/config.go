package lab

// SignalConfig holds all tunable signal thresholds.
// Default values match the current production settings.
type SignalConfig struct {
	// HERD thresholds
	HERDPriceVel      float64 // standard HERD price velocity (default: 5)
	HERDListingVel    float64 // standard HERD listing velocity (default: 10)
	PreHERDPriceVel   float64 // pre-HERD extreme price velocity (default: 30)
	PreHERDListingVel float64 // pre-HERD listing velocity (default: 3)

	// STABLE thresholds
	StablePriceVel   float64 // max |priceVel| for STABLE (default: 2)
	StableListingVel float64 // max |listingVel| for STABLE (default: 3)

	// DUMPING/RECOVERY thresholds
	DumpPriceVel    float64 // price velocity for DUMPING (default: -5)
	DumpListingVel  float64 // listing velocity for DUMPING (default: 5)
	RecoveryMaxList int     // max listings for RECOVERY (default: 20)
	RecoveryMaxPVel float64 // max |priceVel| for RECOVERY stabilization (default: 5)

	// BREWING/WINDOW thresholds
	BrewingMinPVel float64 // min price velocity for BREWING (default: 2)
	OpenMinPVel    float64 // min price velocity for OPEN (default: 2)
	DrainPct       float64 // base drain % per hour (default: 0.04)
	ThinPoolFloor  float64 // drain floor for baseLst<20 (default: -1.5)
	NormalFloor    float64 // drain floor for baseLst>=20 (default: -1.0)

	// Tier thresholds
	TierTopMult float64 // TOP = > wt10 * this (default: 0.70)
	TierMidMult float64 // MID = > wt10 * this (default: 0.20)

	// BREAKOUT thresholds
	BreakoutMaxPrice float64 // max price for BREAKOUT (default: 200)
	BreakoutMaxList  int     // max listings for BREAKOUT (default: 30)
	BreakoutMinLVel  float64 // min listing velocity drop (default: -5)

	// TRAP threshold
	TrapCV float64 // CV threshold for TRAP (default: 100)
}

// DefaultSignalConfig returns production defaults.
func DefaultSignalConfig() SignalConfig {
	return SignalConfig{
		HERDPriceVel:      5,
		HERDListingVel:    10,
		PreHERDPriceVel:   30,
		PreHERDListingVel: 3,
		StablePriceVel:    2,
		StableListingVel:  3,
		DumpPriceVel:      -5,
		DumpListingVel:    5,
		RecoveryMaxList:   20,
		RecoveryMaxPVel:   5,
		BrewingMinPVel:    2,
		OpenMinPVel:       2,
		DrainPct:          0.04,
		ThinPoolFloor:     -1.5,
		NormalFloor:       -1.0,
		TierTopMult:       0.70,
		TierMidMult:       0.20,
		BreakoutMaxPrice:  200,
		BreakoutMaxList:   30,
		BreakoutMinLVel:   -5,
		TrapCV:            100,
	}
}
