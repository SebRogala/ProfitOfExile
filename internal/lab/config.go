package lab

// SignalConfig holds all tunable signal thresholds.
//
// Price velocity thresholds are percentages of current gem price (e.g., 8.0 = 8%).
// Listing velocity thresholds are percentages of current listing count (e.g., 15.0 = 15%).
// This makes signals tier-agnostic: a 3% price move means the same thing for a
// 1500c TOP gem and a 30c FLOOR gem.
type SignalConfig struct {
	// HERD thresholds (percentage-based + absolute floor)
	HERDPriceVelPct      float64 // standard HERD: min price velocity % (default: 8)
	HERDListingVelPct    float64 // standard HERD: min listing velocity % (default: 15)
	HERDListingAbsFloor  float64 // min absolute listing velocity for HERD (default: 5)
	PreHERDPriceVelPct   float64 // pre-HERD extreme: min price velocity % (default: 20)
	PreHERDListingVelPct float64 // pre-HERD: min listing velocity % (default: 5)

	// STABLE thresholds (percentage-based)
	StablePriceVelPct   float64 // max |priceVel%| for STABLE (default: 3)
	StableListingVelPct float64 // max |listingVel%| for STABLE (default: 5)

	// DUMPING/DEMAND/RECOVERY thresholds (percentage-based + absolute floor)
	DumpPriceVelPct       float64 // price velocity % for DUMPING (default: -8)
	DumpListingVelPct     float64 // listing velocity % for DUMPING (default: 10)
	DumpListingAbsFloor   float64 // min absolute listing velocity for DUMPING (default: 3)
	DemandListingVelPct   float64 // max listing velocity % for DEMAND — significant drain (default: -15)
	DemandListingAbsFloor float64 // min absolute listing drain for DEMAND (default: 5)
	DemandPriceVelPct     float64 // min price velocity % for DEMAND — price not crashing (default: -5)
	RecoveryListingVelPct float64 // min listing drain % for RECOVERY (default: -8)
	RecoveryMaxListings   int     // max absolute listings for RECOVERY — thin markets only (default: 20)

	// BREWING/WINDOW thresholds (absolute — these are base gem signals, low-value)
	BrewingMinPVel float64 // min price velocity for BREWING (default: 2)
	OpenMinPVel    float64 // min price velocity for OPEN (default: 2)
	DrainPct       float64 // base drain % per hour (default: 0.04)
	ThinPoolFloor  float64 // drain floor for baseLst<20 (default: -1.5)
	NormalFloor    float64 // drain floor for baseLst>=20 (default: -1.0)

	// BREAKOUT thresholds (absolute — targets LOW tier only)
	BreakoutMaxPrice float64 // max price for BREAKOUT (default: 200)
	BreakoutMaxList  int     // max listings for BREAKOUT (default: 30)
	BreakoutMinLVel  float64 // min listing velocity drop (default: -5)

	// TRAP threshold
	TrapCV          float64 // CV threshold for TRAP (default: 50)
	TrapVelPct      float64 // min |velocity%| to confirm TRAP (default: 5)
}

// DefaultSignalConfig returns production defaults.
func DefaultSignalConfig() SignalConfig {
	return SignalConfig{
		HERDPriceVelPct:      8,
		HERDListingVelPct:    15,
		HERDListingAbsFloor:  5,
		PreHERDPriceVelPct:   20,
		PreHERDListingVelPct: 5,
		StablePriceVelPct:    3,
		StableListingVelPct:  5,
		DumpPriceVelPct:      -8,
		DumpListingVelPct:    10,
		DumpListingAbsFloor:   3,
		DemandListingVelPct:   -15,
		DemandListingAbsFloor: 5,
		DemandPriceVelPct:     -8,
		RecoveryListingVelPct: -8,
		RecoveryMaxListings:   20,
		BrewingMinPVel:       2,
		OpenMinPVel:          2,
		DrainPct:             0.04,
		ThinPoolFloor:        -1.5,
		NormalFloor:          -1.0,
		BreakoutMaxPrice:     200,
		BreakoutMaxList:      30,
		BreakoutMinLVel:      -5,
		TrapCV:               50,
		TrapVelPct:           5,
	}
}
