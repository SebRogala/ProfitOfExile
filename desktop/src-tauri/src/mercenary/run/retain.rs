use super::*;

/// How long a retired capture's confirmations stay available to a re-read of
/// the SAME panel (2026-08-25 smoke).
///
/// Long enough to cover a retire the player caused, short enough that a panel
/// reopened much later is read fresh rather than inheriting a stale session's
/// opinions.
pub(super) const RETAINED_TTL: Duration = Duration::from_secs(60);

/// What a retired capture left behind for a re-detect of the same panel.
///
/// One slot, replaced by each retire. The 2026-08-25 smoke is the whole reason
/// it exists: a tooltip retires the capture, the mercenary's voice line re-arms
/// a burst two seconds later, and the SAME window is detected fresh — so
/// without this the player's confirmations are gone every time they hover.
pub(super) struct Retained {
    /// The capture as last published. Its header and named-skill set are the
    /// key: `read::same_panel_positive` weighs them against the next detect.
    pub(super) capture: MercCapture,
    pub(super) confirmed: HashMap<(String, u8), ConfirmedCell>,
    /// Carried with the confirmations so a cell that already spent its re-read
    /// budget does not get a fresh one out of the retire.
    pub(super) hover_budget: HoverBudget,
    pub(super) at: Instant,
}

/// Whether a retired capture's confirmations may be re-applied to `next`.
///
/// The identity rule is [`same_panel_positive`], NOT the live path's
/// [`panel_replaced`]. The live path abstains on absence and re-detects two
/// seconds later; across a retire the same abstention would write one
/// mercenary's supports onto another's rows as `Confirmed`, which no hover can
/// undo. The retained slot therefore restores only on positive evidence.
///
/// `age` bounds it: past [`RETAINED_TTL`] the panel is read fresh, because a
/// window reopened a minute later is no longer the one the player was working
/// on even when it looks identical.
pub fn retained_applies(retired: &MercCapture, next: &MercCapture, age: Duration) -> bool {
    age <= RETAINED_TTL && same_panel_positive(retired, next)
}

/// What a detect did with the retained slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Restore {
    /// No slot was held — the ordinary first detect.
    Nothing,
    /// The slot applied: this many confirmations, plus the retired capture's
    /// header, are back on the session.
    Applied(usize),
    /// The slot was dropped, for the stated reason.
    Dropped(&'static str),
}

impl Restore {
    /// The line to log, or `None` when there is nothing to say.
    pub(super) fn log_line(self) -> Option<String> {
        match self {
            Restore::Nothing => None,
            Restore::Applied(n) => Some(format!(
                "Merc: same recruit window as the retired one — {n} confirmation(s) and its header restored"
            )),
            Restore::Dropped(why) => Some(format!(
                "Merc: the retired confirmations were dropped — {why}"
            )),
        }
    }
}

/// Put a retired capture's confirmations back, if [`retained_applies`] says so.
///
/// The slot is CONSUMED either way: it holds one retire's worth of state, and a
/// detect that rejected it has answered the only question it was kept for.
///
/// Takes no [`AppHandle`] and returns what it did, so the whole rule — the
/// identity gate, the TTL, and what lands back on the session — is testable off
/// Windows. The caller does the logging.
pub(super) fn restore_retained(session: &mut Session, next: &MercCapture) -> Restore {
    let Some(retained) = session.retained.take() else {
        return Restore::Nothing;
    };
    // One reading of the clock, so the decision and the reason it reports
    // cannot land on opposite sides of the TTL.
    let age = retained.at.elapsed();
    if !retained_applies(&retained.capture, next, age) {
        return Restore::Dropped(if age > RETAINED_TTL {
            "the slot expired"
        } else {
            "nothing positively says it is the same recruit window"
        });
    }
    let restored = retained.confirmed.len();
    session.confirmed = retained.confirmed;
    session.hover_budget = retained.hover_budget;
    // The retired capture goes back as `current` so the header fold downstream
    // SEES it. Name, class and level are as much a property of one window as
    // the confirmations are, and `fold_header` is already the single rule that
    // merges them — its own `panel_replaced` gate cannot fire here, because
    // `same_panel_positive` just required that rule to say no.
    session.current = Some(retained.capture);
    Restore::Applied(restored)
}
