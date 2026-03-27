//! Multi-tier sliding-window rate limiter for the GGG trade API.
//!
//! Direct port of Go's internal/trade/ratelimiter.go.
//!
//! GGG reports rate limits via response headers:
//!   X-Rate-Limit-Rules: ip
//!   X-Rate-Limit-Ip: 5:10:60,15:60:120       (max:window_sec:penalty_sec per tier)
//!   X-Rate-Limit-Ip-State: 3:10:0,8:60:0     (current:window_sec:ban_sec per tier)
//!
//! The limiter maintains separate "search" and "fetch" pools (the two GGG endpoints
//! have independent rate limits). Each pool has multiple tiers with sliding windows.

use reqwest::header::HeaderMap;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// One rate-limit tier within a pool. GGG can report multiple tiers per rule
/// (e.g., "5:10:60,15:60:120" = 5 req/10s AND 15 req/60s). Each independently
/// gates requests.
struct Tier {
    max_hits: usize,
    window: Duration,
    _penalty: Duration,
    slots: Vec<Instant>,
}

/// Groups tiers that share a ceiling factor and latency padding.
/// Typical pools: "search" and "fetch".
struct Pool {
    tiers: Vec<Tier>,
    padding: Duration,
    ceiling_factor: f64,
}

/// Default ceiling factor — use only 65% of the reported budget.
/// Leaves headroom for clock desync between client and GGG servers.
const DEFAULT_CEILING_FACTOR: f64 = 0.65;

/// Extra padding added to computed wait times for network latency safety.
/// Awakened PoE Trade uses 2s; Go server uses 1s. We use 1s.
const DEFAULT_LATENCY_PADDING: Duration = Duration::from_secs(1);

pub struct TradeRateLimiter {
    pools: Mutex<HashMap<String, Pool>>,
}

impl TradeRateLimiter {
    /// Create a rate limiter with conservative defaults:
    /// - search: 1 request per 5 seconds
    /// - fetch: 1 request per 2 seconds
    ///
    /// These are replaced with real limits once GGG headers arrive.
    pub fn new() -> Self {
        let mut pools = HashMap::new();
        pools.insert(
            "search".to_string(),
            Pool {
                tiers: vec![Tier {
                    max_hits: 1,
                    window: Duration::from_secs(5),
                    _penalty: Duration::ZERO,
                    slots: Vec::new(),
                }],
                padding: DEFAULT_LATENCY_PADDING,
                ceiling_factor: DEFAULT_CEILING_FACTOR,
            },
        );
        pools.insert(
            "fetch".to_string(),
            Pool {
                tiers: vec![Tier {
                    max_hits: 1,
                    window: Duration::from_secs(2),
                    _penalty: Duration::ZERO,
                    slots: Vec::new(),
                }],
                padding: DEFAULT_LATENCY_PADDING,
                ceiling_factor: DEFAULT_CEILING_FACTOR,
            },
        );
        Self {
            pools: Mutex::new(pools),
        }
    }

    /// Stamp the current time into every tier of the named pool.
    /// Call immediately after a successful HTTP request.
    pub fn record(&self, pool_name: &str) {
        let mut pools = self.pools.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(pool) = pools.get_mut(pool_name) {
            let now = Instant::now();
            for tier in &mut pool.tiers {
                tier.slots.push(now);
            }
        }
    }

    /// How long the caller should sleep before the next request to this pool.
    /// Returns Duration::ZERO if all tiers have remaining budget.
    pub fn estimate_wait(&self, pool_name: &str) -> Duration {
        let mut pools = self.pools.lock().unwrap_or_else(|e| e.into_inner());
        let pool = match pools.get_mut(pool_name) {
            Some(p) => p,
            None => return Duration::ZERO,
        };

        let now = Instant::now();
        let mut max_wait = Duration::ZERO;

        for tier in &mut pool.tiers {
            purge_expired(&mut tier.slots, tier.window, now);

            let effective_max = ((tier.max_hits as f64) * pool.ceiling_factor).max(1.0) as usize;

            if tier.slots.len() >= effective_max {
                // Oldest slot determines when budget reopens.
                let reopens_at = tier.slots[0] + tier.window + pool.padding;
                if reopens_at > now {
                    let wait = reopens_at - now;
                    if wait > max_wait {
                        max_wait = wait;
                    }
                }
            }
        }

        max_wait
    }

    /// Wait until the rate limit allows the next request to this pool.
    pub async fn wait_for_capacity(&self, pool_name: &str) {
        let wait = self.estimate_wait(pool_name);
        if !wait.is_zero() {
            log::info!(
                "Rate limiter: waiting {:?} for {} pool capacity",
                wait,
                pool_name
            );
            tokio::time::sleep(wait).await;
        }
    }

