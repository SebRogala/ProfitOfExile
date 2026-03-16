package trade

import (
	"net/http"
	"strconv"
	"strings"
	"sync"
	"time"
)

// Tier represents one rate-limit tier within a pool. GGG's trade API can report
// multiple tiers per rule (e.g., "5:10:60,15:60:120" means 5 req/10s AND
// 15 req/60s). Each tier independently gates requests.
type Tier struct {
	maxHits int
	window  time.Duration
	penalty time.Duration
	slots   []time.Time
}

// Pool groups tiers that share a ceiling factor and latency padding. Typical
// pools are "search" and "fetch", corresponding to the two GGG trade endpoints.
type Pool struct {
	tiers   []Tier
	padding time.Duration
	ceiling float64
}

// RateLimiter enforces GGG's multi-tier sliding-window rate limits across
// independent pools. It starts with conservative defaults and refines its model
// via SyncFromHeaders when real X-Rate-Limit-* headers arrive.
type RateLimiter struct {
	mu    sync.Mutex
	pools map[string]*Pool
}

// NewRateLimiter creates a RateLimiter with conservative default pools derived
// from the provided config. The "search" pool defaults to DefaultSearchRate
// requests per 5 seconds, and "fetch" defaults to DefaultFetchRate per 2 seconds.
func NewRateLimiter(cfg TradeConfig) *RateLimiter {
	return &RateLimiter{
		pools: map[string]*Pool{
			"search": {
				tiers: []Tier{
					{maxHits: cfg.DefaultSearchRate, window: 5 * time.Second},
				},
				padding: cfg.LatencyPadding,
				ceiling: cfg.CeilingFactor,
			},
			"fetch": {
				tiers: []Tier{
					{maxHits: cfg.DefaultFetchRate, window: 2 * time.Second},
				},
				padding: cfg.LatencyPadding,
				ceiling: cfg.CeilingFactor,
			},
		},
	}
}

// Record stamps the current time into every tier in the named pool. Call this
// immediately after a successful HTTP request to that endpoint.
func (rl *RateLimiter) Record(poolName string) {
	rl.mu.Lock()
	defer rl.mu.Unlock()

	pool, ok := rl.pools[poolName]
	if !ok {
		return
	}

	now := time.Now()
	for i := range pool.tiers {
		pool.tiers[i].slots = append(pool.tiers[i].slots, now)
	}
}

// EstimateWait returns how long the caller should sleep before sending the next
// request to the named pool. Returns 0 if all tiers have remaining budget.
//
// For each tier, expired slots are purged first. If the active slot count has
// reached the effective ceiling (maxHits * ceilingFactor), the wait is computed
// as the time until the oldest slot expires plus the latency padding.
func (rl *RateLimiter) EstimateWait(poolName string) time.Duration {
	rl.mu.Lock()
	defer rl.mu.Unlock()

	pool, ok := rl.pools[poolName]
	if !ok {
		return 0
	}

	now := time.Now()
	var maxWait time.Duration

	for i := range pool.tiers {
		tier := &pool.tiers[i]

		// Purge expired slots.
		tier.slots = purgeExpired(tier.slots, tier.window, now)

		effectiveMax := int(float64(tier.maxHits) * pool.ceiling)
		if effectiveMax < 1 {
			effectiveMax = 1
		}

		if len(tier.slots) >= effectiveMax {
			// Oldest active slot determines when budget reopens.
			wait := tier.slots[0].Add(tier.window).Add(pool.padding).Sub(now)
			if wait > maxWait {
				maxWait = wait
			}
		}
	}

	if maxWait < 0 {
		return 0
	}
	return maxWait
}

// SyncFromHeaders updates the named pool's tier definitions and slot counts
// using GGG's X-Rate-Limit-* response headers. This is the primary mechanism
// for staying in sync with the server's actual rate-limit state.
//
// Expected headers (example for rule "account"):
//
//	X-Rate-Limit-Rules: account
//	X-Rate-Limit-Account: 5:10:60,15:60:120
//	X-Rate-Limit-Account-State: 3:10:0,8:60:0
//
// The method rebuilds tier definitions when server-reported limits differ from
// current ones, and injects phantom slots when the server reports more hits
// than we have tracked locally (e.g., after a restart or from another client
// sharing the same account).
func (rl *RateLimiter) SyncFromHeaders(poolName string, headers http.Header) {
	rules := headers.Get("X-Rate-Limit-Rules")
	if rules == "" {
		return
	}

	rl.mu.Lock()
	defer rl.mu.Unlock()

	pool, ok := rl.pools[poolName]
	if !ok {
		return
	}

	// GGG may report multiple comma-separated rule names; we take the first
	// one that has both a limit and state header present.
	for _, rule := range strings.Split(rules, ",") {
		rule = strings.TrimSpace(rule)
		if rule == "" {
			continue
		}

		limitHeader := headers.Get("X-Rate-Limit-" + capitalize(rule))
		stateHeader := headers.Get("X-Rate-Limit-" + capitalize(rule) + "-State")
		if limitHeader == "" || stateHeader == "" {
			continue
		}

		limitParts := strings.Split(limitHeader, ",")
		stateParts := strings.Split(stateHeader, ",")
		if len(limitParts) != len(stateParts) {
			continue
		}

		serverTiers := make([]parsedTier, 0, len(limitParts))
		for j := range limitParts {
			lt := parseTierDef(limitParts[j])
			st := parseTierState(stateParts[j])
			if lt == nil || st == nil {
				continue
			}
			serverTiers = append(serverTiers, parsedTier{def: *lt, state: *st})
		}
		if len(serverTiers) == 0 {
			continue
		}

		// Rebuild tier definitions if the server reports different limits.
		if tiersChanged(pool.tiers, serverTiers) {
			pool.tiers = make([]Tier, len(serverTiers))
			for j, st := range serverTiers {
				pool.tiers[j] = Tier{
					maxHits: st.def.maxHits,
					window:  st.def.window,
					penalty: st.def.penalty,
				}
			}
		}

		// Inject phantom slots where the server reports more hits than we track.
		now := time.Now()
		for j := range pool.tiers {
			tier := &pool.tiers[j]
			tier.slots = purgeExpired(tier.slots, tier.window, now)

			serverHits := serverTiers[j].state.currentHits
			if serverHits > len(tier.slots) {
				deficit := serverHits - len(tier.slots)
				tier.slots = injectPhantoms(tier.slots, deficit, tier.window, now)
			}
		}

		// Only process the first matching rule.
		return
	}
}

