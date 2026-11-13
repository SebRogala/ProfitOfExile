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
//! observed **after** a `FontOpened` is a new round. The direction of the
//! change is irrelevant — an "increase" after an event is still a different
//! panel.
//!
//! Every count is accepted on its first readable frame (POE-391): the font is
//! read once per key press, so there is no second frame to confirm against.
//! Trade-off: a garbled first count ("Crafts Remaining: 78") is now accepted,
//! and the session's craft total rests on `finalize_font_session`'s 1..=20
//! check, which falls back to the number of sealed rounds. The event gate still
//! holds: a misread can create at most one spurious round per `FontOpened`
//! event, and it can never destroy one, so the round count stays the floor the
//! craft total is taken against.

use crate::font_parser::CountRead;

/// What the ledger made of one frame's count read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerVerdict {
    /// First count of the session — the round is now open.
    Seed,
    /// The count already accepted; still the same panel.
    Same,
    /// A different count with no `FontOpened` since the last acceptance, or an
    /// unreadable label. Not a round boundary — the frame is still part of the
    /// current round.
    Ignored,
    /// A different count after a `FontOpened` — the current round ends here,
    /// labelled with the count that was accepted before it.
    Seal {
        /// The count the sealed round ran under, in raw panel semantics
        /// (`None` = the panel showed no count, i.e. the last craft).
        sealed_remaining: Option<i32>,
    },
}

