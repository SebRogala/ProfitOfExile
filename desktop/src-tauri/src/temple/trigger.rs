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
//! # Trigger — the lines this reads (MEASURED)
//!
//! Read off Sebastian's PC Client.txt on 2026-09-02, over the **3 incursions**
//! that file still held (2026-07 and 2026-08). Three samples is a small sample:
//! the table is provenance for the design, not the matcher.
//!
//! | moment | line |
//! |---|---|
//! | incursion start | `Alva, Master Explorer: It's time!` (×1), `… : Time to go.` (×2) |
//! | incursion end | `Alva, Master Explorer: Good job.` (×1), `… : Good job, exile.` (×2) |
//! | temple banter | `… : No wonder it's lost…`, `… : At last... Atzoatl.` |
//! | temple entry | `Generating level N area "Incursion_Temple8"`, then `: You have entered The Temple of Atzoatl.` |
//!
//! # Why a speaker match and not the phrases
//!
//! Six phrases out of three incursions is not the vocabulary — it is what three
//! incursions happened to say. A phrase list would silently miss every variant
//! nobody has heard yet, and a missed start line costs the whole capture. So
//! the match is SPEAKER-shaped, the same shape `mercenary::trigger` uses:
//! any line whose speaker is exactly [`ALVA_SPEAKER`] arms.
//!
//! That is deliberately wider than "the incursion is starting", and the END
//! lines are the case it widens onto: after an incursion closes the player is
//! still map-side and may open the layout panel to read what the kill changed.
//! Arming on `Good job.` is therefore WANTED, not the free-running bug coming
//! back — the window it buys is bounded by [`ALVA_TAIL_MS`], and the first
//! `You have entered` line after it disarms whatever the tail had left.
//!
//! It is an ENGLISH match: a client running in another language writes Alva's
//! name and title in that language and no voice line on that machine will ever
//! arm. Such a player is not broken, only unautomated — `temple_rearm`
//! ("Re-arm") is the same fallback the merc module's **Scan now** is.
//!
//! # The three clocks
//!
//! - a **voice line** is evidence about the screen at the moment it was spoken,
//!   so it arms for [`ALVA_TAIL_MS`] measured from the LINE's own stamp
//!   ([`crate::mercenary::trigger::line_timestamp_ms`]), and a line older than
//!   [`LINE_STALE_MS`] on arrival is not evidence about now at all;
//! - an **area** is a state, not a burst: `: You have entered The Temple of
//!   Atzoatl.` arms with NO deadline, and the next `You have entered` line —
//!   whatever it names — is what ends it;
//! - the **panel on screen** is not in Client.txt at all: while the capture
//!   loop's detect tick keeps finding the layout panel the gate stays open, and
//!   [`PANEL_TAIL_MS`] runs from the tick that saw it LAST (POE-246,
//!   [`arm_source`]).
//!
//! # The clock measures absence, not presence (POE-246)
//!
//! MEASURED on the laptop, 2026-09-03, on the build POE-242 shipped:
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
//! Both are one bug. The two clocks above measure how long ago something was
//! SAID; nothing asked the screen. A tick that had just anchored the panel did
//! not extend the arm, and a loop that started with the panel already open never
//! armed at all.
//!
//! So the gate reads a third input — when the loop last SAW the panel — and the
//! deadline restarts from every sighting. Stand-down now means "the panel has
//! been gone for [`PANEL_TAIL_MS`]", which is what POE-242 was reaching for all
//! along: its goal was no free-running capture while nothing is on screen, and a
//! panel that IS on screen was never the case it meant to bound.
//!
//! Two costs, both accepted. Up to [`PANEL_TAIL_MS`] of cheap detect ticks after
//! a panel closes — the same price the voice-line tail already pays, and cut
//! short by the next area change, which is positive evidence that the screen the
//! sighting described is gone ([`ArmState`]). And one
//! trust the module did not need before: the gate is now bounded by the anchor's
//! own honesty, so a detector that anchored on background pixels every tick
//! would hold it open with nothing on screen. `anchor::NCC_FLOOR` is what that
//! rests on, and it is the same floor the read itself is believed on.
//!
//! # The start-up probe
//!
//! A gate that reads the screen has to look at least once. A module switched on
//! — or an app started — with the panel already open has no Client.txt event
//! coming (the 17:28:31 line above is that case), so the first detect tick a
//! loop runs is not gated on the arm at all: [`ArmSource::StartupProbe`] opens
//! the gate for exactly one tick and what that tick sees decides the rest. It
//! anchors, the sighting is stamped and the gate stays open on the panel; it
//! finds nothing, the next iteration stands down.
//!
//! ONE tick per loop start, spent by `super::run::LoopState::on_detect`
//! whatever the tick found — **including a tick that could not look at all**,
//! because `super::run::miss` folds a failed screen grab through the same call.
//! That is deliberate: the alternative is a loop that keeps the gate open for
//! the whole session on a machine whose capture never succeeds, which is the
//! free-running capture with an error message on it. The cost is that a single
//! transient grab failure at module start costs the probe, and Re-arm is the
//! recovery. `temple_rearm` does not re-arm it and does not need
//! to: the button already arms the capture for [`MANUAL_ARM_GRACE_MS`], which is
//! a longer version of the same look. Keying the probe on that counter would be
//! worse than redundant — every settings command bumps it (see
//! `super::run::wants_full_read`), so a settings change would start a capture
//! nobody asked for, which is the behaviour POE-242 removed.
//!
//! Arming never SHORTENS what is already armed ([`TempleArm::arm`]). That rule
//! is what keeps Alva's own temple banter (`At last... Atzoatl.`, spoken
//! seconds after the area line) from replacing the deadline-free temple arm
//! with a two-minute one, which would blind the module part-way through a
//! temple that took longer than that to run.
//!
//! # The premise this rests on, and the smoke item that tests it
//!
//! UNVERIFIED: that the layout panel is only ever opened with an Alva line or
//! the temple area in scope. Alva stands in the hideout and her panel may open
//! there with no voice line at all. Manual Re-arm is the fallback if so, and
//! smoke item 12 is the measurement: *open the layout panel from Alva in the
//! hideout — does the module read, or does it need Re-arm?* Its answer decides
//! whether a hideout arm is needed.
//!
//! ## What the log settles, and what it cannot (2026-09-02)
//!
//! OBSERVED, in the PC's Client.txt over the two incursions of 2026-08-07: no
//! `You have entered` line is written between Alva's start line and her
//! `Good job` line. The incursion instance logs NO area change at all, in
//! either direction. Two consequences, both load-bearing:
//!
//! - an arm bought by `It's time!` is never disarmed by the incursion, and
//!   survives the return to the map; `Good job…` then extends it by a fresh
//!   [`ALVA_TAIL_MS`]. **The post-incursion panel read is covered** — the
//!   player who walks out of the incursion and opens the layout panel to see
//!   what the kill changed is inside a live arm.
//! - the ordinary end of a voice-line arm is therefore [`ALVA_TAIL_MS`]
//!   expiring, not the next area line. The area line is the end only for a
//!   temple run, where there is one.
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

