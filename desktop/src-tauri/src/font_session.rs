//! Font craft session state and its pure transitions.
//!
//! Everything here is Tauri-free: the font reader and the event handlers call
//! these functions under the session lock and do the logging, emitting and
//! spawning outside it. That split is what makes the round-boundary rules
//! testable — see `font_ledger` for the rule the boundary itself follows.

use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;

use crate::font_ledger::{CraftLedger, LedgerVerdict};
use crate::font_parser::{merge_options, CraftOption, FontPanelState};

/// Accumulated font session data — shared between the font reader and event handlers.
#[derive(Debug, Clone, Default, Serialize)]
pub struct FontSessionData {
    /// Sealed rounds (one per accepted craft-count change).
    pub rounds: Vec<FontRound>,
    /// Current round's detected options (not yet sealed).
    /// None = no options detected yet this round.
    #[serde(skip)]
    pub current_options: Option<Vec<CraftOption>>,
    /// Craft-count ledger — owns the round boundary.
    #[serde(skip)]
    pub ledger: CraftLedger,
}

/// A sealed font round — the options captured while one craft was available.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FontRound {
    pub options: Vec<CraftOption>,
    pub crafts_remaining: Option<i32>,
}

/// A round that just moved into `rounds`, for the caller to log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealedRound {
    /// 1-based position in the session.
    pub number: usize,
    pub round: FontRound,
}

/// What one font read did to the session, for the caller to log and emit
/// outside the session lock.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameOutcome {
    /// How the ledger read this frame's count.
    pub verdict: LedgerVerdict,
    /// The option buffer this frame merged into, after the merge.
    pub buffer: Vec<CraftOption>,
    /// True when the merge added an option type or filled in a value.
    pub buffer_grew: bool,
    /// Set when this frame closed a round.
    pub sealed: Option<SealedRound>,
    /// Crafts remaining now accepted (raw panel semantics).
    pub crafts_remaining: Option<i32>,
}

/// Apply one parsed font panel read to the session.
///
/// `event_seq` is the `FontOpened` counter as of this read; the ledger gates
/// every count change on it. Callers must only pass reads of an active panel
/// that actually carried options — an empty read carries no evidence either
/// way.
pub fn apply_font_frame(
    session: &mut FontSessionData,
    panel: &FontPanelState,
    event_seq: u64,
) -> FrameOutcome {
    let verdict = session.ledger.observe(panel.count_read, event_seq);

    let sealed = match verdict {
        LedgerVerdict::Seal { sealed_remaining } => {
            let round = FontRound {
                options: session.current_options.take().unwrap_or_default(),
                crafts_remaining: sealed_remaining,
            };
            session.rounds.push(round.clone());
            Some(SealedRound {
                number: session.rounds.len(),
                round,
            })
        }
        LedgerVerdict::Seed | LedgerVerdict::Same | LedgerVerdict::Ignored => None,
    };

    let previous = session.current_options.take().unwrap_or_default();
    let merged = merge_options(&previous, &panel.options);
    // A sealing read opens a new round from an empty buffer; report it as
    // growth even if nothing merged, so the new round's options are logged.
    let buffer_grew = merged != previous || sealed.is_some();
    session.current_options = Some(merged.clone());

    FrameOutcome {
        verdict,
        buffer: merged,
        buffer_grew,
        sealed,
        crafts_remaining: session.ledger.current(),
    }
}

/// Seal whatever the current round buffer still holds, at send time.
///
/// Used when the session ends without another count change — the player left
/// the area, or clicked CRAFT on the last craft and walked out. The round is
/// labelled with the count the ledger currently accepts.
///
/// Returns `None` when nothing was captured, so an empty round is never
/// appended (it would inflate the session's craft total).
pub fn seal_leftover_round(session: &mut FontSessionData) -> Option<SealedRound> {
    let options = session.current_options.take()?;
    let round = FontRound {
        options,
        crafts_remaining: session.ledger.current(),
    };
    session.rounds.push(round.clone());
    Some(SealedRound {
        number: session.rounds.len(),
        round,
    })
}

/// Build the `POST /api/desktop/font-session` payload, or `None` when the
/// session has no rounds to report.
///
/// `total_crafts` is the larger of the ledger's highest accepted count and the
/// number of sealed rounds: a misread can mislabel one round, but it can
/// neither create more than one per event nor destroy one, so the round count
/// is the floor.
pub fn finalize_font_session(
    session: &FontSessionData,
    lab_type: &str,
    variant: &str,
    device_id: &str,
    pair_code: &str,
) -> Option<serde_json::Value> {
    if session.rounds.is_empty() {
        return None;
    }

    // The server rejects `total_crafts` outside 1..=20 and the session is gone
    // by the time the POST fails (`internal/server/handlers/font_session.go`),
    // so an out-of-range ledger total — a garbled "Crafts Remaining: 78"
    // accepted on its one frame — is dropped in favour of the rounds sealed.
    let ledger_total = session.ledger.total();
    let total_crafts = if (1..=20).contains(&ledger_total) {
        session.rounds.len().max(ledger_total as usize)
    } else {
        session.rounds.len()
    };

    Some(serde_json::json!({
        "lab_type": lab_type,
        "total_crafts": total_crafts,
        "variant": variant,
        "device_id": device_id,
        "pair_code": pair_code,
        "rounds": session.rounds.iter().map(|r| serde_json::json!({
            "craft_options": r.options,
            "crafts_remaining": r.crafts_remaining,
        })).collect::<Vec<_>>(),
    }))
}

