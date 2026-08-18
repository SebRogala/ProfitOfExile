//! Font craft session state and its pure transitions.
//!
//! Everything here is Tauri-free: the scan loop and the `FontOpened` handler
//! call these functions under the session lock and do the logging, emitting and
//! spawning outside it. That split is what makes the round-boundary rules
//! testable — see `font_ledger` for the rule the boundary itself follows.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::font_ledger::{CraftLedger, LedgerVerdict};
use crate::font_parser::{merge_options, CraftOption, FontPanelState};

/// Accumulated font session data — shared between font scan loop and event handlers.
#[derive(Debug, Clone, Default, Serialize)]
pub struct FontSessionData {
    /// Sealed rounds (one per accepted craft-count change).
    pub rounds: Vec<FontRound>,
    /// Current round's detected options (not yet sealed).
    /// None = no options detected yet this round.
    #[serde(skip)]
    pub current_options: Option<Vec<CraftOption>>,
    /// Options read by frames showing a count change that is not yet confirmed.
    /// They belong to the next round if the change is accepted, and to the
    /// current one if it turns out to have been a torn frame.
    #[serde(skip)]
    pub pending_options: Option<Vec<CraftOption>>,
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

/// What one OCR frame did to the session, for the caller to log and emit
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

/// Apply one parsed font panel frame to the session.
///
/// `event_seq` is the `FontOpened` counter as of this frame; the ledger gates
/// every count change on it. Callers must only pass frames of an active panel
/// that actually carried options — an empty frame carries no evidence either
/// way and would only reset the debounce.
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
            // The pending frames were the first reads of the panel that is now
            // current, so they open the new round rather than being discarded.
            session.current_options = Some(session.pending_options.take().unwrap_or_default());
            Some(SealedRound {
                number: session.rounds.len(),
                round,
            })
        }
        LedgerVerdict::Seed | LedgerVerdict::Same => {
            // The count that produced a pending buffer never repeated, so those
            // frames were torn reads of this same panel: fold them back in
            // rather than losing the options they carried. `Seed` lands here
            // too — the session's first count is debounced like a change, so
            // the frames before it went to the pending buffer.
            if let Some(pending) = session.pending_options.take() {
                let current = session.current_options.take().unwrap_or_default();
                session.current_options = Some(merge_options(&current, &pending));
            }
            None
        }
        // `Ignored` leaves a standing pending buffer alone: an unreadable frame
        // is a hole in the evidence, not evidence against the change it was
        // interrupting.
        LedgerVerdict::Ignored | LedgerVerdict::Pending => None,
    };

    let buffer = if matches!(verdict, LedgerVerdict::Pending) {
        &mut session.pending_options
    } else {
        &mut session.current_options
    };
    let previous = buffer.take().unwrap_or_default();
    let merged = merge_options(&previous, &panel.options);
    // A sealing frame opens a round whose buffer may equal the pending one it
    // inherited, growing by nothing — report it as growth anyway so the new
    // round's option list is logged rather than silently reusing the old entry.
    let buffer_grew = merged != previous || sealed.is_some();
    *buffer = Some(merged.clone());

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
/// labelled with the count the ledger currently accepts, and a pending buffer
/// is folded in: it holds options read from a panel that was on screen, and
/// dropping it would lose reads the session paid OCR frames for.
///
/// Returns `None` when nothing was captured, so an empty round is never
/// appended (it would inflate the session's craft total).
pub fn seal_leftover_round(session: &mut FontSessionData) -> Option<SealedRound> {
    let current = session.current_options.take();
    let pending = session.pending_options.take();
    let options = match (current, pending) {
        (None, None) => return None,
        (Some(current), None) => current,
        (None, Some(pending)) => pending,
        (Some(current), Some(pending)) => merge_options(&current, &pending),
    };

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
/// number of sealed rounds: a stable OCR misread can mislabel one round, but it
/// can neither create nor destroy one, so the round count is the floor.
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
    // so an out-of-range ledger total — a garbled "Crafts Remaining: 78" that
    // held for two frames — is dropped in favour of the rounds actually sealed.
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

/// Side effects a `FontOpened` event asks the handler to perform.
///
/// Sealing a round is deliberately not one of them: the event fires both when
/// the font is opened and when CRAFT is clicked, so it cannot mark a round
/// boundary. Only the panel's craft count can (`font_ledger`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontOpenedEffect {
    StartGemScan,
    StopGemScan,
    RearmFontScan,
}

