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

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::read::confident;
use super::vocab::MercVocab;
use super::{MercCapture, MercRow};

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

/// Where the merc trade lookup stands, as the page and the overlay read it.
///
/// Defined here for now; it belongs with the rest of the merc slice types and
/// moves there when the slice gains its `trade` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MercTradeStatus {
    /// The module is off — nothing to say.
    Off,
    /// Nothing to search: no capture, no query, or the user turned the
    /// auto-search off.
    Idle,
    /// A query exists but the league is not resolved yet, so nothing was
    /// enqueued. Distinct from `Error`: nothing failed, the app just cannot
    /// address a trade site without a league.
    WaitingLeague,
    /// Handed to the trade queue, waiting for its turn behind the rate limiter.
    Queued,
    /// In flight.
    Searching,
    /// A result is on the slice.
    Done,
    /// The lookup failed; the message is on the slice.
    Error,
}

impl Default for MercTradeStatus {
    fn default() -> Self {
        MercTradeStatus::Idle
    }
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
/// Ordered so the cheap refusals come first and nothing below them has to
/// re-check them. The one non-obvious rule is the `Error` clause: a failed
/// lookup makes the session forget what it searched for, so the very next
/// publish of the same capture may retry — still against the same ceiling, so a
/// persistently failing query costs at most [`MAX_SEARCHES`] attempts.
pub fn decide(
    session: &mut MercTradeSession,
    input: TriggerInput<'_>,
    now_ms: u64,
) -> TriggerAction {
    if !input.auto {
        return TriggerAction::SetIdle;
    }
    let Some(query) = input.query else {
        return TriggerAction::SetIdle;
    };
    if !input.league_resolved {
        return TriggerAction::SetWaitingLeague;
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
