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

use crate::trade::client::CANCELLED;
use crate::trade::{MercTradeListing, MercTradeResult, RawSearch, TradeQueueEvent, TradeSource};
use crate::AppState;

use super::read::confident;
use super::run::publish;
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
    /// on `Error` after a successful retry re-enqueues the same query on every
    /// publish — bounded only by [`MAX_SEARCHES`].
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
/// one non-obvious rule is the `Error` clause: a failed
/// lookup makes the session forget what it searched for, so the very next
/// publish of the same capture may retry — still against the same ceiling, so a
/// persistently failing query costs at most [`MAX_SEARCHES`] attempts.
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
    // either.
    let acted = if session.state == MercTradeStatus::Error {
        None
    } else {
        session.last_hash.as_deref()
    };
    if acted == Some(query.hash.as_str()) {
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
///   back once (see [`Settled`]), so the steady state does no work at all.
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
    let cached = session
        .built_query
        .as_ref()
        .and_then(|q| cache_get(&state, &q.hash, now_ms));

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
        matches!(
            slice.trade.status,
            MercTradeStatus::Queued | MercTradeStatus::Searching
        )
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
                            if slice.trade.query_hash.as_deref() == Some(emit_hash.as_str())
                                && slice.trade.status == MercTradeStatus::Queued
                            {
                                slice.trade.status = MercTradeStatus::Searching;
                            }
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
                    cache_insert(&state, now_ms(), result.clone());
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
                publish(&app, |slice| {
                    // A result that no longer answers the slice's question is
                    // dropped whole: the capture moved on while this was in
                    // flight (a hover-confirm, a rematch, a tier-floor change).
                    if slice.trade.query_hash.as_deref() != Some(hash.as_str()) {
                        return;
                    }
                    slice.trade.status = MercTradeStatus::Done;
                    slice.trade.result = Some(result);
                    slice.trade.error = None;
                });
            }
            // A cancel normally comes with its own publish — `close_session`
            // sets `Idle` before this task wakes — so the canceller owns the
            // state and this arm leaves it alone. The one case it must NOT
            // leave alone is a slice still reading `Queued`/`Searching`: the
            // cancel epoch can stop a lookup whose session never published
            // anything about it (a cancel that lands in the gap between the
            // enqueue and the snapshot), and without this that session waits
            // for a result that is never coming.
            Err(e) => {
                if e != CANCELLED {
                    crate::app_log(&app, format!("Merc trade error: {label} — {e}"));
                }
                publish(&app, |slice| {
                    if slice.trade.query_hash.as_deref() != Some(hash.as_str()) {
                        return;
                    }
                    if e == CANCELLED
                        && !matches!(
                            slice.trade.status,
                            MercTradeStatus::Queued | MercTradeStatus::Searching
                        )
                    {
                        return;
                    }
                    slice.trade.status = MercTradeStatus::Error;
                    slice.trade.error = Some(e);
                    slice.trade.result = None;
                });
            }
        }
    });
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

/// The cached result for `hash`, if one is still young enough to serve.
fn cache_get(state: &AppState, hash: &str, now_ms: u64) -> Option<MercTradeResult> {
    let cache = state
        .merc_trade_cache
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    cache
        .get(hash)
        .filter(|(at, _)| now_ms.saturating_sub(*at) < RESULT_TTL_MS)
        .map(|(_, result)| result.clone())
}

/// Store a result under its own hash, dropping every entry that has aged out.
///
/// Pruned on insert rather than on a timer: the map only ever grows on this
/// path, so the write is the one moment it is worth walking, and a user who
/// captures nothing pays nothing.
fn cache_insert(state: &AppState, now_ms: u64, result: MercTradeResult) {
    let mut cache = state
        .merc_trade_cache
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    cache.retain(|_, (at, _)| now_ms.saturating_sub(*at) < RESULT_TTL_MS);
    cache.insert(result.query_hash.clone(), (now_ms, result));
}

/// Unix ms. Re-exported from the loop so both writers of the trade state read
/// one clock.
fn now_ms() -> u64 {
    super::run::now_ms()
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
