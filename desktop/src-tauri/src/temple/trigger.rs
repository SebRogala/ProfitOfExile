//! What arms the temple capture loop (POE-242, POE-246).
//!
//! Before this file the loop ran a detect tick every second for as long as the
//! module was on and the game was in front — a screen grab plus an anchor
//! correlation, forever, for a panel the player opens a handful of times an
//! hour. Owner report, 2026-09-02: *"currently Alva module seems to be
//! capturing all the time while module is active, which is wrong"*.
//!
//! The loop now captures only while an incursion is in scope — while Client.txt
//! says so, or while the loop can still see the layout panel it last found
//! (POE-246). Everything that decides is a plain function over plain data and is
//! tested here on Linux; the `AppHandle` wrappers at the bottom only lock and
//! log.
//!
//! # Trigger — the lines this reads (MEASURED, 684 of them)
//!
//! Mined off Sebastian's PC Client.txt on 2026-09-04, covering 2026-01-29 →
//! 2026-09-04: **684 Alva lines across 144 map instances**. The laptop's whole
//! history (9 lines) agrees on every line it holds. `docs/TEMPLE-LIFECYCLE.md`
//! is the normative write-up; this is the table [`classify`] is built from.
//!
//! | line | PC count | role |
//! |---|---|---|
//! | `Time to go.` | 122 | start |
//! | `Let's go.` | 118 | start |
//! | `It's time!` | 101 | start |
//! | `Good job.` | 168 | end |
//! | `Good job, exile.` | 174 | end |
//! | `Just in time.` | 1 | end (no incursion followed it) |
//! | `No wonder it's lost…`, `At last... Atzoatl.` | — | temple banter; an end by the same rule |
//! | temple entry | `Generating level N area "Incursion_Temple8"`, then `: You have entered The Temple of Atzoatl.` |
//!
//! # Why the phrases, and why only for the START
//!
//! The three start lines are used about equally, so a phrase gate needs all
//! three — any one alone misses two thirds of incursions. What 684 lines buy is
//! the START half of the rule: a CYCLE (the waiting notice, the board epoch)
//! begins only on one of the three phrases above. **End lines can arrive after
//! a zone change** — 3 of 342 in the mining, the player having left the map
//! mid-incursion with `Good job` firing seconds after re-entering — and a cycle
//! started by one of those would put a waiting notice on screen for an
//! incursion that is already over.
//!
//! ANY Alva line still ENDS a cycle, and since 2026-09-07 (WI-1) only a START
//! phrase ARMS one. The asymmetry is deliberate: an unheard start variant costs
//! that incursion's read (one incursion in 342 carried no start line at all),
//! while an unheard END variant would leave the notice standing — a claim on
//! screen that is false.
//!
//! **A non-START Alva line now stands the capture DOWN** rather than arming it
//! for two minutes. Owner, 2026-09-07: *"After leaving the incursion, OCR never
//! stops probing, and it should on any Alva voiceline, or zone change."* What
//! that gives up is the map-side reopen after `Good job.`: since POE-389 a
//! read-key press there reads no temple board. What it buys is the
//! thing the owner asked for: nothing is looking between incursions. The one
//! exception is Alva's own banter inside The Temple of Atzoatl, which must not
//! cut a [`ArmReason::TempleArea`] arm short — see [`apply_line`].
//!
//! `Time to go, exile.` does not exist in either log — the `, exile` variant is
//! on the END line — which is why the start table is matched EXACTLY rather
//! than by prefix.
//!
//! It is an ENGLISH match: a client running in another language writes Alva's
//! name and title in that language and no voice line on that machine will ever
//! arm, so the read key reads no temple board there (POE-389 removed the
//! Re-arm fallback).
//!
//! # What opens the gate, and what shuts it (2026-09-07, WI-1)
//!
//! - a **START phrase** arms with NO deadline. The portal wait is unbounded
//!   (the mining holds one 22-minute gap), so nothing but an END line, a zone
//!   change or the completed cycle below may end it. A line older than
//!   [`LINE_STALE_MS`] on arrival is not evidence about now at all and arms
//!   nothing;
//! - an **area** is a state, not a burst: `: You have entered The Temple of
//!   Atzoatl.` arms with NO deadline, and the next `You have entered` line —
//!   whatever it names — is what ends it.
//!
//! Two things shut it, and each has a word for the app log ([`StandDown`]):
//! a non-START Alva line and a `You have entered <not the temple>` line.
//! The screen is no input since POE-389: nothing
//! looks between presses, so nothing can see a sheet open or close.
//!
//! # There are no tails any more (2026-09-07, WI-1)
//!
//! Until WI-1 a voice line armed for `ALVA_TAIL_MS` (120 s) and the last panel
//! sighting held the gate for `PANEL_TAIL_MS` (120 s) after the sheet closed.
//! Both are retired. Owner, 2026-09-07: *"Once the full sheet is read once in
//! the incursion, we stop reading the sheet … once the sheet is closed, we can
//! already stop OCRing, hide the explanation, keep the diamond overlay, and upon
//! stop encounter, we hide the diamond overlay."*
//!
//! What replaced them was a CYCLE ended by the capture loop watching the sheet
//! close; POE-389 deleted that loop and its cycle rule with it. A START arm now
//! holds until an END line or a zone change, and each read-key press under it
//! reads once.
//!
//! Arming never SHORTENS what is already armed ([`TempleArm::arm`]); since
//! POE-389 removed Re-arm no bid carries a deadline. Alva's temple banter no longer reaches this rule at
//! all: it does not arm, it is refused outright over a [`ArmReason::TempleArea`]
//! arm ([`apply_line`]), and over anything else it stands the capture down.
//!
//! # The scope this draws, and the override outside it
//!
//! SETTLED by owner order, 2026-09-04 (`docs/TEMPLE-LIFECYCLE.md`,
//! "Consequences that follow from the order"): a sheet opened from the hideout
//! with Alva silent is NOT in scope. The module follows incursions, and a panel
//! nobody was sent to by a voice line or by a zone is not one — so the absence
//! of an arm there is the design and not a gap in it. The **Re-arm** override
//! (POE-242) was removed by POE-389; Recalibrate stays the manual fix for a
//! wrong placement.
//!
//! ## What the log settles, and what it cannot (2026-09-02)
//!
//! OBSERVED, in the PC's Client.txt over the two incursions of 2026-08-07: no
//! `You have entered` line is written between Alva's start line and her
//! `Good job` line. The incursion instance logs NO area change at all, in
//! either direction. Two consequences, both load-bearing:
//!
//! - an arm bought by `It's time!` is never disarmed by the incursion, and
//!   survives the return to the map. Since WI-1 `Good job…` is what ENDS it
//!   rather than what extends it, so the post-incursion panel read is no longer
//!   covered by the arm (owner decision 2026-09-07, above).
//! - the ordinary end of a START arm is therefore that END line, or the
//!   completed cycle when the player read the sheet and closed it. The area
//!   line is the end only when the player leaves before either.
//!
//! What the log CANNOT show is the one thing the design turns on: whether
//! Alva's line fires when the dialogue/panel OPENS or only when **Enter
//! Incursion** is clicked. If it fires on open, the DECISION read (the panel
//! the player studies before choosing a room) is inside the arm and covered.
//! If it fires on the click, the panel was open while the module was disarmed
//! and the decision read is LOST — the module would only ever see the board
//! after the choice was already made.
//!
//! INFERRED, weakly: the former. The start line lands 3–7 s after the
//! `[WINDOW] Gained focus` that precedes it, which is more consistent with a
//! greeting on open than with a click after the panel has been read. Three
//! samples, and a timing argument is not a measurement.
//!
//! **If the smoke run shows it fires on the click, there is no earlier signal
//! in Client.txt to move to** — the file writes nothing when a dialogue opens.
//! The owner then chooses between the free-running cheap detect this file
//! replaced (correct, and the behaviour the owner reported as wrong) and losing
//! the decision read. That is a product decision, not a code one.
//!
//! A third gap the log leaves: a catch-up tail with no `You have entered` line
//! in it at all — a quiet log, or one truncated between area changes — is read
//! as `Disarmed`, because "unknown" must not be guessed into "the temple". The
//! next temple area or START line recovers it.
//!
//! # Not gated
//!
//! `temple_debug_capture` captures whatever the arm state says. It is an
//! explicit user action — the command a user runs *because* something else went
//! wrong — and gating it would make the diagnostic unavailable in exactly the
//! state that needs diagnosing.