// --- internal helpers ---

// parsedTier holds parsed limit definition + current server state for one tier.
type parsedTier struct {
	def   tierDef
	state tierState
}

// tierDef is a parsed "maxHits:window:penalty" triple from X-Rate-Limit-{Rule}.
type tierDef struct {
	maxHits int
	window  time.Duration
	penalty time.Duration
}

// tierState is a parsed "currentHits:window:banTime" triple from X-Rate-Limit-{Rule}-State.
type tierState struct {
	currentHits int
	window      time.Duration
	banTime     time.Duration
}

// parseTierDef parses a single tier definition like "5:10:60" into maxHits=5,
// window=10s, penalty=60s.
func parseTierDef(s string) *tierDef {
	parts := strings.Split(strings.TrimSpace(s), ":")
	if len(parts) != 3 {
		return nil
	}
	maxHits, err := strconv.Atoi(parts[0])
	if err != nil || maxHits <= 0 {
		return nil
	}
	windowSec, err := strconv.Atoi(parts[1])
	if err != nil || windowSec <= 0 {
		return nil
	}
	penaltySec, err := strconv.Atoi(parts[2])
	if err != nil {
		return nil
	}
	return &tierDef{
		maxHits: maxHits,
		window:  time.Duration(windowSec) * time.Second,
		penalty: time.Duration(penaltySec) * time.Second,
	}
}

// parseTierState parses a single tier state like "3:10:0" into currentHits=3,
// window=10s, banTime=0s.
func parseTierState(s string) *tierState {
	parts := strings.Split(strings.TrimSpace(s), ":")
	if len(parts) != 3 {
		return nil
	}
	hits, err := strconv.Atoi(parts[0])
	if err != nil {
		return nil
	}
	windowSec, err := strconv.Atoi(parts[1])
	if err != nil {
		return nil
	}
	banSec, err := strconv.Atoi(parts[2])
	if err != nil {
		return nil
	}
	return &tierState{
		currentHits: hits,
		window:      time.Duration(windowSec) * time.Second,
		banTime:     time.Duration(banSec) * time.Second,
	}
}

// tiersChanged returns true if the server-reported tier definitions differ from
// the current pool tiers (in count, maxHits, or window).
func tiersChanged(current []Tier, server []parsedTier) bool {
	if len(current) != len(server) {
		return true
	}
	for i := range current {
		if current[i].maxHits != server[i].def.maxHits ||
			current[i].window != server[i].def.window {
			return true
		}
	}
	return false
}

// purgeExpired removes slots older than the tier's window from the front of
// the slice. Slots are append-only so they are already time-sorted.
func purgeExpired(slots []time.Time, window time.Duration, now time.Time) []time.Time {
	cutoff := now.Add(-window)
	i := 0
	for i < len(slots) && slots[i].Before(cutoff) {
		i++
	}
	if i == 0 {
		return slots
	}
	// Avoid memory leak from holding old backing array elements.
	remaining := make([]time.Time, len(slots)-i)
	copy(remaining, slots[i:])
	return remaining
}

// injectPhantoms adds deficit phantom timestamps evenly spread across the
// window so that EstimateWait sees realistic pressure. Phantoms are placed
// between the start of the window and now.
func injectPhantoms(slots []time.Time, deficit int, window time.Duration, now time.Time) []time.Time {
	if deficit <= 0 {
		return slots
	}
	windowStart := now.Add(-window)
	step := window / time.Duration(deficit+1)

	phantoms := make([]time.Time, 0, len(slots)+deficit)
	for i := 1; i <= deficit; i++ {
		phantoms = append(phantoms, windowStart.Add(step*time.Duration(i)))
	}

	// Merge phantoms with existing slots, keeping time order.
	merged := make([]time.Time, 0, len(phantoms)+len(slots))
	pi, si := 0, 0
	for pi < len(phantoms) && si < len(slots) {
		if phantoms[pi].Before(slots[si]) {
			merged = append(merged, phantoms[pi])
			pi++
		} else {
			merged = append(merged, slots[si])
			si++
		}
	}
	merged = append(merged, phantoms[pi:]...)
	merged = append(merged, slots[si:]...)
	return merged
}

// capitalize returns s with its first letter upper-cased (ASCII only). GGG
// header names use title-case rule names (e.g., "Account" not "account").
func capitalize(s string) string {
	if s == "" {
		return s
	}
	first := s[0]
	if first >= 'a' && first <= 'z' {
		first -= 32
	}
	return string(first) + s[1:]
}
