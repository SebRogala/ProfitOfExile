//! Trade query for the mercenary AS CAPTURED, and the pure trigger policy that
//! decides whether to spend a search on it (POE-202).
//!
//! # The probe that fixed the query shape
//!
//! The row-group arithmetic was measured against the live API rather than
//! reasoned about, because the two candidate readings (count matched filters vs
//! count filled slots) produce the same query for an unambiguous read and
//! different ones for every ambiguous or tier-loosened cell. Verbatim from the
//! task's design summary:
//!
//! > **Probe result (2026-08-26, live Allflame, 4 spaced searches):** a
//! > `mercenary` group counts matched filters within one row with union
//! > semantics — `[KBoC, MP-T1, GMP-T3] min 2` = 7774 = `[KBoC, GMP-T3] min 2`
//! > (7774) + `[KBoC, MP-T1] min 2` (0); the same three ids with `min 3` = 0, so
//! > a row never holds two tiers of one family. Expanded ids stay inside the row
//! > group with `value.min = contributing cell count`. No sibling `count` group.
//! > Audit CONFIRM Q2 closed.
//!
//! Two consequences the builder leans on:
//!
//! - A cell that resolves to a SET of ids — a confident read (`Matched |
//!   Confirmed`) whose `ids.len() > 1`, or a tier loosened down to the floor —
//!   puts every id in the same row group and still counts **once** toward the
//!   minimum, because at most one of them can be on the row. That is why a
//!   multi-id read and tier loosening are one mechanism and not two.
//!   `ReadState::Ambiguous` is not that case: it is filtered out with every
//!   other non-confident state, and it cannot reach the `note_complete` edge
//!   anyway, because `read::capture_complete` (`read.rs:484-489`) only calls a
//!   capture complete when every skill and support cell is confident.
//! - One `mercenary` group per row, never an `and` group: `and` matches the
//!   skill of row 1 against the support of row 3 and comps a mercenary nobody
//!   has.
//!
//! # What is deliberately absent from the query
//!
//! `status.option = "securable"`, `sort.price = "asc"` and
//! `filters.trade_filters.filters.sale_type.option = "priced"` are set; nothing
//! else is. `sale_type` follows the gem path (`trade/query.rs:67-72`) and keeps
//! unpriced listings out of the floor and the median. The gem path's
//! `collapse` is NOT copied: collapsing per seller hides how deep a price level
//! actually is, which is the thing a capture comp is asking about. The guide
//! rulesets carry `ilvl.min 83`, but that is a guide's ruling about which
//! mercenaries are worth comping — the captured query asks "what does THIS
//! mercenary sell for", and an ilvl floor would drop the cheap half of its own
//! market out of the answer.

use std::collections::BTreeMap;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};

use crate::trade::{
    MercTradeListing, MercTradeResult, RawSearch, TradeQueueEvent, TradeSource, CANCELLED,
};
use crate::AppState;

use super::read::confident;
use super::run::{now_ms, publish};
use super::vocab::MercVocab;
use super::{MercCapture, MercRow};

/// Re-exported so the trigger policy, the builder and the slice type read as
/// one vocabulary. The definition lives in `mercenary/mod.rs` with the rest of
/// the slice's wire types.
pub use super::MercTradeStatus;

/// GGG rejects a search carrying more than this many stat filters
/// (`docs/RESEARCH-poe-trade-api.md:65`). A 5-row mercenary with 4 supports a
/// row is 25 cells before any tier loosening, so the cap is reachable in normal
/// play once the floor drops below 3.
const MAX_FILTERS: usize = 35;

/// A hash change has to hold still this long before it is worth a search. The
/// capture is settled when the session opens, but every hover-confirm after
/// that can still correct a cell, and each correction moves the hash.
pub const DEBOUNCE_MS: u64 = 2000;

/// Searches one capture session may spend before the app stops asking GGG and
/// hands the user the URL instead.
pub const MAX_SEARCHES: u8 = 3;

/// The query built from one capture, ready to POST and to link.
#[derive(Debug, Clone, PartialEq)]
pub struct CaptureQuery {
    /// The full request body — `{"query": {...}, "sort": {"price": "asc"}}`,
    /// the same envelope `trade::query::build_search_query_with_mode` produces
    /// for gems.
    pub body: Value,
    /// SHA-256 (hex) of [`Self::body`] rendered with sorted keys. The capture's
    /// identity for dedupe, caching and late-result discard — two captures that
    /// ask the same question share it.
    pub hash: String,
    /// Stat filters across every row group, after any cap degradation.
    pub filter_count: usize,
    /// The query asks for less than the capture and the floor called for: tier
    /// loosening was dropped, or support cells were, to fit [`MAX_FILTERS`].
    /// Surfaced to the user — a truncated search's "no listings" is not the
    /// same signal as a complete one's.
    pub truncated: bool,
}

/// One row's contribution to the query: the skill's ids, and one id set per
/// support cell that survived confidence filtering and tier expansion.
///
/// Kept as sets rather than a flat filter list so the cap can drop a whole cell
/// (which lowers the row's minimum by exactly one) instead of an id (which
/// would narrow a set and silently make the row unmatchable).
#[derive(Debug, Clone)]
struct RowPlan {
    skill_ids: Vec<String>,
    supports: Vec<Vec<String>>,
}

impl RowPlan {
    /// Every id in this row's group, in cell order, deduplicated.
    ///
    /// Deduplication is defensive — two cells of one row cannot share an id,
    /// since skills and supports use different id prefixes and a row holds one
    /// tier of one family. A repeated id would inflate `filter_count` against a
    /// cap that GGG enforces, so it is removed rather than trusted not to
    /// happen. A cell whose ids are ALL already on the row is not deduped here
    /// but dropped in [`plan_row`], so it stops counting toward [`Self::min`]
    /// too: keeping the count while deduping the filters would ask the row to
    /// match one id twice, which no row can do.
    fn ids(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for id in self.skill_ids.iter().chain(self.supports.iter().flatten()) {
            if !out.iter().any(|seen| seen == id) {
                out.push(id.clone());
            }
        }
        out
    }

    /// The group minimum: how many of this row's cells must match. One for the
    /// skill plus one per surviving support — never the id count, per the probe.
    fn min(&self) -> usize {
        1 + self.supports.len()
    }
}

/// Build the trade query for a capture, or `None` when no row can be expressed.
///
/// `tier_floor` is the lowest support tier the search will accept, clamped to
/// `1..=3`: 3 asks for the mercenary exactly as read, 1 comps it against every
/// grade of the same links.
pub fn build_capture_query(
    capture: &MercCapture,
    vocab: &MercVocab,
    tier_floor: u8,
) -> Option<CaptureQuery> {
    let floor = tier_floor.clamp(1, 3);

    let mut rows = plan_rows(capture, vocab, floor);
    if rows.is_empty() {
        return None;
    }

    let mut truncated = false;
    let mut count = count_filters(&rows);

    // Degradation order: loosening first, because it is the app's own
    // widening and the user did not read it off the panel. Only then do we
    // start dropping cells the capture actually saw.
    if count > MAX_FILTERS && floor < 3 {
        rows = plan_rows(capture, vocab, 3);
        truncated = true;
        count = count_filters(&rows);
    }

    // Then support cells from the last row inward. The last row is the one a
    // mercenary is least often bought for, and the skill of every row is kept
    // whatever happens — a row without its skill comps nothing.
    while count > MAX_FILTERS {
        let Some(row) = rows.iter_mut().rev().find(|row| !row.supports.is_empty()) else {
            // Only skills left and still over the cap. Unreachable with the
            // 5-row panel the game shows. There is nothing left to degrade, and
            // an over-cap body is a search GGG rejects — so the capture has no
            // expressible query, which is exactly what `None` means to the
            // caller (publish `Idle`, no failed lookup, no spent search).
            return None;
        };
        row.supports.pop();
        truncated = true;
        count = count_filters(&rows);
    }

    let stats: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "type": "mercenary",
                "value": {"min": row.min()},
                "filters": row.ids().into_iter().map(|id| json!({"id": id})).collect::<Vec<_>>(),
            })
        })
        .collect();

    let body = json!({
        "query": {
            "stats": stats,
            "status": {"option": "securable"},
            "filters": {
                "trade_filters": {
                    "filters": {
                        "sale_type": {"option": "priced"},
                    },
                },
            },
        },
        "sort": {"price": "asc"},
    });

    let hash = hash_body(&body);

    Some(CaptureQuery {
        body,
        hash,
        filter_count: count,
        truncated,
    })
}

/// The trade-site URL for a query the app assembled itself.
///
/// Same envelope as `derivedSearchUrl` in
/// `desktop/src/lib/mercenaries/trade-links.ts:147-151`: the `q` parameter
/// carries a `{"query": ...}` object holding the request body's `query` — and
/// no `sort`, which the TS side omits too because the trade site owns the sort
/// control.
///
/// Parity with the TS link is structural, not byte-for-byte: `serde_json`
/// renders object keys sorted while the TS object literal keeps insertion
/// order, so the two percent-encoded strings can differ character by character
/// and still name the same search.
///
/// `league` must be resolved before this is called; there is no sensible
/// fallback and a wrong league silently comps against another economy.
pub fn capture_url(league: &str, query: &CaptureQuery) -> String {
    let inner = query.body.get("query").cloned().unwrap_or(Value::Null);
    let envelope = json!({"query": inner});
    let encoded = serde_json::to_string(&envelope).unwrap_or_default();
    format!(
        "https://www.pathofexile.com/trade/search/{}?q={}",
        encode_uri_component(league),
        encode_uri_component(&encoded)
    )
}