use tauri::{AppHandle, Manager};

use crate::lab_navigation;
use crate::client_log::{
    arm_at, line_timestamp_ms, speaker_of, LINE_STALE_MS,
};
use crate::AppState;

/// Alva's speaker string, exactly as Client.txt writes it.
pub const ALVA_SPEAKER: &str = "Alva, Master Explorer";

/// The area name the temple itself enters under.
pub const TEMPLE_AREA: &str = "The Temple of Atzoatl";

// ---------------------------------------------------------------------------
// Pure — the state machine
// ---------------------------------------------------------------------------

/// Why the loop is armed. Carried for the log line and for nothing else — the
/// loop asks [`TempleArm::is_armed`], never the reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmReason {
    /// One of the three measured START phrases ([`classify`]). Armed with NO
    /// deadline: the start line fires when the portal OPENS, and nothing in the
    /// game times an open portal out — the mining holds one gap of 22 minutes
    /// between a start and its end, the player being away from the PC. What
    /// ends this arm is an end line or an area change, never a clock.
    AlvaStart,
    /// The player is inside The Temple of Atzoatl.
    TempleArea,
}

impl ArmReason {
    /// The words the app log uses for this reason.
    pub fn label(self) -> &'static str {
        match self {
            ArmReason::AlvaStart => "Alva's start line",
            ArmReason::TempleArea => "the temple",
        }
    }
}

/// Why the capture is NOT looking — the second half of the app log's gate
/// vocabulary, added by WI-1 (2026-09-07).
///
/// Before WI-1 there was one stand-down line for every cause, because there was
/// really one cause: a clock had run out. The clocks are gone (POE-389 removed
/// Re-arm's grace, the last one), and the three ways a gate now shuts are three
/// different things to report.
///
/// Carried on [`ArmState`] rather than derived, because all three are
/// EVENTS with no trace left in [`TempleArm`] afterwards — a disarmed arm looks
/// the same whichever line disarmed it. What the field holds is the cause the
/// NEXT stand-down reports, so every writer of the arm sets it: the two
/// disarming lines in [`apply_line`].
///
/// The two deadline-free arms ([`ArmReason::AlvaStart`], [`ArmReason::TempleArea`])
/// deliberately leave it alone: they cannot end on a clock, so whatever ends
/// them writes its own cause first.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StandDown {
    /// Nothing has put an incursion in scope. The resting state, and where a
    /// session spends nearly all of its time.
    #[default]
    Waiting,
    /// The sheet was read and then closed: this cycle is finished (WI-1, owner
    /// 2026-09-07). The next START phrase begins a fresh one.
    CycleComplete,
    /// A non-START [`ALVA_SPEAKER`] line: the incursion is over, or another has
    /// begun.
    AlvaLine,
    /// `: You have entered <not the temple>` — the player left.
    LeftArea,
}

/// Whether an incursion is in scope — what the read key's temple reader
/// (`super::press`) asks at each press.
///
/// The single owner of that answer, held in `AppState.temple_state.arm` and written
/// from the Client.txt watcher (every line, whether or not the module is on).
///
/// **Written while the module is off, too.** The state is a fact about the
/// game, not about the module, and keeping it current is what lets a player who
/// switches the module on INSIDE a temple get a read without pressing anything.
/// Gating the writes on the module flag would leave that player disarmed with
/// no further area change coming.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TempleArm {
    /// Nothing is in scope. The loop's resting state, and where it spends most
    /// of a session.
    #[default]
    Disarmed,
    /// Armed until `until_ms`, or — `None` — until the next area change.
    Armed {
        until_ms: Option<u64>,
        reason: ArmReason,
    },
}

/// What one Client.txt line did to the state.
///
/// [`apply_line`]'s observable outcome beyond the state itself, and the seam
/// this module's tests assert "extended, not replaced" through — a distinction
/// [`TempleArm`] alone cannot express, because both spellings leave the same
/// `Armed`. No production caller branches on it: the arm/disarm log line is the
/// capture loop's (see [`on_client_line`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    /// The line says nothing about the temple.
    Ignored,
    /// A resting (or expired) gate is now armed — the one case worth a log
    /// line.
    Armed(ArmReason),
    /// A live arm was pushed out, was already reaching further than this line
    /// would have bought, or — the two cases that are not about horizons — had
    /// its reason replaced in place by the temple area line, or was left exactly
    /// as it was because the line was Alva's banter inside the temple (both in
    /// [`apply_line`]). What the spelling means is "the gate was already open",
    /// not "it now reaches further". Silent: Alva speaks several times per
    /// incursion.
    Extended(ArmReason),
    /// Something ended the arm: an area change or a non-START Alva line.
    Disarmed,
}

/// Whether horizon `held` reaches at least as far as `next`. `None` is the far
/// horizon — an area arm outlives every deadline.
fn outlives(held: Option<u64>, next: Option<u64>) -> bool {
    match (held, next) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(held), Some(next)) => held >= next,
    }
}

impl TempleArm {
    /// Whether the loop may capture at `now_ms`.
    pub fn is_armed(&self, now_ms: u64) -> bool {
        match self {
            TempleArm::Disarmed => false,
            TempleArm::Armed { until_ms, .. } => until_ms.is_none_or(|until| now_ms < until),
        }
    }

    /// Why it is armed, for the log. `None` when it is not.
    pub fn reason(&self) -> Option<ArmReason> {
        match self {
            TempleArm::Disarmed => None,
            TempleArm::Armed { reason, .. } => Some(*reason),
        }
    }

    /// Arm until `until_ms` (`None` = until the next area change), **never
    /// shortening a live arm**.
    ///
    /// The no-shortening rule is what makes the three reasons composable
    /// without an order of precedence between them. Since POE-389 removed
    /// Re-arm no bid carries a deadline, so nothing reaches the refusal.
    ///
    /// An EXPIRED arm is not a live one: it is replaced, and reported as a
    /// fresh arm, because that is what the log reader sees.
    ///
    /// # Nothing shortens a live arm any more
    ///
    /// Until WI-1 an END line replaced a deadline-free [`ArmReason::AlvaStart`]
    /// arm with a two-minute one — the one place in this module where a live arm
    /// got a nearer horizon. That line now DISARMS ([`apply_line`]), which needs
    /// no horizon at all, so the exception is gone rather than moved.
    ///
    /// The temple area line still ASSIGNS its reason rather than bidding through
    /// this method. A bid would buy nothing over a live deadline-free
    /// `AlvaStart` arm — `outlives(None, None)` holds — and Alva's banter
    /// seconds later would then arrive to find a START arm and stand the capture
    /// down mid-run.
    fn arm(&mut self, reason: ArmReason, until_ms: Option<u64>, now_ms: u64) -> Transition {
        let live = self.is_armed(now_ms);
        if live {
            if let TempleArm::Armed {
                until_ms: held,
                reason: held_reason,
            } = *self
            {
                if outlives(held, until_ms) {
                    return Transition::Extended(held_reason);
                }
            }
        }
        *self = TempleArm::Armed { until_ms, reason };
        if live {
            Transition::Extended(reason)
        } else {
            Transition::Armed(reason)
        }
    }

    /// Stop looking, whatever armed it: the area changed, Alva spoke, or the
    /// cycle finished.
    fn disarm(&mut self) -> Transition {
        let was_armed = matches!(self, TempleArm::Armed { .. });
        *self = TempleArm::Disarmed;
        if was_armed {
            Transition::Disarmed
        } else {
            Transition::Ignored
        }
    }
}