/// Mint the generation for a new scan, never handing out 0.
///
/// 0 is the liveness token's "no scan running" value, so a loop that owned it
/// would read as dead and every `FontOpened` would spawn another loop on top of
/// it. `fetch_add(1) + 1` only produces 0 once the counter wraps, which needs
/// 2^64 scans — the skip costs one generation and removes the state entirely.
pub fn next_scan_generation(counter: &AtomicU64) -> u64 {
    let minted = counter.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
    if minted == 0 {
        counter.fetch_add(1, Ordering::SeqCst).wrapping_add(1)
    } else {
        minted
    }
}

/// Advance the font scan's idle clock by one frame: the deadline to carry into
/// the next iteration, and whether the scan should give up.
///
/// Only a frame that actually saw the panel resets the clock; anything else —
/// an inactive frame, a failed capture, a failed OCR — lets it run down. The
/// loop otherwise only stops on a generation bump, and every event that bumps
/// it (`ZoneChanged`, lab exit, shutdown) can be missed: the game can be killed
/// rather than exited, or the log line can simply never arrive. Without this
/// the scan captures and OCRs the screen every 250 ms for the rest of the
/// app's life.
pub fn idle_tick(
    last_active: Instant,
    now: Instant,
    font_active: bool,
    limit: Duration,
) -> (Instant, bool) {
    if font_active {
        return (now, false);
    }
    (last_active, now.duration_since(last_active) >= limit)
}

/// Whether a font scan loop is running, read from the liveness token.
///
/// The token holds the generation of the loop that owns it. Generations are
/// handed out as `fetch_add(1) + 1`, so no loop can own 0 and 0 is free to mean
/// "no scan running".
pub fn font_scan_is_live(live_token: u64) -> bool {
    live_token != 0
}

