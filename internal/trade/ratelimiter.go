package trade

import (
	"errors"
	"log/slog"
	"net/http"
	"strconv"
	"strings"
	"sync"
	"time"
)

// These are conservative server policy, not GGG-published thresholds: the
// invalid-request research gives no numeric threshold or window (POE-297).
const (
	invalidRequestThreshold = 5
	invalidRequestWindow    = 10 * time.Minute
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
	mu              sync.Mutex
	pools           map[string]*Pool
	blockedUntil    time.Time
	invalidRequests []time.Time
	now             func() time.Time
}

// NewRateLimiter creates a RateLimiter with conservative default pools derived
// from the provided config. The "search" pool defaults to DefaultSearchRate
// requests per 5 seconds, and "fetch" defaults to DefaultFetchRate per 2 seconds.
func NewRateLimiter(cfg TradeConfig) *RateLimiter {
	return &RateLimiter{
		now: time.Now,
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
// immediately before sending an HTTP request, including requests that fail.
func (rl *RateLimiter) Record(poolName string) {
	rl.mu.Lock()
	defer rl.mu.Unlock()

	pool, ok := rl.pools[poolName]
	if !ok {
		return
	}

	now := rl.now()
	for i := range pool.tiers {
		pool.tiers[i].slots = append(pool.tiers[i].slots, now)
	}
}

// EstimateWait returns how long the caller should sleep before sending the next
// request to the named pool. Returns 0 if all tiers have remaining budget.
//
// For each tier, expired slots are purged first. If the active slot count has
// reached the effective ceiling (maxHits * ceilingFactor), the wait is computed
// as the time until enough slots expire, including latency padding.
func (rl *RateLimiter) EstimateWait(poolName string) time.Duration {
	rl.mu.Lock()
	defer rl.mu.Unlock()

	pool, ok := rl.pools[poolName]
	if !ok {
		return 0
	}

	now := rl.now()
	maxWait := rl.blockedUntil.Sub(now)
	if maxWait < 0 {
		maxWait = 0
	}

	for i := range pool.tiers {
		tier := &pool.tiers[i]

		// Purge expired slots.
		tier.slots = purgeExpired(tier.slots, tier.window+pool.padding, now)

		effectiveMax := int(float64(tier.maxHits) * pool.ceiling)
		if effectiveMax < 1 {
			effectiveMax = 1
		}

		if len(tier.slots) >= effectiveMax {
			// Enough slots must expire to leave room below the ceiling.
			wait := tier.slots[len(tier.slots)-effectiveMax].Add(tier.window).Add(pool.padding).Sub(now)
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

// BlockedUntil returns the active global restriction deadline, or zero when
// unrestricted. Endpoint budget waits are separate from this restriction.
func (rl *RateLimiter) BlockedUntil() time.Time {
	rl.mu.Lock()
	defer rl.mu.Unlock()
	if !rl.blockedUntil.After(rl.now()) {
		return time.Time{}
	}
	return rl.blockedUntil
}

// restrictionDeadlineAfter retains a restriction overlapping a caller's queued
// lifetime even if its timeout callback runs after the restriction expires.
// Deadlines at or before the caller's submission do not defer that caller.
func (rl *RateLimiter) restrictionDeadlineAfter(since time.Time) time.Time {
	rl.mu.Lock()
	defer rl.mu.Unlock()
	if rl.blockedUntil.After(since) {
		return rl.blockedUntil
	}
	return time.Time{}
}

// SyncFromHeaders updates all applicable rule tiers and honours restriction
// headers, including restrictions reported without usable tier definitions.
func (rl *RateLimiter) SyncFromHeaders(poolName string, headers http.Header) {
	rl.ObserveResponse(poolName, headers, nil)
}

// ObserveResponse consumes headers even on unsuccessful requests. Restrictions
// apply to the whole limiter: search and fetch originate from the same IP.
// Headerless 401/403/429 use a bounded precautionary fallback. Other 4xx
// are request-specific unless five headerless non-429 4xx arrive within ten
// minutes. Parsed upstream restrictions are honoured on every response.
func (rl *RateLimiter) ObserveResponse(poolName string, headers http.Header, responseErr error) {
	rl.mu.Lock()
	defer rl.mu.Unlock()

	now := rl.now()
	until := retryAfterUntil(headers.Get("Retry-After"), now)
	for name, values := range headers {
		name = strings.ToLower(name)
		if !strings.HasPrefix(name, "x-rate-limit-") || !strings.HasSuffix(name, "-state") {
			continue
		}
		for _, value := range values {
			for _, part := range strings.Split(value, ",") {
				state := parseTierState(part)
				if state != nil && state.banTime > 0 && now.Add(state.banTime).After(until) {
					until = now.Add(state.banTime)
				}
			}
		}
	}

	pool, ok := rl.pools[poolName]
	if ok {
		serverTiers := make([]parsedTier, 0)
		for _, rule := range strings.Split(headers.Get("X-Rate-Limit-Rules"), ",") {
			rule = strings.TrimSpace(rule)
			if rule == "" {
				continue
			}
			limits := strings.Split(headers.Get("X-Rate-Limit-"+rule), ",")
			states := strings.Split(headers.Get("X-Rate-Limit-"+rule+"-State"), ",")
			if len(limits) != len(states) {
				continue
			}
			for i := range limits {
				def, state := parseTierDef(limits[i]), parseTierState(states[i])
				if def != nil && state != nil && def.window == state.window {
					serverTiers = append(serverTiers, parsedTier{def: *def, state: *state})
				}
			}
		}
		if len(serverTiers) > 0 {
			if tiersChanged(pool.tiers, serverTiers) {
				pool.tiers = make([]Tier, len(serverTiers))
				for i, st := range serverTiers {
					pool.tiers[i] = Tier{maxHits: st.def.maxHits, window: st.def.window, penalty: st.def.penalty}
				}
			}
			for i, st := range serverTiers {
				tier := &pool.tiers[i]
				tier.slots = purgeExpired(tier.slots, tier.window+pool.padding, now)
				if deficit := st.state.currentHits - len(tier.slots); deficit > 0 {
					tier.slots = injectPhantoms(tier.slots, deficit, tier.window, now)
				}
			}
		}
	}

	invalidFlood := false
	if !until.After(now) && isInvalidRequestError(responseErr) && !errors.Is(responseErr, ErrRateLimited) {
		// The sliding window is (now-W, now], so an event exactly W old no
		// longer contributes. Keep only the last K events; older ones cannot
		// affect whether at least K invalid requests remain in the window.
		rl.invalidRequests = purgeExpired(rl.invalidRequests, invalidRequestWindow, now)
		rl.invalidRequests = append(rl.invalidRequests, now)
		if len(rl.invalidRequests) >= invalidRequestThreshold {
			invalidFlood = true
			rl.invalidRequests = rl.invalidRequests[len(rl.invalidRequests)-invalidRequestThreshold:]
		}
	}
	if (isGlobalRestrictionError(responseErr) || invalidFlood) && !until.After(now) {
		fallback := time.Minute
		if ok {
			for _, tier := range pool.tiers {
				if tier.penalty > fallback {
					fallback = tier.penalty
				}
				if tier.window > fallback {
					fallback = tier.window
				}
			}
		}
		if fallback > 30*time.Minute {
			fallback = 30 * time.Minute
		}
		until = now.Add(fallback)
	}
	if until.After(now) && until.After(rl.blockedUntil) {
		rl.blockedUntil = until
		slog.Warn("trade limiter: send pause", "blockedUntil", until, "wait", until.Sub(now))
	}
}

// retryAfterUntil accepts the HTTP delta-seconds and HTTP-date forms. Invalid,
// negative and overflowing values supply no deadline; precautionary pauses
// and invalid-request floods then use the bounded fallback.
func retryAfterUntil(value string, now time.Time) time.Time {
	value = strings.TrimSpace(value)
	if seconds, err := strconv.ParseInt(value, 10, 64); err == nil {
		if seconds > 0 && seconds <= int64((1<<63-1)/time.Second) {
			return now.Add(time.Duration(seconds) * time.Second)
		}
		return time.Time{}
	}
	if until, err := http.ParseTime(value); err == nil && until.After(now) {
		return until
	}
	return time.Time{}
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
	if err != nil || windowSec <= 0 || int64(windowSec) > int64((1<<63-1)/time.Second) {
		return nil
	}
	penaltySec, err := strconv.Atoi(parts[2])
	if err != nil || penaltySec < 0 || int64(penaltySec) > int64((1<<63-1)/time.Second) {
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
	if err != nil || hits < 0 {
		return nil
	}
	windowSec, err := strconv.Atoi(parts[1])
	if err != nil || windowSec <= 0 || int64(windowSec) > int64((1<<63-1)/time.Second) {
		return nil
	}
	banSec, err := strconv.Atoi(parts[2])
	if err != nil || banSec < 0 || int64(banSec) > int64((1<<63-1)/time.Second) {
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
			current[i].window != server[i].def.window ||
			current[i].penalty != server[i].def.penalty {
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
	for i < len(slots) && !slots[i].After(cutoff) {
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