/// One capture's worth of trade-search state.
///
/// Opened at the first settle edge of a capture and closed when the window
/// retires, so the search ceiling is per mercenary rather than per app run: a
/// re-detected window is a new session and gets its own budget, but the hash
/// cache is what stops it paying for the same question twice.
#[derive(Debug, Clone, Default)]
pub struct MercTradeSession {
    /// Searches actually sent to GGG in this session.
    pub searches_used: u8,
    /// The hash the session last ACTED on (enqueued, published from cache, or
    /// answered with a URL). Not the last hash seen — see [`Self::pending_hash`].
    pub last_hash: Option<String>,
    /// The hash last SEEN, whether or not anything was done about it. The
    /// debounce needs to know when the query stopped moving, which is a
    /// different question from what was last searched for; without this, a
    /// caller that re-evaluates faster than the debounce window would push the
    /// deadline forward on every tick and never search at all.
    pub pending_hash: Option<String>,
    /// When [`Self::pending_hash`] last changed.
    pub last_change_ms: u64,
    /// The status currently published for this session. Read, never written,
    /// by [`decide`] — the caller owns what the slice says.
    ///
    /// The caller MUST write it back before the next `decide`: the `Error`
    /// clause below makes the session forget its last hash, so a session left
    /// on `Error` after a successful retry re-enqueues the same query once per
    /// debounce window — bounded only by [`MAX_SEARCHES`].
    pub state: MercTradeStatus,
    /// The hash-independent verdict last handed to the caller, if the last one
    /// was hash-independent. The other refusals are deduplicated by
    /// [`Self::last_hash`]; these three have no hash to dedupe against and the
    /// tick asks ten times a second, so without this the caller re-publishes
    /// `Idle` or `WaitingLeague` on every one of them.
    pub settled: Option<Settled>,
    /// The `(capture revision, tier floor)` [`Self::built_query`] was built
    /// from. See [`tick`] — the build is the one costly thing on the per-tick
    /// path, and these two values are the whole of its input.
    pub built_key: Option<(u64, u8)>,
    /// The query that key produced. A `None` under a `Some` key is a real
    /// answer — this capture has no expressible query — and not a cache miss.
    pub built_query: Option<CaptureQuery>,
}

/// A verdict [`decide`] reaches without looking at the query hash.
///
/// The two `SetIdle` reasons are separate variants because the caller treats
/// them differently: only "no query" clears the link and the listings off the
/// slice, so collapsing them would let a capture that stops being expressible
/// keep the previous one's answer on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Settled {
    /// `SetIdle` — the user's auto-search toggle is off.
    AutoOff,
    /// `SetIdle` — the capture has no expressible query.
    NoQuery,
    /// `SetWaitingLeague` — the league is not resolved yet.
    NoLeague,
}

impl MercTradeSession {
    /// A fresh session: no budget spent, nothing searched.
    pub fn new() -> Self {
        Self::default()
    }
}

/// Everything outside the session that the decision depends on.
#[derive(Debug, Clone, Copy)]
pub struct TriggerInput<'a> {
    /// The user's auto-search toggle.
    pub auto: bool,
    /// The query for the current capture, or `None` when none can be built.
    pub query: Option<&'a CaptureQuery>,
    /// Whether the league is known. Unresolved means fail closed, not guess.
    pub league_resolved: bool,
    /// A result for this exact hash is in the cache and young enough to serve.
    pub cached: bool,
}

/// What the caller should do about the current capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerAction {
    /// Nothing changed; leave the slice alone.
    None,
    /// Publish `Idle` — no query to run, or auto-search is off.
    SetIdle,
    /// Publish `WaitingLeague` and wait for the league fetch.
    SetWaitingLeague,
    /// Serve the cached result for this hash.
    PublishCached,
    /// The query is still moving; re-evaluate after the debounce window.
    Debounce,
    /// Spend a search.
    Enqueue,
    /// Out of searches — publish the link and let the user follow it.
    UrlOnly,
}

/// Decide what to do at one publish, and record the decision on the session.
///
/// Ordered so the refusals come first and nothing below them has to re-check
/// them — `NoQuery` ahead of `AutoOff` for the reason given at the check. The
/// one non-obvious rule is the `Error` clause: a failed lookup makes the
/// session forget what it searched for, so a later publish of the same capture
/// may retry — one debounce window later, and still against the same ceiling,
/// so a persistently failing query costs at most [`MAX_SEARCHES`] attempts
/// spread over at least that many windows.
///
/// Every verdict is returned ONCE per condition. The hash-driven ones dedupe
/// against [`MercTradeSession::last_hash`]; the three that never look at a hash
/// dedupe against [`MercTradeSession::settled`], because the caller asks at the
/// capture loop's cadence (see [`tick`]) and a re-publish of the same `Idle` is
/// a slice clone ten times a second for nothing.
pub fn decide(
    session: &mut MercTradeSession,
    input: TriggerInput<'_>,
    now_ms: u64,
) -> TriggerAction {
    // `NoQuery` is tested BEFORE `AutoOff` even though auto-off is the cheaper
    // check: a capture that stops being expressible has to clear its link and
    // listings, and that only happens under the `SetIdle` arm's `query.is_none()`
    // (see [`tick`], whose auto-off early return states the same rule). Settling
    // on `AutoOff` first would swallow the clear.
    let Some(query) = input.query else {
        return settle(session, Settled::NoQuery, TriggerAction::SetIdle);
    };
    if !input.auto {
        return settle(session, Settled::AutoOff, TriggerAction::SetIdle);
    }
    if !input.league_resolved {
        return settle(session, Settled::NoLeague, TriggerAction::SetWaitingLeague);
    }
    // Past every hash-independent refusal: the next one that fires is about a
    // condition that has been away since, so it publishes again.
    //
    // `last_hash` goes with it: the settled condition published `Idle` (or
    // `WaitingLeague`) OVER whatever the hash had been answered with, so the
    // recorded hash no longer describes the slice. Keeping it would make the
    // clause below return `None` for the current hash and leave that `Idle`
    // standing over a valid `result` — auto off→on would go silent. Cleared,
    // the next pass re-decides from scratch: `PublishCached` while the 15-min
    // cache holds, otherwise one search against the same ceiling.
    if session.settled.take().is_some() {
        session.last_hash = None;
    }

    // Track when the query stopped moving before anything acts on it, so the
    // debounce measures the age of the CHANGE and not of the last poll.
    if session.pending_hash.as_deref() != Some(query.hash.as_str()) {
        session.pending_hash = Some(query.hash.clone());
        session.last_change_ms = now_ms;
    }

    // A failed lookup is not an answer, so the hash it failed on is not one
    // either: forgetting it is what lets the next pass retry.
    //
    // The failure is folded into the debounce clock in the same breath, so the
    // retry has to survive the same quiet window a fresh query does. Without
    // it the whole budget goes in one burst — the error arrives, the very next
    // tick (100 ms later) re-enqueues, and three transient failures are spent
    // inside half a second, which is the shape of an outage rather than a
    // retry.
    //
    // Fires ONCE per failure because folding it also clears the record that
    // made it visible. Resetting on every tick that observes `Error` would
    // push the deadline forward faster than the window closes and the session
    // would never retry at all — the same trap `pending_hash` exists to avoid
    // for hash changes.
    if session.state == MercTradeStatus::Error && session.last_hash.is_some() {
        session.last_hash = None;
        session.last_change_ms = now_ms;
    }
    if session.last_hash.as_deref() == Some(query.hash.as_str()) {
        return TriggerAction::None;
    }

    if input.cached {
        session.last_hash = Some(query.hash.clone());
        return TriggerAction::PublishCached;
    }

    if now_ms.saturating_sub(session.last_change_ms) < DEBOUNCE_MS {
        return TriggerAction::Debounce;
    }

    if session.searches_used >= MAX_SEARCHES {
        // Recorded as acted-on: the URL is the answer for this hash, and
        // repeating it every publish would churn the slice for nothing.
        session.last_hash = Some(query.hash.clone());
        return TriggerAction::UrlOnly;
    }

    session.searches_used += 1;
    session.last_hash = Some(query.hash.clone());
    TriggerAction::Enqueue
}

/// Hand `action` back the first time this condition is reached, and
/// [`TriggerAction::None`] while it holds.
fn settle(
    session: &mut MercTradeSession,
    settled: Settled,
    action: TriggerAction,
) -> TriggerAction {
    if session.settled == Some(settled) {
        return TriggerAction::None;
    }
    session.settled = Some(settled);
    action
}

/// One row group per confident row, with every support cell expanded to the
/// floor.
///
/// A row whose SKILL is not confident is skipped entirely: the skill is what
/// makes the row a row, and a group of supports with no skill comps every
/// mercenary that happens to carry those links. At the settle edge every cell is
/// confident by construction (`read::capture_complete`), so this is a guard
/// against a caller that asks earlier, not a path normal play takes.
fn plan_rows(capture: &MercCapture, vocab: &MercVocab, floor: u8) -> Vec<RowPlan> {
    capture.rows.iter().filter_map(|row| plan_row(row, vocab, floor)).collect()
}

fn plan_row(row: &MercRow, vocab: &MercVocab, floor: u8) -> Option<RowPlan> {
    if !confident(row.skill.state) || row.skill.ids.is_empty() {
        return None;
    }
    let mut supports: Vec<Vec<String>> = Vec::new();
    for cell in row.supports.iter().filter(|cell| confident(cell.state)) {
        let ids = expand(cell.family.as_deref(), cell.tier, &cell.ids, vocab, floor);
        // An empty set is not a cell: it would raise the row's minimum by one
        // with nothing that can satisfy it, making the whole row unmatchable.
        if ids.is_empty() {
            continue;
        }
        // Nor is a cell that repeats what the row already asks for. Defensive
        // (see [`RowPlan::ids`]), and dropped whole rather than deduped: the
        // duplicate must stop counting toward the minimum as well, since the
        // row would have to match the same id twice to satisfy it.
        let covered = ids.iter().all(|id| {
            row.skill.ids.iter().any(|seen| seen == id)
                || supports.iter().flatten().any(|seen| seen == id)
        });
        if covered {
            continue;
        }
        supports.push(ids);
    }
    Some(RowPlan {
        skill_ids: row.skill.ids.clone(),
        supports,
    })
}

/// The id set one support cell contributes: what was read, plus the family's
/// ids at every tier from the floor up to — but NOT including — the tier read.
///
/// The read's own ids already represent tier T, and they are the more precise
/// answer: a hover-`Confirmed` cell names one exact support, so re-adding
/// `vocab.resolve(family, T)` would widen it back to every support of that
/// family at that grade. The loosening the floor asks for is about grades BELOW
/// what was read, so the range is `N..T`, exclusive at the top.
///
/// The read's own ids lead and are kept even when the vocabulary has nothing at
/// that `(family, tier)` — a cell that was confidently read is never dropped by
/// a lookup miss.
fn expand(
    family: Option<&str>,
    tier: Option<u8>,
    read_ids: &[String],
    vocab: &MercVocab,
    floor: u8,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |id: &str| {
        if !out.iter().any(|seen| seen == id) {
            out.push(id.to_string());
        }
    };
    for id in read_ids {
        push(id);
    }
    if let (Some(family), Some(tier)) = (family, tier) {
        for t in floor..tier {
            for stat in vocab.resolve(family, t) {
                push(&stat.id);
            }
        }
    }
    out
}

fn count_filters(rows: &[RowPlan]) -> usize {
    rows.iter().map(|row| row.ids().len()).sum()
}