/// What a `FontOpened` event does to the gem reader.
///
/// Sealing a round is deliberately not one of them: the event fires both when
/// the font is opened and when CRAFT is clicked, so it cannot mark a round
/// boundary. Only the panel's craft count can (`font_ledger`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontOpenedEffect {
    StartGemScan,
    StopGemScan,
}

/// Mint a read token, never handing out 0.
///
/// 0 is the contexts' "unarmed" value, so a context that owned it would read
/// as disarmed. `fetch_add(1) + 1` only produces 0 once the counter wraps,
/// which needs 2^64 arms — the skip costs one value and removes the state
/// entirely.
pub fn next_scan_generation(counter: &AtomicU64) -> u64 {
    let minted = counter.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
    if minted == 0 {
        counter.fetch_add(1, Ordering::SeqCst).wrapping_add(1)
    } else {
        minted
    }
}

/// Decide what a `FontOpened` event does to the gem reader.
///
/// `count` is the running event count for the current lab run; its odd/even
/// split is a known-imperfect CRAFT/CONFIRM guess that this module deliberately
/// leaves alone.
pub fn font_opened_effect(count: u32) -> FontOpenedEffect {
    if count % 2 == 1 {
        FontOpenedEffect::StartGemScan
    } else {
        FontOpenedEffect::StopGemScan
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font_parser::CountRead;

    fn opt(option_type: &str, value: Option<i32>) -> CraftOption {
        CraftOption {
            option_type: option_type.to_string(),
            text: format!("{} line", option_type),
            value,
        }
    }

    fn panel(options: Vec<CraftOption>, count_read: CountRead) -> FontPanelState {
        FontPanelState {
            options,
            crafts_remaining: match count_read {
                CountRead::Count(n) => Some(n),
                _ => None,
            },
            count_read,
            font_active: true,
            jackpot_detected: false,
        }
    }

    fn types_of(options: &[CraftOption]) -> Vec<&str> {
        options.iter().map(|o| o.option_type.as_str()).collect()
    }

    #[test]
    fn a_gift_lab_read_once_per_round_seals_seven_rounds_and_one_leftover_in_order() {
        // Gift: 8 crafts, one press per panel, each panel after its own
        // FontOpened (the CRAFT click). Each round carries a distinct option
        // value so the order is checkable.
        let mut session = FontSessionData::default();
        for (event, remaining) in (1..=8u64).zip((1..=8).rev()) {
            let read = if remaining == 1 { CountRead::LabelAbsent } else { CountRead::Count(remaining) };
            apply_font_frame(&mut session, &panel(vec![opt("quality", Some(remaining))], read), event);
        }
        let leftover = seal_leftover_round(&mut session).expect("the last panel's options must seal");

        let sealed: Vec<(Option<i32>, Option<i32>)> = session
            .rounds
            .iter()
            .map(|r| (r.crafts_remaining, r.options[0].value))
            .collect();
        assert_eq!(
            sealed,
            vec![
                (Some(8), Some(8)),
                (Some(7), Some(7)),
                (Some(6), Some(6)),
                (Some(5), Some(5)),
                (Some(4), Some(4)),
                (Some(3), Some(3)),
                (Some(2), Some(2)),
                (None, Some(1)),
            ],
        );
        assert_eq!(leftover.number, 8);
        let payload = finalize_font_session(&session, "Gift", "1/0", "dev", "pair").unwrap();
        assert_eq!(payload["total_crafts"], 8);
    }

    #[test]
    fn a_craft_click_seals_the_round_it_ended_on_the_next_panels_first_read() {
        let mut session = FontSessionData::default();
        let first = panel(vec![opt("transform_random", None), opt("quality", Some(20))], CountRead::Count(2));
        let second = panel(vec![opt("facetors_lens", Some(60))], CountRead::LabelAbsent);

        apply_font_frame(&mut session, &first, 1);
        let sealing = apply_font_frame(&mut session, &second, 2);

        assert_eq!(
            sealing.sealed,
            Some(SealedRound {
                number: 1,
                round: FontRound { options: first.options.clone(), crafts_remaining: Some(2) },
            }),
        );
        assert_eq!(types_of(session.current_options.as_deref().unwrap_or_default()), vec!["facetors_lens"]);
    }

    #[test]
    fn a_torn_read_with_a_wrong_count_only_contributes_its_options() {
        // No FontOpened between the reads, so the 6 cannot be a new panel —
        // but the option it read is still a real read of the current one.
        let mut session = FontSessionData::default();
        let steady = panel(vec![opt("transform_random", None), opt("quality", Some(20))], CountRead::Count(8));
        let torn = panel(vec![opt("sacrifice_keys", None)], CountRead::Count(6));

        apply_font_frame(&mut session, &steady, 1);
        apply_font_frame(&mut session, &torn, 1);
        let repeat = apply_font_frame(&mut session, &steady, 1);

        assert!(session.rounds.is_empty(), "a countless torn read must not seal a round");
        assert_eq!(
            types_of(session.current_options.as_deref().unwrap_or_default()),
            vec!["transform_random", "quality", "sacrifice_keys"],
        );
        assert!(!repeat.buffer_grew, "a read that repeats what the buffer already holds must not re-log it");
    }

    #[test]
    fn a_sealing_read_reports_growth_so_the_new_round_gets_logged() {
        let mut session = FontSessionData::default();
        let same_options = vec![opt("transform_random", None)];

        apply_font_frame(&mut session, &panel(same_options.clone(), CountRead::Count(2)), 1);
        let sealing = apply_font_frame(&mut session, &panel(same_options, CountRead::LabelAbsent), 2);

        assert!(sealing.sealed.is_some());
        assert!(sealing.buffer_grew, "a sealing read must report growth so the new round's options are logged");
    }

    #[test]
    fn a_session_with_nothing_buffered_seals_no_leftover() {
        let mut session = FontSessionData::default();

        assert_eq!(seal_leftover_round(&mut session), None);
        assert!(session.rounds.is_empty());
    }

    #[test]
    fn a_facetor_percentage_survives_into_the_payload() {
        let mut session = FontSessionData::default();
        session.rounds.push(FontRound {
            options: vec![opt("facetors_lens", Some(60))],
            crafts_remaining: Some(8),
        });

        let payload = finalize_font_session(&session, "Unknown", "1/0", "dev", "pair")
            .expect("a session with a round must produce a payload");

        assert_eq!(payload["rounds"][0]["craft_options"][0]["type"], "facetors_lens");
        assert_eq!(payload["rounds"][0]["craft_options"][0]["value"], 60);
    }

    #[test]
    fn rounds_the_reader_missed_still_count_towards_the_craft_total() {
        // A Gift run whose panel started at 8: five rounds went unread, but
        // the run still had 8 crafts.
        let mut session = FontSessionData::default();
        for _ in 0..3 {
            session.rounds.push(FontRound {
                options: vec![opt("transform_random", None)],
                crafts_remaining: Some(8),
            });
        }
        session.ledger.observe(CountRead::Count(8), 1);

        let payload = finalize_font_session(&session, "Unknown", "1/0", "dev", "pair").unwrap();

        assert_eq!(payload["total_crafts"], 8);
    }

    #[test]
    fn more_sealed_rounds_than_the_ledger_counted_raise_the_craft_total() {
        let mut session = FontSessionData::default();
        for _ in 0..3 {
            session.rounds.push(FontRound {
                options: vec![opt("transform_random", None)],
                crafts_remaining: None,
            });
        }
        session.ledger.observe(CountRead::LabelAbsent, 1);

        let payload = finalize_font_session(&session, "Unknown", "1/0", "dev", "pair").unwrap();

        assert_eq!(payload["total_crafts"], 3);
    }

    #[test]
    fn a_garbled_first_count_the_server_would_reject_falls_back_to_the_round_count() {
        // "Crafts Remaining: 78" accepted on its one read: the server rejects
        // total_crafts > 20 outright, so the rounds sealed are what is sent.
        let mut session = FontSessionData::default();
        for _ in 0..2 {
            session.rounds.push(FontRound {
                options: vec![opt("transform_random", None)],
                crafts_remaining: Some(78),
            });
        }
        session.ledger.observe(CountRead::Count(78), 1);

        let payload = finalize_font_session(&session, "Unknown", "1/0", "dev", "pair").unwrap();

        assert_eq!(payload["total_crafts"], 2);
    }

    #[test]
    fn a_session_without_rounds_produces_no_payload() {
        let session = FontSessionData::default();

        assert!(finalize_font_session(&session, "Dedication", "20/20", "dev", "pair").is_none());
    }

    #[test]
    fn font_opened_events_alternate_starting_and_stopping_the_gem_reader() {
        let effects: Vec<FontOpenedEffect> = (1..=3).map(font_opened_effect).collect();

        assert_eq!(
            effects,
            vec![
                FontOpenedEffect::StartGemScan,
                FontOpenedEffect::StopGemScan,
                FontOpenedEffect::StartGemScan,
            ],
        );
    }

    #[test]
    fn successive_tokens_are_minted_distinct_values() {
        let counter = AtomicU64::new(0);

        assert_eq!([next_scan_generation(&counter), next_scan_generation(&counter)], [1, 2]);
    }

    #[test]
    fn a_wrapped_counter_never_mints_the_unarmed_token() {
        let counter = AtomicU64::new(u64::MAX);

        assert_ne!(next_scan_generation(&counter), 0);
    }
}
