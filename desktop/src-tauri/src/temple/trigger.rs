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
//! a Re-arm (one incursion in 342 carried no start line at all, so that
//! fallback is load-bearing anyway), while an unheard END variant would leave
//! the notice standing — a claim on screen that is false.
//!
//! **A non-START Alva line now stands the capture DOWN** rather than arming it
//! for two minutes. Owner, 2026-09-07: *"After leaving the incursion, OCR never
//! stops probing, and it should on any Alva voiceline, or zone change."* What
//! that gives up is the map-side reopen after `Good job.` — the player who
//! wants to see what the kill changed presses Re-arm, which is the same
//! fallback an unheard start variant already leans on. What it buys is the
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
//! arm. Such a player is not broken, only unautomated — `temple_rearm`
//! ("Re-arm") is the same fallback the merc module's **Scan now** is.
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
//!   whatever it names — is what ends it;
//! - the **panel on screen** is not in Client.txt at all: while the capture
//!   loop holds the layout panel live — from a sighting until `RETIRE_AFTER`
//!   consecutive clean misses retire it (two since 2026-09-11, POE-275) — the
//!   gate stays open ([`arm_source`]'s `panel_live`,
//!   `super::run::LoopState::live`), whatever
//!   Client.txt says. That is what lets a Re-arm read finish and retry while the
//!   player holds the sheet open past the grace;
//! - **Re-arm** arms for [`MANUAL_ARM_GRACE_MS`], the one deadline left in this
//!   module.
//!
//! Four things shut it, and each has a word for the app log
//! ([`StandDown`]): the sheet was read and then closed
//! ([`ArmState::complete_cycle`], driven by the capture loop), a non-START Alva
//! line, a `You have entered <not the temple>` line, and Re-arm's grace running
//! out.
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
//! What replaces them is a CYCLE rather than a clock: the START arm holds
//! indefinitely, the loop reads the board once, and the retire after that read
//! completes the cycle and stands the capture down
//! (`super::run::cycle_complete`). The retire is the second consecutive clean
//! miss since 2026-09-11 (POE-275, owner — `super::run::RETIRE_AFTER`); from
//! WI-1 until then it was the first. A sheet closed BEFORE a read completed does
//! not complete anything — the loop keeps probing, which is the case the tails
//! were really covering.
//!
//! The accepted cost, owner-decided 2026-09-07: reopening the sheet later in the
//! SAME incursion shows no offer boxes until Re-arm, because nothing is looking
//! any more. The room diamond is unaffected — it lives with the incursion, not
//! with the capture (POE-248).
//!
//! What POE-246 measured still holds and is what `panel_live` keeps. MEASURED on
//! the laptop, 2026-09-03, on the build POE-242 shipped:
//!
//! - 14:30:24 `capture armed by Re-arm` → 14:36:14 `layout panel found` →
//!   14:37:00 `capture stood down — waiting for Alva`, **with the layout panel
//!   still open on screen**. The overlay went with the status — `waiting` is not
//!   in the webview's `OVERLAY_VISIBLE_STATUSES` — so the advice vanished out
//!   from under a player who was reading the board it described.
//! - 17:28:31 the module was toggled off and on with the panel already open and
//!   Alva silent: `capture loop started`, then `capture stood down` in the same
//!   second. Owner: *"it blinked and disappeared"*.
//!
//! Both are one bug: nothing asked the SCREEN. The gate therefore still reads
//! whether the loop's last tick found the panel, and the trust that buys is
//! unchanged — a detector that anchored on background pixels every tick would
//! hold the gate open with nothing on screen. `anchor::NCC_FLOOR` is what that
//! rests on, and it is the same floor the read itself is believed on. What WI-1
//! changed is the clock behind it: `panel_live` is one tick (650 ms at
//! `super::run::DETECT_INTERVAL`), not two minutes. Two ticks since 2026-09-11
//! (POE-275): `panel_live` survives one held miss and goes on the second
//! consecutive one.
//!
//! # The start-up probe
//!
//! A gate that reads the screen has to look at least once. A module switched on
//! — or an app started — with the panel already open has no Client.txt event
//! coming (the 17:28:31 line above is that case), so the first detect tick a
//! loop runs is not gated on the arm at all: [`ArmSource::StartupProbe`] opens
//! the gate for exactly one tick and what that tick sees decides the rest. It
//! anchors, the panel is live and the gate stays open on it; it finds nothing,
//! the next iteration stands down.
//!
//! ONE tick per loop start, spent whatever the tick found — by
//! `super::run::LoopState::on_detect` on a tick that looked and by
//! `super::run::LoopState::on_blind_tick` on a tick whose grab failed,
//! **including a tick that could not look at all**.
//! That is deliberate: the alternative is a loop that keeps the gate open for
//! the whole session on a machine whose capture never succeeds, which is the
//! free-running capture with an error message on it. The cost is that a single
//! transient grab failure at module start costs the probe, and Re-arm is the
//! recovery. `temple_rearm` does not re-arm it and does not need
//! to: the button already arms the capture for [`MANUAL_ARM_GRACE_MS`], which is
//! a longer version of the same look. Keying the probe on that counter would be
//! worse than redundant — every settings command bumps it (see
//! the temple read gate), so a settings change would start a capture nobody
//! asked for, which is the behaviour POE-242 removed.
//!
//! Arming never SHORTENS what is already armed ([`TempleArm::arm`]). Since WI-1
//! retired the voice-line tail the only bid that could shorten anything is
//! Re-arm's sixty seconds, and the rule is what stops it trading a deadline-free
//! temple arm for that — the button is the user asking for MORE looking and must
//! not be able to buy less. Alva's temple banter no longer reaches this rule at
//! all: it does not arm, it is refused outright over a [`ArmReason::TempleArea`]
//! arm ([`apply_line`]), and over anything else it stands the capture down.
//!
//! # The scope this draws, and the override outside it
//!
//! SETTLED by owner order, 2026-09-04 (`docs/TEMPLE-LIFECYCLE.md`,
//! "Consequences that follow from the order"): a sheet opened from the hideout
//! with Alva silent is NOT in scope. The module follows incursions, and a panel
//! nobody was sent to by a voice line or by a zone is not one — so the absence
//! of an arm there is the design and not a gap in it. **Re-arm** is the manual
//! override, unchanged since POE-242, and it is the same fallback that covers a
//! start variant nobody has heard.
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
//!   covered by the arm — **Re-arm** is what covers it, by owner decision
//!   (2026-09-07, above).
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
//! recovery is the same Re-arm.
//!
//! # Not gated
//!
//! `temple_debug_capture` captures whatever the arm state says. It is an
//! explicit user action — the command a user runs *because* something else went
//! wrong — and gating it would make the diagnostic unavailable in exactly the
//! state that needs diagnosing.