/// SHA-256 (hex) of the body rendered with every object's keys sorted.
///
/// Sorting is explicit rather than inherited from `serde_json`'s default map:
/// the crate's `preserve_order` feature turns that map into insertion-ordered,
/// and feature unification means any dependency in the tree can switch it on.
/// The hash is the capture's identity across a cache and a late-result discard,
/// so it must not depend on a feature flag nobody in this file can see.
fn hash_body(body: &Value) -> String {
    let canonical = canonicalise(body);
    let rendered = serde_json::to_string(&canonical).unwrap_or_default();
    hex::encode(Sha256::digest(rendered.as_bytes()))
}

fn canonicalise(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let sorted: BTreeMap<String, Value> = map
                .iter()
                .map(|(key, val)| (key.clone(), canonicalise(val)))
                .collect();
            Value::Object(sorted.into_iter().collect())
        }
        // Arrays are ordered data — `stats` order is the row order, and the
        // filter order inside a group is the cell order. Sorting them would
        // make two different captures hash alike.
        Value::Array(items) => Value::Array(items.iter().map(canonicalise).collect()),
        other => other.clone(),
    }
}

/// JavaScript's `encodeURIComponent`, byte for byte.
///
/// The link has to survive a round trip through the same function the TS side
/// uses (`trade-links.ts`), which means the unreserved set is JS's — `!~*'()`
/// stay literal, unlike in RFC 3986 — and the escapes are uppercase hex over
/// UTF-8 bytes.
fn encode_uri_component(raw: &str) -> String {
    const KEEP: &[u8] = b"-_.!~*'()";
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(raw.len());
    for byte in raw.as_bytes() {
        if byte.is_ascii_alphanumeric() || KEEP.contains(byte) {
            out.push(*byte as char);
        } else {
            out.push('%');
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Wiring: settings, the per-tick evaluation, the lookup task, the cache
// ---------------------------------------------------------------------------

/// How long a result answers for its hash.
///
/// The window is what makes a retire-then-re-detect of the same recruit window
/// free: the new session starts with a fresh search budget, but the question it
/// asks is the same one, so the cache answers it without spending any of it.
pub const RESULT_TTL_MS: u64 = 15 * 60 * 1000;

/// The queue-event label for a capture — what the shared trade queue calls this
/// lookup while it waits.
///
/// The mercenary's name when the header was read, and a constant otherwise: the
/// label is display text on a shared queue, not an identity (the hash is), so a
/// missing header must not stop a lookup that is otherwise fully specified.
fn label_for(capture: &MercCapture) -> String {
    capture
        .header
        .name
        .clone()
        .unwrap_or_else(|| "mercenary".to_string())
}

/// Evaluate the trade policy for the capture on screen and apply its verdict.
///
/// # Cadence
///
/// Called from the capture loop on EVERY iteration while a complete capture is
/// on screen (`run::run_loop`) — every 100 ms with the game focused (`TICK`),
/// every 1 s without it (`UNFOCUSED_NAP`). That is what makes the DEBOUNCE work
/// without a timer: the loop comes back, re-evaluates, and eventually finds the
/// change old enough to act on. A `tokio` timer would have to be cancelled and
/// re-armed on every hover-confirm, and would fire into a loop that may since
/// have retired the capture.
///
/// Ten calls a second is also why nothing here may be expensive by default:
///
/// - `revision` is the capture's version from `run::Session`, bumped by the two
///   ticks that write a capture. Together with the tier floor it is the whole
///   input to [`build_capture_query`], so the built query is cached against the
///   pair — a JSON body plus a SHA-256 over it is not something to redo ten
///   times a second for a capture that has not moved.
/// - The `!auto` refusal returns before publishing anything once the slice
///   already says `Idle`, and [`decide`] hands each hash-independent verdict
///   back once (see [`Settled`]), so the steady state publishes nothing. What
///   it does still pay every tick is the [`cache_get`] below: it clones the
///   stored result whether or not an arm goes on to use it.
/// - [`capture_url`] renders and percent-encodes the whole body, so it is built
///   per arm rather than up front.
///
/// `session.state` is re-read from the SLICE first rather than tracked here,
/// because the lookup task publishes `Searching` / `Done` / `Error` after
/// `decide` has returned — the slice is the only place that knows the current
/// status, and [`decide`]'s `Error` clause is a decision about it.
pub fn tick(
    app: &AppHandle,
    capture: &MercCapture,
    vocab: &MercVocab,
    session: &mut MercTradeSession,
    revision: u64,
    now_ms: u64,
) {
    let state = app.state::<AppState>();
    let auto = *state
        .merc_trade_auto
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let floor = *state
        .merc_tier_floor
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    let key = (revision, floor);
    if session.built_key != Some(key) {
        session.built_query = build_capture_query(capture, vocab, floor);
        session.built_key = Some(key);
    }

    session.state = {
        let slice = state.mercenary.lock().unwrap_or_else(|e| e.into_inner());
        slice.trade.status
    };

    // Auto off and the slice already saying so: there is nothing left to
    // publish. The `query.is_some()` guard is what keeps this from swallowing
    // the one thing an auto-off tick still has to do — a capture that stops
    // being expressible has to have its link and listings cleared, which is the
    // `SetIdle` arm's job below.
    if !auto
        && session.state == MercTradeStatus::Idle
        && session.built_query.is_some()
    {
        return;
    }

    let league = state.trade_client.league().ok();
    // Taken BEFORE `decide`, not looked up after it: `decide` is told whether a
    // cached answer exists, and a lookup that raced the check would leave the
    // caller told "cached" with nothing to publish.
    //
    // No league, no cache read: the cache is keyed by league (see
    // [`cache_get`]), and an unresolved league is on its way to
    // `SetWaitingLeague` anyway.
    let cached = match (&league, session.built_query.as_ref()) {
        (Some(league), Some(query)) => {
            let cache = state
                .merc_trade_cache
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            cache_get(&cache, league, &query.hash, now_ms)
        }
        _ => None,
    };

    // Moved out and straight back: `decide` takes `&mut session` while
    // `TriggerInput` borrows the query out of that same session.
    let taken = session.built_query.take();
    let action = decide(
        session,
        TriggerInput {
            auto,
            query: taken.as_ref(),
            league_resolved: league.is_some(),
            cached: cached.is_some(),
        },
        now_ms,
    );
    session.built_query = taken;

    let used = session.searches_used;
    let query = session.built_query.as_ref();
    let link = |query: Option<&CaptureQuery>| match (query, &league) {
        (Some(query), Some(league)) => Some(capture_url(league, query)),
        _ => None,
    };

    match action {
        // Nothing moved. The slice already says what this capture is worth.
        TriggerAction::None => {}
        // The query stopped being expressible, or the user switched the
        // auto-search off. Only the first case clears the answer: a link and a
        // listing table are still true about a capture whose owner has since
        // turned the automation off, but they describe nothing at all once
        // there is no query to describe.
        TriggerAction::SetIdle => {
            let clear = query.is_none();
            publish(app, |slice| {
                slice.trade.status = MercTradeStatus::Idle;
                slice.trade.error = None;
                slice.trade.searches_used = used;
                if clear {
                    slice.trade.query_hash = None;
                    slice.trade.url = None;
                    slice.trade.result = None;
                }
            });
        }
        // No league, no URL — `capture_url` cannot name a search without one,
        // and guessing is how a capture gets comped against another economy.
        TriggerAction::SetWaitingLeague => {
            let hash = query.map(|q| q.hash.clone());
            publish(app, |slice| {
                slice.trade.status = MercTradeStatus::WaitingLeague;
                slice.trade.query_hash = hash;
                slice.trade.url = None;
                slice.trade.result = None;
                slice.trade.error = None;
                slice.trade.searches_used = used;
            });
        }
        TriggerAction::PublishCached => {
            let hash = query.map(|q| q.hash.clone());
            let url = link(query);
            publish(app, |slice| {
                slice.trade.status = MercTradeStatus::Done;
                slice.trade.query_hash = hash;
                slice.trade.url = url;
                slice.trade.result = cached;
                slice.trade.error = None;
                slice.trade.searches_used = used;
            });
        }
        // Deliberately nothing: the loop's next iteration re-evaluates, and the
        // debounce is measured off `session.last_change_ms`, not off a timer
        // this arm would have to own. Publishing an interim status here would
        // also flicker the page once per hover-confirm.
        TriggerAction::Debounce => {}
        TriggerAction::Enqueue => {
            let Some(query) = query else { return };
            let hash = query.hash.clone();
            let url = link(Some(query));
            publish(app, |slice| {
                slice.trade.status = MercTradeStatus::Queued;
                slice.trade.query_hash = Some(hash);
                slice.trade.url = url;
                // The hash moved, so whatever was on the slice answered a
                // different question. Keeping it would show one mercenary's
                // prices under another's link.
                slice.trade.result = None;
                slice.trade.error = None;
                slice.trade.searches_used = used;
            });
            spawn_lookup(app, label_for(capture), query.clone());
        }
        // Out of searches. The link is the answer for this hash — the user can
        // follow it and see the market the app stopped asking about.
        //
        // `error` is cleared like everywhere else. [`decide`] treats a session
        // in `Error` as having acted on nothing, so this can fire on the very
        // hash the last attempt failed on — but the status published here is
        // `Idle`, and `error` is only ever set alongside
        // [`MercTradeStatus::Error`] (`mercenary/mod.rs`'s `MercTradeState`).
        // Leaving the message attached to an `Idle` slice would put a failure
        // under a link that works.
        TriggerAction::UrlOnly => {
            let hash = query.map(|q| q.hash.clone());
            publish(app, |slice| {
                slice.trade.status = MercTradeStatus::Idle;
                slice.trade.query_hash = hash;
                slice.trade.url = link(query);
                slice.trade.result = None;
                slice.trade.error = None;
                slice.trade.searches_used = used;
            });
        }
    }
}

/// Close a session: the recruit window retired, so nothing is coming.
///
/// Cancels the shared queue ONLY from the two states that have something to
/// cancel. Not because a needless cancel would latch — `cancel(Mercenary)`
/// bumps an epoch that only lookups already in the queue compare against, so
/// calling it over an empty queue stops nothing — but because it publishes
/// `Idle` in the same breath, and doing that from `Done` would throw away the
/// retired capture's answer.
///
/// The publish is what the cancelled lookup's own task then defers to: it
/// leaves the status alone unless the slice is still reading `Queued` or
/// `Searching` (see [`spawn_lookup`]). That task can also get there FIRST, in
/// the gap between the cancel and the publish, which is why the publish clears
/// `error` as well as setting the status.
///
/// `trade` itself stays on the slice: the page goes on showing the retired
/// capture's verdict (`run::miss`), and the listings are part of that verdict.
pub fn close_session(app: &AppHandle) {
    let state = app.state::<AppState>();
    let in_flight = {
        let slice = state.mercenary.lock().unwrap_or_else(|e| e.into_inner());
        awaiting_lookup(slice.trade.status)
    };
    if in_flight {
        state.trade_client.cancel(TradeSource::Mercenary);
        publish(app, |slice| {
            slice.trade.status = MercTradeStatus::Idle;
            // `error` goes with the status: the cancelled lookup's task can
            // wake between the `cancel()` above and this publish, and its
            // `Err` arm still sees `Queued`/`Searching` and writes
            // `Error` + `error: Some(CANCELLED)`. Without this the status is
            // overwritten and the string is not, leaving "cancelled" showing
            // on a slice that reads `Idle`.
            slice.trade.error = None;
        });
    }
}

/// Run one merc lookup on the shared trade queue and publish what comes back.
///
/// Deliberately NOT the `trade_lookup` command: that one POSTs every result to
/// `/api/trade/submit` for the server's shared gem cache (`lib.rs`), keyed by
/// (gem, variant), and a mercenary search has neither. This path reaches
/// `TradeApiClient` directly, so there is no branch in it that could ever get
/// there.
fn spawn_lookup(app: &AppHandle, label: String, query: CaptureQuery) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let hash = query.hash.clone();
        let emit_app = app.clone();
        let emit_hash = hash.clone();
        let outcome = {
            let state = app.state::<AppState>();
            state
                .trade_client
                .lookup_query(TradeSource::Mercenary, &label, query.body.clone(), |event| {
                    // `Fetching` is the queue saying the request is going out
                    // now — the difference between "waiting behind the rate
                    // limiter" and "asking GGG", which is the whole reason the
                    // page shows two states.
                    if matches!(event, TradeQueueEvent::Fetching { .. }) {
                        publish(&emit_app, |slice| {
                            note_fetching(&mut slice.trade, &emit_hash)
                        });
                    }
                    use tauri::Emitter;
                    if let Err(e) = emit_app.emit("trade-queue", &event) {
                        log::warn!("emit trade-queue failed: {}", e);
                    }
                })
                .await
        };

        match outcome {
            // `Done` on the queue means the FETCH succeeded, not that the
            // lookup did — the terminal state is derived here, from the value.
            Ok(raw) => {
                let (result, skipped) = to_result(raw, &query, now_ms());
                if skipped > 0 {
                    crate::app_log(
                        &app,
                        format!(
                            "Merc trade: {skipped} listing(s) of {label} could not be read and were left out"
                        ),
                    );
                }
                {
                    let state = app.state::<AppState>();
                    let mut cache = state
                        .merc_trade_cache
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());
                    cache_insert(&mut cache, now_ms(), result.clone());
                }
                // The CHEAPEST LISTING, in the seller's own currency — the first
                // row of a `sort.price=asc` fetch. Not `floor_chaos`: with no
                // divine rate on this side that number is a bare `amount` with
                // the currency stripped off, so logging it as a price says
                // "floor 1.0" for a 1-divine mercenary.
                let cheapest = result
                    .listings
                    .first()
                    .map(|l| format!("{} {}", l.amount, l.currency))
                    .unwrap_or_else(|| "none".to_string());
                crate::app_log(
                    &app,
                    format!(
                        "Merc trade: {label} — {} listed, {} shown, cheapest {cheapest}{}",
                        result.total,
                        result.listings.len(),
                        if result.truncated { " (truncated query)" } else { "" },
                    ),
                );
                publish(&app, |slice| accept_result(&mut slice.trade, &hash, result));
            }
            // A cancel is not logged as a failure; what it does to the slice
            // is [`accept_error`]'s rule, stated there.
            Err(e) => {
                if e != CANCELLED {
                    crate::app_log(&app, format!("Merc trade error: {label} — {e}"));
                }
                publish(&app, |slice| accept_error(&mut slice.trade, &hash, e));
            }
        }
    });
}