/// Everything one lock holds about the gate: what has armed the loop, and what
/// the next stand-down will be reported as.
///
/// The second field is not a second gate — [`TempleArm`] is still the only thing
/// that decides whether the loop may look. It is the WORD for the closed gate,
/// and it is a field rather than a derivation because its causes are events that leave no trace in `TempleArm`: a `Disarmed` arm looks
/// identical whether Alva ended it, the zone did, or the cycle finished. See
/// [`StandDown`], which owns the rule about which writer sets what.
///
/// It replaced `left_area_ms` in WI-1 (2026-09-07). That field existed to stop
/// POE-246's 120 s panel clock outliving the screen it was measured on; the
/// clock is now one detect tick, so a zone change can carry at most 650 ms of
/// capture into the next zone — and the tick that carries it is the one that
/// finds the panel gone. Two ticks since 2026-09-11 (POE-275): a zone change
/// can carry two captures, the first a held miss and the second the retire.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ArmState {
    /// What has armed the loop.
    pub arm: TempleArm,
    /// The words the NEXT stand-down is reported with — see [`StandDown`].
    pub stood_down: StandDown,
}

/// The three phrases Alva speaks when a portal OPENS, exactly as Client.txt
/// writes them (module doc: 122 / 118 / 101 of 684 mined lines).
///
/// Matched whole and trimmed rather than by prefix, because `Time to go,
/// exile.` — which is NOT in either log, while `Good job, exile.` is — would
/// pass a prefix test and start a cycle on an end line.
const START_PHRASES: [&str; 3] = ["Time to go.", "Let's go.", "It's time!"];

/// What one Client.txt line IS, as far as this module is concerned.
///
/// The single vocabulary [`classify`] answers in, and the reason there is one
/// classifier rather than a predicate per consumer: the arm, the cycle flag,
/// the board epoch and the advice clear all key on the same four facts, and a
/// fifth kind added to one of four private tests would have been skipped in
/// production and passed every test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEvent {
    /// `: You have entered The Temple of Atzoatl.` — the temple's own area
    /// line, which arms and ends nothing.
    EnteredTemple,
    /// `: You have entered <anything else>`.
    LeftArea,
    /// An [`ALVA_SPEAKER`] line whose message is one of [`START_PHRASES`].
    AlvaStart,
    /// Any other [`ALVA_SPEAKER`] line — the two `Good job` variants, the
    /// one-off `Just in time.`, and the temple banter.
    AlvaEnd,
}

/// Whether this event ends a cycle: the notice comes down and the board the
/// loop has read is no longer about the incursion in front of the player.
///
/// A START ends one as well as beginning one — starts and ends pair 341 : 342
/// in the mining, so a second start with no end between them is theoretical,
/// but if it happens the board from the previous incursion must not survive
/// into the next. The temple's OWN area line ends nothing: the player walking
/// into the temple is inside the cycle the start line opened.
pub fn ends_epoch(event: LineEvent) -> bool {
    matches!(
        event,
        LineEvent::LeftArea | LineEvent::AlvaStart | LineEvent::AlvaEnd
    )
}

/// The KIND of a line, before any clock is consulted — [`classify`]'s first two
/// questions and the whole of [`may_end_advice`].
///
/// The area branch comes first and returns: an area line carries no speaker
/// (`: You have entered …`), so the two cannot both match, and reading the area
/// first is what makes "the first `You have entered` after the tail always
/// wins" true by construction rather than by ordering luck.
///
/// Area changes are read with [`crate::client_log::parse_entered_area`], the app's
/// one owner of that parse — blind spot included: a player's chat line quoting
/// the sentence reads as an area change here. The cost is bounded (a wrong arm
/// captures a screen with no panel on it; a wrong disarm is undone by the next
/// real area line).
fn line_kind(line: &str) -> Option<LineKind> {
    if let Some(area) = crate::client_log::parse_entered_area(line) {
        return Some(LineKind::Area {
            temple: area == TEMPLE_AREA,
        });
    }
    (speaker_of(line) == Some(ALVA_SPEAKER)).then_some(LineKind::Alva)
}

/// [`line_kind`]'s answer: the two shapes of line this module reads at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineKind {
    Area { temple: bool },
    Alva,
}

/// The words after `"<speaker>: "`, trimmed. `None` for a line with no tag and
/// no separator — the shape [`speaker_of`] already refused.
fn message_of(line: &str) -> Option<&str> {
    let message = line.get(line.find("] ")? + 2..)?;
    Some(message.split_once(": ")?.1.trim())
}

/// What one Client.txt line says about the temple. `None` for a line that says
/// nothing.
///
/// **The one owner of every per-line decision in this module.** [`apply_line`],
/// [`advice_end`] and [`on_client_line`]'s cycle flag and epoch bump all read
/// this answer, so "which lines matter" is one question with one place to be
/// answered.
///
/// Runs on EVERY line the watcher reads, so the order is the cost order: the
/// area parse is one `str::find`, and the timestamp is parsed only for a line
/// that is already known to be Alva's.
///
/// The staleness gate applies to the two Alva kinds and to neither area kind.
/// A voice line is evidence about the screen at the moment it was SPOKEN, and
/// one older than [`LINE_STALE_MS`] on arrival reached us through a log the
/// watcher was not tailing (a path change, a restart) — it is evidence about a
/// screen that is minutes gone. An AREA is a state rather than a burst, so its
/// age is irrelevant.
pub fn classify(line: &str, now_ms: u64) -> Option<LineEvent> {
    match line_kind(line)? {
        LineKind::Area { temple: true } => Some(LineEvent::EnteredTemple),
        LineKind::Area { temple: false } => Some(LineEvent::LeftArea),
        LineKind::Alva => {
            // Read off the RAW stamp, not off `arm_at`'s answer: `arm_at`
            // clamps a stamp further than `MAX_BACKDATE_MS` back to `now`, so
            // asking it would launder every stale line into a fresh one.
            let stamp = line_timestamp_ms(line);
            if stamp.is_some_and(|ms| now_ms.saturating_sub(ms) >= LINE_STALE_MS) {
                return None;
            }
            Some(match message_of(line) {
                Some(message) if START_PHRASES.contains(&message) => LineEvent::AlvaStart,
                _ => LineEvent::AlvaEnd,
            })
        }
    }
}