use std::path::Path;

use tauri::{AppHandle, Manager};

use crate::lab_navigation;
use crate::mercenary::trigger::{arm_at, line_timestamp_ms, speaker_of, LINE_STALE_MS};
use crate::AppState;

/// Alva's speaker string, exactly as Client.txt writes it.
pub const ALVA_SPEAKER: &str = "Alva, Master Explorer";

/// The area name the temple itself enters under.
pub const TEMPLE_AREA: &str = "The Temple of Atzoatl";

/// How long a manual **Re-arm** keeps the loop armed.
///
/// The merc module's own value, and for the same reason: it is a promise to the
/// PERSON who pressed it rather than a deadline measured from an event, so it
/// has to outlive an alt-tab back into the game.
///
/// **The only deadline left in this module** since WI-1 retired `ALVA_TAIL_MS`
/// and `PANEL_TAIL_MS` (2026-09-07). It is what a manual stand-down reports
/// ([`StandDown::GraceOver`]), and it is not the only way a manual arm ends: a
/// Re-arm whose sheet is read and then closed ends by the cycle rule like any
/// other arm.
pub const MANUAL_ARM_GRACE_MS: u64 = crate::mercenary::trigger::MANUAL_ARM_GRACE_MS;

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
    /// ends this arm is an end line, an area change or the completed cycle
    /// (`super::run::cycle_complete`), never a clock.
    AlvaStart,
    /// The player is inside The Temple of Atzoatl.
    TempleArea,
    /// The user pressed Re-arm.
    Manual,
}

impl ArmReason {
    /// The words the app log uses for this reason.
    pub fn label(self) -> &'static str {
        match self {
            ArmReason::AlvaStart => "Alva's start line",
            ArmReason::TempleArea => "the temple",
            ArmReason::Manual => "Re-arm",
        }
    }
}