/// How long one Alva voice line keeps the loop armed when no area change ever
/// arrives.
///
/// Measured 2026-09-02, and it turns out to be the ORDINARY end of a
/// voice-line arm rather than the backstop it was written as: the incursion
/// instance logs no area change in either direction (see the module doc), so
/// the player who hears `Time to go.`, runs the incursion and comes back to the
/// map is inside one continuous arm that `Good job…` extends and nothing
/// disarms. The area line ends a voice-line arm only when there is one — a
/// temple run, or the next map.
///
/// **A GUESS, tunable** (owner decision 5 in the POE-223 follow-up plan): two
/// minutes is "long enough to open a panel, read it and think", and its cost if
/// wrong is bounded in both directions — too short loses a late panel open
/// (Re-arm recovers it), too long spends detect ticks on a map, which is what
/// the module did on every tick before this file existed.
pub const ALVA_TAIL_MS: u64 = 120_000;

/// How long a manual **Re-arm** keeps the loop armed.
///
/// The merc module's own value, and for the same reason: it is a promise to the
/// PERSON who pressed it rather than a deadline measured from an event, so it
/// has to outlive an alt-tab back into the game.
pub const MANUAL_ARM_GRACE_MS: u64 = crate::mercenary::trigger::MANUAL_ARM_GRACE_MS;

/// How long the loop stays armed after the last detect tick that SAW the layout
/// panel (POE-246).
///
/// The same 120 s as [`ALVA_TAIL_MS`], and deliberately the same NUMBER: no
/// measurement separates them, and both answer one question — how long after the
/// last evidence is it still worth looking? What the pair has to cover is the
/// player who closes the panel to kill an architect and reopens it to see what
/// changed, and neither the close nor the reopen writes a line anywhere. A
/// shorter panel tail stands the loop down inside that gap; a longer one spends
/// cheap detect ticks on a screen with nothing on it. Split the two constants
/// the moment either gets a measurement of its own.
pub const PANEL_TAIL_MS: u64 = ALVA_TAIL_MS;