/// Release the liveness token on behalf of the loop generation `own_gen`.
///
/// A compare-exchange rather than a store, because a superseded loop only
/// notices its generation changed on its next iteration — up to a scan interval
/// after the replacement scan already published its own generation. A blind
/// store would mark that running scan dead and every later `FontOpened` would
/// spawn another loop on top of it.
///
/// Returns whether this call was the one that released the token; callers that
/// only need the token cleared may ignore it.
pub fn try_clear_live(token: &AtomicU64, own_gen: u64) -> bool {
    token
        .compare_exchange(own_gen, 0, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
}

/// Decide what a `FontOpened` event should trigger.
///
/// `count` is the running event count for the current lab run; its odd/even
/// split drives the gem scan only (a known-imperfect CRAFT/CONFIRM guess that
/// this task deliberately leaves alone). `font_scan_live` says whether a font
/// scan loop is still running — after a portal trip it is not, and the event is
/// the only chance to bring the panel OCR back.
pub fn font_opened_effects(count: u32, font_scan_live: bool) -> Vec<FontOpenedEffect> {
    let mut effects = Vec::new();
    if count % 2 == 1 {
        effects.push(FontOpenedEffect::StartGemScan);
    } else {
        effects.push(FontOpenedEffect::StopGemScan);
    }
    if !font_scan_live {
        effects.push(FontOpenedEffect::RearmFontScan);
    }
    effects
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

    /// An Uber run: two crafts, three frames of each panel, one FontOpened (the
    /// CRAFT click) between them.
    fn uber_panels() -> (FontPanelState, FontPanelState) {
        (
            panel(
                vec![opt("transform_random", None), opt("quality", Some(20))],
                CountRead::Count(2),
            ),
            panel(
                vec![opt("facetors_lens", Some(60)), opt("sacrifice_keys", None)],
                CountRead::LabelAbsent,
            ),
        )
    }

    fn run_uber_session(session: &mut FontSessionData, first: &FontPanelState, second: &FontPanelState) {
        for _ in 0..3 {
            apply_font_frame(session, first, 1);
        }
        for _ in 0..3 {
            apply_font_frame(session, second, 2);
        }
    }

    #[test]
    fn a_craft_click_seals_the_round_it_ended() {
        let mut session = FontSessionData::default();
        let (first, second) = uber_panels();

        run_uber_session(&mut session, &first, &second);

        assert_eq!(
            session.rounds,
            vec![FontRound {
                options: first.options.clone(),
                crafts_remaining: Some(2),
            }],
        );
    }

    #[test]
    fn a_craft_click_opens_the_next_round_on_the_new_panel() {
        let mut session = FontSessionData::default();
        let (first, second) = uber_panels();

        run_uber_session(&mut session, &first, &second);

        assert_eq!(
            session.current_options.as_deref(),
            Some(second.options.as_slice()),
        );
    }

    #[test]
    fn both_rounds_of_an_uber_run_reach_the_payload() {
        let mut session = FontSessionData::default();
        let first = panel(vec![opt("transform_random", None)], CountRead::Count(2));
        let second = panel(vec![opt("facetors_lens", Some(60))], CountRead::LabelAbsent);

        for _ in 0..3 {
            apply_font_frame(&mut session, &first, 1);
        }
        for _ in 0..3 {
            apply_font_frame(&mut session, &second, 2);
        }
        seal_leftover_round(&mut session);

        let payload = finalize_font_session(&session, "Dedication", "20/20", "dev", "pair")
            .expect("two sealed rounds must produce a payload");
        assert_eq!(payload["total_crafts"], 2);
        assert_eq!(payload["rounds"].as_array().map(Vec::len), Some(2));
    }

    #[test]
    fn a_torn_frame_with_a_wrong_count_only_contributes_its_options() {
        // No FontOpened between the frames, so the 6 cannot be a new panel —
        // but the option it read is still a real read of the current one.
        let mut session = FontSessionData::default();
        let steady = panel(
            vec![opt("transform_random", None), opt("quality", Some(20))],
            CountRead::Count(8),
        );
        let torn = panel(vec![opt("sacrifice_keys", None)], CountRead::Count(6));

        apply_font_frame(&mut session, &steady, 1);
        apply_font_frame(&mut session, &steady, 1);
        apply_font_frame(&mut session, &torn, 1);
        let repeat = apply_font_frame(&mut session, &steady, 1);

        assert!(session.rounds.is_empty(), "a countless torn frame must not seal a round");
        assert_eq!(
            types_of(session.current_options.as_deref().unwrap_or_default()),
            vec!["transform_random", "quality", "sacrifice_keys"],
        );
        assert!(
            !repeat.buffer_grew,
            "a frame that repeats what the buffer already holds must not re-log it",
        );
    }

    #[test]
    fn the_first_frame_of_the_next_round_stays_out_of_the_sealed_one() {
        let mut session = FontSessionData::default();
        let first = panel(vec![opt("transform_random", None)], CountRead::Count(2));
        let second = panel(vec![opt("facetors_lens", Some(60))], CountRead::LabelAbsent);

        apply_font_frame(&mut session, &first, 1);
        apply_font_frame(&mut session, &first, 1);
        apply_font_frame(&mut session, &second, 2);
        let sealing = apply_font_frame(&mut session, &second, 2);

        assert_eq!(
            sealing.sealed,
            Some(SealedRound {
                number: 1,
                round: FontRound {
                    options: first.options.clone(),
                    crafts_remaining: Some(2),
                },
            }),
        );
        assert_eq!(
            types_of(session.current_options.as_deref().unwrap_or_default()),
            vec!["facetors_lens"],
        );
    }

    #[test]
    fn options_read_while_a_count_change_was_pending_return_to_the_round() {
        // The pending frame's count never repeated, so it was a torn read of
        // the panel that is still open — its options belong to this round.
        let mut session = FontSessionData::default();
        let steady = panel(vec![opt("transform_random", None)], CountRead::Count(8));
        let torn = panel(vec![opt("sacrifice_keys", None)], CountRead::Count(6));

        apply_font_frame(&mut session, &steady, 1);
        apply_font_frame(&mut session, &steady, 1);
        apply_font_frame(&mut session, &torn, 2);
        apply_font_frame(&mut session, &steady, 2);

        assert!(session.pending_options.is_none(), "the pending buffer must be consumed");
        assert_eq!(
            types_of(session.current_options.as_deref().unwrap_or_default()),
            vec!["transform_random", "sacrifice_keys"],
        );
    }

    #[test]
    fn a_sealing_frame_reports_growth_so_the_new_round_gets_logged() {
        // The sealing frame inherits the pending buffer, which already holds
        // everything it reads — the merge adds nothing. Without an explicit
        // signal the caller would never log the round that just opened.
        let mut session = FontSessionData::default();
        let open = panel(vec![opt("transform_random", None)], CountRead::Count(2));
        let next = panel(vec![opt("facetors_lens", Some(60))], CountRead::LabelAbsent);

        apply_font_frame(&mut session, &open, 1);
        apply_font_frame(&mut session, &open, 1);
        apply_font_frame(&mut session, &next, 2);
        let sealing = apply_font_frame(&mut session, &next, 2);

        assert!(sealing.sealed.is_some(), "the second frame of the new count must seal");
        assert!(
            sealing.buffer_grew,
            "a sealing frame must report growth so the new round's options are logged",
        );
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
    fn rounds_the_scan_missed_still_count_towards_the_craft_total() {
        // A Gift run whose panel started at 8: five rounds went unsealed
        // (portal trip, torn frames), but the run still had 8 crafts.
        let mut session = FontSessionData::default();
        for _ in 0..3 {
            session.rounds.push(FontRound {
                options: vec![opt("transform_random", None)],
                crafts_remaining: Some(8),
            });
        }
        session.ledger.observe(CountRead::Count(8), 1);
        session.ledger.observe(CountRead::Count(8), 1);

        let payload = finalize_font_session(&session, "Unknown", "1/0", "dev", "pair")
            .expect("a session with rounds must produce a payload");

        assert_eq!(payload["total_crafts"], 8);
    }

    #[test]
    fn more_sealed_rounds_than_the_ledger_counted_raise_the_craft_total() {
        // The floor half of the max: a misread count cannot report fewer
        // crafts than the number of rounds actually sealed.
        let mut session = FontSessionData::default();
        for _ in 0..3 {
            session.rounds.push(FontRound {
                options: vec![opt("transform_random", None)],
                crafts_remaining: None,
            });
        }
        session.ledger.observe(CountRead::LabelAbsent, 1);
        session.ledger.observe(CountRead::LabelAbsent, 1);

        let payload = finalize_font_session(&session, "Unknown", "1/0", "dev", "pair")
            .expect("a session with rounds must produce a payload");

        assert_eq!(payload["total_crafts"], 3);
    }

    #[test]
    fn a_craft_total_the_server_would_reject_falls_back_to_the_round_count() {
        // A garbled "Crafts Remaining: 78" that held for two frames. The server
        // rejects total_crafts > 20 outright and the session is already gone by
        // then, so the rounds actually sealed are what gets reported.
        let mut session = FontSessionData::default();
        for _ in 0..2 {
            session.rounds.push(FontRound {
                options: vec![opt("transform_random", None)],
                crafts_remaining: Some(78),
            });
        }
        session.ledger.observe(CountRead::Count(78), 1);
        session.ledger.observe(CountRead::Count(78), 1);

        let payload = finalize_font_session(&session, "Unknown", "1/0", "dev", "pair")
            .expect("a session with rounds must produce a payload");

        assert_eq!(payload["total_crafts"], 2);
    }

    #[test]
    fn options_still_awaiting_confirmation_reach_the_leftover_round() {
        // The player left with a count change half-confirmed. Those frames read
        // a panel that was really on screen — the round must carry them.
        let mut session = FontSessionData::default();
        let open = panel(vec![opt("transform_random", None)], CountRead::Count(2));
        let unconfirmed = panel(vec![opt("facetors_lens", Some(60))], CountRead::LabelAbsent);

        apply_font_frame(&mut session, &open, 1);
        apply_font_frame(&mut session, &open, 1);
        apply_font_frame(&mut session, &unconfirmed, 2);
        let sealed = seal_leftover_round(&mut session).expect("a buffered round must seal");

        assert_eq!(
            types_of(&sealed.round.options),
            vec!["transform_random", "facetors_lens"],
        );
    }

    #[test]
    fn a_session_without_rounds_produces_no_payload() {
        let session = FontSessionData::default();

        assert!(finalize_font_session(&session, "Dedication", "20/20", "dev", "pair").is_none());
    }

    #[test]
    fn three_font_opened_events_ask_for_no_round_sealing() {
        // Open, reopen after a stash trip, then CRAFT: whatever the events mean
        // for the gem scan, none of them may touch the round ledger.
        let effects: Vec<Vec<FontOpenedEffect>> =
            (1..=3).map(|n| font_opened_effects(n, true)).collect();

        assert_eq!(
            effects,
            vec![
                vec![FontOpenedEffect::StartGemScan],
                vec![FontOpenedEffect::StopGemScan],
                vec![FontOpenedEffect::StartGemScan],
            ],
        );
    }

    #[test]
    fn an_event_arriving_with_no_font_scan_running_rearms_it() {
        assert_eq!(
            font_opened_effects(1, false),
            vec![FontOpenedEffect::StartGemScan, FontOpenedEffect::RearmFontScan],
        );
    }

    #[test]
    fn a_loop_that_still_owns_the_token_releases_it_on_exit() {
        let token = AtomicU64::new(3);

        assert!(try_clear_live(&token, 3), "the owning loop must win the release");
        assert!(
            !font_scan_is_live(token.load(Ordering::SeqCst)),
            "a released token must read as no scan running",
        );
    }

    #[test]
    fn a_superseded_loop_exiting_leaves_the_replacement_scan_live() {
        // Loop 7 was still sleeping between frames when `spawn_font_scan`
        // published generation 8; it exits a scan interval later.
        let token = AtomicU64::new(8);

        assert!(!try_clear_live(&token, 7), "a stale loop must not win the release");
        assert!(
            !font_opened_effects(1, font_scan_is_live(token.load(Ordering::SeqCst)))
                .contains(&FontOpenedEffect::RearmFontScan),
            "the running scan must not be re-armed on top of itself",
        );
    }

    #[test]
    fn a_zone_change_leaves_the_next_font_opened_to_rearm_the_scan() {
        // ZoneChanged clears the token synchronously with the generation bump,
        // so the event that follows re-arms even before the old loop notices.
        let token = AtomicU64::new(0);

        assert!(
            font_opened_effects(1, font_scan_is_live(token.load(Ordering::SeqCst)))
                .contains(&FontOpenedEffect::RearmFontScan),
            "a dead scan must be re-armed by the next event",
        );
    }

    #[test]
    fn successive_scans_are_minted_distinct_generations() {
        let counter = AtomicU64::new(0);

        assert_eq!(
            [next_scan_generation(&counter), next_scan_generation(&counter)],
            [1, 2],
        );
    }

    #[test]
    fn a_wrapped_counter_never_mints_the_no_scan_generation() {
        // The next `fetch_add(1) + 1` is 0 — the value the liveness token uses
        // for "no scan running", which no live loop may own.
        let counter = AtomicU64::new(u64::MAX);

        assert!(font_scan_is_live(next_scan_generation(&counter)));
    }

    #[test]
    fn a_frame_that_saw_the_panel_pushes_the_idle_deadline_out() {
        let opened = Instant::now();

        let (deadline, _) = idle_tick(
            opened,
            opened + Duration::from_secs(300),
            true,
            Duration::from_secs(600),
        );

        assert_eq!(deadline, opened + Duration::from_secs(300));
    }

    #[test]
    fn a_frame_without_the_panel_leaves_the_idle_deadline_alone() {
        let opened = Instant::now();

        let (deadline, _) = idle_tick(
            opened,
            opened + Duration::from_secs(300),
            false,
            Duration::from_secs(600),
        );

        assert_eq!(deadline, opened);
    }

    #[test]
    fn a_scan_that_has_just_reached_the_idle_limit_gives_up() {
        let opened = Instant::now();

        let (_, expired) = idle_tick(
            opened,
            opened + Duration::from_secs(600),
            false,
            Duration::from_secs(600),
        );

        assert!(expired);
    }

    #[test]
    fn a_scan_one_tick_short_of_the_idle_limit_keeps_going() {
        let opened = Instant::now();

        let (_, expired) = idle_tick(
            opened,
            opened + Duration::from_millis(599_999),
            false,
            Duration::from_secs(600),
        );

        assert!(!expired);
    }

    #[test]
    fn a_scan_reading_the_panel_past_the_idle_limit_keeps_going() {
        // The player left the font open for eleven minutes. The panel is right
        // there on screen, so the scan that would report it must not expire.
        let opened = Instant::now();

        let (_, expired) = idle_tick(
            opened,
            opened + Duration::from_secs(660),
            true,
            Duration::from_secs(600),
        );

        assert!(!expired);
    }
}