/// One Client.txt line, folded into the arm state.
///
/// [`classify`] answers what the line IS and this decides what that does to the
/// gate.
///
/// # The END line stands the capture DOWN (WI-1, 2026-09-07)
///
/// A START arm has no deadline (see [`ArmReason::AlvaStart`]), so something has
/// to end it, and the END line is the game itself saying the incursion is over.
/// Until WI-1 that line traded the deadline-free arm for a two-minute tail so
/// the player could reopen the sheet map-side; the owner's rule is that nothing
/// should be probing between incursions, so it now disarms outright. The module
/// doc records the trade.
///
/// # The one exception: Alva inside the temple
///
/// `At last... Atzoatl.` and `No wonder it's lost…` are [`LineEvent::AlvaEnd`]
/// by the phrase table and are spoken seconds into a temple run. Standing the
/// capture down on them would blind the module for the whole run — the sheet is
/// the navigation aid in there — so a live [`ArmReason::TempleArea`] arm is left
/// exactly as it is, and only the area line out of the temple ends it.
///
/// The exception is keyed on the REASON and not on the area, because the reason
/// is what the temple's own area line assigns ([`LineEvent::EnteredTemple`]).
/// Every other arm — a START arm map-side — is ended by the line.
pub fn apply_line(state: &mut ArmState, line: &str, now_ms: u64) -> Transition {
    match classify(line, now_ms) {
        None => Transition::Ignored,
        Some(LineEvent::EnteredTemple) => {
            // A FACT, not a bid: the player IS in the temple, so the reason is
            // assigned rather than offered to [`TempleArm::arm`]'s
            // no-shortening rule. Bidding would leave a live
            // [`ArmReason::AlvaStart`] arm's reason in place (both horizons are
            // `None`, so the bid buys nothing), and Alva's temple banter
            // seconds later — an [`LineEvent::AlvaEnd`] — would then find a
            // START arm and, since WI-1 (2026-09-07), stand it DOWN outright:
            // blind for the rest of the run rather than for the tail's 120 s,
            // and on the surface the run is played on.
            let live = state.arm.is_armed(now_ms);
            state.arm = TempleArm::Armed {
                until_ms: None,
                reason: ArmReason::TempleArea,
            };
            if live {
                Transition::Extended(ArmReason::TempleArea)
            } else {
                Transition::Armed(ArmReason::TempleArea)
            }
        }
        Some(LineEvent::LeftArea) => {
            // Written before the disarm and unconditionally: the gate may
            // already be closed, and the cause is about the LINE rather than
            // about what the line moved.
            state.stood_down = StandDown::LeftArea;
            state.arm.disarm()
        }
        Some(LineEvent::AlvaStart) => state.arm.arm(ArmReason::AlvaStart, None, now_ms),
        Some(LineEvent::AlvaEnd) => {
            let in_temple = state.arm.reason() == Some(ArmReason::TempleArea);
            if in_temple && state.arm.is_armed(now_ms) {
                // Alva's temple banter. The arm is untouched — `Extended` is
                // this module's word for "the gate was already open", which is
                // exactly what happened.
                return Transition::Extended(ArmReason::TempleArea);
            }
            state.stood_down = StandDown::AlvaLine;
            state.arm.disarm()
        }
    }
}

/// Why the advice the module is showing has stopped describing the board in
/// front of the player (POE-248).
///
/// The room widget lives with the INCURSION, not with the capture: the layout
/// panel closes the moment the player walks into the room, and POE-244's
/// stand-down clear took the door diamond off screen at exactly the moment it
/// was the only surface left. So the loop standing down no longer ends the
/// advice — these two lines do, and both are facts about the GAME rather than
/// about whether anything is looking at it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdviceEnd {
    /// `: You have entered <not the temple>` — the board the advice describes
    /// is not on this screen and cannot be got back to.
    LeftArea,
    /// An [`ALVA_SPEAKER`] line stamped after the read. One incursion ended or
    /// another began; either way the panel behind the advice is a panel from
    /// the previous one.
    NewIncursion,
}

impl AdviceEnd {
    /// The words the app log uses for this end.
    pub fn label(self) -> &'static str {
        match self {
            AdviceEnd::LeftArea => "the zone changed",
            AdviceEnd::NewIncursion => "Alva spoke again",
        }
    }
}

/// Whether a line is of a KIND this module reads at all: an area change, or a
/// voice line from [`ALVA_SPEAKER`].
///
/// The clock-free half of [`classify`] — the same [`line_kind`] call, so there
/// is ONE answer to which lines matter and a fifth kind added to the classifier
/// cannot be skipped here.
///
/// It exists as its own function because [`on_client_line`] wants that answer
/// BEFORE it does anything else, and that is not a micro-optimisation: the glue
/// runs on EVERY Client.txt line the watcher reads, and
/// [`super::run::publish`] clones the whole [`super::slice::TempleSlice`] —
/// thirteen plates, forty-two rects and a board — to decide whether the
/// snapshot changed. Two string searches decide that it need not.
pub fn may_end_advice(line: &str) -> bool {
    line_kind(line).is_some()
}

/// Whether one Client.txt line ends the advice a read at `last_read_ms`
/// produced.
///
/// The sibling of [`apply_line`] and deliberately a SECOND function rather than
/// a second return value: the arm is about whether to look at the screen and
/// this is about whether what was last seen still holds, and the two answers
/// diverge on every line that matters. `: You have entered <a map>` disarms AND
/// ends the advice; `Alva, Master Explorer: Good job.` ARMS (the player may
/// open the panel to see what the kill changed) and ends the advice of the
/// incursion that just finished; the temple's own area line arms and ends
/// nothing.
///
/// `None` for a board that was never read (`last_read_ms` is `None`) — there is
/// nothing to end, and answering otherwise would put a log line on every zone
/// change of a session that never opened a panel.
///
/// # Why the Alva line is compared to the READ and not just accepted
///
/// The line that ARMS the capture is an Alva line, and it is spoken seconds
/// before the read it buys. Ending the advice on any Alva line at all would
/// therefore clear the board the same voice line was the reason for reading.
/// So the comparison is against the read's own stamp, and the tie is broken
/// TOWARD keeping the advice: Client.txt writes whole seconds, so a line
/// spoken in the same second as the read reads as older than it and is ignored.
/// The cost of that direction is one stale board until the next line or the
/// next read; the cost of the other is the widget blinking out at the moment
/// it appears.
///
/// Staleness and the kind test are [`classify`]'s, which is what this reads: a
/// line that reached us through a log the watcher was not tailing is evidence
/// about a screen that is minutes gone, and [`arm_at`] would launder its stamp
/// into `now`.
///
/// Both Alva kinds end the advice. A START is the next incursion beginning and
/// an END is this one finishing, and the board behind the advice belongs to the
/// previous one either way.
pub fn advice_end(line: &str, last_read_ms: Option<u64>, now_ms: u64) -> Option<AdviceEnd> {
    let last_read = last_read_ms?;
    match classify(line, now_ms)? {
        LineEvent::EnteredTemple => None,
        LineEvent::LeftArea => Some(AdviceEnd::LeftArea),
        LineEvent::AlvaStart | LineEvent::AlvaEnd => {
            (arm_at(now_ms, line_timestamp_ms(line)) > last_read).then_some(AdviceEnd::NewIncursion)
        }
    }
}

/// The arm state an app that started mid-session should begin in.
///
/// `lab_navigation::replay_recent_log` hands its events to the lab overlays and
/// never reaches a trigger, so without this an app started (or a Client.txt
/// path changed) INSIDE the temple would sit `Disarmed` for the rest of the
/// run: no further Alva line is coming, and no further area change until the
/// player leaves. That is worse than the free-running loop this replaces.
///
/// Only the newest `You have entered` line in the buffer is read. **A voice
/// line in the replay never arms**, however recent it looks: a voice line is
/// evidence about a screen at a moment that has already passed, and the whole
/// of [`LINE_STALE_MS`] says so. An AREA is not a burst — it is where the
/// player is standing right now — so its age is irrelevant and it arms
/// regardless.
pub fn catch_up_state(tail: &str) -> TempleArm {
    match lab_navigation::newest_entered_area(tail) {
        Some(TEMPLE_AREA) => TempleArm::Armed {
            until_ms: None,
            reason: ArmReason::TempleArea,
        },
        _ => TempleArm::Disarmed,
    }
}

/// Seed `state` from a catch-up tail, and report what it was seeded to.
///
/// [`catch_up`]'s pure half, the sibling of [`apply_line`]. It OVERWRITES,
/// including with [`TempleArm::Disarmed`], and that is the whole point of it
/// being a function: a catch-up runs when the watcher (re)starts, which is also
/// when the Client.txt PATH changes, and a `TempleArea` arm carries no deadline.
/// Skipping the write when the new log says "not the temple" would leave that
/// arm standing with no area line ever coming to end it — the free-running loop
/// this module replaces, restored by a settings change.
///
/// Unlike [`apply_line`] this does not honour the no-shortening rule: the rule
/// composes reasons *within* one log, and a catch-up is the app changing which
/// log it believes.
///
/// A seeded `Disarmed` also resets [`ArmState::stood_down`] to
/// [`StandDown::Waiting`], which is what it means: this log says nothing is in
/// scope, and the cause carried over from the previous log describes an
/// incursion the app is no longer watching. The temple seed leaves it alone —
/// that arm has no deadline, so whatever ends it writes its own cause first.
pub fn apply_catch_up(state: &mut ArmState, tail: &str) -> TempleArm {
    state.arm = catch_up_state(tail);
    if state.arm == TempleArm::Disarmed {
        state.stood_down = StandDown::Waiting;
    }
    state.arm
}