// ---------------------------------------------------------------------------
// Pure — the state machine
// ---------------------------------------------------------------------------

/// Why the loop is armed. Carried for the log line and for nothing else — the
/// loop asks [`TempleArm::is_armed`], never the reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmReason {
    /// A `Alva, Master Explorer:` voice line, map-side.
    AlvaLine,
    /// The player is inside The Temple of Atzoatl.
    TempleArea,
    /// The user pressed Re-arm.
    Manual,
}

impl ArmReason {
    /// The words the app log uses for this reason.
    pub fn label(self) -> &'static str {
        match self {
            ArmReason::AlvaLine => "Alva",
            ArmReason::TempleArea => "the temple",
            ArmReason::Manual => "Re-arm",
        }
    }
}

/// Why the capture loop is looking — [`arm_source`]'s answer, and the app log's
/// whole vocabulary for the gate.
///
/// [`ArmReason`] is the Client.txt half: the three ways a LINE can put an
/// incursion in scope. The other two answers come from the loop itself and have
/// no line behind them, which is why this is a second enum rather than two more
/// variants of the first — a [`TempleArm`] can only ever hold an [`ArmReason`],
/// and a type that could also hold [`ArmSource::PanelOnScreen`] would be able to
/// claim Client.txt said something it cannot say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmSource {
    /// Client.txt put an incursion in scope.
    Trigger(ArmReason),
    /// A detect tick saw the layout panel less than [`PANEL_TAIL_MS`] ago.
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
/// from exactly two places: the Client.txt watcher (every line, whether or not
/// the module is on) and `temple_rearm`.
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
    /// A live arm was pushed out, or was already reaching further than this
    /// line would have bought. Silent: Alva speaks several times per incursion.
    Extended(ArmReason),
    /// An area change ended the arm.
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
    /// without an order of precedence between them. Alva's temple banter lands
    /// seconds after the temple area line and a plain "latest wins" would swap
    /// the deadline-free arm for a two-minute one; Re-arm pressed inside a
    /// temple would do the same with sixty seconds. Both are the user asking
    /// for MORE looking, and neither can be allowed to buy less.
    ///
    /// An EXPIRED arm is not a live one: it is replaced, and reported as a
    /// fresh arm, because that is what the log reader sees.
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

    /// The user pressed Re-arm: look for [`MANUAL_ARM_GRACE_MS`].
    pub fn arm_manual(&mut self, now_ms: u64) -> Transition {
        self.arm(
            ArmReason::Manual,
            Some(now_ms.saturating_add(MANUAL_ARM_GRACE_MS)),
            now_ms,
        )
    }

    /// The area changed to something that is not the temple: stop looking,
    /// whatever armed it.
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

/// Everything one lock holds about the gate: what Client.txt has armed, and when
/// the player was last seen LEAVING for somewhere that is not the temple.
///
/// The second field is not a second gate — [`TempleArm`] is still the only thing
/// a line arms — it is what stops POE-246's panel clock outliving the screen it
/// was measured on. `: You have entered <a map>` is positive evidence that
/// whatever panel the loop last anchored went with the zone, so a sighting older
/// than the change says nothing about the screen in front of the player now.
/// Without it, walking out of a temple carried up to [`PANEL_TAIL_MS`] of 1 Hz
/// capture into the next zone, under a log line claiming the panel was on
/// screen.
///
/// Stamped on EVERY non-temple area line rather than only on the ones that move
/// the gate: the case that needs it is a loop the PANEL is keeping armed, whose
/// `TempleArm` has usually expired already, and [`TempleArm::disarm`] reports
/// `Ignored` rather than `Disarmed` for those.
///
/// # The clock this one is on, and why it is the exception
///
/// The three clocks above measure an Alva line from the LINE's own stamp. This
/// one is `now_ms` at the moment the line was READ, and it has to be: its only
/// use is a comparison against `super::run::LoopState::panel_seen_ms`, which the
/// capture loop stamps off the same wall clock, and two values compared against
/// each other must be on one clock or the comparison means nothing. The line's
/// own stamp would also be a different RESOLUTION — Client.txt writes whole
/// seconds — which is a second reason not to mix them.
///
/// The watcher's lag makes this later than the zone change really was, never
/// earlier, and later is the safe direction: it can only invalidate a sighting
/// that was in fact fine (the loop stands down and Re-arm recovers it), never
/// keep one taken on a screen the player has already left.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ArmState {
    /// What Client.txt has armed.
    pub arm: TempleArm,
    /// When the newest `You have entered <not the temple>` line was READ — the
    /// capture loop's wall clock, not the line's own stamp. `None` until one is.
    pub left_area_ms: Option<u64>,
}

