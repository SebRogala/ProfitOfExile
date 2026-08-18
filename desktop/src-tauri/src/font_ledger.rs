//! Event-gated ledger over the font panel's "Crafts Remaining" count.
//!
//! The count is the only signal that tells one craft round from the next: the
//! option list of two consecutive rounds can be identical, and `FontOpened`
//! fires both when the font is opened and when CRAFT is clicked, so neither the
//! options nor the event count can delimit a round.
//!
//! The count cannot change without a CRAFT click, and every CRAFT click fires
//! `FontOpened`. That gives the ledger its rule: a count change observed with
//! **no `FontOpened` since the last accepted count** is a misread (torn frame,
//! gem tooltip over the panel, OCR noise) and is ignored; a count change
//! observed **after** a `FontOpened` and **stable for two consecutive readable
//! frames** is a new round. The direction of the change is irrelevant — an
//! "increase" after an event is still a different panel.
//!
//! What the event gate buys: OCR that consistently misreads a static panel is
//! not caught by the 2-frame debounce (same pixels, same wrong read), but
//! without an event that stable misread cannot be accepted at all. It cannot
//! destroy a round, and it can create at most one spurious round per
//! `FontOpened` event — a stable misread that begins after an event. The round
//! count is therefore the floor the session's craft total is taken against, not
//! a number to trust over the ledger.
//!
//! **Accepted limitation:** a round that is visible for fewer than two frames
//! (under ~500 ms on screen at the 250 ms scan interval) is never accepted, so
//! its options fold into the neighbouring round instead of sealing one of their
//! own. The player has to read the options before clicking, so a sub-500 ms
//! round does not occur in play.

use crate::font_parser::CountRead;

/// Consecutive readable frames a count needs before it is accepted — for the
/// first count of the session as much as for a change.
const FRAMES_TO_ACCEPT: u8 = 2;

/// What the ledger made of one frame's count read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerVerdict {
    /// First count of the session, confirmed by a second frame — the round is
    /// now open.
    Seed,
    /// The count already accepted; still the same panel.
    Same,
    /// A different count with no `FontOpened` since the last acceptance, or an
    /// unreadable label. Not a round boundary — the frame is still part of the
    /// current round.
    Ignored,
    /// A count awaiting confirmation: the session's first, or a change after a
    /// `FontOpened`. Held back until a second frame shows the same number.
    Pending,
    /// A different count confirmed by a second frame — the current round ends
    /// here, labelled with the count that was accepted before it.
    Seal {
        /// The count the sealed round ran under, in raw panel semantics
        /// (`None` = the panel showed no count, i.e. the last craft).
        sealed_remaining: Option<i32>,
    },
}

/// Accepted craft count and the pending-change debounce behind it.
#[derive(Debug, Clone, Default)]
pub struct CraftLedger {
    /// Outer `None` = nothing accepted yet. Inner value is raw panel
    /// semantics: `None` means the panel showed no count (last craft).
    accepted: Option<Option<i32>>,
    /// `font_opened_seq` at the moment `accepted` was set.
    accepted_at_event: u64,
    /// A changed count seen after an event, with how many consecutive readable
    /// frames have now shown it.
    candidate: Option<(Option<i32>, u8)>,
    /// Highest normalised count accepted this session.
    total: i32,
}

/// Crafts a panel state stands for: a panel with no count line is the last one.
fn normalise(raw: Option<i32>) -> i32 {
    raw.unwrap_or(1)
}

impl CraftLedger {
    /// Feed one frame's count read, tagged with the `FontOpened` counter as of
    /// that frame, and get back what it means for the round boundary.
    pub fn observe(&mut self, read: CountRead, event_seq: u64) -> LedgerVerdict {
        let raw = match read {
            CountRead::Count(n) => Some(n),
            CountRead::LabelAbsent => None,
            // A garbled label carries no count, so it never seeds, changes or
            // seals anything. It deliberately leaves a candidate standing: a
            // single unreadable frame in the middle of a change is a hole in
            // the evidence, not evidence against the change.
            CountRead::LabelUnreadable => return LedgerVerdict::Ignored,
        };

        let accepted = match self.accepted {
            // Unseeded: the very first count goes through the same two-frame
            // debounce as a change. A single garbled first frame ("Crafts
            // Remaining: 78") would otherwise fix the session's craft total at
            // a number the server rejects outright, losing the whole POST.
            None => {
                if !self.confirmed(raw) {
                    return LedgerVerdict::Pending;
                }
                self.accept(raw, event_seq);
                self.total = normalise(raw);
                return LedgerVerdict::Seed;
            }
            Some(accepted) => accepted,
        };

        if raw == accepted {
            // A candidate that never repeated was a torn frame of this panel.
            self.candidate = None;
            return LedgerVerdict::Same;
        }

        if event_seq == self.accepted_at_event {
            // The panel cannot have changed: no CRAFT click since this count
            // was accepted, and a CRAFT click always fires `FontOpened`.
            return LedgerVerdict::Ignored;
        }

        if !self.confirmed(raw) {
            return LedgerVerdict::Pending;
        }

        self.accept(raw, event_seq);
        self.total = self.total.max(normalise(raw));
        LedgerVerdict::Seal {
            sealed_remaining: accepted,
        }
    }