/// [`apply_catch_up`] over the watcher start's shared tail, which is `None`
/// when there is no log to read. `None` is no answer, not an empty history:
/// it leaves `state` as it was and reports nothing seeded.
pub fn apply_catch_up_tail(state: &mut ArmState, tail: Option<&str>) -> Option<TempleArm> {
    tail.map(|tail| apply_catch_up(state, tail))
}

// ---------------------------------------------------------------------------
// Glue — the same operations against `AppState`
// ---------------------------------------------------------------------------

/// The arm state, copied out. The loop asks once per iteration.
pub fn arm_state(app: &AppHandle) -> ArmState {
    let state = app.state::<AppState>();
    let arm = *state.temple_state.arm.lock().unwrap_or_else(|e| e.into_inner());
    arm
}

/// The Client.txt seam: one line in, the arm state maybe moved.
///
/// Wired as a third call in the app's ONE Client.txt consumer (`lib.rs`) — the
/// trigger must not add a second tailer.
///
/// **It never logs the ARM.** That app-log line has one owner, and it is the
/// capture loop's `run::gate_line`, for three reasons: this function runs
/// whether or not the temple module is on (logging here would narrate a module
/// the user has switched off); when the module IS on, both would fire within a
/// second and put two lines in `app.log` for one event; and the loop covers the
/// two transitions this function cannot see at all — `MANUAL_ARM_GRACE_MS`
/// running out and the cycle completing, neither of which any Client.txt line
/// announces.
///
/// What it DOES log is the two facts the loop cannot see: the cycle beginning
/// and ending (POE-249), and the advice being cleared (POE-248). Both are
/// statements about the incursion rather than about whether anything is
/// looking, and both are logged only when the slice actually moved.
pub fn on_client_line(app: &AppHandle, line: &str) {
    // The kind test comes FIRST, before the clock and before either lock: this
    // runs on every Client.txt line the watcher reads, and a line that is
    // neither an area change nor Alva's cannot move anything below (it is the
    // `None` [`classify`] would answer, so [`apply_line`] would be a no-op).
    if !may_end_advice(line) {
        return;
    }
    let now = super::run::now_ms();
    let event = classify(line, now);
    let armed_after = {
        let state = app.state::<AppState>();
        let mut arm = state.temple_state.arm.lock().unwrap_or_else(|e| e.into_inner());
        // Classified a second time inside, which is two string searches on the
        // handful of lines a map produces: one entry point for the arm beats a
        // second one that only the glue could reach.
        apply_line(&mut arm, line, now);
        arm.arm.is_armed(now)
    };
    crate::read_prompt::apply(app, crate::read_prompt::temple_change(event, armed_after));
    // A stale Alva line is the remaining `None`: it says nothing about the
    // screen now, so it moves neither the cycle nor the epoch nor the advice.
    let Some(event) = event else {
        return;
    };
    if ends_epoch(event) {
        // INVALIDATE, never force. See `TempleState::epoch`.
        app.state::<AppState>()
            .temple_state
            .epoch
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
    // ONE publish for both writes: `publish` clones the slice to decide whether
    // to emit, and two closures would clone it twice and emit twice for one
    // line. The advice clear is decided INSIDE it so the read stamp it is
    // compared against is the one being overwritten.
    let waiting = match event {
        LineEvent::AlvaStart => Some(true),
        LineEvent::AlvaEnd | LineEvent::LeftArea => Some(false),
        // The temple's own area line is inside the cycle, not a boundary of it.
        LineEvent::EnteredTemple => None,
    };
    let mut moved = None;
    let mut ended = None;
    super::run::publish(app, |slice| {
        if let Some(waiting) = waiting {
            if slice.waiting_for_panel != waiting {
                // Read back rather than trusting the INTENT: `start_cycle`
                // refuses an `Unavailable` slice, and a log line for a wait
                // that was never raised is the same lie on the page one surface
                // over.
                let before = slice.waiting_for_panel;
                if waiting {
                    super::slice::start_cycle(slice);
                } else {
                    super::slice::end_cycle(slice);
                }
                moved = (slice.waiting_for_panel != before).then_some(slice.waiting_for_panel);
            }
        }
        // Its own guard, and only this one: the flag above has to be written on
        // a START, which is exactly the state that arrives with no advice.
        if slice.advice.is_some() {
            if let Some(end) = advice_end(line, slice.last_read_at, now) {
                ended = Some(end);
                super::slice::clear_advice(slice);
            }
        }
    });
    if let Some(waiting) = moved {
        // The ARM is not logged here — `super::run::gate_line` owns that line.
        // This one is about the cycle, which the loop cannot see.
        crate::app_log(
            app,
            if waiting {
                format!(
                    "Temple: waiting for the temple panel ({})",
                    ArmReason::AlvaStart.label()
                )
            } else {
                let reason = match event {
                    LineEvent::LeftArea => AdviceEnd::LeftArea,
                    _ => AdviceEnd::NewIncursion,
                };
                format!("Temple: cycle ended — {}", reason.label())
            },
        );
    }
    if let Some(end) = ended {
        crate::app_log(
            app,
            format!("Temple: advice cleared — {} (the room widget is down)", end.label()),
        );
    }
}

/// Seed the arm state from the log the watcher is about to tail.
///
/// Called once per watcher start, beside the lab catch-up and over the same
/// 32 KB tail, which the caller reads once ([`crate::client_log::recent_log_tail`])
/// and hands to every catch-up pass. `None` leaves the arm state alone. See
/// [`catch_up_state`] for why this exists at all.
pub fn catch_up(app: &AppHandle, tail: Option<&str>) {
    let Some(seeded) = ({
        let state = app.state::<AppState>();
        let mut arm = state.temple_state.arm.lock().unwrap_or_else(|e| e.into_inner());
        apply_catch_up_tail(&mut arm, tail)
    }) else {
        return;
    };
    // The WRITE is unconditional (see [`apply_catch_up`]); only the line is
    // conditional, because "the newest area is not the temple" is the ordinary
    // case and every watcher start would otherwise log it.
    if seeded != TempleArm::Disarmed {
        crate::app_log(
            app,
            "Temple: catch-up — the log's newest area is the temple, armed".to_string(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wall-clock moment inside a plain, unambiguous local hour on every zone
    /// this suite runs in (2025-09-02 08:00 UTC).
    const NOW: u64 = 1_756_800_000_000;

    /// A Client.txt line stamped at `at_ms`, in the shape the game writes.
    ///
    /// The stamp is formatted through `chrono::Local` because that is the zone
    /// [`line_timestamp_ms`] reads it back in — writing a fixed string would
    /// make every timing assertion here a function of the host's offset.
    fn stamped(at_ms: u64, message: &str) -> String {
        use chrono::TimeZone;
        let when = chrono::Local
            .timestamp_millis_opt(at_ms as i64)
            .single()
            .expect("a plain local hour");
        format!(
            "{} 105432578 cffb0716 [INFO Client 12345] {message}",
            when.format("%Y/%m/%d %H:%M:%S"),
        )
    }

    /// The most-used measured START phrase — 122 of the 684 mined lines.
    fn alva_start(at_ms: u64) -> String {
        stamped(at_ms, "Alva, Master Explorer: Time to go.")
    }

    /// A measured END line: what Alva says when the incursion closes, and the
    /// generic "Alva spoke" fixture for everything the phrase table does not
    /// admit as a start.
    fn alva_end(at_ms: u64) -> String {
        stamped(at_ms, "Alva, Master Explorer: Good job.")
    }

    /// The area line for a map — anything that is not the temple.
    fn map_line(at_ms: u64) -> String {
        stamped(at_ms, ": You have entered Ancient City.")
    }

    /// The temple's own area line, exactly as measured.
    fn temple_line(at_ms: u64) -> String {
        stamped(at_ms, ": You have entered The Temple of Atzoatl.")
    }

    // -------------------------------------------------------- the classifier --

    /// All three START phrases, because the mining splits 122 / 118 / 101
    /// between them: a table missing one misses a third of incursions, and it
    /// misses them SILENTLY — nothing on screen says the notice did not appear.
    #[test]
    fn every_measured_start_phrase_starts_a_cycle() {
        for phrase in ["Time to go.", "Let's go.", "It's time!"] {
            let line = stamped(NOW, &format!("Alva, Master Explorer: {phrase}"));

            assert_eq!(classify(&line, NOW), Some(LineEvent::AlvaStart), "{phrase}");
        }
    }

    /// Everything else Alva says ends one: both `Good job` variants, the
    /// one-off `Just in time.` (mined once, with no incursion after it) and the
    /// temple banter.
    ///
    /// Fails if the fallback arm of the phrase match is `None` or a start — an
    /// end variant nobody has heard would then leave the waiting notice
    /// standing over a finished incursion, which is the asymmetry the
    /// START-only rule exists to buy.
    #[test]
    fn every_other_alva_line_ends_one() {
        for phrase in [
            "Good job.",
            "Good job, exile.",
            "Just in time.",
            "No wonder it's lost…",
            "At last... Atzoatl.",
        ] {
            let line = stamped(NOW, &format!("Alva, Master Explorer: {phrase}"));

            assert_eq!(classify(&line, NOW), Some(LineEvent::AlvaEnd), "{phrase}");
        }
    }

    /// `Time to go, exile.` is in NEITHER log — the `, exile` variant is on the
    /// end line — so a prefix match on `Time to go.` would open a cycle on a
    /// line the game does not speak. Fails the moment the table is matched by
    /// prefix instead of whole.
    #[test]
    fn the_exile_variant_of_a_start_phrase_is_not_a_start() {
        let line = stamped(NOW, "Alva, Master Explorer: Time to go, exile.");

        assert_eq!(classify(&line, NOW), Some(LineEvent::AlvaEnd));
    }

    /// Another NPC saying one of the phrases is not Alva. Fails if the phrase
    /// table is searched before (or instead of) the speaker.
    #[test]
    fn another_speaker_saying_a_start_phrase_is_not_classified() {
        let line = stamped(NOW, "Einhar, Beastmaster: Time to go.");

        assert_eq!(classify(&line, NOW), None);
    }

    /// The temple's own area line is an ENTRY and not a departure — it is
    /// inside the cycle the start line opened, and reading it as a departure
    /// would end the cycle at the moment the player walks in.
    #[test]
    fn the_temples_own_area_line_is_an_entry() {
        assert_eq!(classify(&temple_line(NOW), NOW), Some(LineEvent::EnteredTemple));
    }

    /// Every other area is the player leaving.
    #[test]
    fn any_other_area_line_is_a_departure() {
        assert_eq!(classify(&map_line(NOW), NOW), Some(LineEvent::LeftArea));
    }

    /// A start phrase that reached us through a log the watcher was not tailing
    /// is evidence about a screen that is minutes gone. Fails if the staleness
    /// gate is applied after the phrase match, or only to end lines — a restart
    /// over an old log would then open a cycle and put the notice up.
    #[test]
    fn a_stale_start_phrase_is_not_classified_at_all() {
        let spoken = NOW - LINE_STALE_MS;

        assert_eq!(classify(&alva_start(spoken), NOW), None, "the gate is inclusive");
        // A whole second inside, not a millisecond: Client.txt writes seconds,
        // so a finer step would format to the same stamp and pin nothing.
        assert_eq!(
            classify(&alva_start(spoken + 1_000), NOW),
            Some(LineEvent::AlvaStart),
            "one second inside it",
        );
    }

    /// An unstamped line is not a stale one. Fails if the missing stamp is read
    /// as "infinitely old" — every line of a log written without timestamps
    /// would be dropped, and the module would never arm on that machine.
    #[test]
    fn an_alva_line_with_no_timestamp_is_classified() {
        let line = "[INFO Client 12345] Alva, Master Explorer: Let's go.";

        assert_eq!(classify(line, NOW), Some(LineEvent::AlvaStart));
    }

    /// Which events are cycle boundaries: the epoch the capture loop keys its
    /// board on moves on all three, and NOT on the temple's own area line —
    /// bumping there would invalidate the board the player is walking in to
    /// read.
    #[test]
    fn the_cycle_boundaries_are_the_three_that_are_not_the_temple_door() {
        for (event, boundary) in [
            (LineEvent::LeftArea, true),
            (LineEvent::AlvaStart, true),
            (LineEvent::AlvaEnd, true),
            (LineEvent::EnteredTemple, false),
        ] {
            assert_eq!(ends_epoch(event), boundary, "{event:?}");
        }
    }

    // ------------------------------------------------------- voice lines --

    /// The WI-1 rule, on a resting gate: an END line puts nothing in scope.
    ///
    /// Until 2026-09-07 this line armed for two minutes so the player could
    /// reopen the sheet map-side; the owner's rule is that nothing probes
    /// between incursions. Fails the
    /// moment the `AlvaEnd` branch arms again — the free-running capture between
    /// incursions is exactly what the owner reported.
    #[test]
    fn an_end_line_over_a_resting_gate_arms_nothing() {
        let mut state = ArmState::default();

        let transition = apply_line(&mut state, &alva_end(NOW), NOW);

        assert_eq!(transition, Transition::Ignored);
        assert!(!state.arm.is_armed(NOW));
    }

    /// A line from any other speaker is not Alva. Fails if the match is a
    /// substring search or a speaker SHAPE (which `Alva, Master Explorer`
    /// deliberately does not have — see `mercenary::trigger`).
    #[test]
    fn another_npcs_voice_line_does_not_arm_the_loop() {
        let mut state = ArmState::default();

        let transition = apply_line(
            &mut state,
            &stamped(NOW, "Varashta, the Winter Sekhema: Come closer."),
            NOW,
        );

        assert_eq!(transition, Transition::Ignored);
        assert!(!state.arm.is_armed(NOW));
    }

    /// A START line the watcher only reached a minute late — a path change, a
    /// restart over an old log — says nothing about the screen now, so it arms
    /// nothing and (through `on_client_line`) opens no cycle.
    ///
    /// Fails if staleness is read off `arm_at`'s answer instead of the raw
    /// stamp: `arm_at` clamps a stamp further back than `MAX_BACKDATE_MS` to
    /// `now`, so asking it launders exactly this line into a fresh arm. (The
    /// two constants are equal today, so the laundering only shows up beyond
    /// the clamp — which is why this test is a minute old and not ten seconds.)
    #[test]
    fn a_stale_alva_line_does_not_arm() {
        let mut state = ArmState::default();
        let spoken = NOW - 60_000;

        let transition = apply_line(&mut state, &alva_start(spoken), NOW);

        assert_eq!(transition, Transition::Ignored);
        assert_eq!(state.arm, TempleArm::Disarmed);
    }

    /// The other side of the staleness boundary, over the branch that ARMS: a
    /// START line one second inside the window is still evidence about now.
    ///
    /// Fails if the gate is inclusive, or is applied to `arm_at`'s laundered
    /// stamp rather than the raw one — either way the module stops arming on
    /// perfectly fresh lines.
    #[test]
    fn a_start_line_just_inside_the_stale_window_still_arms() {
        let mut state = ArmState::default();
        let spoken = NOW - (LINE_STALE_MS - 1_000);

        apply_line(&mut state, &alva_start(spoken), NOW);

        assert_eq!(
            state.arm,
            TempleArm::Armed {
                until_ms: None,
                reason: ArmReason::AlvaStart,
            },
        );
    }

    /// A START line arms with NO deadline, unlike every other voice line.
    ///
    /// The start fires when the PORTAL OPENS and nothing in the game times an
    /// open portal out — the mining holds one 22-minute gap between a start and
    /// its end, the player being away from the PC. Fails if the start is given
    /// any deadline at all: the module would go blind part-way into a wait the
    /// game itself does not bound.
    #[test]
    fn a_start_phrase_arms_the_loop_with_no_deadline() {
        let mut state = ArmState::default();

        let transition = apply_line(&mut state, &alva_start(NOW), NOW);

        assert_eq!(transition, Transition::Armed(ArmReason::AlvaStart));
        assert_eq!(
            state.arm,
            TempleArm::Armed {
                until_ms: None,
                reason: ArmReason::AlvaStart,
            },
        );
        assert!(state.arm.is_armed(NOW + 3_600_000), "an hour later, still armed");
    }

    /// The measured end of an incursion, and WI-1's headline: the game says the
    /// incursion is over, so the deadline-free start arm ends there and then.
    ///
    /// Owner, 2026-09-07: *"After leaving the incursion, OCR never stops
    /// probing, and it should on any Alva voiceline, or zone change."* Fails if
    /// the end line is dropped over a live arm (the start arm would last until
    /// the next zone change, capturing across the rest of the map) or if it is
    /// put back to buying a tail.
    #[test]
    fn an_end_line_stands_a_start_arm_down() {
        let mut state = ArmState::default();
        apply_line(&mut state, &alva_start(NOW), NOW);
        let ended = NOW + 34_000;

        let transition = apply_line(&mut state, &alva_end(ended), ended);

        assert_eq!(transition, Transition::Disarmed);
        assert_eq!(state.arm, TempleArm::Disarmed);
        assert_eq!(state.stood_down, StandDown::AlvaLine, "and the log has a word for it");
    }

    // ------------------------------------------------------ area changes --

    /// Entering the temple arms with no deadline: a temple run is as long as it
    /// is. Fails if the area arm is given a tail — the module would go blind
    /// part-way through the longest runs, which are the ones worth advising on.
    #[test]
    fn entering_the_temple_arms_until_the_next_area_change() {
        let mut state = ArmState::default();

        let transition = apply_line(&mut state, &temple_line(NOW), NOW);

        assert_eq!(transition, Transition::Armed(ArmReason::TempleArea));
        assert_eq!(
            state.arm,
            TempleArm::Armed {
                until_ms: None,
                reason: ArmReason::TempleArea,
            },
        );
        assert!(state.arm.is_armed(NOW + 3_600_000), "an hour later, still armed");
    }

    /// Leaving the temple ends it. Fails if the area branch only handles the
    /// temple — a deadline-free arm would then last the rest of the session,
    /// which is the free-running loop this file replaces.
    #[test]
    fn the_next_area_change_disarms_a_temple_arm() {
        let mut state = ArmState::default();
        apply_line(&mut state, &temple_line(NOW), NOW);

        let transition = apply_line(&mut state, &map_line(NOW + 60_000), NOW + 60_000);

        assert_eq!(transition, Transition::Disarmed);
        assert!(!state.arm.is_armed(NOW + 60_000));
    }

    /// The START arm carries no deadline, so the zone change is one of only
    /// three things that can end it — and the one that fires when the player
    /// abandons the incursion instead of finishing it.
    ///
    /// Fails if the [`LineEvent::LeftArea`] branch spares an
    /// [`ArmReason::AlvaStart`] arm the way [`apply_line`]'s end-line branch
    /// singles that reason out: a start heard before a portal the player never
    /// took would then keep the loop capturing for the rest of the session.
    #[test]
    fn an_area_change_disarms_a_start_arm() {
        let mut state = ArmState::default();
        apply_line(&mut state, &alva_start(NOW), NOW);

        let transition = apply_line(&mut state, &map_line(NOW + 5_000), NOW + 5_000);

        assert_eq!(transition, Transition::Disarmed);
        assert!(!state.arm.is_armed(NOW + 5_000));
    }

    /// The measured sequence, in the order the game writes it: the START line
    /// opens the portal, the area line follows when the player steps through,
    /// and Alva's temple banter (`At last... Atzoatl.`) lands seconds later.
    ///
    /// **The START comes first because that is the only arrangement that can
    /// see the bug.** Enter the temple from a resting gate and the arm is
    /// `TempleArea` whatever the entry branch does; enter it with a live
    /// deadline-free `AlvaStart` arm and a BID buys nothing (`outlives(None,
    /// None)` holds), so the reason would stay `AlvaStart` and the banter — an
    /// [`LineEvent::AlvaEnd`] by the phrase table — would find no `TempleArea`
    /// arm to be excused by and stand the capture down seconds into the run.
    ///
    /// Fails if the temple entry bids through [`TempleArm::arm`] rather than
    /// assigning, if arming is "latest wins", or if the end line's temple
    /// exception is dropped: the module would go blind for the whole of every
    /// temple run, which is where the sheet is the navigation aid.
    #[test]
    fn an_alva_line_inside_the_temple_does_not_stand_the_area_arm_down() {
        let mut state = ArmState::default();
        apply_line(&mut state, &alva_start(NOW), NOW);
        apply_line(&mut state, &temple_line(NOW + 1_000), NOW + 1_000);

        let transition = apply_line(
            &mut state,
            &stamped(NOW + 5_000, "Alva, Master Explorer: At last... Atzoatl."),
            NOW + 5_000,
        );

        assert_eq!(transition, Transition::Extended(ArmReason::TempleArea));
        assert_eq!(
            state.arm,
            TempleArm::Armed {
                until_ms: None,
                reason: ArmReason::TempleArea,
            },
        );
        assert!(
            state.arm.is_armed(NOW + 3_600_000),
            "an hour into the run, still armed",
        );
    }

    // ---------------------------------------------------------- catch-up --

    /// An app started inside a temple gets no further area line and no further
    /// voice line. Fails if the catch-up is dropped — the module would sit
    /// `Waiting` for the whole run, which is worse than the loop it replaced.
    #[test]
    fn a_replay_whose_newest_area_is_the_temple_arms() {
        let tail = format!("{}\n{}\n", map_line(NOW - 600_000), temple_line(NOW - 300_000));

        assert_eq!(
            catch_up_state(&tail),
            TempleArm::Armed {
                until_ms: None,
                reason: ArmReason::TempleArea,
            },
        );
    }

    /// The ordinary start: the player is in a map. Fails if the pass takes the
    /// FIRST area in the buffer rather than the newest, which would arm on
    /// every temple the tail happens to still hold.
    #[test]
    fn a_replay_whose_newest_area_is_a_map_does_not_arm() {
        let tail = format!("{}\n{}\n", temple_line(NOW - 600_000), map_line(NOW - 300_000));

        assert_eq!(catch_up_state(&tail), TempleArm::Disarmed);
    }

    /// A tail with no `You have entered` line at all — a quiet log, or one
    /// truncated between area changes — says nothing about where the player is
    /// standing, and "unknown" must not be guessed into "the temple". Fails if the `None` arm of the match is folded in with the
    /// temple one.
    #[test]
    fn a_replay_with_no_area_line_at_all_does_not_arm() {
        let tail = format!("{}\n", stamped(NOW - 5_000, "[WINDOW] Gained focus"));

        assert_eq!(catch_up_state(&tail), TempleArm::Disarmed);
    }

    /// The catch-up runs when the watcher restarts, which is also when the
    /// Client.txt PATH changes — and a `TempleArea` arm carries no deadline, so
    /// nothing else will ever end it. Fails if the seed is skipped when it
    /// comes out `Disarmed`, which leaves the module capturing on a map for the
    /// rest of the session.
    #[test]
    fn a_catch_up_over_a_tail_with_no_temple_clears_a_live_arm() {
        let mut state = ArmState {
            arm: TempleArm::Armed {
                until_ms: None,
                reason: ArmReason::TempleArea,
            },
            stood_down: StandDown::CycleComplete,
        };
        let tail = format!("{}\n", map_line(NOW - 1_000));

        apply_catch_up(&mut state, &tail);

        assert_eq!(state.arm, TempleArm::Disarmed);
        assert_eq!(
            state.stood_down,
            StandDown::Waiting,
            "and the cause carried over from the log it stopped tailing is gone",
        );
    }

    /// Voice lines in the replay are history, not evidence about the screen.
    /// Fails if the catch-up folds `apply_line` over the buffer — the app would
    /// then start armed off a line spoken before it was running.
    #[test]
    fn an_alva_line_in_the_replay_does_not_arm() {
        let tail = format!(
            "{}\n{}\n",
            alva_start(NOW - 5_000),
            stamped(NOW - 1_000, "Alva, Master Explorer: Good job, exile."),
        );

        assert_eq!(catch_up_state(&tail), TempleArm::Disarmed);
    }

    /// The watcher start reads one tail for every catch-up pass. A temple
    /// entry 8 KB from the end of a 50 KB log is inside that tail, so the
    /// seed arms on it. Fails if the shared tail shrinks below the catch-up
    /// window — the temple line would fall outside it and the app would start
    /// disarmed inside the temple.
    #[test]
    fn a_temple_entry_inside_the_shared_tail_arms_the_catch_up() {
        let temple = temple_line(NOW - 300_000);
        let path = crate::client_log::write_catch_up_fixture("temple", &[&map_line(NOW - 600_000), &temple]);
        let shared_tail = crate::client_log::recent_log_tail(&path);

        let mut state = ArmState::default();
        let seeded = apply_catch_up_tail(&mut state, shared_tail.as_deref());

        let expected = TempleArm::Armed {
            until_ms: None,
            reason: ArmReason::TempleArea,
        };
        assert_eq!(seeded, Some(expected));
        assert_eq!(state.arm, expected);
        let mut from_path = ArmState::default();
        let tail = crate::client_log::recent_log_tail(&path).expect("a tail for a non-empty file");
        assert_eq!(state.arm, apply_catch_up(&mut from_path, &tail));

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    // ----------------------------------------------- what ends the advice --

    /// A read at `NOW - 30 s`, which is what every case below is measured
    /// against: the board was read half a minute ago and the player is acting
    /// on it.
    const READ: Option<u64> = Some(NOW - 30_000);

    /// The zone change, which is the unambiguous end: the board the advice
    /// describes is not on this screen and cannot be walked back to.
    #[test]
    fn leaving_the_zone_ends_the_advice() {
        assert_eq!(
            advice_end(&map_line(NOW), READ, NOW),
            Some(AdviceEnd::LeftArea),
        );
    }

    /// Entering the TEMPLE does not. It is the arm's own area line, and the
    /// board that follows replaces the advice by being read.
    #[test]
    fn entering_the_temple_leaves_the_advice_alone() {
        assert_eq!(advice_end(&temple_line(NOW), READ, NOW), None);
    }

    /// The next voice line after the read — `Good job.` at the end of the
    /// incursion, or `Time to go.` at the start of the next one. Either way the
    /// panel behind the advice belonged to the previous one.
    #[test]
    fn an_alva_line_after_the_read_ends_the_advice() {
        assert_eq!(
            advice_end(&alva_start(NOW), READ, NOW),
            Some(AdviceEnd::NewIncursion),
        );
    }

    /// The regression this comparison exists for: the line that ARMS the
    /// capture is an Alva line, spoken seconds BEFORE the read it buys. Ending
    /// the advice on any Alva line at all would clear the board the same voice
    /// line was the reason for reading — the widget would blink out the moment
    /// it appeared, which is the shape of the bug POE-246 fixed one layer down.
    #[test]
    fn the_alva_line_that_armed_the_read_does_not_end_it() {
        let spoke = NOW - 40_000;
        let read = Some(NOW - 30_000);

        assert_eq!(advice_end(&alva_start(spoke), read, NOW), None);
    }

    /// A line stamped in the same SECOND as the read reads as older than it.
    /// Client.txt has one-second resolution and the tie is broken toward
    /// keeping the advice: one stale board costs a glance, a widget that
    /// vanishes costs the incursion.
    #[test]
    fn a_line_stamped_in_the_read_s_own_second_keeps_the_advice() {
        let second = NOW - 5_000;

        assert_eq!(advice_end(&alva_start(second), Some(second + 400), NOW), None);
    }

    /// Nothing has been read, so there is nothing to end — and the app log must
    /// not narrate a clear on every zone change of a session that never opened
    /// a panel.
    #[test]
    fn a_board_that_was_never_read_has_no_advice_to_end() {
        assert_eq!(advice_end(&map_line(NOW), None, NOW), None);
        assert_eq!(advice_end(&alva_start(NOW), None, NOW), None);
    }

    /// A line old enough to be about a screen that is minutes gone reaches us
    /// only through a log the watcher was not tailing. `apply_line` refuses to
    /// arm on one; this refuses to clear on one, and for the same reason —
    /// `arm_at` would otherwise launder its stamp into `now` and every restart
    /// would blank the board.
    #[test]
    fn a_stale_alva_line_does_not_end_the_advice() {
        let ancient = NOW - LINE_STALE_MS - 1_000;

        assert_eq!(advice_end(&alva_start(ancient), READ, NOW), None);
    }

    /// Ordinary chatter is not an end. Fails if the speaker match is widened.
    #[test]
    fn a_line_that_is_neither_an_area_nor_alva_ends_nothing() {
        let line = stamped(NOW, "Einhar, Beastmaster: What a beast!");

        assert_eq!(advice_end(&line, READ, NOW), None);
        assert!(
            !may_end_advice(&line),
            "the fast path must skip the slice lock for a line like this",
        );
    }

    /// The fast path admits exactly the two kinds `advice_end` can answer on.
    ///
    /// `on_client_line` returns before touching the slice when this is false,
    /// so a line it rejects can never be cleared on however the rest of the
    /// function is written — the two must not drift.
    #[test]
    fn the_fast_path_admits_both_kinds_of_line_that_can_end_the_advice() {
        assert!(may_end_advice(&map_line(NOW)), "an area change");
        assert!(may_end_advice(&temple_line(NOW)), "the temple's own area line");
        assert!(may_end_advice(&alva_start(NOW)), "an Alva voice line");
        // A line it admits is not automatically an end — that is `advice_end`'s
        // half, and the temple line is the case that separates them.
        assert_eq!(advice_end(&temple_line(NOW), READ, NOW), None);
    }

    /// The two functions are asked about every line and must not have been
    /// collapsed into one: `Good job, exile.` is an [`LineEvent::AlvaEnd`] — it
    /// starts no cycle, it ENDS the advice of the incursion that produced it,
    /// and since WI-1 it also stands the CAPTURE down. Two of those are one
    /// answer and one is the other, and a single verdict cannot say all three.
    ///
    /// The seam matters in the direction the merge would break it: the advice
    /// clear is compared against the READ's stamp ([`advice_end`]) and the
    /// stand-down is not, so folding them would make a stand-down conditional on
    /// a board having been read after the line — which is the reopen case, and
    /// exactly the one WI-1 stopped probing for.
    #[test]
    fn the_end_line_stands_the_capture_down_and_ends_the_advice() {
        let mut state = ArmState::default();
        apply_line(&mut state, &alva_start(NOW - 60_000), NOW - 60_000);
        let line = stamped(NOW, "Alva, Master Explorer: Good job, exile.");

        let transition = apply_line(&mut state, &line, NOW);

        assert_eq!(transition, Transition::Disarmed);
        assert!(!state.arm.is_armed(NOW));
        assert_eq!(advice_end(&line, READ, NOW), Some(AdviceEnd::NewIncursion));
    }
}