/// Whether this status still has a lookup outstanding on the shared queue.
///
/// The two questions both callers ask: [`close_session`] cancels only from
/// here (for the reason stated there), and [`accept_error`] absorbs a
/// `cancelled` only from here, because anywhere else something has already
/// settled the slice.
fn awaiting_lookup(status: MercTradeStatus) -> bool {
    matches!(
        status,
        MercTradeStatus::Queued | MercTradeStatus::Searching
    )
}

/// The queue says this lookup's request is going out now.
///
/// The hash guard is what keeps a slow lookup from relabelling the capture
/// that replaced it, and the `Queued` guard keeps the event from reviving a
/// session something else has already finished, cancelled or failed.
fn note_fetching(trade: &mut crate::mercenary::MercTradeState, hash: &str) {
    if trade.query_hash.as_deref() == Some(hash) && trade.status == MercTradeStatus::Queued {
        trade.status = MercTradeStatus::Searching;
    }
}

/// A lookup came back with listings.
///
/// A result that no longer answers the slice's question is dropped whole: the
/// capture moved on while this was in flight (a hover-confirm, a rematch, a
/// tier-floor change), and showing one mercenary's prices under another's link
/// is worse than showing none.
fn accept_result(
    trade: &mut crate::mercenary::MercTradeState,
    hash: &str,
    result: MercTradeResult,
) {
    if trade.query_hash.as_deref() != Some(hash) {
        return;
    }
    trade.status = MercTradeStatus::Done;
    trade.result = Some(result);
    trade.error = None;
}

/// A lookup came back with an error.
///
/// Dropped on a stale hash for the same reason a result is. A cancel normally
/// comes with its own publish — [`close_session`] sets `Idle` before this task
/// wakes — so the canceller owns the state and this leaves it alone. The one
/// case it must NOT leave alone is a slice still reading `Queued`/`Searching`:
/// the cancel epoch can stop a lookup whose session never published anything
/// about it (a cancel landing between the enqueue and the snapshot), and
/// without this that session waits for a result that is never coming.
fn accept_error(trade: &mut crate::mercenary::MercTradeState, hash: &str, error: String) {
    if trade.query_hash.as_deref() != Some(hash) {
        return;
    }
    if error == CANCELLED && !awaiting_lookup(trade.status) {
        return;
    }
    trade.status = MercTradeStatus::Error;
    trade.error = Some(error);
    trade.result = None;
}

/// Shape one [`RawSearch`] into the result the slice carries, plus how many
/// fetch entries could not be read.
///
/// A malformed entry is DROPPED rather than failing the lookup, unlike the gem
/// path: the search has already been paid for out of a budget of three, and
/// nine good listings are a better answer than an error. The count is returned
/// so the loss is logged rather than silent.
///
/// # Prices are raw seller numbers, and the order is GGG's
///
/// There is no divine→chaos rate on the Rust side (the gem path is handed one
/// by the webview, which reads it off the market page the Mercenaries page does
/// not have), so `normalize_to_chaos` is called with a rate of 0 and
/// `chaos_price` is just the seller's own `amount` — 1 divine and 1 chaos both
/// read as 1. `floor_chaos` and `median_chaos` are statistics over those raw
/// numbers, NOT a value floor and not a value median, which is why the page
/// renders the currency beside every row and quotes the cheapest listing as
/// "amount currency".
///
/// The row order is left exactly as GGG returned it, which is `sort.price=asc`
/// — a real value order, computed by the only party in this pipeline that knows
/// the exchange rates. Re-sorting by `chaos_price` here would replace it with
/// an ordering that puts 5 chaos above 1 divine.
fn to_result(raw: RawSearch, query: &CaptureQuery, fetched_at_ms: u64) -> (MercTradeResult, usize) {
    let seen = raw.items.len();
    let listings: Vec<MercTradeListing> =
        raw.items.iter().filter_map(parse_listing).collect();
    let skipped = seen - listings.len();
    let chaos: Vec<f64> = listings.iter().map(|l| l.chaos_price).collect();
    (
        MercTradeResult {
            query_hash: query.hash.clone(),
            league: raw.league,
            total: raw.total,
            floor_chaos: crate::trade::signals::floor_chaos_price(&chaos),
            median_chaos: crate::trade::signals::median_chaos_price(&chaos),
            listings,
            fetched_at_ms,
            truncated: query.truncated,
        },
        skipped,
    )
}