/// Accepted craft count and the event it was accepted at.
#[derive(Debug, Clone, Default)]
pub struct CraftLedger {
    /// Outer `None` = nothing accepted yet. Inner value is raw panel
    /// semantics: `None` means the panel showed no count (last craft).
    accepted: Option<Option<i32>>,
    /// `font_opened_seq` at the moment `accepted` was set.
    accepted_at_event: u64,
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
            // seals anything.
            CountRead::LabelUnreadable => return LedgerVerdict::Ignored,
        };

        let Some(accepted) = self.accepted else {
            self.accept(raw, event_seq);
            self.total = normalise(raw);
            return LedgerVerdict::Seed;
        };

        if raw == accepted {
            return LedgerVerdict::Same;
        }

        if event_seq == self.accepted_at_event {
            // The panel cannot have changed: no CRAFT click since this count
            // was accepted, and a CRAFT click always fires `FontOpened`.
            return LedgerVerdict::Ignored;
        }

        self.accept(raw, event_seq);
        self.total = self.total.max(normalise(raw));
        LedgerVerdict::Seal {
            sealed_remaining: accepted,
        }
    }

    /// Take a count as the one the panel is now showing.
    fn accept(&mut self, raw: Option<i32>, event_seq: u64) {
        self.accepted = Some(raw);
        self.accepted_at_event = event_seq;
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

    fn count(n: i32) -> CountRead {
        CountRead::Count(n)
    }

    #[test]
    fn each_craft_click_seals_the_count_the_round_ran_under_on_one_frame() {
        let mut ledger = CraftLedger::default();

        let verdicts = [
            ledger.observe(count(8), 0),
            ledger.observe(count(7), 1),
            ledger.observe(count(6), 2),
        ];

        assert_eq!(
            verdicts,
            [
                LedgerVerdict::Seed,
                LedgerVerdict::Seal { sealed_remaining: Some(8) },
                LedgerVerdict::Seal { sealed_remaining: Some(7) },
            ],
        );
        assert_eq!(ledger.current(), Some(6));
    }

    #[test]
    fn the_highest_count_of_the_run_stays_the_total() {
        let mut ledger = CraftLedger::default();

        ledger.observe(count(8), 0);
        ledger.observe(count(7), 1);
        ledger.observe(count(6), 2);

        assert_eq!(ledger.total(), 8);
    }

    #[test]
    fn the_first_readable_count_seeds_the_session_on_one_frame() {
        let mut ledger = CraftLedger::default();

        let verdict = ledger.observe(count(8), 1);

        assert_eq!(verdict, LedgerVerdict::Seed);
        assert_eq!(ledger.total(), 8);
        assert_eq!(ledger.current(), Some(8));
    }

    #[test]
    fn an_unreadable_first_label_does_not_seed() {
        let mut ledger = CraftLedger::default();

        let verdict = ledger.observe(CountRead::LabelUnreadable, 1);

        assert_eq!(verdict, LedgerVerdict::Ignored);
        assert_eq!(ledger.total(), 0);
        let next = ledger.observe(count(5), 1);
        assert_eq!(next, LedgerVerdict::Seed, "the next readable count must still seed");
    }

    #[test]
    fn a_stash_trip_between_three_events_seals_nothing() {
        // Open the font, close it for the stash, reopen: three FontOpened
        // events, no craft, so the count never moves off 2.
        let mut ledger = CraftLedger::default();

        let verdicts = [
            ledger.observe(count(2), 1),
            ledger.observe(count(2), 2),
            ledger.observe(count(2), 3),
        ];

        assert_eq!(
            verdicts,
            [LedgerVerdict::Seed, LedgerVerdict::Same, LedgerVerdict::Same],
        );
        assert_eq!(ledger.current(), Some(2));
    }

    #[test]
    fn the_count_line_disappearing_after_an_event_seals_the_second_to_last_round() {
        // The game hides "Crafts Remaining" on the last craft, so the label
        // going absent is a real count change, not a failed read.
        let mut ledger = CraftLedger::default();
        ledger.observe(count(2), 1);

        let verdict = ledger.observe(CountRead::LabelAbsent, 2);

        assert_eq!(verdict, LedgerVerdict::Seal { sealed_remaining: Some(2) });
        assert_eq!(ledger.current(), None);
        assert_eq!(ledger.total(), 2);
    }

    #[test]
    fn a_single_craft_panel_seeds_a_total_of_one() {
        // Merciless: one craft, so the panel never shows a count line at all.
        let mut ledger = CraftLedger::default();

        let verdict = ledger.observe(CountRead::LabelAbsent, 1);

        assert_eq!(verdict, LedgerVerdict::Seed);
        assert_eq!(ledger.total(), 1);
        assert_eq!(ledger.current(), None);
    }

    #[test]
    fn a_count_change_without_an_event_is_a_misread() {
        let mut ledger = CraftLedger::default();
        ledger.observe(count(8), 4);

        let verdict = ledger.observe(count(6), 4);

        assert_eq!(verdict, LedgerVerdict::Ignored);
        assert_eq!(ledger.current(), Some(8));
        assert_eq!(ledger.total(), 8);
    }

    #[test]
    fn an_unreadable_label_leaves_the_accepted_count_alone() {
        let mut ledger = CraftLedger::default();
        ledger.observe(count(8), 1);

        let verdict = ledger.observe(CountRead::LabelUnreadable, 2);

        assert_eq!(verdict, LedgerVerdict::Ignored);
        assert_eq!(ledger.current(), Some(8));
        assert_eq!(ledger.total(), 8);
    }

    #[test]
    fn a_misread_after_an_event_is_accepted_once_and_the_next_event_seals_it() {
        // The accepted trade-off: one torn read after a CRAFT click seals a
        // round; the true count, after the next event, seals the spurious one.
        let mut ledger = CraftLedger::default();
        ledger.observe(count(8), 1);

        let torn = ledger.observe(count(3), 2);
        let same_event_truth = ledger.observe(count(7), 2);
        let next_event_truth = ledger.observe(count(7), 3);

        assert_eq!(
            (torn, same_event_truth, next_event_truth),
            (
                LedgerVerdict::Seal { sealed_remaining: Some(8) },
                LedgerVerdict::Ignored,
                LedgerVerdict::Seal { sealed_remaining: Some(3) },
            ),
        );
    }
}