/// Why the capture is NOT looking — the second half of the app log's gate
/// vocabulary, added by WI-1 (2026-09-07).
///
/// Before WI-1 there was one stand-down line for every cause, because there was
/// really one cause: a clock had run out. The clocks are gone, and the four ways
/// a gate now shuts are four different things for a smoke run to check, so the
/// line names which ([`super::run::gate_line`]).
///
/// Carried on [`ArmState`] rather than derived, because three of the four are
/// EVENTS with no trace left in [`TempleArm`] afterwards — a disarmed arm looks
/// the same whichever line disarmed it. What the field holds is the cause the
/// NEXT stand-down reports, so every writer of the arm sets it: the two
/// disarming lines in [`apply_line`], [`ArmState::complete_cycle`], and
/// [`ArmState::arm_manual`], which sets [`Self::GraceOver`] up front because the
/// grace expiring is the one stand-down no event announces.
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
    /// 2026-09-07). The next START phrase — or Re-arm — begins a fresh one.
    CycleComplete,
    /// A non-START [`ALVA_SPEAKER`] line: the incursion is over, or another has
    /// begun.
    AlvaLine,
    /// `: You have entered <not the temple>` — the player left.
    LeftArea,
    /// A Re-arm nobody opened a sheet for, [`MANUAL_ARM_GRACE_MS`] later.
    GraceOver,
}

impl StandDown {
    /// The words the app log uses for this stand-down.
    pub fn label(self) -> &'static str {
        match self {
            // The wording POE-242 shipped, kept verbatim: it is what
            // `docs/OVERLAY-GUIDE.md` smoke item 12 tells the runner to look
            // for.
            StandDown::Waiting => "waiting for Alva",
            StandDown::CycleComplete => "the sheet was read and closed",
            StandDown::AlvaLine => "Alva's line",
            StandDown::LeftArea => "the zone changed",
            StandDown::GraceOver => "Re-arm's grace is over",
        }
    }
}

/// Why the capture loop is looking — [`arm_source`]'s answer, and the app log's
/// whole vocabulary for the gate.
///
/// [`ArmReason`] is the Client.txt half: the two ways a LINE can put an
/// incursion in scope, plus Re-arm. The other two answers come from the loop
/// itself and have
/// no line behind them, which is why this is a second enum rather than two more
/// variants of the first — a [`TempleArm`] can only ever hold an [`ArmReason`],
/// and a type that could also hold [`ArmSource::PanelOnScreen`] would be able to
/// claim Client.txt said something it cannot say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmSource {
    /// Client.txt put an incursion in scope.
    Trigger(ArmReason),
    /// The loop's LAST detect tick found the layout panel, or missed it once
    /// after one that did (`super::run::LoopState::live`, POE-275). Two ticks,
    /// not a tail — see the module doc's "There are no tails any more".
    PanelOnScreen,
    /// The one tick a starting loop runs before it may stand down.
    StartupProbe,
}

impl ArmSource {
    /// The words the app log uses for this source.
    pub fn label(self) -> &'static str {
        match self {
            ArmSource::Trigger(reason) => reason.label(),
            ArmSource::PanelOnScreen => "the panel on screen",
            ArmSource::StartupProbe => "the start-up probe",
        }
    }
}

/// Whether the capture loop may look at the screen at all.
///
/// The single owner of that answer, held in `AppState.temple_arm` and written
/// from exactly three places: the Client.txt watcher (every line, whether or not
/// the module is on), `temple_rearm`, and — since WI-1 — the capture loop when
/// it has read a board and watched the sheet close ([`complete_cycle`]).
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
    /// Something ended the arm: an area change, a non-START Alva line, or the
    /// completed cycle ([`ArmState::complete_cycle`]).
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
    /// without an order of precedence between them. Re-arm pressed inside a
    /// temple would swap the deadline-free arm for sixty seconds under a plain
    /// "latest wins"; the button is the user asking for MORE looking, and it
    /// cannot be allowed to buy less. Since WI-1 retired the voice-line tail
    /// (2026-09-07) Re-arm is the ONLY bid with a deadline on it, so it is also
    /// the only bid this rule can refuse.
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
/// and it is a field rather than a derivation because three of the four causes
/// are events that leave no trace in `TempleArm`: a `Disarmed` arm looks
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

impl ArmState {
    /// The user pressed Re-arm: look for [`MANUAL_ARM_GRACE_MS`].
    ///
    /// The cause is written up front, and only when the bid actually took: a
    /// Re-arm refused by the no-shortening rule ([`TempleArm::arm`]) has bought
    /// no deadline, so it must not claim the next stand-down. This is the one
    /// stand-down with no event behind it — a clock runs out and nothing is
    /// written at the moment it does — which is why it is stamped here instead.
    pub fn arm_manual(&mut self, now_ms: u64) -> Transition {
        let transition = self.arm.arm(
            ArmReason::Manual,
            Some(now_ms.saturating_add(MANUAL_ARM_GRACE_MS)),
            now_ms,
        );
        if self.arm.reason() == Some(ArmReason::Manual) {
            self.stood_down = StandDown::GraceOver;
        }
        transition
    }