    /// Update pool tier definitions and slot counts from GGG response headers.
    ///
    /// Parses X-Rate-Limit-Rules, X-Rate-Limit-{Rule}, X-Rate-Limit-{Rule}-State.
    /// Rebuilds tiers if server-reported limits differ from current ones.
    /// Injects phantom slots when server reports more hits than we track locally.
    pub fn sync_from_response_headers(&self, pool_name: &str, headers: &HeaderMap) {
        let rules_header = match headers.get("x-rate-limit-rules") {
            Some(val) => match val.to_str() {
                Ok(s) => s.to_string(),
                Err(e) => {
                    log::warn!("Rate limiter: non-UTF8 x-rate-limit-rules header: {}", e);
                    return;
                }
            },
            None => return, // No rate limit headers — normal for some responses
        };

        let mut pools = self.pools.lock().unwrap_or_else(|e| e.into_inner());
        let pool = match pools.get_mut(pool_name) {
            Some(p) => p,
            None => {
                log::warn!("Rate limiter: unknown pool '{}', skipping sync", pool_name);
                return;
            }
        };

        for rule in rules_header.split(',') {
            let rule = rule.trim();
            if rule.is_empty() {
                continue;
            }

            let capitalized = capitalize(rule);
            let limit_key = format!("x-rate-limit-{}", capitalized.to_lowercase());
            let state_key = format!("x-rate-limit-{}-state", capitalized.to_lowercase());

            let limit_header = match headers.get(limit_key.as_str()).and_then(|v| v.to_str().ok()) {
                Some(s) => s.to_string(),
                None => {
                    log::warn!("Rate limiter: rule '{}' declared but {} header missing", rule, limit_key);
                    continue;
                }
            };
            let state_header = match headers.get(state_key.as_str()).and_then(|v| v.to_str().ok()) {
                Some(s) => s.to_string(),
                None => {
                    log::warn!("Rate limiter: rule '{}' declared but {} header missing", rule, state_key);
                    continue;
                }
            };

            let limit_parts: Vec<&str> = limit_header.split(',').collect();
            let state_parts: Vec<&str> = state_header.split(',').collect();
            if limit_parts.len() != state_parts.len() {
                continue;
            }

            let mut server_tiers: Vec<(TierDef, TierState)> = Vec::new();
            for (limit_str, state_str) in limit_parts.iter().zip(state_parts.iter()) {
                if let (Some(def), Some(state)) = (parse_tier_def(limit_str), parse_tier_state(state_str)) {
                    server_tiers.push((def, state));
                }
            }
            if server_tiers.is_empty() {
                continue;
            }

            // Rebuild tier definitions if limits changed.
            let changed = pool.tiers.len() != server_tiers.len()
                || pool.tiers.iter().zip(server_tiers.iter()).any(|(current, (def, _))| {
                    current.max_hits != def.max_hits || current.window != def.window
                });

            if changed {
                log::info!(
                    "Rate limiter: rebuilding {} pool tiers from server headers ({} tiers)",
                    pool_name,
                    server_tiers.len()
                );
                pool.tiers = server_tiers
                    .iter()
                    .map(|(def, _)| Tier {
                        max_hits: def.max_hits,
                        window: def.window,
                        _penalty: def.penalty,
                        slots: Vec::new(),
                    })
                    .collect();
            }

            // Inject phantom slots where server reports more hits than we track.
            let now = Instant::now();
            for (tier, (_, state)) in pool.tiers.iter_mut().zip(server_tiers.iter()) {
                purge_expired(&mut tier.slots, tier.window, now);

                if state.current_hits > tier.slots.len() {
                    let deficit = state.current_hits - tier.slots.len();
                    inject_phantoms(&mut tier.slots, deficit, tier.window, now);
                }
            }

            // Only process first matching rule.
            return;
        }
    }
}

// ---------------------------------------------------------------------------
// Parsing helpers
// ---------------------------------------------------------------------------

struct TierDef {
    max_hits: usize,
    window: Duration,
    penalty: Duration,
}

struct TierState {
    current_hits: usize,
}

/// Parse "5:10:60" → max_hits=5, window=10s, penalty=60s
fn parse_tier_def(s: &str) -> Option<TierDef> {
    let parts: Vec<&str> = s.trim().split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    let max_hits: usize = parts[0].parse().ok().filter(|&v| v > 0)?;
    let window_secs: u64 = parts[1].parse().ok().filter(|&v| v > 0)?;
    let penalty_secs: u64 = parts[2].parse().ok()?;
    Some(TierDef {
        max_hits,
        window: Duration::from_secs(window_secs),
        penalty: Duration::from_secs(penalty_secs),
    })
}

/// Parse "3:10:0" → current_hits=3
fn parse_tier_state(s: &str) -> Option<TierState> {
    let parts: Vec<&str> = s.trim().split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    let hits: usize = parts[0].parse().ok()?;
    Some(TierState {
        current_hits: hits,
    })
}

/// Remove slots older than the tier's window.
fn purge_expired(slots: &mut Vec<Instant>, window: Duration, now: Instant) {
    slots.retain(|&slot| now.duration_since(slot) < window);
}

/// Add phantom timestamps spread across the window to match server-reported usage.
fn inject_phantoms(slots: &mut Vec<Instant>, deficit: usize, window: Duration, now: Instant) {
    if deficit == 0 {
        return;
    }
    let step = window / (deficit as u32 + 1);
    for i in 1..=deficit {
        let phantom = now - window + step * (i as u32);
        slots.push(phantom);
    }
    slots.sort();
}

/// Capitalize first letter: "account" → "Account" (for header name matching).
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}