/// The capture loop's whole arm gate: may it look right now, and on whose word.
///
/// `None` is stood down. The three inputs are ORed — each is its own reason to
/// look and none can shorten another — so the order below decides only which
/// source the app log names ([`super::run::gate_line`]).
///
/// Client.txt comes first because a live incursion is what the module exists to
/// follow and `armed by Alva` is the line the smoke item reads. The panel comes
/// second, and is the whole of POE-246: it is checked against a deadline
/// measured from the last SIGHTING, so a tick that anchors restarts it and the
/// gate closes only once the panel has been gone for [`PANEL_TAIL_MS`]. The
/// probe comes last because it is not evidence at all — it is the one look a
/// starting loop owes itself before it may believe an empty screen.
///
/// `panel_seen_ms` is `super::run::LoopState::panel_seen_ms`, stamped by an
/// anchored tick and never by a miss; `probe_pending` is that state machine's
/// unspent first look. Both are plain data here, so the whole gate is tested on
/// Linux without a screen or a clock.
///
/// A sighting has to be NEWER than [`ArmState::left_area_ms`] to count. The
/// panel clock measures a screen, and an area change is the one thing in the log
/// that says the screen is gone.
pub fn arm_source(
    state: ArmState,
    panel_seen_ms: Option<u64>,
    probe_pending: bool,
    now_ms: u64,
) -> Option<ArmSource> {
    if let Some(reason) = state.arm.reason().filter(|_| state.arm.is_armed(now_ms)) {
        return Some(ArmSource::Trigger(reason));
    }
    let seen_here = panel_seen_ms.filter(|seen| state.left_area_ms.is_none_or(|left| *seen > left));
    if seen_here.is_some_and(|seen| now_ms < seen.saturating_add(PANEL_TAIL_MS)) {
        return Some(ArmSource::PanelOnScreen);
    }
    probe_pending.then_some(ArmSource::StartupProbe)
}