    /// The capture loop read this incursion's board and has now watched the
    /// sheet close: the cycle is over, so stop looking (WI-1, owner
    /// 2026-09-07).
    ///
    /// The loop OWNS the observation — it is the only thing that can see a sheet
    /// close, and `super::run::cycle_complete` is the rule it applies — and this
    /// owns what the observation does to the gate. What re-opens the gate is the
    /// next START phrase or Re-arm, both of which arm from `Disarmed` like any
    /// other resting state.
    ///
    /// # The exception, and it is the same one the END line has
    ///
    /// A live [`ArmReason::TempleArea`] arm is NOT ended by a completed cycle.
    /// Inside The Temple of Atzoatl the sheet is the navigation aid and the
    /// player opens and closes it once per ROOM, with nothing between the rooms
    /// that moves the board key — no `EnteredTemple` bumps the epoch and the run
    /// writes no area lines — so a cycle that completed on the first close would
    /// cost a Re-arm for every remaining room of the run.
    ///
    /// That arm ends on leaving the area, as it always has. It is the same
    /// carve-out [`apply_line`] gives Alva's temple banter and it is keyed the
    /// same way, on the reason the temple's own area line ASSIGNS.
    ///
    /// # `key_current` — the observation is about a cycle that may be over
    ///
    /// The loop reads the board key `(temple_epoch, temple_rearm)` at the TOP of
    /// its tick and reaches this at the BOTTOM, and a tick takes seconds — 5.3 s
    /// for a cold sweep, ~4 s for a full read on a debug build. A START phrase
    /// heard inside that window bumps the epoch and arms a fresh cycle; Re-arm
    /// bumps the counter. Either way the arm this would disarm is no longer the
    /// arm the sheet closed under, and standing it down would leave the NEXT
    /// board unread until the player pressed Re-arm.
    ///
    /// So the caller says whether the key it observed is still the current one,
    /// and a stale observation moves nothing — [`Transition::Ignored`], the same
    /// word a line that changed nothing gets. A bool rather than the key itself
    /// keeps the rule testable without an `AppHandle`, as [`arm_source`]'s two
    /// loop inputs are; `complete_cycle` (the glue) is what reads the two
    /// counters.
    pub fn complete_cycle(&mut self, key_current: bool) -> Transition {
        if !key_current {
            return Transition::Ignored;
        }
        if self.arm.reason() == Some(ArmReason::TempleArea) {
            return Transition::Extended(ArmReason::TempleArea);
        }
        self.stood_down = StandDown::CycleComplete;
        self.arm.disarm()
    }
}