/// One GGG fetch entry as a mercenary listing, or `None` when the fields a
/// price needs are missing.
///
/// Read off the JSON rather than through a typed struct because the gem path's
/// (`trade::client::GggFetchEntry`) is private to that module and gem-shaped —
/// it parses item properties for level and quality, which a mercenary has none
/// of. Only `amount` and `currency` are required: an entry without a price
/// cannot join a floor or a median, while a missing account or timestamp costs
/// the row a label and nothing else.
fn parse_listing(item: &Value) -> Option<MercTradeListing> {
    let listing = item.get("listing")?;
    let price = listing.get("price")?;
    let amount = price.get("amount")?.as_f64()?;
    let currency = price.get("currency")?.as_str()?.to_string();
    let account = listing
        .get("account")
        .and_then(|a| a.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or_default()
        .to_string();
    let indexed_at = listing
        .get("indexed")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    Some(MercTradeListing {
        chaos_price: crate::trade::signals::normalize_to_chaos(amount, &currency, 0.0),
        currency,
        amount,
        account,
        indexed_at,
    })
}

/// The merc result cache behind `AppState::merc_trade_cache`, keyed by
/// `(league, query hash)` and carrying the unix ms each result was fetched at.
///
/// Named here rather than spelled out at the field, so the key stays one
/// decision: the two functions below are the only readers and writers.
pub type MercResultCache = std::collections::HashMap<(String, String), (u64, MercTradeResult)>;

/// The cached result for this league's `hash`, if one is still young enough to
/// serve.
///
/// The league is half the key because the hash is not computed over it —
/// [`build_capture_query`] hashes the request BODY, and the league only ever
/// appears as a path segment ([`capture_url`]). Keyed on the hash alone, a
/// league switch inside [`RESULT_TTL_MS`] would answer the new league's link
/// with the old economy's prices.
///
/// Takes the map rather than the `AppState` that owns it: the caller holds the
/// lock for exactly its own read, and the age rule and the key are then
/// testable without an app handle.
fn cache_get(
    cache: &MercResultCache,
    league: &str,
    hash: &str,
    now_ms: u64,
) -> Option<MercTradeResult> {
    cache
        .get(&(league.to_string(), hash.to_string()))
        .filter(|(at, _)| now_ms.saturating_sub(*at) < RESULT_TTL_MS)
        .map(|(_, result)| result.clone())
}

/// Store a result under its own league and hash, dropping every entry that has
/// aged out.
///
/// The league comes off the RESULT rather than off the request: `raw.league` is
/// what GGG answered in, so a result can never be filed under a league it does
/// not describe.
///
/// Pruned on insert rather than on a timer: the map only ever grows on this
/// path, so the write is the one moment it is worth walking, and a user who
/// captures nothing pays nothing.
fn cache_insert(cache: &mut MercResultCache, now_ms: u64, result: MercTradeResult) {
    cache.retain(|_, (at, _)| now_ms.saturating_sub(*at) < RESULT_TTL_MS);
    let key = (result.league.clone(), result.query_hash.clone());
    cache.insert(key, (now_ms, result));
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Turn the captured mercenary's auto-search on or off.
///
/// Same shape as `merc_set_sources_off`: written to the owner, persisted, and
/// nudged out — the value is composed onto the merc slice at read time, so
/// there is no slice field to write and no second copy to keep in step.
#[tauri::command]
pub fn merc_set_trade_auto(auto: bool, app: AppHandle) -> Result<(), String> {
    {
        let state = app.state::<AppState>();
        *state
            .merc_trade_auto
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = auto;
    }
    crate::persist_settings(&app);
    crate::ssot::emit_ssot(&app);
    crate::app_log(
        &app,
        format!(
            "Merc: trade auto-search {}",
            if auto { "on" } else { "off" }
        ),
    );
    Ok(())
}

/// Set the lowest support tier the captured search accepts (1..=3).
///
/// The rejection is returned AND logged, like every other merc/temple setter:
/// an `Err` alone leaves no trace a shipped build can read. Changing the floor
/// changes the query, so the loop's next tick sees a new hash and re-searches
/// — bounded by the session's remaining budget.
#[tauri::command]
pub fn merc_set_tier_floor(floor: u8, app: AppHandle) -> Result<(), String> {
    let accepted = match super::validate_tier_floor(floor) {
        Ok(accepted) => accepted,
        Err(e) => {
            crate::app_log(&app, format!("Merc: tier floor rejected — {e}"));
            return Err(e);
        }
    };
    {
        let state = app.state::<AppState>();
        *state
            .merc_tier_floor
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = accepted;
    }
    crate::persist_settings(&app);
    crate::ssot::emit_ssot(&app);
    crate::app_log(&app, format!("Merc: trade tier floor set to {accepted}"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mercenary::vocab::{MercRole, MercStat};
    use crate::mercenary::{MercSkillRead, MercSupportRead, MercTradeState, ReadState};

    // -----------------------------------------------------------------------
    // Fixtures
    // -----------------------------------------------------------------------

    fn skill(id: &str) -> MercSkillRead {
        MercSkillRead {
            raw: "Ice Shot".to_string(),
            ids: vec![id.to_string()],
            name: Some("Ice Shot".to_string()),
            score: 0.99,
            state: ReadState::Matched,
        }
    }

    fn support(
        ids: &[&str],
        family: Option<&str>,
        tier: Option<u8>,
        state: ReadState,
    ) -> MercSupportRead {
        MercSupportRead {
            slot: 0,
            rect: [0, 0, 0, 0],
            family: family.map(|f| f.to_string()),
            tier,
            ids: ids.iter().map(|id| id.to_string()).collect(),
            name: None,
            score: 0.95,
            state,
            candidates: Vec::new(),
        }
    }

    fn row(index: u8, skill_id: &str, supports: Vec<MercSupportRead>) -> MercRow {
        MercRow {
            index,
            skill: skill(skill_id),
            supports,
        }
    }

    fn capture(rows: Vec<MercRow>) -> MercCapture {
        MercCapture {
            rows,
            ..Default::default()
        }
    }

    /// A vocabulary with nothing in it — every builder test that is not about
    /// tier loosening uses this, so a cell contributes exactly what was read.
    fn no_vocab() -> MercVocab {
        MercVocab::from_stats(Vec::new())
    }

    fn stat(id: &str, family: &str, tier: u8) -> MercStat {
        MercStat {
            id: id.to_string(),
            name: format!("{family} (Tier {tier})"),
            qualified: family.to_string(),
            family: family.to_string(),
            role: MercRole::Support,
            tier: Some(tier),
        }
    }

    /// One family across all three grades, with the tier-3 collision the icon
    /// read cannot resolve on its own (Greater vs Gilded).
    fn chain_vocab() -> MercVocab {
        MercVocab::from_stats(vec![
            stat("sup_lesser_chain", "Chain", 1),
            stat("sup_chain", "Chain", 2),
            stat("sup_greater_chain", "Chain", 3),
            stat("sup_gilded_chain", "Chain", 3),
        ])
    }

    /// The one capture the cross-language parity fixture was built from
    /// (`lib/mercenaries/__fixtures__/capture-query.expected.json`), together
    /// with [`chain_vocab`] and a tier floor of 2.
    ///
    /// Deliberately not the simplest capture that builds: it carries two rows
    /// (so the fixture pins group ORDER), a cell the icon read could not narrow
    /// to one id (so it pins that a set still counts once), and a confirmed
    /// tier-3 cell under a floor of 2 (so it pins the loosening range and the
    /// order the expanded ids join the group in).
    fn parity_capture() -> MercCapture {
        capture(vec![
            row(
                0,
                "skill_a",
                vec![
                    support(&["sup_a"], None, None, ReadState::Matched),
                    support(&["sup_b1", "sup_b2"], None, None, ReadState::Matched),
                ],
            ),
            row(
                1,
                "skill_b",
                vec![support(
                    &["sup_greater_chain"],
                    Some("Chain"),
                    Some(3),
                    ReadState::Confirmed,
                )],
            ),
        ])
    }

    fn groups(query: &CaptureQuery) -> Vec<Value> {
        query.body["query"]["stats"]
            .as_array()
            .expect("the body carries a stats array")
            .clone()
    }

    fn group_ids(group: &Value) -> Vec<String> {
        group["filters"]
            .as_array()
            .expect("a group carries filters")
            .iter()
            .map(|f| f["id"].as_str().expect("a filter carries an id").to_string())
            .collect()
    }

    fn group_min(group: &Value) -> u64 {
        group["value"]["min"].as_u64().expect("a row group carries value.min")
    }

    // -----------------------------------------------------------------------
    // The query builder
    // -----------------------------------------------------------------------

    /// The probe's arithmetic: one `mercenary` group per row, and the minimum
    /// counts CELLS. A group asking for 3 of 3 ids matches nothing the moment
    /// one cell resolves to a set.
    #[test]
    fn a_row_becomes_one_group_whose_minimum_counts_its_cells() {
        let capture = capture(vec![row(
            0,
            "skill_a",
            vec![
                support(&["sup_a"], None, None, ReadState::Matched),
                support(&["sup_b"], None, None, ReadState::Confirmed),
            ],
        )]);

        let query = build_capture_query(&capture, &no_vocab(), 3).expect("a confident row builds");

        let groups = groups(&query);
        assert_eq!(groups.len(), 1, "one group per row, never an `and`: {groups:?}");
        assert_eq!(groups[0]["type"], "mercenary");
        assert_eq!(group_min(&groups[0]), 3, "the skill and both supports");
        assert_eq!(group_ids(&groups[0]), ["skill_a", "sup_a", "sup_b"]);
        assert_eq!(query.filter_count, 3);
        assert!(!query.truncated, "nothing was dropped");
    }

    /// Every row is its own group. Folding two rows into one would comp row
    /// 1's skill against row 2's support — a mercenary nobody has.
    #[test]
    fn each_row_gets_its_own_group() {
        let capture = capture(vec![
            row(0, "skill_a", vec![support(&["sup_a"], None, None, ReadState::Matched)]),
            row(1, "skill_b", vec![support(&["sup_b"], None, None, ReadState::Matched)]),
        ]);

        let query = build_capture_query(&capture, &no_vocab(), 3).expect("both rows build");

        let groups = groups(&query);
        assert_eq!(groups.len(), 2);
        assert_eq!(group_ids(&groups[0]), ["skill_a", "sup_a"]);
        assert_eq!(group_ids(&groups[1]), ["skill_b", "sup_b"]);
    }

    /// A cell the icon read could not narrow to one support contributes all of
    /// its ids and still counts ONCE: at most one of them is on the row, so a
    /// minimum that counted ids would make the row unmatchable.
    #[test]
    fn a_cell_that_reads_as_several_ids_contributes_all_of_them_and_counts_once() {
        let capture = capture(vec![row(
            0,
            "skill_a",
            vec![support(&["sup_a1", "sup_a2"], None, None, ReadState::Matched)],
        )]);

        let query = build_capture_query(&capture, &no_vocab(), 3).expect("a confident row builds");

        let groups = groups(&query);
        assert_eq!(group_ids(&groups[0]), ["skill_a", "sup_a1", "sup_a2"]);
        assert_eq!(group_min(&groups[0]), 2, "the skill and ONE support cell");
    }

    /// Defensive path — `read::capture_complete` only calls a capture complete
    /// when every cell is confident, so the settle edge never carries one of
    /// these. A caller that asks earlier must still get a query that means
    /// what it says: an unread cell asks for nothing rather than for anything.
    #[test]
    fn a_support_that_was_not_confidently_read_is_left_out_of_its_row() {
        let capture = capture(vec![row(
            0,
            "skill_a",
            vec![
                support(&["sup_a"], None, None, ReadState::Matched),
                support(&["sup_b"], None, None, ReadState::Unknown),
                support(&["sup_c"], None, None, ReadState::LowConfidence),
                support(&["sup_d"], None, None, ReadState::Ambiguous),
            ],
        )]);

        let query = build_capture_query(&capture, &no_vocab(), 3).expect("the confident cells build");

        let groups = groups(&query);
        assert_eq!(group_ids(&groups[0]), ["skill_a", "sup_a"]);
        assert_eq!(group_min(&groups[0]), 2, "three unread cells raise no minimum");
    }

    /// Defensive path, same reason. The skill is what makes a row a row: a
    /// group of supports with no skill comps every mercenary carrying those
    /// links, so the row is dropped whole rather than weakened.
    #[test]
    fn a_row_whose_skill_was_not_confidently_read_is_dropped_whole() {
        let mut second = row(1, "skill_b", vec![support(&["sup_b"], None, None, ReadState::Matched)]);
        second.skill.state = ReadState::LowConfidence;
        let capture = capture(vec![
            row(0, "skill_a", vec![support(&["sup_a"], None, None, ReadState::Matched)]),
            second,
        ]);

        let query = build_capture_query(&capture, &no_vocab(), 3).expect("the confident row builds");

        let groups = groups(&query);
        assert_eq!(groups.len(), 1, "the unread row must not become a support-only group");
        assert_eq!(group_ids(&groups[0]), ["skill_a", "sup_a"]);
    }

    /// Nothing expressible is not an empty search — an empty `stats` array
    /// would ask GGG for every mercenary in the league.
    #[test]
    fn a_capture_with_no_readable_row_has_no_query_at_all() {
        let mut only = row(0, "skill_a", vec![support(&["sup_a"], None, None, ReadState::Matched)]);
        only.skill.state = ReadState::Unknown;

        assert!(build_capture_query(&capture(vec![only]), &no_vocab(), 3).is_none());
    }

    /// The floor is where the loosening stops. At 1 the row accepts every
    /// weaker grade of the family — and NOT the other tier-3 grade, which is a
    /// different support the hover confirmed this cell is not.
    #[test]
    fn a_confirmed_tier_3_cell_at_floor_1_gains_the_weaker_grades_but_not_its_tier_3_sibling() {
        let capture = capture(vec![row(
            0,
            "skill_a",
            vec![support(&["sup_greater_chain"], Some("Chain"), Some(3), ReadState::Confirmed)],
        )]);

        let query = build_capture_query(&capture, &chain_vocab(), 1).expect("the row builds");

        let groups = groups(&query);
        assert_eq!(
            group_ids(&groups[0]),
            ["skill_a", "sup_greater_chain", "sup_lesser_chain", "sup_chain"],
        );
        assert!(
            !group_ids(&groups[0]).contains(&"sup_gilded_chain".to_string()),
            "re-widening a confirmed grade would undo the hover that confirmed it",
        );
        assert_eq!(group_min(&groups[0]), 2, "a loosened cell is still one cell");
    }

    /// The shipped default: the mercenary exactly as read, nothing added.
    #[test]
    fn the_exact_floor_asks_only_for_what_was_read() {
        let capture = capture(vec![row(
            0,
            "skill_a",
            vec![support(&["sup_greater_chain"], Some("Chain"), Some(3), ReadState::Confirmed)],
        )]);

        let query = build_capture_query(&capture, &chain_vocab(), 3).expect("the row builds");

        assert_eq!(group_ids(&groups(&query)[0]), ["skill_a", "sup_greater_chain"]);
    }

    /// The boundary between the two: a floor of 2 names tier 2 and stops. The
    /// range is `floor..tier`, so an off-by-one at either end shows up here.
    #[test]
    fn a_floor_of_2_adds_the_middle_grade_and_not_the_lowest() {
        let capture = capture(vec![row(
            0,
            "skill_a",
            vec![support(&["sup_greater_chain"], Some("Chain"), Some(3), ReadState::Confirmed)],
        )]);

        let query = build_capture_query(&capture, &chain_vocab(), 2).expect("the row builds");

        assert_eq!(
            group_ids(&groups(&query)[0]),
            ["skill_a", "sup_greater_chain", "sup_chain"],
        );
    }

    /// A floor outside 1..=3 is clamped rather than refused — the settings
    /// loader clamps too, and a query builder that panicked or widened without
    /// bound on a hand-edited file would take the whole capture down with it.
    ///
    /// Asserted on the EXPANDED id set and not on the hash alone: two illegal
    /// floors that clamp to the same wrong tier still share a hash, so hash
    /// equality says the clamp is a function of its input and nothing about
    /// which tier it lands on. The set is what pins the bound.
    #[test]
    fn a_floor_below_the_lowest_tier_asks_what_the_lowest_tier_asks() {
        let capture = capture(vec![row(
            0,
            "skill_a",
            vec![support(&["sup_greater_chain"], Some("Chain"), Some(3), ReadState::Confirmed)],
        )]);

        let floor_0 = build_capture_query(&capture, &chain_vocab(), 0).expect("the row builds");
        let floor_1 = build_capture_query(&capture, &chain_vocab(), 1).expect("the row builds");

        assert_eq!(
            group_ids(&groups(&floor_0)[0]),
            ["skill_a", "sup_greater_chain", "sup_lesser_chain", "sup_chain"],
            "floor 0 must ask for tier 1 and up, exactly as floor 1 does",
        );
        assert_eq!(floor_0.hash, floor_1.hash, "0 must ask exactly what 1 asks");
    }

    /// The other end of the same clamp, its own test because a bound is its own
    /// boundary: a clamp narrowed to `1..=2` leaves the floor-0 case above
    /// passing and only shows up here, as a tier-2 grade the user never asked
    /// for.
    #[test]
    fn a_floor_above_the_highest_tier_asks_what_the_highest_tier_asks() {
        let capture = capture(vec![row(
            0,
            "skill_a",
            vec![support(&["sup_greater_chain"], Some("Chain"), Some(3), ReadState::Confirmed)],
        )]);

        let floor_9 = build_capture_query(&capture, &chain_vocab(), 9).expect("the row builds");
        let floor_3 = build_capture_query(&capture, &chain_vocab(), 3).expect("the row builds");

        assert_eq!(
            group_ids(&groups(&floor_9)[0]),
            ["skill_a", "sup_greater_chain"],
            "floor 9 must ask for the mercenary exactly as read, as floor 3 does",
        );
        assert_eq!(floor_9.hash, floor_3.hash, "9 must ask exactly what 3 asks");
    }

    /// Five families' worth of loosening blows the 35-filter cap. The app's own
    /// widening goes first — the user did not read those grades off the panel,
    /// and every cell they DID read is still in the query afterwards.
    #[test]
    fn a_query_over_the_cap_drops_the_loosening_before_any_cell() {
        let vocab = MercVocab::from_stats(
            (0..5)
                .flat_map(|f| {
                    (1..=2).flat_map(move |t| {
                        (0..3).map(move |n| stat(&format!("sup_f{f}_t{t}_{n}"), &format!("F{f}"), t))
                    })
                })
                .collect(),
        );
        let rows = (0..2)
            .map(|r| {
                row(
                    r,
                    &format!("skill_{r}"),
                    (0..4)
                        .map(|f| {
                            support(
                                &[&format!("sup_read_{r}_{f}")],
                                Some(&format!("F{f}")),
                                Some(3),
                                ReadState::Matched,
                            )
                        })
                        .collect(),
                )
            })
            .collect();
        let capture = capture(rows);

        let loose = build_capture_query(&capture, &vocab, 1).expect("the rows build");

        assert!(loose.truncated, "the user must be told the search was widened less than asked");
        assert_eq!(loose.filter_count, 10, "2 rows of skill + 4 read cells");
        let groups = groups(&loose);
        assert_eq!(group_min(&groups[0]), 5, "every cell the capture read is kept");
        assert_eq!(group_min(&groups[1]), 5);
        assert!(
            !group_ids(&groups[0]).iter().any(|id| id.starts_with("sup_f")),
            "no expanded grade may survive the cap: {:?}",
            group_ids(&groups[0]),
        );
    }

    /// Still over the cap with no loosening left to drop: support cells go
    /// from the LAST row inward, and every row keeps its skill.
    #[test]
    fn a_query_over_the_cap_at_the_exact_floor_drops_cells_from_the_last_row_inward() {
        let rows = (0..5)
            .map(|r| {
                row(
                    r,
                    &format!("skill_{r}"),
                    (0..7)
                        .map(|c| support(&[&format!("sup_{r}_{c}")], None, None, ReadState::Matched))
                        .collect(),
                )
            })
            .collect();

        let query = build_capture_query(&capture(rows), &no_vocab(), 3).expect("the rows build");

        assert!(query.truncated);
        assert_eq!(query.filter_count, MAX_FILTERS, "degraded to the cap, not past it");
        let groups = groups(&query);
        assert_eq!(group_min(&groups[0]), 8, "the first row is untouched");
        assert_eq!(group_min(&groups[4]), 3, "five cells came off the last row");
        assert_eq!(group_ids(&groups[4])[0], "skill_4", "a row never loses its skill");
    }

    /// The query the whole feature is built on: securable, priced, cheapest
    /// first. An ilvl filter or a `collapse` would answer a different question
    /// than "what does THIS mercenary sell for".
    #[test]
    fn the_query_asks_for_securable_priced_listings_cheapest_first() {
        let capture = capture(vec![row(0, "skill_a", vec![])]);

        let query = build_capture_query(&capture, &no_vocab(), 3).expect("a bare skill row builds");

        assert_eq!(query.body["query"]["status"]["option"], "securable");
        assert_eq!(
            query.body["query"]["filters"]["trade_filters"]["filters"]["sale_type"]["option"],
            "priced",
        );
        assert_eq!(query.body["sort"]["price"], "asc");
        assert!(
            query.body["query"]["filters"]["misc_filters"].is_null(),
            "no ilvl floor: {}",
            query.body,
        );
    }

    /// The hash is the capture's identity across the cache, the dedupe and the
    /// late-result discard. Two captures asking the same question share it.
    #[test]
    fn two_captures_asking_the_same_question_share_a_hash() {
        let one = capture(vec![row(
            0,
            "skill_a",
            vec![support(&["sup_a"], None, None, ReadState::Matched)],
        )]);
        let two = capture(vec![row(
            0,
            "skill_a",
            vec![support(&["sup_a"], None, None, ReadState::Confirmed)],
        )]);

        let one = build_capture_query(&one, &no_vocab(), 3).expect("builds");
        let two = build_capture_query(&two, &no_vocab(), 3).expect("builds");

        assert_eq!(
            one.hash, two.hash,
            "a hover-confirm of a cell already matched asks the same question",
        );
    }

    /// …and a capture asking a different one does not, or a hover-confirm that
    /// corrects a cell would be answered out of the cache with the wrong
    /// mercenary's listings.
    #[test]
    fn a_corrected_cell_changes_the_hash() {
        let before = capture(vec![row(
            0,
            "skill_a",
            vec![support(&["sup_a"], None, None, ReadState::Matched)],
        )]);
        let after = capture(vec![row(
            0,
            "skill_a",
            vec![support(&["sup_b"], None, None, ReadState::Confirmed)],
        )]);

        let before = build_capture_query(&before, &no_vocab(), 3).expect("builds");
        let after = build_capture_query(&after, &no_vocab(), 3).expect("builds");

        assert_ne!(before.hash, after.hash);
    }

    /// Row order is part of the question: the hash must not collapse two
    /// different mercenaries onto one cache entry.
    #[test]
    fn two_captures_whose_rows_differ_do_not_share_a_hash() {
        let one = capture(vec![row(0, "skill_a", vec![]), row(1, "skill_b", vec![])]);
        let two = capture(vec![row(0, "skill_b", vec![]), row(1, "skill_a", vec![])]);

        let one = build_capture_query(&one, &no_vocab(), 3).expect("builds");
        let two = build_capture_query(&two, &no_vocab(), 3).expect("builds");

        assert_ne!(one.hash, two.hash, "the stats array is ordered data");
    }

    // -----------------------------------------------------------------------
    // The trade-site link
    // -----------------------------------------------------------------------

    /// `%XX` back to bytes — the inverse of the JS `encodeURIComponent` the
    /// link is built with.
    fn percent_decode(raw: &str) -> String {
        let bytes = raw.as_bytes();
        let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).expect("ascii hex");
                out.push(u8::from_str_radix(hex, 16).expect("a percent escape is hex"));
                i += 3;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        String::from_utf8(out).expect("the encoding is UTF-8")
    }

    /// Cross-language parity, pinned by a SHARED fixture rather than by two
    /// literals that can drift apart:
    /// `lib/mercenaries/__fixtures__/capture-query.expected.json` is the query
    /// object [`parity_capture`] builds, and `trade-links.test.ts` sends that
    /// same file through `derivedSearchUrl` and asserts the same round trip. A
    /// change to either side's envelope now fails on its own side.
    ///
    /// The `q` parameter carries `{"query": ...}` and NO `sort`, because the
    /// trade site owns the sort control — the strict comparison below is what
    /// says so: a `sort` riding along out of the request body would be an
    /// extra key.
    ///
    /// Compared as JSON, not as text — `serde_json` renders keys sorted while
    /// the TS object literal keeps insertion order, so the two encoded strings
    /// name one search without matching byte for byte.
    #[test]
    fn the_link_carries_the_shared_fixture_query_under_a_bare_query_envelope() {
        let query = build_capture_query(&parity_capture(), &chain_vocab(), 2).expect("builds");
        let expected: Value = serde_json::from_str(include_str!(
            "../../../src/lib/mercenaries/__fixtures__/capture-query.expected.json"
        ))
        .expect("the parity fixture is JSON");

        let url = capture_url("Allflame", &query);

        let (base, encoded) = url.split_once("?q=").expect("the link carries a q parameter");
        assert_eq!(base, "https://www.pathofexile.com/trade/search/Allflame");
        let decoded: Value =
            serde_json::from_str(&percent_decode(encoded)).expect("q decodes to JSON");
        assert_eq!(decoded, json!({ "query": expected }));
    }

    /// The league is a path segment encoded the way `encodeURIComponent` does
    /// it — a raw space would make the link 404 on a two-word league.
    #[test]
    fn a_league_with_a_space_is_percent_encoded_in_the_link() {
        let capture = capture(vec![row(0, "skill_a", vec![])]);
        let query = build_capture_query(&capture, &no_vocab(), 3).expect("builds");

        let url = capture_url("Hardcore Allflame", &query);

        assert!(
            url.starts_with("https://www.pathofexile.com/trade/search/Hardcore%20Allflame?q="),
            "got {url}",
        );
    }

    // -----------------------------------------------------------------------
    // The trigger policy
    // -----------------------------------------------------------------------

    fn query_of(tag: &str) -> CaptureQuery {
        CaptureQuery {
            body: json!({"query": {"stats": [tag]}}),
            hash: format!("hash-{tag}"),
            filter_count: 1,
            truncated: false,
        }
    }

    /// The ordinary input: auto on, league known, nothing cached.
    fn live(query: &CaptureQuery) -> TriggerInput<'_> {
        TriggerInput {
            auto: true,
            query: Some(query),
            league_resolved: true,
            cached: false,
        }
    }

    /// Hand the session a query and let the debounce window pass, returning
    /// what it settles on. Only valid where the first call debounces.
    fn after_debounce(
        session: &mut MercTradeSession,
        query: &CaptureQuery,
        at: u64,
    ) -> TriggerAction {
        assert_eq!(
            decide(session, live(query), at),
            TriggerAction::Debounce,
            "precondition: a query that has just moved is not acted on",
        );
        decide(session, live(query), at + DEBOUNCE_MS)
    }

    /// Every hover-confirm moves the hash. Searching each one would spend the
    /// session's whole budget correcting a single capture, so the changes
    /// coalesce: only the query that stopped moving is searched for.
    #[test]
    fn several_hash_changes_inside_the_debounce_window_cost_one_search() {
        let mut session = MercTradeSession::new();
        let (first, second, third) = (query_of("a"), query_of("b"), query_of("c"));

        assert_eq!(decide(&mut session, live(&first), 0), TriggerAction::Debounce);
        assert_eq!(decide(&mut session, live(&second), 500), TriggerAction::Debounce);
        assert_eq!(decide(&mut session, live(&third), 1_000), TriggerAction::Debounce);
        assert_eq!(decide(&mut session, live(&third), 3_000), TriggerAction::Enqueue);

        assert_eq!(session.searches_used, 1, "three corrections, one search");
    }

    /// The loop asks ten times a second. A hash already answered asks for
    /// nothing at all — not a re-publish, and certainly not a second search.
    #[test]
    fn a_hash_the_session_has_already_acted_on_asks_for_nothing() {
        let mut session = MercTradeSession::new();
        let query = query_of("a");
        assert_eq!(after_debounce(&mut session, &query, 0), TriggerAction::Enqueue);

        assert_eq!(decide(&mut session, live(&query), 9_000), TriggerAction::None);
        assert_eq!(session.searches_used, 1);
    }

    /// The ceiling: after three searches the session stops asking GGG and
    /// hands the user the link instead.
    #[test]
    fn the_fourth_query_of_a_session_is_answered_with_the_link_instead_of_a_search() {
        let mut session = MercTradeSession::new();
        let queries = [query_of("a"), query_of("b"), query_of("c"), query_of("d")];
        let mut at = 0;
        for query in &queries[..MAX_SEARCHES as usize] {
            assert_eq!(after_debounce(&mut session, query, at), TriggerAction::Enqueue);
            at += 10_000;
        }

        let action = after_debounce(&mut session, &queries[3], at);

        assert_eq!(action, TriggerAction::UrlOnly);
        assert_eq!(session.searches_used, MAX_SEARCHES, "the ceiling holds");
    }

    /// A re-detected window asks the same question as the one that retired;
    /// the cache answers it for free, so the fresh budget stays unspent.
    #[test]
    fn a_cached_hash_is_published_without_spending_a_search() {
        let mut session = MercTradeSession::new();
        let query = query_of("a");

        let action = decide(
            &mut session,
            TriggerInput { cached: true, ..live(&query) },
            0,
        );

        assert_eq!(action, TriggerAction::PublishCached);
        assert_eq!(session.searches_used, 0, "a cache hit is not a search");
    }

    /// The user's toggle. Published once — the caller asks at the capture
    /// loop's cadence, and a re-publish is a slice clone ten times a second.
    #[test]
    fn auto_off_publishes_idle_once() {
        let mut session = MercTradeSession::new();
        let query = query_of("a");
        let off = TriggerInput { auto: false, ..live(&query) };

        assert_eq!(decide(&mut session, off, 0), TriggerAction::SetIdle);
        assert_eq!(decide(&mut session, off, 100), TriggerAction::None);
    }

    /// No league, no search: the trade site cannot be addressed without one,
    /// and guessing comps the capture against another economy.
    #[test]
    fn an_unresolved_league_waits_instead_of_searching() {
        let mut session = MercTradeSession::new();
        let query = query_of("a");
        let no_league = TriggerInput { league_resolved: false, ..live(&query) };

        assert_eq!(decide(&mut session, no_league, 0), TriggerAction::SetWaitingLeague);
        assert_eq!(decide(&mut session, no_league, 100), TriggerAction::None);
        assert_eq!(session.searches_used, 0);
    }

    /// A capture that stops being expressible has to clear its link and
    /// listings, and only the `NoQuery` refusal does that. Deduplicating it
    /// against a standing auto-off would leave the previous capture's answer
    /// on screen.
    #[test]
    fn a_capture_that_stops_being_expressible_publishes_again_under_a_standing_auto_off() {
        let mut session = MercTradeSession::new();
        let query = query_of("a");
        assert_eq!(
            decide(&mut session, TriggerInput { auto: false, ..live(&query) }, 0),
            TriggerAction::SetIdle,
        );

        let action = decide(
            &mut session,
            TriggerInput { auto: false, query: None, ..live(&query) },
            1_000,
        );

        assert_eq!(action, TriggerAction::SetIdle, "the clear must not be swallowed");
    }

    /// A failed lookup is not an answer, so the hash it failed on is not one
    /// either: a later publish retries it — against the same ceiling.
    ///
    /// The retry has to arrive: the failure moves the debounce clock once, and
    /// a version that moved it on every tick that observes `Error` would sit
    /// behind a window that never closes.
    #[test]
    fn the_hash_a_lookup_failed_on_is_searched_again() {
        let mut session = MercTradeSession::new();
        let query = query_of("a");
        assert_eq!(after_debounce(&mut session, &query, 0), TriggerAction::Enqueue);

        session.state = MercTradeStatus::Error;

        assert_eq!(after_debounce(&mut session, &query, 9_000), TriggerAction::Enqueue);
        assert_eq!(session.searches_used, 2, "a retry is still a search");
    }

    /// …but not immediately. The lookup task publishes `Error` and the capture
    /// loop comes back 100 ms later, so a retry that ignored the debounce
    /// would spend the whole three-search budget on one transient failure
    /// inside half a second.
    #[test]
    fn a_retry_waits_out_the_debounce_window_the_failure_started() {
        let mut session = MercTradeSession::new();
        let query = query_of("a");
        assert_eq!(after_debounce(&mut session, &query, 0), TriggerAction::Enqueue);

        session.state = MercTradeStatus::Error;

        assert_eq!(decide(&mut session, live(&query), 9_000), TriggerAction::Debounce);
        assert_eq!(
            decide(&mut session, live(&query), 9_000 + DEBOUNCE_MS - 1),
            TriggerAction::Debounce,
            "the window is measured from the failure, not from the last search",
        );
        assert_eq!(session.searches_used, 1, "nothing is spent inside the window");
    }

    /// Leaving a settled refusal re-decides from scratch. Without that, the
    /// `Idle` published over the answer would stand for the rest of the
    /// capture and switching the toggle back on would go silent.
    #[test]
    fn switching_the_auto_search_back_on_republishes_the_answer() {
        let mut session = MercTradeSession::new();
        let query = query_of("a");
        assert_eq!(after_debounce(&mut session, &query, 0), TriggerAction::Enqueue);
        assert_eq!(
            decide(&mut session, TriggerInput { auto: false, ..live(&query) }, 5_000),
            TriggerAction::SetIdle,
        );

        let action = decide(
            &mut session,
            TriggerInput { cached: true, ..live(&query) },
            6_000,
        );

        assert_eq!(action, TriggerAction::PublishCached);
    }

    /// The budget belongs to the capture, not to the app run: the window that
    /// replaces this one gets its own three searches.
    #[test]
    fn a_new_session_starts_with_a_full_search_budget() {
        let mut spent = MercTradeSession::new();
        let queries = [query_of("a"), query_of("b"), query_of("c"), query_of("d")];
        let mut at = 0;
        for query in &queries[..MAX_SEARCHES as usize] {
            after_debounce(&mut spent, query, at);
            at += 10_000;
        }
        assert_eq!(after_debounce(&mut spent, &queries[3], at), TriggerAction::UrlOnly);

        let mut fresh = MercTradeSession::new();

        assert_eq!(after_debounce(&mut fresh, &queries[3], at), TriggerAction::Enqueue);
        assert_eq!(fresh.searches_used, 1);
    }

    // -----------------------------------------------------------------------
    // What a finished lookup does to the slice
    // -----------------------------------------------------------------------

    fn merc_result(hash: &str) -> MercTradeResult {
        MercTradeResult {
            query_hash: hash.to_string(),
            league: "Allflame".to_string(),
            total: 7,
            listings: Vec::new(),
            floor_chaos: 0.0,
            median_chaos: 0.0,
            fetched_at_ms: 0,
            truncated: false,
        }
    }

    /// The cache's two contracts: what a hit is, and what ages out.
    ///
    /// `league` is on the key because the hash is not computed over it — the
    /// query body names the mercenary, and nothing in it says which economy the
    /// prices came from.
    fn cached_in(league: &str, hash: &str) -> MercTradeResult {
        MercTradeResult {
            league: league.to_string(),
            ..merc_result(hash)
        }
    }

    #[test]
    fn a_result_is_served_back_for_the_league_and_hash_it_was_stored_under() {
        let mut cache = MercResultCache::new();
        cache_insert(&mut cache, 1_000, cached_in("Allflame", "hash-a"));

        let hit = cache_get(&cache, "Allflame", "hash-a", 2_000);

        assert_eq!(hit, Some(cached_in("Allflame", "hash-a")));
    }

    /// The regression the league half of the key exists for: the same
    /// mercenary hashes the same in every league, so a standard-league capture
    /// would otherwise be answered with the temp league's prices for a quarter
    /// of an hour.
    #[test]
    fn the_same_hash_in_another_league_is_not_a_cache_hit() {
        let mut cache = MercResultCache::new();
        cache_insert(&mut cache, 1_000, cached_in("Allflame", "hash-a"));

        assert_eq!(cache_get(&cache, "Standard", "hash-a", 2_000), None);
    }

    /// The TTL is what stops a re-detected window from quoting a price the
    /// market has moved past. Read at the boundary: one ms inside the window
    /// still serves.
    #[test]
    fn a_result_older_than_the_ttl_is_not_served() {
        let mut cache = MercResultCache::new();
        cache_insert(&mut cache, 1_000, cached_in("Allflame", "hash-a"));

        assert!(cache_get(&cache, "Allflame", "hash-a", 1_000 + RESULT_TTL_MS - 1).is_some());
        assert_eq!(cache_get(&cache, "Allflame", "hash-a", 1_000 + RESULT_TTL_MS), None);
    }

    /// The map only grows on the insert path, so the insert is where it is
    /// walked — an app left running all league would otherwise hold every
    /// mercenary it ever comped.
    #[test]
    fn an_insert_drops_the_entries_that_have_aged_out() {
        let mut cache = MercResultCache::new();
        cache_insert(&mut cache, 1_000, cached_in("Allflame", "hash-old"));

        cache_insert(
            &mut cache,
            1_000 + RESULT_TTL_MS,
            cached_in("Allflame", "hash-new"),
        );

        assert_eq!(
            cache.keys().collect::<Vec<_>>(),
            [&("Allflame".to_string(), "hash-new".to_string())],
        );
    }

    fn waiting_on(hash: &str, status: MercTradeStatus) -> MercTradeState {
        MercTradeState {
            status,
            query_hash: Some(hash.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn a_result_for_the_hash_the_slice_is_waiting_on_lands_as_done() {
        let mut trade = waiting_on("hash-a", MercTradeStatus::Searching);

        accept_result(&mut trade, "hash-a", merc_result("hash-a"));

        assert_eq!(trade.status, MercTradeStatus::Done);
        assert_eq!(trade.result, Some(merc_result("hash-a")));
    }

    /// The capture moved on while the lookup was in flight — a hover-confirm,
    /// a rematch, a tier-floor change. Showing those listings would put one
    /// mercenary's prices under another's link.
    #[test]
    fn a_result_answering_a_question_the_capture_has_moved_on_from_is_discarded() {
        let mut trade = waiting_on("hash-new", MercTradeStatus::Searching);

        accept_result(&mut trade, "hash-old", merc_result("hash-old"));

        assert_eq!(trade.status, MercTradeStatus::Searching, "the newer lookup still owns the slice");
        assert_eq!(trade.result, None);
    }

    /// The retire published `Idle` before this task woke, so the canceller
    /// owns the state. Writing `cancelled` over it would show a failure under
    /// a link that works.
    #[test]
    fn a_cancel_that_lands_on_a_settled_slice_changes_nothing() {
        let mut trade = waiting_on("hash-a", MercTradeStatus::Idle);

        accept_error(&mut trade, "hash-a", CANCELLED.to_string());

        assert_eq!(trade.status, MercTradeStatus::Idle);
        assert_eq!(trade.error, None);
    }

    /// The case it must NOT leave alone: a cancel can stop a lookup whose
    /// session never published anything about it, and that slice would
    /// otherwise wait for a result that is never coming.
    #[test]
    fn a_cancel_that_lands_while_the_slice_still_waits_ends_the_wait() {
        let mut trade = waiting_on("hash-a", MercTradeStatus::Queued);

        accept_error(&mut trade, "hash-a", CANCELLED.to_string());

        assert_eq!(trade.status, MercTradeStatus::Error);
        assert_eq!(trade.error.as_deref(), Some(CANCELLED));
    }

    /// The deference is to CANCELS only. A real failure is reported wherever
    /// the slice stands, or a lookup that errored after a cached publish would
    /// leave stale listings looking live.
    #[test]
    fn a_real_failure_is_reported_over_a_settled_slice() {
        let mut trade = waiting_on("hash-a", MercTradeStatus::Done);
        trade.result = Some(merc_result("hash-a"));

        accept_error(&mut trade, "hash-a", "Rate limited by GGG (429)".to_string());

        assert_eq!(trade.status, MercTradeStatus::Error);
        assert_eq!(trade.error.as_deref(), Some("Rate limited by GGG (429)"));
        assert_eq!(trade.result, None, "the listings did not survive their own error");
    }

    #[test]
    fn a_failure_answering_a_stale_hash_is_discarded() {
        let mut trade = waiting_on("hash-new", MercTradeStatus::Queued);

        accept_error(&mut trade, "hash-old", "boom".to_string());

        assert_eq!(trade.status, MercTradeStatus::Queued);
        assert_eq!(trade.error, None);
    }

    /// `Fetching` is the queue saying the request is going out now — the
    /// difference between "waiting behind the rate limiter" and "asking GGG".
    #[test]
    fn the_fetching_event_moves_a_queued_lookup_to_searching() {
        let mut trade = waiting_on("hash-a", MercTradeStatus::Queued);

        note_fetching(&mut trade, "hash-a");

        assert_eq!(trade.status, MercTradeStatus::Searching);
    }

    /// A slow lookup must not relabel the capture that replaced it.
    #[test]
    fn the_fetching_event_of_a_superseded_lookup_leaves_the_slice_alone() {
        let mut trade = waiting_on("hash-new", MercTradeStatus::Queued);

        note_fetching(&mut trade, "hash-old");

        assert_eq!(trade.status, MercTradeStatus::Queued);
    }

    /// …nor revive a session something else has already finished or cancelled.
    #[test]
    fn the_fetching_event_does_not_revive_a_slice_that_is_no_longer_queued() {
        let mut trade = waiting_on("hash-a", MercTradeStatus::Error);

        note_fetching(&mut trade, "hash-a");

        assert_eq!(trade.status, MercTradeStatus::Error);
    }

    /// A retire cancels the shared queue only from the two statuses that have
    /// something on it.
    #[test]
    fn a_retire_cancels_the_queue_while_a_lookup_is_in_flight() {
        assert!(awaiting_lookup(MercTradeStatus::Queued));
        assert!(awaiting_lookup(MercTradeStatus::Searching));
    }

    /// From anywhere else the cancel is not just needless: `close_session`
    /// publishes `Idle` alongside it, and doing that from `Done` would throw
    /// away the retired capture's answer — which the page goes on showing.
    #[test]
    fn a_retire_leaves_a_settled_status_alone() {
        for status in [
            MercTradeStatus::Off,
            MercTradeStatus::Idle,
            MercTradeStatus::WaitingLeague,
            MercTradeStatus::Done,
            MercTradeStatus::Error,
        ] {
            assert!(!awaiting_lookup(status), "{status:?} has nothing on the queue");
        }
    }

    /// The merc lookup reaches `TradeApiClient` directly and never the
    /// `trade_lookup` command, whose result POST feeds the server's shared GEM
    /// cache keyed by (gem, variant) — a mercenary search has neither, and the
    /// design's "fully local" promise is this file not knowing the server
    /// exists.
    ///
    /// Asserted over the source because there is no seam to count calls
    /// through: the submit is written inline in `lib.rs`'s `trade_lookup`
    /// command body, not behind a helper a test could stub. `trade_lookup` is
    /// a needle in its own right — it is reachable from here (a private item
    /// of the crate root is visible to every descendant module), so scanning
    /// only for the server strings would miss the one call that actually
    /// reintroduces the submit. Comment lines are stripped so the doc comments
    /// that EXPLAIN the rule do not trip it; the test half is split off
    /// before the scan, so these needles cannot match themselves.
    #[test]
    fn the_mercenary_lookup_path_never_reaches_the_servers_submit_endpoint() {
        let source = include_str!("search.rs");
        let production = source
            .split("\n#[cfg(test)]")
            .next()
            .expect("the file has a production half");
        let code: String = production
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");

        for needle in ["trade/submit", "trade_lookup", "server_http", "server_url"] {
            assert!(
                !code.contains(needle),
                "the mercenary trade path must not reach the server ({needle})",
            );
        }
    }
}
