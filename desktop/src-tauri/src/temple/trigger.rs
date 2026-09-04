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
//! ANY Alva line still ARMS, and ANY Alva line ENDS a cycle. The asymmetry is
//! deliberate: an unheard start variant costs a Re-arm (one incursion in 342
//! carried no start line at all, so that fallback is load-bearing anyway),
//! while an unheard END variant would leave the notice standing — a claim on
//! screen that is false. Arming is also wider than "the incursion is starting"
//! on purpose: after an incursion closes the player is still map-side and may
//! open the layout panel to read what the kill changed, so arming on
//! `Good job.` is WANTED, not the free-running bug coming back — the window it
//! buys is bounded by [`ALVA_TAIL_MS`], and the first `You have entered` line
//! after it disarms whatever the tail had left.
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
//! temple that took longer than that to run. It has exactly one exception, and
//! [`apply_line`] rather than [`TempleArm::arm`] is where it lives: an END line
//! over an [`ArmReason::AlvaStart`] arm.
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
    /// A `Alva, Master Explorer:` voice line that is not one of the three
    /// measured START phrases, map-side. Bounded by [`ALVA_TAIL_MS`].
    AlvaLine,
    /// One of the three measured START phrases ([`classify`]). Armed with NO
    /// deadline: the start line fires when the portal OPENS, and nothing in the
    /// game times an open portal out — the mining holds one gap of 22 minutes
    /// between a start and its end, the player being away from the PC. What
    /// ends this arm is an end line or an area change, never a clock.
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
            ArmReason::AlvaLine => "Alva",
            ArmReason::AlvaStart => "Alva's start line",
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
    /// A live arm was pushed out, was already reaching further than this line
    /// would have bought, or — the two cases that are not about horizons — was
    /// pulled IN to [`ALVA_TAIL_MS`] by an end line over a live
    /// [`ArmReason::AlvaStart`] arm, or had its reason replaced in place by the
    /// temple area line (both in [`apply_line`]). What the spelling means is
    /// "the gate was already open", not "it now reaches further". Silent: Alva
    /// speaks several times per incursion.
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
    ///
    /// # The one permitted shortening, and why it is not here
    ///
    /// An [`ArmReason::AlvaStart`] arm carries no deadline and an END line
    /// replaces it with an [`ALVA_TAIL_MS`] one — the only place in this module
    /// where a live arm gets a nearer horizon. It is written out longhand in
    /// [`apply_line`] rather than folded in here because its licence is not a
    /// rule about horizons at all: the GAME said the incursion ended, which is
    /// the one thing that outranks "the user asked for more looking". Every
    /// other caller — Alva's temple banter over a [`ArmReason::TempleArea`]
    /// arm, Re-arm pressed inside a temple — still gets the plain rule above.
    ///
    /// That the banter case reads on a `TempleArea` arm at all is what the
    /// temple area line's own branch in [`apply_line`] buys: it does not bid
    /// through this method, it ASSIGNS the reason. A bid would buy nothing over
    /// a live deadline-free `AlvaStart` arm — `outlives(None, None)` holds — and
    /// the banter would arrive to find a START arm and shorten it.
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
/// Without it, walking out of a temple carried up to [`PANEL_TAIL_MS`] of
/// capture at `super::run`'s `DETECT_INTERVAL` (~1.5 Hz, 650 ms) into the next
/// zone, under a log line claiming the panel was on screen.
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
/// gate. An [`LineEvent::AlvaEnd`] tail is measured from the LINE's own stamp
/// rather than from `now_ms` — the watcher's hop is up to its 5 s
/// `recv_timeout` fallback, and the tail would otherwise outlive the design by
/// exactly that.
///
/// # The END line over a START arm
///
/// A START arm has no deadline (see [`ArmReason::AlvaStart`]), so something has
/// to end it, and the END line is the game itself saying the incursion is over.
/// It is the ONE case where a live arm is given a nearer horizon
/// ([`TempleArm::arm`]) — and the horizon it is given is the ordinary
/// [`ALVA_TAIL_MS`] one, because the player who walks out of the incursion may
/// still open the layout panel to see what the kill changed.
///
/// A [`ArmReason::TempleArea`] or [`ArmReason::Manual`] arm is NOT shortened by
/// an end line, and the case that needs that is Alva's own temple banter
/// (`At last... Atzoatl.`): it is an [`LineEvent::AlvaEnd`] by this
/// classification, and shortening the deadline-free area arm with it would
/// blind the module part-way through a temple run.
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
            // START arm and shorten it to the tail, going blind part-way
            // through the run.
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
            // Before the disarm, and unconditionally: this stamp is about the
            // SCREEN (see [`ArmState`]) and the gate may already be closed.
            state.left_area_ms = Some(now_ms);
            state.arm.disarm()
        }
        Some(LineEvent::AlvaStart) => state.arm.arm(ArmReason::AlvaStart, None, now_ms),
        Some(LineEvent::AlvaEnd) => {
            let until = arm_at(now_ms, line_timestamp_ms(line)).saturating_add(ALVA_TAIL_MS);
            let starting = matches!(
                state.arm,
                TempleArm::Armed {
                    reason: ArmReason::AlvaStart,
                    ..
                }
            ) && state.arm.is_armed(now_ms);
            if starting {
                state.arm = TempleArm::Armed {
                    until_ms: Some(until),
                    reason: ArmReason::AlvaLine,
                };
                return Transition::Extended(ArmReason::AlvaLine);
            }
            state.arm.arm(ArmReason::AlvaLine, Some(until), now_ms)
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
/// **It never logs the ARM.** That app-log line has one owner, and it is the
/// capture loop's `run::gate_line`, for three reasons: this function runs
/// whether or not the temple module is on (logging here would narrate a module
/// the user has switched off); when the module IS on, both would fire within a
/// second and put two lines in `app.log` for one event; and the loop covers the
/// transition this function cannot see at all — an [`ALVA_TAIL_MS`] arm
/// expiring, which no Client.txt line announces.
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

    /// The trigger's whole reason for existing: until Alva speaks, the loop
    /// must not look. Fails if the speaker match is wrong or absent.
    #[test]
    fn an_alva_voice_line_arms_the_loop() {
        let mut state = ArmState::default();

        let transition = apply_line(&mut state, &alva_end(NOW), NOW);

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
        apply_line(&mut state, &alva_end(NOW), NOW);

        let transition = apply_line(&mut state, &alva_end(later), later);

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
        apply_line(&mut state, &alva_end(NOW), NOW);

        assert!(state.arm.is_armed(NOW + ALVA_TAIL_MS - 1), "one ms inside the tail");
        assert!(!state.arm.is_armed(NOW + ALVA_TAIL_MS), "the tail is exclusive");
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

        apply_line(&mut state, &alva_end(spoken), NOW);

        assert_eq!(
            state.arm,
            TempleArm::Armed {
                until_ms: Some(spoken + ALVA_TAIL_MS),
                reason: ArmReason::AlvaLine,
            },
        );
    }

    /// A START line arms with NO deadline, unlike every other voice line.
    ///
    /// The start fires when the PORTAL OPENS and nothing in the game times an
    /// open portal out — the mining holds one 22-minute gap between a start and
    /// its end, the player being away from the PC. Fails if the start is armed
    /// with [`ALVA_TAIL_MS`] like the rest: the module would go blind two
    /// minutes into a wait the game itself does not bound.
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

    /// The one permitted shortening: the game says the incursion ended, so the
    /// deadline-free start arm becomes an ordinary [`ALVA_TAIL_MS`] one,
    /// measured from the END line's own stamp.
    ///
    /// Fails if the end line is dropped over a live arm (the start arm would
    /// last until the next zone change, capturing across the rest of the map)
    /// or if the tail is measured from `now` rather than from the line.
    #[test]
    fn an_end_line_shortens_a_start_arm_to_the_tail_from_its_own_stamp() {
        let mut state = ArmState::default();
        apply_line(&mut state, &alva_start(NOW), NOW);
        let ended = NOW + 34_000;

        let transition = apply_line(&mut state, &alva_end(ended), ended);

        assert_eq!(transition, Transition::Extended(ArmReason::AlvaLine));
        assert!(
            state.arm.is_armed(ended + ALVA_TAIL_MS - 1),
            "one ms inside the tail the end line bought",
        );
        assert!(
            !state.arm.is_armed(ended + ALVA_TAIL_MS),
            "and the start arm's own horizon is gone",
        );
    }

    /// The shortening is keyed on the REASON, and this is the case that proves
    /// it: a `Manual` arm reaching past the tail keeps its own horizon.
    ///
    /// The arm is hand-built because today's constants cannot produce one —
    /// [`MANUAL_ARM_GRACE_MS`] is 60 s against [`ALVA_TAIL_MS`]'s 120 s, so a
    /// live Re-arm never outlives an end line's tail. What the test pins is the
    /// rule rather than the arithmetic: split those constants (their own docs
    /// invite it) and the case becomes reachable. Fails if the shortening drops
    /// its `AlvaStart` condition and takes any live arm with a further horizon.
    #[test]
    fn an_end_line_does_not_shorten_a_manual_arm_reaching_further() {
        let far = NOW + 10 * ALVA_TAIL_MS;
        let mut state = ArmState {
            arm: TempleArm::Armed {
                until_ms: Some(far),
                reason: ArmReason::Manual,
            },
            left_area_ms: None,
        };

        let transition = apply_line(&mut state, &alva_end(NOW), NOW);

        assert_eq!(transition, Transition::Extended(ArmReason::Manual));
        assert_eq!(
            state.arm,
            TempleArm::Armed {
                until_ms: Some(far),
                reason: ArmReason::Manual,
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
        apply_line(&mut state, &alva_end(NOW), NOW);

        let transition = apply_line(&mut state, &map_line(NOW + 5_000), NOW + 5_000);

        assert_eq!(transition, Transition::Disarmed);
        assert!(!state.arm.is_armed(NOW + 5_000));
    }

    /// The START arm carries no deadline, so the zone change is one of only two
    /// things that can end it — and the one that fires when the player
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
    /// None)` holds), so the reason stays `AlvaStart` and the banter — an
    /// [`LineEvent::AlvaEnd`] by the phrase table — hits the START-arm
    /// shortening and trades the deadline-free arm for a two-minute one.
    ///
    /// Fails if the temple entry bids through [`TempleArm::arm`] rather than
    /// assigning, if arming is "latest wins", or if the end line's shortening
    /// is applied to any live arm rather than only to an
    /// [`ArmReason::AlvaStart`] one: the module would go blind in every temple
    /// that took longer than the tail to run.
    #[test]
    fn an_alva_line_inside_the_temple_does_not_shorten_the_area_arm() {
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
        apply_line(&mut state, &alva_end(NOW), NOW);
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
        apply_line(&mut state, &alva_end(NOW), NOW);
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
        apply_line(&mut state, &alva_end(NOW), NOW);

        assert_eq!(
            arm_source(state, Some(NOW), false, NOW + 1_000),
            Some(ArmSource::Trigger(ArmReason::AlvaLine)),
        );
    }

    /// Walking out ends the PANEL's claim on the gate, not just Client.txt's:
    /// the panel the loop last anchored went with the zone.
    ///
    /// Fails if the panel branch reads the sighting alone — the loop then
    /// carries up to `PANEL_TAIL_MS` of capture at `super::run`'s
    /// `DETECT_INTERVAL` (~1.5 Hz, 650 ms) into the next zone, under a log line
    /// saying the panel is on screen.
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
    /// starts no cycle, it ARMS the capture for [`ALVA_TAIL_MS`] (the player
    /// may open the panel to see what the kill changed) and it ENDS the advice
    /// of the incursion that produced it. A single verdict cannot say all
    /// three.
    #[test]
    fn the_end_line_still_arms_the_capture() {
        let mut state = ArmState::default();
        let line = stamped(NOW, "Alva, Master Explorer: Good job, exile.");

        let transition = apply_line(&mut state, &line, NOW);

        assert_eq!(transition, Transition::Armed(ArmReason::AlvaLine));
        assert!(state.arm.is_armed(NOW));
        assert_eq!(advice_end(&line, READ, NOW), Some(AdviceEnd::NewIncursion));
    }
}