/// The capture loop's whole arm gate: may it look right now, and on whose word.
///
/// `None` is stood down, and [`ArmState::stood_down`] is the word for it. The
/// three inputs are ORed — each is its own reason to look and none can shorten
/// another — so the order below decides only which source the app log names
/// ([`super::run::gate_line`]).
///
/// Client.txt comes first because a live incursion is what the module exists to
/// follow and `armed by Alva's start line` is the line the smoke item reads. The
/// panel comes second, and is what is left of POE-246 after WI-1 retired its
/// tail: `panel_live` is `super::run::LoopState::live`, which is true from a
/// sighting until the loop retires the sheet — `super::run::RETIRE_AFTER`
/// consecutive clean misses, two since 2026-09-11 (POE-275). A player holding
/// the sheet open therefore keeps the gate open whatever Client.txt says, which
/// is what lets a Re-arm read finish and retry past the sixty-second grace; a
/// sheet that closes takes it with them on the second tick after. The probe
/// comes last because it
/// is not evidence at all — it is the one look a starting loop owes itself
/// before it may believe an empty screen.
///
/// `probe_pending` is that state machine's unspent first look. Both loop inputs
/// are plain bools here, so the whole gate is tested on Linux without a screen
/// or a clock.
pub fn arm_source(
    state: ArmState,
    panel_live: bool,
    probe_pending: bool,
    now_ms: u64,
) -> Option<ArmSource> {
    if let Some(reason) = state.arm.reason().filter(|_| state.arm.is_armed(now_ms)) {
        return Some(ArmSource::Trigger(reason));
    }
    if panel_live {
        return Some(ArmSource::PanelOnScreen);
    }
    probe_pending.then_some(ArmSource::StartupProbe)
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
/// Area changes are read with [`lab_navigation::parse_entered_area`], the app's
/// one owner of that parse — blind spot included: a player's chat line quoting
/// the sentence reads as an area change here. The cost is bounded (a wrong arm
/// captures a screen with no panel on it; a wrong disarm is undone by the next
/// real area line, or by Re-arm).
fn line_kind(line: &str) -> Option<LineKind> {
    if let Some(area) = lab_navigation::parse_entered_area(line) {
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
/// should be probing between incursions, so it now disarms outright. The reopen
/// is Re-arm's, and the module doc records the trade.
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
/// Every other arm — a START arm map-side, a Re-arm — is ended by the line.
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

// ---------------------------------------------------------------------------
// Glue — the same operations against `AppState`
// ---------------------------------------------------------------------------

/// The arm state, copied out. The loop asks once per iteration.
pub fn arm_state(app: &AppHandle) -> ArmState {
    let state = app.state::<AppState>();
    let arm = *state.temple_arm.lock().unwrap_or_else(|e| e.into_inner());
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
/// two transitions this function cannot see at all — [`MANUAL_ARM_GRACE_MS`]
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
    {
        let state = app.state::<AppState>();
        let mut arm = state.temple_arm.lock().unwrap_or_else(|e| e.into_inner());
        // Classified a second time inside, which is two string searches on the
        // handful of lines a map produces: one entry point for the arm beats a
        // second one that only the glue could reach.
        apply_line(&mut arm, line, now);
    }
    // A stale Alva line is the remaining `None`: it says nothing about the
    // screen now, so it moves neither the cycle nor the epoch nor the advice.
    let Some(event) = event else {
        return;
    };
    if ends_epoch(event) {
        // INVALIDATE, never force. See `AppState::temple_epoch`.
        app.state::<AppState>()
            .temple_epoch
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

/// Re-arm, from the button.
pub fn arm_manual(app: &AppHandle) {
    let now = super::run::now_ms();
    let state = app.state::<AppState>();
    let mut arm = state.temple_arm.lock().unwrap_or_else(|e| e.into_inner());
    arm.arm_manual(now);
}

/// The cycle is over: the capture loop read this incursion's board and then
/// watched the sheet close (WI-1).
///
/// The loop's own seam into the arm, and the sibling of [`arm_manual`] — both
/// are one lock and one call, with the RULE next door: `super::run::cycle_complete`
/// decides, [`ArmState::complete_cycle`] writes.
///
/// `key` is the board key the observation was taken under, read at the top of
/// the tick that is now reporting the sheet closed. This re-reads the pair
/// through `super::run::board_key` — the same two counters, in the same order,
/// under the atomics the loop itself uses — and hands
/// [`ArmState::complete_cycle`] the comparison, so a cycle that a START line or
/// a Re-arm has already replaced during the tick is not stood down on the
/// strength of the cycle before it. See that method for why a whole tick is
/// long enough for that to happen.
///
/// The counters are read BEFORE the arm lock is taken: the window this closes is
/// the tick's own seconds, and holding the lock across the reads would narrow
/// nothing (they are atomics, and `on_client_line` bumps the epoch after it has
/// released this lock).
///
/// It logs nothing, for the same reason [`on_client_line`] does not: the
/// stand-down line has one owner, and it is `super::run::gate_line` on the next
/// iteration of the loop that called this.
pub fn complete_cycle(app: &AppHandle, key: (u64, u64)) {
    let key_current = super::run::board_key(app) == key;
    let state = app.state::<AppState>();
    let mut arm = state.temple_arm.lock().unwrap_or_else(|e| e.into_inner());
    arm.complete_cycle(key_current);
}

/// Seed the arm state from the log the watcher is about to tail.
///
/// Called once per watcher start, beside the lab catch-up and over the same
/// 32 KB tail. See [`catch_up_state`] for why this exists at all.
pub fn catch_up(app: &AppHandle, client_txt: &Path) {
    let Some(tail) = lab_navigation::recent_log_tail(client_txt) else {
        return;
    };
    let seeded = {
        let state = app.state::<AppState>();
        let mut arm = state.temple_arm.lock().unwrap_or_else(|e| e.into_inner());
        apply_catch_up(&mut arm, &tail)
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
    /// between incursions, and Re-arm is what covers the reopen. Fails the
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
    /// perfectly fresh lines and every incursion needs a Re-arm.
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

    /// The stand-down is keyed on the REASON, and the temple is its ONLY
    /// exception: a live Re-arm is ended by an end line like anything else.
    ///
    /// The case is real — Re-arm pressed on a board that read wrong, then
    /// `Good job.` as the architect dies — and it is the one that separates
    /// "only a `TempleArea` arm survives" from "any live arm survives". Fails if
    /// the exception is widened to every live arm, which would leave a Re-arm
    /// probing for its whole grace after the incursion closed.
    #[test]
    fn an_end_line_stands_a_manual_arm_down() {
        let mut state = ArmState::default();
        state.arm_manual(NOW);

        let transition = apply_line(&mut state, &alva_end(NOW + 5_000), NOW + 5_000);

        assert_eq!(transition, Transition::Disarmed);
        assert!(!state.arm.is_armed(NOW + 5_000));
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

    /// An area change outranks a Re-arm the user pressed: the board they wanted
    /// re-read is not on this screen. Fails if the disarm is conditional on the
    /// reason, which would leave the grace probing across the next zone.
    #[test]
    fn an_area_change_disarms_a_manual_arm() {
        let mut state = ArmState::default();
        state.arm_manual(NOW);

        let transition = apply_line(&mut state, &map_line(NOW + 5_000), NOW + 5_000);

        assert_eq!(transition, Transition::Disarmed);
        assert!(!state.arm.is_armed(NOW + 5_000));
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

    // ------------------------------------------------------ manual re-arm --

    /// Re-arm is the fallback for every case the log does not cover (the
    /// hideout panel, a non-English client). Fails if the grace is not applied.
    #[test]
    fn a_manual_rearm_arms_for_the_grace_window() {
        let mut state = ArmState::default();

        state.arm_manual(NOW);

        assert!(state.arm.is_armed(NOW + MANUAL_ARM_GRACE_MS - 1));
        assert!(!state.arm.is_armed(NOW + MANUAL_ARM_GRACE_MS));
    }

    /// Re-arm inside a temple must not trade the deadline-free arm for sixty
    /// seconds. Fails if `arm_manual` writes unconditionally: pressing Re-arm
    /// on a board that read wrong would blind the module a minute later.
    #[test]
    fn a_manual_rearm_does_not_shorten_a_temple_arm() {
        let mut state = ArmState::default();
        apply_line(&mut state, &temple_line(NOW), NOW);

        state.arm_manual(NOW + 1_000);

        assert!(state.arm.is_armed(NOW + MANUAL_ARM_GRACE_MS + 10_000));
    }

    /// The grace is the one stand-down with no event behind it, so Re-arm
    /// claims it at the moment it arms.
    ///
    /// Fails if the cause is left for the expiry to write — nothing runs at an
    /// expiry, so the line would report whatever last closed the gate, which on
    /// a fresh session is `waiting for Alva` and after an incursion is
    /// `Alva's line`.
    #[test]
    fn a_manual_rearm_claims_the_stand_down_it_will_end_in() {
        let mut state = ArmState::default();

        state.arm_manual(NOW);

        assert_eq!(state.stood_down, StandDown::GraceOver);
    }

    /// …and a Re-arm the no-shortening rule REFUSED has bought no deadline, so
    /// it must not claim the stand-down either.
    ///
    /// The temple arm it lost to ends on the area line out, which writes
    /// `the zone changed`. Fails if the cause is written unconditionally: the
    /// log would blame Re-arm's grace for a stand-down sixty seconds of grace
    /// had nothing to do with.
    #[test]
    fn a_manual_rearm_refused_by_a_temple_arm_does_not_claim_the_stand_down() {
        let mut state = ArmState::default();
        apply_line(&mut state, &temple_line(NOW), NOW);

        state.arm_manual(NOW + 1_000);

        assert_eq!(state.stood_down, StandDown::Waiting, "nothing has shut this gate yet");
        apply_line(&mut state, &map_line(NOW + 2_000), NOW + 2_000);
        assert_eq!(state.stood_down, StandDown::LeftArea);
    }

    // ------------------------------------------------ the completed cycle --

    /// WI-1's own state (owner, 2026-09-07): the loop read the sheet and then
    /// watched it close, so the arm goes down and the log has a word for why.
    ///
    /// Fails if `complete_cycle` leaves the arm alone — the loop would go on
    /// probing for the rest of the incursion, which is the report this work item
    /// answers.
    #[test]
    fn a_completed_cycle_stands_a_start_arm_down() {
        let mut state = ArmState::default();
        apply_line(&mut state, &alva_start(NOW), NOW);

        let transition = state.complete_cycle(true);

        assert_eq!(transition, Transition::Disarmed);
        assert!(!state.arm.is_armed(NOW + 1_000));
        assert_eq!(state.stood_down, StandDown::CycleComplete);
    }

    /// A completion whose board key has moved on stands NOTHING down: the tick
    /// that observed the sheet close read its key seconds ago, and a START
    /// phrase or a Re-arm inside that window armed a different cycle.
    ///
    /// The arm here is the FRESH one — the sheet the observation is about closed
    /// under the previous key — so disarming it would leave the next incursion's
    /// board unread until the player pressed Re-arm, with nothing on screen to
    /// suggest why.
    ///
    /// Fails if the guard is dropped, or inverted so that only a stale key
    /// completes.
    #[test]
    fn a_completed_cycle_whose_key_has_moved_on_stands_nothing_down() {
        let mut state = ArmState::default();
        apply_line(&mut state, &alva_start(NOW), NOW);

        let transition = state.complete_cycle(false);

        assert_eq!(transition, Transition::Ignored);
        assert!(state.arm.is_armed(NOW + 1_000), "the fresh cycle is still armed");
        assert_eq!(
            state.stood_down,
            StandDown::Waiting,
            "and the log has not been handed a cause that did not happen",
        );
    }

    /// And the next incursion is a fresh one. The completion is a resting state,
    /// not a latch: a START phrase arms out of it exactly as it does out of
    /// `Waiting`.
    ///
    /// Fails if completion is spelled as anything a later arm has to clear
    /// explicitly — the module would read one incursion per session and then go
    /// quiet, with Re-arm the only way back.
    #[test]
    fn a_start_phrase_after_a_completed_cycle_arms_a_fresh_one() {
        let mut state = ArmState::default();
        apply_line(&mut state, &alva_start(NOW), NOW);
        state.complete_cycle(true);

        let transition = apply_line(&mut state, &alva_start(NOW + 300_000), NOW + 300_000);

        assert_eq!(transition, Transition::Armed(ArmReason::AlvaStart));
        assert!(state.arm.is_armed(NOW + 300_000));
    }

    /// The Temple of Atzoatl run is the exception, and it is the same one the
    /// END line has: the sheet is the navigation aid in there, opened and closed
    /// once per ROOM, with nothing between the rooms that moves the board key.
    ///
    /// Fails if the completion is applied to every arm: the first room's close
    /// would stand the capture down and every remaining room of the run would
    /// need a Re-arm to get a board — which is the surface the run is played on.
    #[test]
    fn a_completed_cycle_does_not_stand_a_temple_arm_down() {
        let mut state = ArmState::default();
        apply_line(&mut state, &alva_start(NOW), NOW);
        apply_line(&mut state, &temple_line(NOW + 1_000), NOW + 1_000);

        let transition = state.complete_cycle(true);

        assert_eq!(transition, Transition::Extended(ArmReason::TempleArea));
        assert!(state.arm.is_armed(NOW + 3_600_000), "an hour into the run, still armed");
        assert_eq!(state.stood_down, StandDown::Waiting, "and nothing has shut this gate");
    }

    /// …and the area line OUT is what does end it, cycle or no cycle. Fails if
    /// the exception above is written as a latch rather than as a property of
    /// the live arm — the temple would then hold the gate open into the next
    /// map.
    #[test]
    fn leaving_the_temple_ends_the_arm_a_completed_cycle_spared() {
        let mut state = ArmState::default();
        apply_line(&mut state, &temple_line(NOW), NOW);
        state.complete_cycle(true);

        let transition = apply_line(&mut state, &map_line(NOW + 60_000), NOW + 60_000);

        assert_eq!(transition, Transition::Disarmed);
        assert_eq!(state.stood_down, StandDown::LeftArea);
    }

    /// Re-arm is the other way back, and it is the one a player reaches for when
    /// they want the sheet read again inside the SAME incursion — the reopen the
    /// owner accepted the cost of.
    ///
    /// Fails for the same mutation as the test above, through the surface the
    /// user can actually press.
    #[test]
    fn a_rearm_after_a_completed_cycle_arms_again() {
        let mut state = ArmState::default();
        apply_line(&mut state, &alva_start(NOW), NOW);
        state.complete_cycle(true);

        state.arm_manual(NOW + 5_000);

        assert!(state.arm.is_armed(NOW + 5_000));
    }

    // --------------------------------------------- the panel on screen --

    /// The 2026-09-03 bug, as the gate answers it: Client.txt has nothing in
    /// scope and the sheet is still on screen, so the loop keeps looking.
    ///
    /// Fails if the gate reads [`TempleArm`] alone — which is what stood the
    /// capture down at 14:37:00 over a layout panel the player was reading, and
    /// took the overlay with it.
    #[test]
    fn a_live_panel_keeps_the_loop_armed_with_nothing_in_client_txt() {
        let state = ArmState::default();
        assert!(!state.arm.is_armed(NOW), "Client.txt has nothing to say");

        assert_eq!(
            arm_source(state, true, false, NOW),
            Some(ArmSource::PanelOnScreen),
        );
    }

    /// The case rule 5 of WI-1 names: a Re-arm's sixty seconds have run out and
    /// the player still has the sheet open, so the read that Re-arm bought may
    /// finish and retry.
    ///
    /// Fails if the panel input is dropped or is ANDed with the arm instead of
    /// ORed with it — pressing Re-arm and reading a slow board would then go
    /// blind at the minute mark with the sheet in front of the player.
    #[test]
    fn a_live_panel_keeps_a_manual_arm_alive_past_its_grace() {
        let mut state = ArmState::default();
        state.arm_manual(NOW);
        let expired = NOW + MANUAL_ARM_GRACE_MS + 1;
        assert!(!state.arm.is_armed(expired), "the grace has nothing left");

        assert_eq!(
            arm_source(state, true, false, expired),
            Some(ArmSource::PanelOnScreen),
        );
    }

    /// POE-242's goal, and WI-1's: nothing in the log and nothing on screen
    /// means the loop stops looking.
    ///
    /// Fails if the panel branch is a constant, or reads "was ever seen" rather
    /// than "the last tick saw it" — either is the free-running capture POE-242
    /// removed.
    #[test]
    fn a_panel_that_is_no_longer_live_stands_the_loop_down() {
        let mut state = ArmState::default();
        apply_line(&mut state, &alva_start(NOW), NOW);
        apply_line(&mut state, &alva_end(NOW + 30_000), NOW + 30_000);

        assert_eq!(arm_source(state, false, false, NOW + 30_001), None);
    }

    /// A live Client.txt arm is what the log names, even with the panel on
    /// screen. Fails if the panel branch is tried first — `capture armed by
    /// Alva's start line` is the line smoke item 12 reads, and it would become
    /// `by the panel on screen` for every incursion the player has a panel open
    /// during.
    #[test]
    fn a_live_client_txt_arm_is_the_source_the_log_names() {
        let mut state = ArmState::default();
        apply_line(&mut state, &alva_start(NOW), NOW);

        assert_eq!(
            arm_source(state, true, false, NOW + 1_000),
            Some(ArmSource::Trigger(ArmReason::AlvaStart)),
        );
    }

    /// Walking out ends CLIENT.TXT's claim on the gate, and after that only a
    /// sheet the loop's LAST tick saw can hold it open — which is the accepted
    /// one-tick residual of WI-1 retiring `left_area_ms`.
    ///
    /// The tick that carries it is the tick that finds the new zone empty, so
    /// the gate is shut 650 ms later. Fails if the area line stops disarming, in
    /// which case the map carries the whole arm rather than one tick of it.
    /// (two ticks since 2026-09-11, POE-275: `live` survives one held miss)
    #[test]
    fn an_area_change_leaves_only_a_live_panel_holding_the_gate() {
        let mut state = ArmState::default();
        apply_line(&mut state, &alva_start(NOW), NOW);

        apply_line(&mut state, &map_line(NOW + 5_000), NOW + 5_000);

        assert_eq!(
            arm_source(state, false, false, NOW + 5_000),
            None,
            "the zone change stands the loop down",
        );
        assert_eq!(
            arm_source(state, true, false, NOW + 5_000),
            Some(ArmSource::PanelOnScreen),
            "and a sheet still live holds the gate until the retire",
        );
    }

    // -------------------------------------------------- the start-up probe --

    /// The 17:28:31 case: the module is switched on with the panel already open
    /// and Alva silent, so nothing in the log will ever arm it. The loop owes
    /// itself one look before it may believe the screen is empty.
    ///
    /// Fails if the probe is dropped from the gate — the module then stands down
    /// in the same second it started, which is what "it blinked and
    /// disappeared" was.
    #[test]
    fn a_starting_loop_owes_itself_one_look_before_it_may_stand_down() {
        assert_eq!(
            arm_source(ArmState::default(), false, true, NOW),
            Some(ArmSource::StartupProbe),
        );
    }

    /// And the look is ONE. A probe that has been spent over a screen with no
    /// panel on it leaves the loop stood down. Fails if the probe is a constant
    /// rather than a debt the tick settles — the gate would then never close,
    /// which is the free-running capture with extra steps.
    #[test]
    fn a_spent_probe_that_saw_nothing_leaves_the_loop_stood_down() {
        assert_eq!(arm_source(ArmState::default(), false, false, NOW), None);
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
    /// standing, and "unknown" must not be guessed into "the temple". Re-arm is
    /// the recovery. Fails if the `None` arm of the match is folded in with the
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