/// One Client.txt line, folded into the arm state.
///
/// Runs on EVERY line the watcher reads, so the order is the cost order: the
/// area parse is one `str::find`, and the timestamp is parsed only for a line
/// that is already known to be Alva's.
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
pub fn apply_line(state: &mut ArmState, line: &str, now_ms: u64) -> Transition {
    if let Some(area) = lab_navigation::parse_entered_area(line) {
        return if area == TEMPLE_AREA {
            state.arm.arm(ArmReason::TempleArea, None, now_ms)
        } else {
            // Before the disarm, and unconditionally: this stamp is about the
            // SCREEN (see [`ArmState`]) and the gate may already be closed.
            state.left_area_ms = Some(now_ms);
            state.arm.disarm()
        };
    }
    if speaker_of(line) != Some(ALVA_SPEAKER) {
        return Transition::Ignored;
    }
    let stamp = line_timestamp_ms(line);
    // Read off the RAW stamp, not off `arm_at`'s answer: `arm_at` clamps a
    // stamp further than `MAX_BACKDATE_MS` back to `now`, so asking it would
    // launder every stale line into a fresh one. A line this old reached us
    // through a log the watcher was not tailing (a path change, a restart) —
    // it is evidence about a screen that is minutes gone.
    if stamp.is_some_and(|ms| now_ms.saturating_sub(ms) >= LINE_STALE_MS) {
        return Transition::Ignored;
    }
    let at = arm_at(now_ms, stamp);
    state.arm.arm(
        ArmReason::AlvaLine,
        Some(at.saturating_add(ALVA_TAIL_MS)),
        now_ms,
    )
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
/// It writes the ARM only. [`ArmState::left_area_ms`] is a claim about the
/// player's screen, and a catch-up runs when the watcher restarts — including
/// when the user edits the Client.txt path with the layout panel open in front
/// of them. Stamping "left the area" there would stand the capture down over a
/// panel nobody moved away from.
pub fn apply_catch_up(state: &mut ArmState, tail: &str) -> TempleArm {
    state.arm = catch_up_state(tail);
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
/// **Writes state and logs nothing.** The arm/disarm app-log line has one
/// owner, and it is the capture loop's `run::gate_line`, for three reasons:
/// this function runs whether or not the temple module is on (logging here
/// would narrate a module the user has switched off); when the module IS on,
/// both would fire within a second and put two lines in `app.log` for one
/// event; and the loop covers the transition this function cannot see at all —
/// an [`ALVA_TAIL_MS`] arm expiring, which no Client.txt line announces.
pub fn on_client_line(app: &AppHandle, line: &str) {
    let now = super::run::now_ms();
    let state = app.state::<AppState>();
    let mut arm = state.temple_arm.lock().unwrap_or_else(|e| e.into_inner());
    apply_line(&mut arm, line, now);
}

/// Re-arm, from the button.
pub fn arm_manual(app: &AppHandle) {
    let now = super::run::now_ms();
    let state = app.state::<AppState>();
    let mut arm = state.temple_arm.lock().unwrap_or_else(|e| e.into_inner());
    arm.arm.arm_manual(now);
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

    /// One of the two measured incursion-start lines.
    fn alva_line(at_ms: u64) -> String {
        stamped(at_ms, "Alva, Master Explorer: Time to go.")
    }

    /// The area line for a map — anything that is not the temple.
    fn map_line(at_ms: u64) -> String {
        stamped(at_ms, ": You have entered Ancient City.")
    }

    /// The temple's own area line, exactly as measured.
    fn temple_line(at_ms: u64) -> String {
        stamped(at_ms, ": You have entered The Temple of Atzoatl.")
    }

    // ------------------------------------------------------- voice lines --

    /// The trigger's whole reason for existing: until Alva speaks, the loop
    /// must not look. Fails if the speaker match is wrong or absent.
    #[test]
    fn an_alva_voice_line_arms_the_loop() {
        let mut state = ArmState::default();

        let transition = apply_line(&mut state, &alva_line(NOW), NOW);

        assert_eq!(transition, Transition::Armed(ArmReason::AlvaLine));
        assert!(state.arm.is_armed(NOW));
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

    /// A second line pushes the tail out from ITSELF, not from the first line.
    /// Fails if a line over a live arm is dropped — the window would then end
    /// two minutes after the player was first spoken to, part-way through an
    /// incursion Alva is still narrating.
    #[test]
    fn a_second_alva_line_pushes_the_arm_out() {
        let mut state = ArmState::default();
        let later = NOW + 30_000;
        apply_line(&mut state, &alva_line(NOW), NOW);

        let transition = apply_line(&mut state, &alva_line(later), later);

        assert_eq!(transition, Transition::Extended(ArmReason::AlvaLine));
        assert_eq!(
            state.arm,
            TempleArm::Armed {
                until_ms: Some(later + ALVA_TAIL_MS),
                reason: ArmReason::AlvaLine,
            },
        );
    }

    /// The tail is the backstop for a run with no area change, and it does end.
    /// Fails if the deadline is dropped (the arm would never expire) or applied
    /// to the wrong clock.
    #[test]
    fn the_alva_tail_expires_when_no_area_change_arrives() {
        let mut state = ArmState::default();
        apply_line(&mut state, &alva_line(NOW), NOW);

        assert!(state.arm.is_armed(NOW + ALVA_TAIL_MS - 1), "one ms inside the tail");
        assert!(!state.arm.is_armed(NOW + ALVA_TAIL_MS), "the tail is exclusive");
    }

    /// A line the watcher only reached a minute late — a path change, a restart
    /// over an old log — says nothing about the screen now.
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

        let transition = apply_line(&mut state, &alva_line(spoken), NOW);

        assert_eq!(transition, Transition::Ignored);
        assert_eq!(state.arm, TempleArm::Disarmed);
    }

    /// The boundary, and the backdating with it: a line one ms inside the stale
    /// window arms, and its tail is measured from the moment it was SPOKEN
    /// rather than from the moment the watcher handed it over.
    ///
    /// Fails if the tail is measured from `now` (the arm would outlive the
    /// design by the watcher's delay, up to its 5 s `recv_timeout` fallback),
    /// or if the staleness comparison is off by one at the boundary.
    #[test]
    fn a_line_just_inside_the_stale_window_arms_from_its_own_stamp() {
        let mut state = ArmState::default();
        let spoken = NOW - (LINE_STALE_MS - 1_000);

        apply_line(&mut state, &alva_line(spoken), NOW);

        assert_eq!(
            state.arm,
            TempleArm::Armed {
                until_ms: Some(spoken + ALVA_TAIL_MS),
                reason: ArmReason::AlvaLine,
            },
        );
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

    /// The rule the tail is bounded BY: an area change outranks whatever the
    /// voice line had left. Fails if the disarm is conditional on the reason.
    #[test]
    fn an_area_change_disarms_a_voice_line_arm() {
        let mut state = ArmState::default();
        apply_line(&mut state, &alva_line(NOW), NOW);

        let transition = apply_line(&mut state, &map_line(NOW + 5_000), NOW + 5_000);

        assert_eq!(transition, Transition::Disarmed);
        assert!(!state.arm.is_armed(NOW + 5_000));
    }

    /// Alva's temple banter (`At last... Atzoatl.`) lands seconds after the
    /// area line. Fails if arming is "latest wins": the deadline-free temple
    /// arm would become a two-minute one and the module would go blind in every
    /// temple that took longer than that to run.
    #[test]
    fn an_alva_line_inside_the_temple_does_not_shorten_the_area_arm() {
        let mut state = ArmState::default();
        apply_line(&mut state, &temple_line(NOW), NOW);

        let transition = apply_line(
            &mut state,
            &stamped(NOW + 4_000, "Alva, Master Explorer: At last... Atzoatl."),
            NOW + 4_000,
        );

        assert_eq!(transition, Transition::Extended(ArmReason::TempleArea));
        assert_eq!(
            state.arm,
            TempleArm::Armed {
                until_ms: None,
                reason: ArmReason::TempleArea,
            },
        );
    }

    // ------------------------------------------------------ manual re-arm --

    /// Re-arm is the fallback for every case the log does not cover (the
    /// hideout panel, a non-English client). Fails if the grace is not applied.
    #[test]
    fn a_manual_rearm_arms_for_the_grace_window() {
        let mut state = ArmState::default();

        state.arm.arm_manual(NOW);

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

        state.arm.arm_manual(NOW + 1_000);

        assert!(state.arm.is_armed(NOW + MANUAL_ARM_GRACE_MS + 10_000));
    }

    // --------------------------------------------- the panel on screen --

    /// The 2026-09-03 bug, as the gate now answers it: the voice-line clock has
    /// run out and the panel is still on screen, so the loop keeps looking.
    ///
    /// Fails if the gate reads [`TempleArm`] alone — which is what stood the
    /// capture down at 14:37:00 over a layout panel the player was reading, and
    /// took the overlay with it.
    #[test]
    fn a_panel_seen_a_moment_ago_keeps_the_loop_armed_past_the_voice_line_tail() {
        let mut state = ArmState::default();
        apply_line(&mut state, &alva_line(NOW), NOW);
        let expired = NOW + ALVA_TAIL_MS + 1;
        assert!(!state.arm.is_armed(expired), "the voice line has nothing left");

        assert_eq!(
            arm_source(state, Some(expired - 1_000), false, expired),
            Some(ArmSource::PanelOnScreen),
        );
    }

    /// The boundary the tail is: measured from the LAST sighting, exclusive at
    /// its end. Fails if the deadline is measured from the first sighting, or
    /// from the arm, or is inclusive — each of which is a stand-down one tick
    /// away from where the design puts it.
    #[test]
    fn the_panel_tail_runs_from_the_last_sighting() {
        let seen = NOW + 45_000;

        assert_eq!(
            arm_source(ArmState::default(), Some(seen), false, seen + PANEL_TAIL_MS - 1),
            Some(ArmSource::PanelOnScreen),
            "one ms inside the tail",
        );
        assert_eq!(
            arm_source(ArmState::default(), Some(seen), false, seen + PANEL_TAIL_MS),
            None,
            "the tail is exclusive",
        );
    }

    /// POE-242's goal, unchanged by the third clock: nothing in the log and
    /// nothing on screen means the loop stops looking. Fails if the panel input
    /// is read as "was ever seen" rather than "was seen recently", which is the
    /// free-running capture POE-242 removed, restored by this work item.
    #[test]
    fn a_panel_gone_longer_than_its_tail_stands_the_loop_down() {
        let mut state = ArmState::default();
        apply_line(&mut state, &alva_line(NOW), NOW);
        let seen = NOW + 1_000;

        assert_eq!(
            arm_source(state, Some(seen), false, seen + PANEL_TAIL_MS + ALVA_TAIL_MS),
            None,
        );
    }

    /// A live Client.txt arm is what the log names, even with the panel on
    /// screen. Fails if the panel branch is tried first — `capture armed by
    /// Alva` is the line smoke item 12 reads, and it would become
    /// `by the panel on screen` for every incursion the player has a panel open
    /// during.
    #[test]
    fn a_live_client_txt_arm_is_the_source_the_log_names() {
        let mut state = ArmState::default();
        apply_line(&mut state, &alva_line(NOW), NOW);

        assert_eq!(
            arm_source(state, Some(NOW), false, NOW + 1_000),
            Some(ArmSource::Trigger(ArmReason::AlvaLine)),
        );
    }

    /// Walking out ends the PANEL's claim on the gate, not just Client.txt's:
    /// the panel the loop last anchored went with the zone.
    ///
    /// Fails if the panel branch reads the sighting alone — the loop then
    /// carries up to `PANEL_TAIL_MS` of 1 Hz capture into the next zone, under a
    /// log line saying the panel is on screen.
    #[test]
    fn an_area_change_ends_the_panel_arm_as_well_as_the_voice_line_one() {
        let mut state = ArmState::default();
        let seen = NOW;

        apply_line(&mut state, &map_line(seen + 1_000), seen + 1_000);

        assert_eq!(arm_source(state, Some(seen), false, seen + 2_000), None);
    }

    /// A panel seen AFTER the zone change is evidence about the new screen and
    /// arms again. Fails if the stamp is read as "an area change has happened"
    /// rather than as the moment it did — the panel clock would be dead for the
    /// rest of a session after the player's first map.
    #[test]
    fn a_panel_seen_after_the_area_change_arms_the_loop_again() {
        let mut state = ArmState::default();
        apply_line(&mut state, &map_line(NOW), NOW);

        assert_eq!(
            arm_source(state, Some(NOW + 1_000), false, NOW + 2_000),
            Some(ArmSource::PanelOnScreen),
        );
    }

    /// The boundary between the two, pinned from both sides: a sighting stamped
    /// at the moment of the zone change is not evidence about the zone after it,
    /// and one a millisecond later is.
    ///
    /// Fails if the comparison is `>=` (a sighting the change should have
    /// invalidated keeps the loop armed in the new zone) or `<` (the panel clock
    /// dies the moment any area line lands and never recovers).
    #[test]
    fn a_sighting_counts_only_from_the_millisecond_after_the_area_change() {
        let mut state = ArmState::default();
        apply_line(&mut state, &map_line(NOW), NOW);

        assert_eq!(
            arm_source(state, Some(NOW), false, NOW + 1_000),
            None,
            "the same millisecond as the change is not after it",
        );
        assert_eq!(
            arm_source(state, Some(NOW + 1), false, NOW + 1_000),
            Some(ArmSource::PanelOnScreen),
            "one millisecond later is",
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
            arm_source(ArmState::default(), None, true, NOW),
            Some(ArmSource::StartupProbe),
        );
    }

    /// And the look is ONE. A probe that has been spent over a screen with no
    /// panel on it leaves the loop stood down. Fails if the probe is a constant
    /// rather than a debt the tick settles — the gate would then never close,
    /// which is the free-running capture with extra steps.
    #[test]
    fn a_spent_probe_that_saw_nothing_leaves_the_loop_stood_down() {
        assert_eq!(arm_source(ArmState::default(), None, false, NOW), None);
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
            left_area_ms: None,
        };
        let tail = format!("{}\n", map_line(NOW - 1_000));

        apply_catch_up(&mut state, &tail);

        assert_eq!(state.arm, TempleArm::Disarmed);
    }

    /// Voice lines in the replay are history, not evidence about the screen.
    /// Fails if the catch-up folds `apply_line` over the buffer — the app would
    /// then start armed off a line spoken before it was running.
    #[test]
    fn an_alva_line_in_the_replay_does_not_arm() {
        let tail = format!(
            "{}\n{}\n",
            alva_line(NOW - 5_000),
            stamped(NOW - 1_000, "Alva, Master Explorer: Good job, exile."),
        );

        assert_eq!(catch_up_state(&tail), TempleArm::Disarmed);
    }
}