    /// Count this frame towards the candidate it shows, and report whether the
    /// candidate has now been seen on enough consecutive readable frames. A
    /// different number restarts the count.
    fn confirmed(&mut self, raw: Option<i32>) -> bool {
        let frames = match self.candidate {
            Some((candidate, frames)) if candidate == raw => frames + 1,
            _ => 1,
        };
        if frames < FRAMES_TO_ACCEPT {
            self.candidate = Some((raw, frames));
            return false;
        }
        true
    }

    /// Take a confirmed count as the one the panel is now showing.
    fn accept(&mut self, raw: Option<i32>, event_seq: u64) {
        self.accepted = Some(raw);
        self.accepted_at_event = event_seq;
        self.candidate = None;
    }

    /// The accepted count in raw panel semantics — `None` before anything is
    /// accepted and on the last craft.
    pub fn current(&self) -> Option<i32> {
        self.accepted.flatten()
    }

    /// Highest number of crafts this session was ever seen to have left.
    pub fn total(&self) -> i32 {
        self.total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feed the same read twice — the confirmation the ledger needs to accept
    /// any count — and return the two verdicts in order.
    fn twice(ledger: &mut CraftLedger, read: CountRead, event_seq: u64) -> (LedgerVerdict, LedgerVerdict) {
        (
            ledger.observe(read, event_seq),
            ledger.observe(read, event_seq),
        )
    }

    fn count(n: i32) -> CountRead {
        CountRead::Count(n)
    }

    /// Drive a three-craft run: 8 remaining, CRAFT, 7 remaining, CRAFT, 6.
    fn run_three_crafts(ledger: &mut CraftLedger) {
        twice(ledger, count(8), 0);
        twice(ledger, count(7), 1);
        twice(ledger, count(6), 2);
    }

    #[test]
    fn each_craft_click_seals_the_count_the_round_ran_under() {
        let mut ledger = CraftLedger::default();

        let first = twice(&mut ledger, count(8), 0);
        let second = twice(&mut ledger, count(7), 1);
        let third = twice(&mut ledger, count(6), 2);

        assert_eq!(first, (LedgerVerdict::Pending, LedgerVerdict::Seed));
        assert_eq!(
            second,
            (
                LedgerVerdict::Pending,
                LedgerVerdict::Seal { sealed_remaining: Some(8) },
            ),
        );
        assert_eq!(
            third,
            (
                LedgerVerdict::Pending,
                LedgerVerdict::Seal { sealed_remaining: Some(7) },
            ),
        );
        assert_eq!(ledger.current(), Some(6));
    }

    #[test]
    fn the_highest_count_of_the_run_stays_the_total() {
        let mut ledger = CraftLedger::default();

        run_three_crafts(&mut ledger);

        assert_eq!(ledger.total(), 8);
    }

    #[test]
    fn a_single_garbled_first_frame_does_not_seed() {
        // "Crafts Remaining: 78" from one torn frame would otherwise fix the
        // session total at a number the server rejects (1..=20), and the whole
        // POST is lost when it does.
        let mut ledger = CraftLedger::default();

        let verdict = ledger.observe(count(78), 1);

        assert_eq!(verdict, LedgerVerdict::Pending);
        assert_eq!(ledger.total(), 0);
        assert_eq!(ledger.current(), None);
    }

    #[test]
    fn a_first_count_repeated_by_a_second_frame_seeds_the_session() {
        let mut ledger = CraftLedger::default();

        let verdicts = twice(&mut ledger, count(8), 1);

        assert_eq!(verdicts, (LedgerVerdict::Pending, LedgerVerdict::Seed));
        assert_eq!(ledger.total(), 8);
        assert_eq!(ledger.current(), Some(8));
    }

    #[test]
    fn a_stash_trip_between_three_events_seals_nothing() {
        // Open the font, close it for the stash, reopen: three FontOpened
        // events, no craft, so the count never moves off 2.
        let mut ledger = CraftLedger::default();

        let verdicts = [
            twice(&mut ledger, count(2), 1),
            twice(&mut ledger, count(2), 2),
            twice(&mut ledger, count(2), 3),
        ];

        assert_eq!(
            verdicts,
            [
                (LedgerVerdict::Pending, LedgerVerdict::Seed),
                (LedgerVerdict::Same, LedgerVerdict::Same),
                (LedgerVerdict::Same, LedgerVerdict::Same),
            ],
        );
        assert_eq!(ledger.current(), Some(2));
    }

    #[test]
    fn the_count_line_disappearing_after_an_event_seals_the_second_to_last_round() {
        // The game hides "Crafts Remaining" on the last craft, so the label
        // going absent is a real count change, not a failed read.
        let mut ledger = CraftLedger::default();
        twice(&mut ledger, count(2), 1);

        let verdicts = twice(&mut ledger, CountRead::LabelAbsent, 2);

        assert_eq!(
            verdicts,
            (
                LedgerVerdict::Pending,
                LedgerVerdict::Seal { sealed_remaining: Some(2) },
            ),
        );
        assert_eq!(ledger.current(), None);
        assert_eq!(ledger.total(), 2);
    }

    #[test]
    fn a_single_craft_panel_seeds_a_total_of_one() {
        // Merciless: one craft, so the panel never shows a count line at all.
        let mut ledger = CraftLedger::default();

        let verdicts = twice(&mut ledger, CountRead::LabelAbsent, 1);

        assert_eq!(verdicts, (LedgerVerdict::Pending, LedgerVerdict::Seed));
        assert_eq!(ledger.total(), 1);
        assert_eq!(ledger.current(), None);
    }

    #[test]
    fn a_count_change_without_an_event_is_a_misread() {
        let mut ledger = CraftLedger::default();
        twice(&mut ledger, count(8), 4);

        let verdicts = twice(&mut ledger, count(6), 4);

        assert_eq!(verdicts, (LedgerVerdict::Ignored, LedgerVerdict::Ignored));
        assert_eq!(ledger.current(), Some(8));
        assert_eq!(ledger.total(), 8);
    }

    #[test]
    fn a_changed_count_seen_once_is_dropped_when_the_old_count_returns() {
        let mut ledger = CraftLedger::default();
        twice(&mut ledger, count(8), 1);

        let torn = ledger.observe(count(6), 2);
        let back = ledger.observe(count(8), 2);
        // The dropped candidate must not count towards the next sighting of 6:
        // this frame is its first, not its second.
        let again = ledger.observe(count(6), 2);

        assert_eq!(
            (torn, back, again),
            (
                LedgerVerdict::Pending,
                LedgerVerdict::Same,
                LedgerVerdict::Pending,
            ),
        );
        assert_eq!(ledger.current(), Some(8));
    }

    #[test]
    fn an_unreadable_label_leaves_the_accepted_count_alone() {
        let mut ledger = CraftLedger::default();
        twice(&mut ledger, count(8), 1);

        let verdicts: Vec<LedgerVerdict> = (0..5)
            .map(|_| ledger.observe(CountRead::LabelUnreadable, 2))
            .collect();

        assert_eq!(verdicts, vec![LedgerVerdict::Ignored; 5]);
        assert_eq!(ledger.current(), Some(8));
        assert_eq!(ledger.total(), 8);
    }

    #[test]
    fn a_count_shown_for_one_frame_only_never_becomes_a_round() {
        // The documented single-frame limitation: 7 is on screen for one frame
        // before 6 takes over, so the round it would have opened is lost and 6
        // seals directly against 8.
        let mut ledger = CraftLedger::default();
        twice(&mut ledger, count(8), 1);

        let seven = ledger.observe(count(7), 2);
        let six_first = ledger.observe(count(6), 2);
        let six_second = ledger.observe(count(6), 2);

        assert_eq!(
            (seven, six_first, six_second),
            (
                LedgerVerdict::Pending,
                LedgerVerdict::Pending,
                LedgerVerdict::Seal { sealed_remaining: Some(8) },
            ),
        );
        assert_eq!(ledger.current(), Some(6));
    }
}
