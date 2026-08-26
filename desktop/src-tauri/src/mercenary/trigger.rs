//! What starts a capture, and how hard it looks before giving up (POE-198,
//! rewritten as a two-probe gate by POE-204 WI-C).
//!
//! Before this file the capture loop hunted for a recruit window once a second
//! for as long as the module was on. That is a full-screen grab plus a
//! full-screen OCR per second, forever, for a window the player opens a handful
//! of times an hour. The loop now runs OCR only when something has asked it to
//! look, and the ask is an event:
//!
//! - a **Client.txt voice line** whose speaker is shaped like a mercenary's
//!   (`<Name>, <joiner> <Epithet>`) and is not a known NPC, or
//! - the page's **Scan now** button.
//!
//! # The gate (Sebastian's design, 2026-08-26)
//!
//! POE-198 answered a voice line with a ten-second BURST: full-screen detects
//! on the hunt cadence until one found a window or the clock ran out.
//! MEASURED on app.log 2026-08-26 09:14-09:42, that is the wrong shape. A
//! mercenary speaks on APPROACH as well as on click, and PoE's arenas are full
//! of them — so most bursts are ten seconds of full-screen OCR for a window the
//! player never opened, and the one thing the burst got right (the window IS
//! open) it learned on its first tick.
//!
//! Approaching a mercenary fires a voice line sometimes; clicking one fires it
//! always. So the line is not evidence that a window is open — it is evidence
//! that one might be about to be, and the cheapest question that separates the
//! two is "is the recruit window's chrome on screen?".
//!
//! 1. Voice line (module on, speaker ∉ denylist) → **+500 ms** → ONE targeted
//!    probe: `run::probe_tick` OCRs the anchor band alone
//!    ([`super::geometry::probe_band_bounds`]), not the 39-line full screen.
//! 2. Nothing → ONE retry at **+1.5 s from the line**, or [`PROBE_GAP_MS`]
//!    after the first probe actually ran when the game was not in front for it.
//! 3. Still nothing → **stand down**. No burst, no scanning: the player was
//!    walking past. One log line says so.
//! 4. Chrome found → the probe hands its own frame to a full detect in the SAME
//!    iteration, and the pre-existing live behaviour takes over unchanged
//!    (re-detect 2 s, hover 400 ms, retire after two misses).
//! 5. A voice line arriving while a capture is HELD ([`capture_held`]) is
//!    ignored outright. The window is on screen and being read; re-arming can
//!    only make the loop re-detect a panel the cursor is over — which is how
//!    09:41:56 turned a chattering mercenary into a phantom retire.
//!
//! **Scan now bypasses the gate**: it asks for a full detect and gets one, the
//! first moment the game is in front. It is the manual safety net for every
//! window the gate stood down on.
//!
//! # Why a denylist and not a pattern
//!
//! Merc names are generated (first name × epithet), so there is no list to
//! match against. `<Name>, <joiner> <Epithet>` is the shape, but PoE's own NPCs
//! use it too, so the positive pattern ALONE was measured to fail. The NPCs
//! ship in `assets/npc-denylist.txt`, and `<app_data>/merc-npc-denylist.txt`
//! extends them without a rebuild, the same shape as `merc-geometry.json`.
//!
//! MEASURED on Sebastian's Client.txt, 2026-08-25, rescanned with this shape:
//! joiners `the` ×94 and `of` ×1, 25 NPC speakers (`Varashta, the Winter
//! Sekhema` alone accounts for 416 lines), and the 70 speakers left after the
//! denylist are all generated mercenary names. NO NPC in that file uses a
//! lowercase joiner other than `the`.
//!
//! The joiner is a variable rather than the literal `the` because `of` is real
//! on both measured machines: `Swain, of Fractured Faith` is the one on the PC,
//! and on the LAPTOP file the same day — a smaller, merc-heavier sample — the
//! joiners ran `of` ×8 against `the` ×7 (`Fennik, of Unshakeable Faith`). A
//! `, the ` rule silently drops every one of them. See [`is_dialogue_speaker`]
//! for the lowercase requirement that keeps `Alva, Master Explorer` out, and
//! for the trade-off it leaves behind.
//!
//! # The cost of being wrong
//!
//! A false trigger costs two band OCRs per line and is invisible (nothing is
//! found, nothing is published) — an order less than the burst it replaced,
//! which is what makes the shape affordable in an arena full of mercenaries. A
//! missed trigger costs the capture entirely — which is what Scan now exists
//! for. The bias is deliberate: the cheap failure is the one this file prefers.
//!
//! **Per LINE, not per mercenary.** A line over an armed gate RE-ARMS it with
//! two fresh probes ([`BurstGate::hear`]), because the click line lands on top
//! of the approach line and it is the click that opens a window. So a
//! chattering mercenary does keep buying looks — capped by
//! [`LINE_STALE_MS`] from the FIRST line of the chain, which is ten seconds of
//! band OCRs against the ten seconds of full-screen ones POE-198 paid for
//! exactly the same chatter.
//!
//! # Pure vs glue
//!
//! Everything that decides — the speaker parse, the denylist, the line's clock,
//! the gate state machine — is a plain function over plain data and is tested
//! here on Linux. The `AppHandle` wrappers at the bottom only lock, log and
//! publish.

use std::collections::HashSet;
use std::path::Path;

use tauri::{AppHandle, Manager};

use super::run::{now_ms, publish, status};
use super::{MercStatus, DENYLIST_OVERRIDE_FILE, MODULE_ID};
use crate::AppState;

/// The 25 measured NPC speakers. See `assets/README.md` for provenance.
const SHIPPED_DENYLIST: &str = include_str!("assets/npc-denylist.txt");

/// How long after the voice line the FIRST probe looks.
///
/// The window is not open when the line lands: the click that opens it is what
/// fires the line, and the panel animates in behind it. Half a second is the
/// design's allowance for that, and the probe is cheap enough that being early
/// costs one band OCR rather than a capture.
pub const PROBE_DELAY_MS: u64 = 500;

/// How long after the voice line the RETRY probe looks.
///
/// One second after the first, which is the lag allowance: a frame that arrived
/// mid-animation, a machine that took its time, a click a beat behind the line.
/// There is no third — a window that is not on screen a second and a half after
/// the mercenary spoke is a window the player did not open, and the whole point
/// of the gate is that walking past a mercenary costs nothing.
pub const PROBE_RETRY_MS: u64 = 1_500;

/// The shortest gap the gate leaves between two probes of the same line.
///
/// [`PROBE_RETRY_MS`] is a deadline against the LINE, and on the ordinary path
/// that is all it has to be: the first probe runs at [`PROBE_DELAY_MS`] and the
/// retry is a second behind it by arithmetic. It stops being enough when the
/// game was not in front at 500 ms — both deadlines are then already past when
/// the player alt-tabs back, and reading them literally spends BOTH probes on
/// the same frame. That is one probe's worth of evidence at two probes' cost,
/// and it throws away the only thing the retry was for.
///
/// So the retry is due at `max(line + PROBE_RETRY_MS, last probe + this)`. On
/// time the two agree and nothing changes; after a missed deadline the gate
/// looks the moment the game returns and then waits this out before looking
/// again — the lag allowance re-timed from the lag it was allowing for.
pub const PROBE_GAP_MS: u64 = PROBE_RETRY_MS - PROBE_DELAY_MS;

/// How long an unprobed voice line stays worth probing.
///
/// Both probes fire only while the game is the FOREGROUND window, so a line
/// heard as the player alt-tabs never gets its look. This is the give-up: a
/// voice line is evidence about the screen at the moment it was spoken, and ten
/// seconds later it is evidence about nothing. Without it the gate would sit
/// armed until the player came back — an hour later, if that is when they come
/// back — and then probe for a window that closed with the alt-tab.
///
/// Ten seconds because that is what POE-198's burst was sized at, for the same
/// reason: it is the outside of "the player is still standing where they were".
/// What has changed is what the ten seconds BUY. They used to be ten seconds of
/// full-screen OCR; they are now the shelf life of a right to two band looks.
///
/// Measured from the FIRST line of an unbroken chain by the same speaker, not
/// from the latest one. Every line re-arms both probes ([`BurstGate::hear`]),
/// so a clock measured from the latest one would never run out while the
/// mercenary kept talking — POE-198's re-armable expiry, back in the gate's
/// clothes. The chain breaks when the gate goes back to resting (a stand-down,
/// a capture, a disarm) or when a DIFFERENT speaker is heard, and the next line
/// then starts its own ten seconds.
pub const LINE_STALE_MS: u64 = 10_000;

/// The furthest back a line's own timestamp may move the gate's clock.
///
/// The gate is measured from the LINE, not from the watcher's delivery of it —
/// `log_watcher` blocks on a notify event with a 5 s `recv_timeout` fallback, so
/// a line that arrives without a filesystem event reaches this module up to that
/// whole fallback late, and a 500 ms probe measured from delivery would be five
/// and a half seconds after the click. [`line_timestamp_ms`] reads the
/// `2026/08/24 22:51:12` stamp the line carries and [`arm_at`] backdates the
/// gate to it.
///
/// The clamp is what makes that safe. The stamp is LOCAL time with no zone,
/// read against a UTC clock that may not agree with it: a machine whose local
/// offset resolves wrong produces a line timestamp minutes or hours away from
/// `now`. Backdating by an hour would fire both probes at once and stand down
/// before the player finished clicking; backdating FORWARD would leave the
/// probes never due and the module silently dead. So a stamp outside
/// `[now - this, now]` is not believed, and the gate falls back to the delivery
/// clock — the behaviour POE-198 shipped, which is wrong by at most one watcher
/// hop.
pub const MAX_BACKDATE_MS: u64 = 10_000;

/// How long a Scan-now request waits for the game before giving up.
///
/// Scan now is clicked in OUR window, which by definition means the game is not
/// the foreground window and the capture loop is napping. So the request is not
/// a deadline to meet but a debt to settle: it is owed ONE full detect, and it
/// gets it at the first moment the game IS in front. This bounds the wait, so a
/// click that is never followed by an alt-tab does not leave the module holding
/// a detect for the rest of the session.
pub const MANUAL_ARM_GRACE_MS: u64 = 60_000;

/// The separator that splits a dialogue speaker into its name and its title.
const SPEAKER_SEPARATOR: &str = ", ";

/// Sigils PoE puts in front of a PLAYER's name: whisper, guild, global, trade,
/// party. A speaker starting with one of these is a person typing, whatever it
/// says after — `<Guild>` included, which is why `<` is on the list.
const CHAT_SIGILS: [char; 6] = ['@', '&', '#', '$', '%', '<'];

// ---------------------------------------------------------------------------
// Pure — the speaker parse
// ---------------------------------------------------------------------------

/// The speaker of a Client.txt dialogue line: everything before the first
/// `": "` of the MESSAGE (the part after the `[INFO Client 1234]` tag).
///
/// `None` for a line with no tag, no `": "`, or a message that starts with a
/// chat sigil. Taking the text before the FIRST separator is what keeps a
/// player out: `Someone: Nytra, the Cyaxan Loner is cheap` has speaker
/// `Someone`, and searching the line for `, the ` instead would have armed on
/// it.
pub fn speaker_of(line: &str) -> Option<&str> {
    let message = &line[line.find("] ")? + 2..];
    if message.starts_with(CHAT_SIGILS) {
        return None;
    }
    let (speaker, _) = message.split_once(": ")?;
    Some(speaker)
}

/// Whether a speaker is shaped like a mercenary's or an NPC's: one whitespace-
/// free name, `", "`, one all-lowercase joiner word, then a non-empty epithet
/// (`^\S+, [a-z]+ ` plus the requirement that something follows).
///
/// MEASURED, and the reason this is no longer `", the "`. Two Client.txt files,
/// both 2026-08-25: the LAPTOP one caught `Fennik, of Unshakeable Faith` with
/// joiners running `of` ×8 against `the` ×7, and a rescan of the PC one caught
/// `Swain, of Fractured Faith` (`the` ×94, `of` ×1). The epithet is a title,
/// and PoE builds titles with more than one preposition, so a `, the ` rule
/// silently drops every `of` mercenary — on the laptop file, most of them.
///
/// The name is `\S+` and the hyphen in `Al-Hezmin, the Hunter` is inside it:
/// the constraint is NO WHITESPACE, not "letters only". A rule that required
/// letters would have let that NPC past the denylist, which matches on the
/// whole speaker string.
///
/// The joiner is required to be LOWERCASE because that is what separates a
/// title from a two-part name: `Alva, Master Explorer` (the laptop file's only
/// comma-carrying NPC) has a capitalised `Master` that is part of the NAME.
/// Matching any word there would have armed on every one of her lines.
///
/// **A second cost, on a different axis:** `is_ascii_lowercase` is an ENGLISH
/// rule. A client running in another language writes its joiner in that
/// language, and where that word is non-ASCII (or is capitalised by that
/// language's convention) no voice line on that machine will ever arm a burst.
/// Such a player is not broken, only unautomated: **Scan now** covers every
/// capture by hand, which is the same fallback a mercenary who says nothing
/// gets. Widening to `char::is_lowercase` would admit non-ASCII joiners but
/// still not a language that capitalises them, so it buys part of a fix for a
/// case nobody has measured — left until someone runs a non-English client.
///
/// **The trade-off, stated:** the shape is wider than the 25 measured NPCs it
/// is tuned against, all of which use `, the ` — no NPC with a lowercase joiner
/// other than `the` was found in either file. An NPC introduced with one
/// (`Someone, of Somewhere`) would match this shape and would have to be
/// suppressed by name — [`NpcDenylist`], and the override file when a rebuild
/// is not wanted. That is the cheap failure by design (see the module header):
/// a false trigger costs one silent burst, a missed one costs the capture.
///
/// Shape only — it says nothing about whether the speaker is a mercenary. That
/// is [`NpcDenylist`]'s job, because the names are generated and there is no
/// positive list to match.
pub fn is_dialogue_speaker(speaker: &str) -> bool {
    let Some((name, title)) = speaker.split_once(SPEAKER_SEPARATOR) else {
        return false;
    };
    if name.is_empty() || name.contains(char::is_whitespace) || name.starts_with(CHAT_SIGILS) {
        return false;
    }
    let Some((joiner, epithet)) = title.split_once(' ') else {
        return false;
    };
    !joiner.is_empty()
        && joiner.chars().all(|c| c.is_ascii_lowercase())
        && !epithet.trim().is_empty()
}

// ---------------------------------------------------------------------------
// Pure — the NPC denylist
// ---------------------------------------------------------------------------

/// The speakers that are NOT mercenaries.
///
/// Case-insensitive and whitespace-trimmed, because the override file is hand
/// written: a name typed with a trailing space or a lowercase article must
/// still suppress the NPC it names, and a denylist entry that silently fails to
/// match is exactly the OCR-every-Varashta-line waste this exists to stop.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct NpcDenylist {
    names: HashSet<String>,
}

#[allow(clippy::len_without_is_empty)]
impl NpcDenylist {
    /// The shipped list — the 25 measured NPCs, and nothing else.
    pub fn shipped() -> Self {
        Self::parse(SHIPPED_DENYLIST)
    }

    /// Parse the one-name-per-line format: blanks and `#` comments dropped.
    pub fn parse(raw: &str) -> Self {
        let mut list = Self::default();
        list.extend_from(raw);
        list
    }

    /// Merge more names in. Returns how many were NEW — the number the log line
    /// reports, so an override file that is being ignored (wrong name, wrong
    /// directory) is distinguishable from one that is doing nothing.
    pub fn extend_from(&mut self, raw: &str) -> usize {
        let before = self.names.len();
        for line in raw.lines() {
            let name = line.trim();
            if name.is_empty() || name.starts_with('#') {
                continue;
            }
            self.names.insert(name.to_lowercase());
        }
        self.names.len() - before
    }

    /// Whether this speaker is a known NPC.
    pub fn contains(&self, speaker: &str) -> bool {
        self.names.contains(&speaker.trim().to_lowercase())
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// The shipped list merged with `<app_data>/merc-npc-denylist.txt`.
    ///
    /// Returns the list, how many names the override added, and the reason the
    /// override could not be read when there was one. A MISSING file is not a
    /// reason — that is the normal case — but an unreadable one is: a user who
    /// wrote the file and got no effect has to be able to see why.
    pub fn load(dir: Option<&Path>) -> (Self, usize, Option<String>) {
        let mut list = Self::shipped();
        let Some(path) = dir.map(|d| d.join(DENYLIST_OVERRIDE_FILE)) else {
            return (list, 0, None);
        };
        if !path.exists() {
            return (list, 0, None);
        }
        match std::fs::read_to_string(&path) {
            Ok(raw) => {
                let added = list.extend_from(&raw);
                (list, added, None)
            }
            Err(e) => (
                list,
                0,
                Some(format!("{} could not be read: {}", path.display(), e)),
            ),
        }
    }
}

/// The whole trigger decision for one Client.txt line: the speaker that should
/// arm a burst, or `None`.
///
/// Module enablement is deliberately NOT checked here — it is the caller's
/// cheap early-out and it needs a lock, whereas this is pure and runs on every
/// line of the log.
pub fn trigger_speaker<'a>(line: &'a str, denylist: &NpcDenylist) -> Option<&'a str> {
    let speaker = speaker_of(line)?;
    if !is_dialogue_speaker(speaker) || denylist.contains(speaker) {
        return None;
    }
    Some(speaker)
}

// ---------------------------------------------------------------------------
// Pure — the line's own clock
// ---------------------------------------------------------------------------

/// The `2026/08/24 22:51:12` stamp every Client.txt line starts with, as epoch
/// milliseconds, or `None` when the line does not carry one this can read.
///
/// LOCAL time with no zone — that is the format PoE writes — so the conversion
/// goes through the machine's own offset. Two cases have no single answer and
/// both resolve to "do not believe this stamp": the hour a spring-forward
/// deletes does not exist (`None`), and the hour an autumn fall-back repeats
/// happens twice (the EARLIER of the two, which is at most an hour early and is
/// caught by [`MAX_BACKDATE_MS`] rather than by guessing).
///
/// Only the first 19 characters are read. The rest of the prefix
/// (`105432578 cffb0716 [INFO Client 12345]`) is a monotonic tick count and a
/// thread id, neither of which is a clock this can compare against `now`.
pub fn line_timestamp_ms(line: &str) -> Option<u64> {
    stamp_ms_in(&chrono::Local, line)
}

/// [`line_timestamp_ms`] against an EXPLICIT zone.
///
/// Split out for one reason: the two answers that depend on the zone — the
/// deleted spring-forward hour and the repeated autumn one — cannot be
/// exercised through `chrono::Local`, whose transitions are the host's and are
/// UTC (no transitions at all) on the machines this suite runs on. A test zone
/// with one known fall-back is what pins `.earliest()`; see
/// `the_repeated_autumn_hour_resolves_to_the_earlier_of_its_two_instants`.
fn stamp_ms_in<Tz: chrono::TimeZone>(tz: &Tz, line: &str) -> Option<u64> {
    let stamp = line.get(..19)?;
    let naive = chrono::NaiveDateTime::parse_from_str(stamp, "%Y/%m/%d %H:%M:%S").ok()?;
    let resolved = tz.from_local_datetime(&naive).earliest()?;
    u64::try_from(resolved.timestamp_millis()).ok()
}

/// The moment the gate's clock starts, given the wall clock now and whatever
/// the line's own stamp said.
///
/// The line wins when it is BEHIND `now` by no more than [`MAX_BACKDATE_MS`],
/// which is the whole of the believable range: a watcher hop is a delay, never
/// a head start, so a stamp in the future is a clock that disagrees and a stamp
/// hours old is the same disagreement the other way. Everything outside that
/// band falls back to `now` — the delivery clock POE-198 shipped.
///
/// Pure and separate from the parse so the clamp is testable without a
/// timezone: the parse is what depends on the host's offset, and this is what
/// decides.
pub fn arm_at(now_ms: u64, line_ms: Option<u64>) -> u64 {
    match line_ms {
        Some(line) if line <= now_ms && now_ms - line <= MAX_BACKDATE_MS => line,
        _ => now_ms,
    }
}

// ---------------------------------------------------------------------------
// Pure — the gate state machine
// ---------------------------------------------------------------------------

/// Whether a capture is currently HELD — a recruit window is on screen and the
/// loop owns it (rule 5).
///
/// `Live` is a window being read and `Done` is one fully read whose OCR is
/// paused; both mean the panel is on screen and the loop knows where. A voice
/// line arriving over either says nothing the loop does not already have, and
/// arming on it is what produced the 09:41:56 phantom retire: the arm resumed
/// the paused read, the resumed cadence spent its first tick on a frame the
/// player's tooltip was covering, and the capture retired with the window open.
///
/// Read off the PUBLISHED status rather than off `run::LoopState`, because the
/// line arrives on the log-watcher thread and the loop's state lives inside the
/// capture thread's own `Session`. The status is the one copy both can see, and
/// it is written by the same detect that sets `live`.
pub fn capture_held(status: MercStatus) -> bool {
    matches!(status, MercStatus::Live | MercStatus::Done)
}

/// One voice line's right to look.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArmedProbe {
    /// The mercenary the module heard.
    pub speaker: String,
    /// When the LINE was spoken — [`arm_at`], not the delivery. Both probe
    /// deadlines are measured from this.
    pub line_at_ms: u64,
    /// When the FIRST line of this speaker's unbroken chain was spoken.
    ///
    /// Equal to [`Self::line_at_ms`] for a line that armed a resting gate, and
    /// older for every line that re-armed one. [`LINE_STALE_MS`] counts from
    /// here, which is what stops chatter holding the gate open — see
    /// [`BurstGate::hear`].
    pub chain_at_ms: u64,
    /// How many of the two probes THIS line has fired. Reset by every new line;
    /// [`BurstGate::looks`] is the counter that is not.
    pub fired: u8,
    /// When the last probe actually ran, `None` before the first one.
    ///
    /// What re-times the retry after a deadline the game would not let the gate
    /// meet — see [`PROBE_GAP_MS`].
    pub last_probe_ms: Option<u64>,
}

impl ArmedProbe {
    /// Whether the next probe is due.
    ///
    /// The FIRST probe's deadline is absolute against the line, so a gate the
    /// game would not let probe on time does not get its 500 ms back: it finds
    /// the probe overdue and looks the moment the game returns.
    ///
    /// The RETRY is the LATER of its own absolute deadline and [`PROBE_GAP_MS`]
    /// after the probe that actually ran. On the ordinary path the two agree;
    /// after a missed deadline the gap is what stops both probes being spent on
    /// one frame.
    fn probe_due(&self, now_ms: u64) -> bool {
        let since_line = now_ms.saturating_sub(self.line_at_ms);
        match self.fired {
            0 => since_line >= PROBE_DELAY_MS,
            1 => {
                since_line >= PROBE_RETRY_MS
                    && self
                        .last_probe_ms
                        .is_none_or(|at| now_ms.saturating_sub(at) >= PROBE_GAP_MS)
            }
            _ => false,
        }
    }

    /// Whether this line has run out of probes, or its CHAIN has run out of
    /// time. See [`LINE_STALE_MS`] for why the clock is the chain's.
    fn done(&self, now_ms: u64) -> bool {
        self.fired >= PROBES_PER_LINE
            || now_ms.saturating_sub(self.chain_at_ms) >= LINE_STALE_MS
    }
}

/// Probes one voice line buys. Two: the look and the lag allowance.
pub const PROBES_PER_LINE: u8 = 2;

/// Why the gate stopped, in the one form the log needs.
///
/// Three endings rather than one line, because they send the reader to three
/// different places: a gate that spent its probes is the ordinary case and
/// needs no action, a gate the game would not let finish looking is a focus
/// problem, and a Scan now that gave up is a button the player pressed and
/// never alt-tabbed for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StandDown {
    /// The gate spent its last probe and none of them found chrome. The
    /// design's normal ending, and the only one that is a verdict about the
    /// SCREEN.
    Probed { speaker: String },
    /// The chain went stale with probes still unspent ([`LINE_STALE_MS`]),
    /// which can only mean the game was not the foreground window when they
    /// came due. Blaming the OCR here would send the reader hunting a detection
    /// bug that is not there.
    ///
    /// The split from [`Self::Probed`] is on WHY the gate ended, not on whether
    /// any probe ran at all: a gate that looked once and then lost the
    /// foreground reached no verdict about the screen, and reporting it as
    /// `Probed` would claim one it never had.
    Stale { speaker: String },
    /// A Scan now whose alt-tab never came ([`MANUAL_ARM_GRACE_MS`]).
    Manual,
}

impl StandDown {
    pub fn line(&self) -> String {
        match self {
            StandDown::Probed { speaker } => {
                format!("Merc: no recruit window after {speaker} — standing down")
            }
            StandDown::Stale { speaker } => format!(
                "Merc: no recruit window after {speaker} — standing down \
                 (the game was not in front for its remaining probes)"
            ),
            StandDown::Manual => {
                "Merc: Scan now gave up — the game never came to the foreground".to_string()
            }
        }
    }
}

/// What the gate wants of this loop iteration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateStep {
    /// Nothing is armed. The loop's resting state.
    Resting,
    /// Armed, and the next probe is not due yet. Distinct from `Resting`
    /// because the loop naps a quantum here rather than an idle nap: a 500 ms
    /// probe rounded up to the next 250 ms boundary is a 750 ms probe.
    Waiting,
    /// Run the anchor-band probe now.
    Probe,
    /// Run a FULL detect now — Scan now, which does not go through the band.
    FullDetect,
}

/// Whether arming a burst may publish `Scanning` over this status.
///
/// The precedence rule ([`MercStatus`]) in the one form the arming site needs,
/// pulled out of the glue because it is the part that can be wrong quietly: a
/// scan announcing itself over `Live` would replace "a recruit window is on
/// screen" with "looking for one", and over `Off` / `Unavailable` it would
/// claim a loop that is not running.
///
/// **`Done` is NOT overwritten either**, and that is the difference between
/// this and a plain reading of the precedence order. `Done` is a window ON
/// SCREEN whose verdict the player is reading right now; the overlay treats
/// every status but `live`/`done` as a capture that is no longer current, so
/// publishing `scanning` over it would mark a live verdict stale and drop the
/// per-row glyphs under it. Scan now still does its work — the loop resumes the
/// paused read the moment it sees the requested detect
/// (`run::LoopState::resume`) and republishes `live`/`done` from it. Only the
/// ANNOUNCEMENT is withheld, because there is nothing to announce that the
/// window on screen does not already say.
///
/// A VOICE LINE over `Live`/`Done` never reaches here at all — [`capture_held`]
/// drops it one layer up. This still refuses those two statuses, because Scan
/// now does reach here over them and because a predicate that is right for one
/// caller and relies on another caller's filter is a trap.
pub fn scan_outranks(status: MercStatus) -> bool {
    matches!(status, MercStatus::Idle)
}

/// The gate the capture loop asks "should I be looking at all?".
///
/// Two independent slots, because the two triggers are different promises. A
/// voice line buys [`PROBES_PER_LINE`] band looks on a deadline it cannot
/// extend; Scan now buys ONE full detect whenever the game next comes forward.
/// Folding them into one slot is what made the old burst re-armable by a
/// chattering mercenary — every line pushed the expiry out, so a mercenary who
/// spoke every two seconds held a full-screen hunt open indefinitely.
///
/// The log lines hang off the transitions: arming an idle gate says so once,
/// and standing down says so once ([`Self::take_stood_down`], which CLEARS what
/// it reports).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct BurstGate {
    /// The voice line being probed for.
    probe: Option<ArmedProbe>,
    /// When a Scan now asked for its full detect. `None` — nothing owed.
    manual_at_ms: Option<u64>,
    /// Probes this ARMING has spent, across every line that re-armed it.
    ///
    /// Monotonic while the gate stays armed, zeroed only when a RESTING gate is
    /// armed afresh. `ArmedProbe::fired` cannot serve here: every new line
    /// resets it, so a band keyed on that would sit on the remembered rect for
    /// as long as a mercenary kept talking. See [`Self::hear`].
    looks: u32,
}

impl BurstGate {
    /// Arm the gate for a voice line, from the line's own clock.
    ///
    /// `true` when this started something the caller should log.
    ///
    /// **A line arriving over an already-armed gate RE-ARMS it**: two fresh
    /// probes, measured from the new line. That is deliberate, and it is the
    /// case the whole design turns on — approaching a mercenary fires a line
    /// SOMETIMES and clicking one fires it ALWAYS, so the click line lands on
    /// top of the approach line, and it is the click line whose window is worth
    /// probing for. Answering it out of whatever the approach line had left
    /// would stand the gate down on the very window the player just opened.
    ///
    /// The cost is bounded by the CHAIN rather than by the line: a mercenary
    /// who speaks every second buys a probe a second, for the ten seconds
    /// [`LINE_STALE_MS`] allows and no longer. Two rules make that bound hold:
    ///
    /// - [`ArmedProbe::chain_at_ms`] is carried over from the line being
    ///   replaced when the SPEAKER is the same, so the stale clock runs from
    ///   the first line of the chain and continuous chatter still stands the
    ///   gate down. A different speaker starts a new chain — different
    ///   mercenary, different click, its own ten seconds;
    /// - [`Self::looks`] keeps counting across the re-arm, so `run::probe_band`
    ///   widens on the second LOOK rather than on the second probe of some
    ///   line. Chatter cannot pin the probe to the remembered band.
    pub fn hear(&mut self, speaker: String, line_at_ms: u64) -> bool {
        let chain_at_ms = self
            .probe
            .as_ref()
            .filter(|p| p.speaker == speaker)
            .map_or(line_at_ms, |p| p.chain_at_ms);
        let fresh = self.probe.is_none();
        if fresh {
            self.looks = 0;
        }
        self.probe = Some(ArmedProbe {
            speaker,
            line_at_ms,
            chain_at_ms,
            fired: 0,
            last_probe_ms: None,
        });
        fresh
    }

    /// Scan now: owe one full detect.
    ///
    /// `true` when nothing was owed already, so a double-click logs once.
    pub fn request_full_detect(&mut self, now_ms: u64) -> bool {
        let fresh = self.manual_at_ms.is_none();
        self.manual_at_ms = Some(now_ms);
        fresh
    }

    /// What the loop should do this iteration.
    ///
    /// Scan now outranks a probe: it is a person asking, and the full detect it
    /// runs answers the probe's question as a side effect.
    pub fn step(&self, now_ms: u64) -> GateStep {
        if self.manual_at_ms.is_some() {
            return GateStep::FullDetect;
        }
        match &self.probe {
            Some(p) if p.probe_due(now_ms) => GateStep::Probe,
            Some(_) => GateStep::Waiting,
            None => GateStep::Resting,
        }
    }

    /// Who the armed line named, for the strip to print. `None` when nothing is
    /// armed, and for Scan now, which heard nobody.
    pub fn speaker(&self) -> Option<&str> {
        self.probe.as_ref().map(|p| p.speaker.as_str())
    }

    /// How many probes this ARMING has spent — the attempt number
    /// `run::probe_band` widens the band on. 0 for a gate armed afresh.
    ///
    /// Deliberately not `ArmedProbe::fired`: see [`Self::looks`] the field.
    pub fn looks(&self) -> u32 {
        self.looks
    }

    /// A probe just ran, at `now_ms`. Spending it here rather than on the hit
    /// keeps the count honest: a probe that found nothing is still a probe
    /// spent, and it is the counting that makes the retry the LAST one.
    ///
    /// `now_ms` is what re-times the retry when the game would not let the gate
    /// meet its deadlines — see [`PROBE_GAP_MS`].
    pub fn note_probe(&mut self, now_ms: u64) {
        self.looks = self.looks.saturating_add(1);
        if let Some(p) = self.probe.as_mut() {
            p.fired += 1;
            p.last_probe_ms = Some(now_ms);
        }
    }

    /// The Scan now was served. It owes nothing more, whatever the detect found
    /// — a manual scan is one look, not a hunt.
    pub fn note_full_detect(&mut self) {
        self.manual_at_ms = None;
    }

    /// Take whatever has just given up, so it is reported exactly once.
    ///
    /// Both slots are checked, and the probe first: a stand-down the player can
    /// act on outranks a Scan now that timed out behind it.
    ///
    /// The caller keeps the probe slot empty while a capture is LIVE
    /// ([`Self::disarm_probe`]), so under a live capture this can only ever
    /// report the Scan now — which is the one ending that IS still worth
    /// printing over an open window.
    pub fn take_stood_down(&mut self, now_ms: u64) -> Option<StandDown> {
        if self.probe.as_ref().is_some_and(|p| p.done(now_ms)) {
            let p = self.probe.take().expect("just tested");
            return Some(if p.fired >= PROBES_PER_LINE {
                StandDown::Probed { speaker: p.speaker }
            } else {
                StandDown::Stale { speaker: p.speaker }
            });
        }
        if self
            .manual_at_ms
            .is_some_and(|at| now_ms.saturating_sub(at) >= MANUAL_ARM_GRACE_MS)
        {
            self.manual_at_ms = None;
            return Some(StandDown::Manual);
        }
        None
    }

    /// A recruit window was captured — nothing is owed any more, by either
    /// trigger.
    pub fn disarm(&mut self) {
        self.probe = None;
        self.manual_at_ms = None;
    }

    /// Drop the voice-line slot, leaving a Scan now alone.
    ///
    /// The loop calls this on every tick where a capture is LIVE, and it closes
    /// a race rule 5 cannot. [`capture_held`] keeps a voice line from arming
    /// over a window that is on screen, but it reads the PUBLISHED status while
    /// the line arrives on the log-watcher thread: a line landing in the gap
    /// between the detect that captures and the publish that says so arms a
    /// gate the loop will then never probe, because a live capture's re-detect
    /// outranks the probe (`run::look_step`). Left alone, that gate reaches
    /// [`LINE_STALE_MS`] and prints a stand-down for a window the player is
    /// looking at.
    ///
    /// Only the probe slot, because the Scan now's grace is a promise to the
    /// PLAYER and a window on screen does not discharge it.
    pub fn disarm_probe(&mut self) {
        self.probe = None;
    }
}

// ---------------------------------------------------------------------------
// Glue — the same operations against `AppState`
// ---------------------------------------------------------------------------

/// Whether the merc module is switched on.
///
/// `modules_enabled` is the single owner of the effective flag and is safe to
/// take alone (lock order — see `modules.rs`).
fn module_enabled(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    let enabled = state
        .modules_enabled
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(MODULE_ID)
        == Some(&true);
    enabled
}

/// Announce a scan the trigger just armed, if the status lets it.
///
/// **The publish happens HERE, not on the loop's next tick.** Reported on the
/// 2026-08-25 smoke: the strip sat on "waiting for a mercenary" for seconds
/// after the voice line and then jumped to "reading", which reads as a module
/// that missed the trigger. The loop's own reconciliation is up to a
/// [`super::run::IDLE_NAP`] behind — and behind the focus gate, so a player who
/// arms a scan from our own window waits for their own alt-tab before anything
/// on screen changes. Arming is the event; this is where it is announced.
///
/// The speaker rides along so the strip can name who it heard. Both are written
/// only over the module's one RESTING status ([`scan_outranks`]): announcing a
/// scan over a window that is already on screen would take the verdict the
/// player is reading and mark it stale.
fn announce(app: &AppHandle, speaker: Option<String>) {
    publish(app, |slice| {
        if scan_outranks(slice.status) {
            slice.status = MercStatus::Scanning;
            slice.burst_speaker = speaker;
        }
    });
}

/// The Client.txt seam: one line in, a probe gate maybe armed.
///
/// Called for EVERY line the watcher reads, so the order of the checks is the
/// cost order — two string searches before any lock, and the timestamp is
/// parsed only for a line that has already passed both.
///
/// **Rule 5 lives here.** A voice line over a HELD capture is dropped with one
/// debug line and nothing else: no arm, no announcement, and — because the
/// loop's `resume` is now driven by Scan now alone — no resumed read either.
pub fn on_client_line(app: &AppHandle, line: &str, denylist: &NpcDenylist) {
    let Some(speaker) = trigger_speaker(line, denylist) else {
        return;
    };
    if !module_enabled(app) {
        return;
    }
    if capture_held(status(app)) {
        // Debug-gated, not silent-by-omission: a chattering mercenary standing
        // beside an open window produces one of these per line, and an app log
        // full of them would bury the lines that mean something.
        if super::run::debug_mode(app) {
            crate::app_log(
                app,
                format!("Merc: voice line while a capture is on screen — ignored — {speaker}"),
            );
        }
        return;
    }

    let at = arm_at(now_ms(), line_timestamp_ms(line));
    let fresh = {
        let state = app.state::<AppState>();
        let mut gate = state.merc_burst.lock().unwrap_or_else(|e| e.into_inner());
        gate.hear(speaker.to_string(), at)
    };
    announce(app, Some(speaker.to_string()));
    if fresh {
        crate::app_log(
            app,
            format!("Merc: heard {speaker} — probing for the recruit window"),
        );
    }
}

/// What the gate wants of this loop iteration.
pub fn step(app: &AppHandle, now_ms: u64) -> GateStep {
    let state = app.state::<AppState>();
    let step = state
        .merc_burst
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .step(now_ms);
    step
}

/// Who the armed line named, or `None`. See [`BurstGate::speaker`].
pub fn speaker(app: &AppHandle) -> Option<String> {
    let state = app.state::<AppState>();
    let speaker = state
        .merc_burst
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .speaker()
        .map(str::to_string);
    speaker
}

/// Report a gate that just gave up, once.
pub fn take_stood_down(app: &AppHandle, now_ms: u64) -> Option<StandDown> {
    let state = app.state::<AppState>();
    let stood_down = state
        .merc_burst
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take_stood_down(now_ms);
    stood_down
}

/// How many probes this arming has spent. See [`BurstGate::looks`].
pub fn looks(app: &AppHandle) -> u32 {
    let state = app.state::<AppState>();
    let looks = state
        .merc_burst
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .looks();
    looks
}

/// A probe ran — spend it.
pub fn note_probe(app: &AppHandle, now_ms: u64) {
    let state = app.state::<AppState>();
    state
        .merc_burst
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .note_probe(now_ms);
}

/// The Scan now got its full detect.
pub fn note_full_detect(app: &AppHandle) {
    let state = app.state::<AppState>();
    state
        .merc_burst
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .note_full_detect();
}

/// A recruit window was captured — nothing is owed.
pub fn disarm(app: &AppHandle) {
    let state = app.state::<AppState>();
    state
        .merc_burst
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .disarm();
}

/// A capture is live — drop the voice-line slot. See [`BurstGate::disarm_probe`].
pub fn disarm_probe(app: &AppHandle) {
    let state = app.state::<AppState>();
    state
        .merc_burst
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .disarm_probe();
}

/// Scan now: ask for one full detect (POE-198 AC 3).
///
/// Refuses loudly rather than arming a gate nothing will read: with the module
/// off there is no loop, and where capture is unavailable there is no OCR. A
/// button that silently does nothing is the failure this refusal replaces.
///
/// **It bypasses the probe gate entirely** (POE-204 WI-C). The gate exists to
/// decide whether a voice line is worth a look; a person pressing this button
/// has already decided, and the band probe could only turn their answer into a
/// stand-down. So the request is for the full detect the probe would have led
/// to, and it is served the first moment the game is in front — see
/// [`MANUAL_ARM_GRACE_MS`].
///
/// **Over a HELD capture it does exactly one thing: a re-detect off the
/// cadence** — cropped to the panel when one is known, which `run::detect_tick`
/// decides and this does not. The loop's `resume` puts a completed capture back
/// on the working cadence for it, and that is the whole of what "scan a window
/// that is already open" can mean.
///
/// **And in that case it is deliberately silent:** [`scan_outranks`] withholds
/// the `scanning` announcement while a window is on screen, so a player who
/// presses this without alt-tabbing sees no status change — the strip goes on
/// saying `done` until the re-detect publishes. It is accepted rather than
/// papered over: the alternative is announcing a scan over a verdict the player
/// is reading, which marks it stale and drops its glyph rows (the regression
/// this rule exists to prevent).
pub fn scan_now(app: &AppHandle) -> Result<(), String> {
    if !module_enabled(app) {
        return Err("Merc OCR is switched off — turn the module on first".to_string());
    }
    if status(app) == MercStatus::Unavailable {
        return Err("Merc OCR is unavailable on this machine".to_string());
    }
    let fresh = {
        let state = app.state::<AppState>();
        let mut gate = state.merc_burst.lock().unwrap_or_else(|e| e.into_inner());
        gate.request_full_detect(now_ms())
    };
    // Announced here, not on the loop's next tick, for the reason `announce`
    // states — and with no speaker, because Scan now heard nobody.
    announce(app, None);
    if fresh {
        crate::app_log(app, "Merc: Scan now — full detect requested".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real line, verbatim from the measurement session (2026-08-24 22:51).
    const MERC_LINE: &str =
        "2026/08/24 22:51:12 105432578 cffb0716 [INFO Client 12345] Nytra, the Cyaxan Loner: Keep walking.";

    #[test]
    fn speaker_of_reads_the_name_before_the_first_separator() {
        assert_eq!(speaker_of(MERC_LINE), Some("Nytra, the Cyaxan Loner"));
    }

    #[test]
    fn speaker_of_rejects_a_line_with_no_log_tag() {
        assert_eq!(speaker_of("Nytra, the Cyaxan Loner: Keep walking."), None);
    }

    #[test]
    fn speaker_of_rejects_a_whispering_player() {
        let line = "2026/08/24 22:51:12 105432578 cffb0716 [INFO Client 12345] @From Vertolka: Nytra, the Cyaxan Loner is cheap";
        assert_eq!(speaker_of(line), None);
    }

    #[test]
    fn speaker_of_rejects_a_guild_message() {
        let line = "2026/08/24 22:51:12 105432578 cffb0716 [INFO Client 12345] &<POE> Vertolka: hi";
        assert_eq!(speaker_of(line), None);
    }

    #[test]
    fn is_dialogue_speaker_accepts_a_one_word_name_and_an_epithet() {
        assert!(is_dialogue_speaker("Nytra, the Cyaxan Loner"));
    }

    /// MEASURED on the laptop Client.txt (2026-08-25): this is the line that
    /// showed the `, the ` rule never arming. `of` outnumbered `the` in that
    /// file's dialogue speakers.
    #[test]
    fn is_dialogue_speaker_accepts_an_of_joiner() {
        assert!(is_dialogue_speaker("Fennik, of Unshakeable Faith"));
    }

    /// The one comma-carrying NPC in the same file. `Master` is part of her
    /// NAME, not a joiner, and its capital is what says so — a rule that took
    /// any word there would arm on every line Alva speaks.
    #[test]
    fn is_dialogue_speaker_rejects_a_capitalised_joiner() {
        assert!(!is_dialogue_speaker("Alva, Master Explorer"));
    }

    #[test]
    fn is_dialogue_speaker_rejects_a_joiner_with_nothing_after_it() {
        assert!(!is_dialogue_speaker("Fennik, of"));
    }

    /// The name is `\S+`, not `[A-Za-z]+`. `Al-Hezmin, the Hunter` is a
    /// denylisted NPC, and a shape that rejected the hyphen would never reach
    /// the denylist to be suppressed by it — it would look like a name nobody
    /// had ever listed.
    #[test]
    fn is_dialogue_speaker_accepts_a_hyphenated_name() {
        assert!(is_dialogue_speaker("Al-Hezmin, the Hunter"));
    }

    #[test]
    fn is_dialogue_speaker_rejects_a_multi_word_name() {
        assert!(!is_dialogue_speaker("Some Guy, the Lout"));
    }

    #[test]
    fn is_dialogue_speaker_rejects_a_speaker_with_no_article() {
        assert!(!is_dialogue_speaker("Nytra the Lout"));
    }

    #[test]
    fn is_dialogue_speaker_rejects_an_empty_epithet() {
        assert!(!is_dialogue_speaker("Nytra, the "));
    }

    /// The fixture is embedded, so a truncated or unparsed file would leave an
    /// empty list that suppresses nothing and re-arms on every NPC line. The
    /// COUNT is not pinned — the list is meant to grow as more NPCs are met.
    #[test]
    fn the_shipped_denylist_parses_into_named_npcs() {
        let list = NpcDenylist::shipped();

        assert!(list.len() > 0, "the shipped fixture parsed into nothing");
        assert!(list.contains("Varashta, the Winter Sekhema"));
        assert!(list.contains("Tujen, the Haggler"));
        assert!(list.contains("Al-Hezmin, the Hunter"));
    }

    #[test]
    fn the_denylist_matches_an_npc_whatever_its_case_and_padding() {
        assert!(NpcDenylist::shipped().contains("  varashta, THE Winter Sekhema "));
    }

    #[test]
    fn the_denylist_does_not_match_a_mercenary() {
        assert!(!NpcDenylist::shipped().contains("Nytra, the Cyaxan Loner"));
    }

    /// The shape admits the hyphen (see `is_dialogue_speaker_accepts_a_
    /// hyphenated_name`); this is the other half — the denylist is what
    /// actually stops the Hunter, so the two must agree on the same string.
    #[test]
    fn a_hyphenated_npc_name_is_suppressed_by_the_shipped_list() {
        let line = "2026/08/25 19:04:31 105432578 cffb0716 [INFO Client 12345] Al-Hezmin, the Hunter: You will not escape.";
        assert!(is_dialogue_speaker("Al-Hezmin, the Hunter"));
        assert_eq!(trigger_speaker(line, &NpcDenylist::shipped()), None);
    }

    #[test]
    fn parsing_skips_comments_and_blank_lines() {
        let list = NpcDenylist::parse("# a comment\n\n  Vertolka, the Helper  \n");
        assert_eq!(list.len(), 1);
        assert!(list.contains("Vertolka, the Helper"));
    }

    #[test]
    fn extending_reports_only_the_names_that_were_new() {
        let mut list = NpcDenylist::shipped();
        let before = list.len();

        let added = list.extend_from("Zana, the Originator\nVertolka, the Helper\n");

        assert_eq!(added, 1);
        assert_eq!(list.len(), before + 1);
    }

    #[test]
    fn an_override_file_suppresses_a_speaker_the_shipped_list_never_heard_of() {
        let dir = std::env::temp_dir().join(format!("merc-denylist-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(dir.join(DENYLIST_OVERRIDE_FILE), "Vertolka, the Helper\n")
            .expect("write override");

        let (list, added, error) = NpcDenylist::load(Some(&dir));

        assert_eq!((added, error), (1, None));
        assert!(list.contains("Vertolka, the Helper"));
        assert!(list.contains("Zana, the Originator"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_missing_override_file_is_not_an_error() {
        let dir = std::env::temp_dir()
            .join(format!("merc-denylist-absent-{}", std::process::id()));
        let (list, added, error) = NpcDenylist::load(Some(&dir));

        assert_eq!((added, error), (0, None));
        assert_eq!(list, NpcDenylist::shipped());
    }

    #[test]
    fn a_mercenary_voice_line_names_its_speaker() {
        assert_eq!(
            trigger_speaker(MERC_LINE, &NpcDenylist::shipped()),
            Some("Nytra, the Cyaxan Loner")
        );
    }

    /// Both measured `of` mercenaries, one per machine — the laptop's and the
    /// PC's. Neither armed under the old `, the ` rule.
    #[test]
    fn a_mercenary_whose_title_uses_of_names_its_speaker() {
        for name in ["Fennik, of Unshakeable Faith", "Swain, of Fractured Faith"] {
            let line = format!(
                "2026/08/25 19:04:31 105432578 cffb0716 [INFO Client 12345] {name}: I am ready."
            );
            assert_eq!(
                trigger_speaker(&line, &NpcDenylist::shipped()),
                Some(name),
                "{name} must arm a burst"
            );
        }
    }

    /// Alva is not on the denylist and does not need to be: the shape rejects
    /// her, because her comma is followed by a capitalised name-part.
    #[test]
    fn an_npc_whose_comma_carries_a_name_and_not_a_title_triggers_nothing() {
        let line = "2026/08/25 19:04:31 105432578 cffb0716 [INFO Client 12345] Alva, Master Explorer: Another incursion awaits.";
        assert_eq!(trigger_speaker(line, &NpcDenylist::shipped()), None);
    }

    #[test]
    fn a_denylisted_npc_line_triggers_nothing() {
        let line = "2026/08/24 22:51:12 105432578 cffb0716 [INFO Client 12345] Varashta, the Winter Sekhema: Come closer.";
        assert_eq!(trigger_speaker(line, &NpcDenylist::shipped()), None);
    }

    #[test]
    fn a_player_quoting_a_mercenary_name_triggers_nothing() {
        let line = "2026/08/24 22:51:12 105432578 cffb0716 [INFO Client 12345] Vertolka: Nytra, the Cyaxan Loner is cheap";
        assert_eq!(trigger_speaker(line, &NpcDenylist::shipped()), None);
    }

    // -- the line's own clock ----------------------------------------------

    /// The gate's deadlines are 500 ms and 1.5 s. A watcher hop is up to 5 s
    /// (`log_watcher`'s `recv_timeout` fallback), so a gate measured from
    /// delivery would probe six seconds after the click that opened the window.
    /// The line carries the moment it was spoken; this is that read.
    #[test]
    fn a_client_line_is_stamped_with_the_moment_it_was_spoken() {
        use chrono::TimeZone;
        let expected = chrono::Local
            .with_ymd_and_hms(2026, 8, 24, 22, 51, 12)
            .earliest()
            .expect("22:51 on that date exists in every zone with a fixed offset");

        assert_eq!(
            line_timestamp_ms(MERC_LINE),
            u64::try_from(expected.timestamp_millis()).ok()
        );
    }

    /// A line whose prefix is not a timestamp must not resolve to SOME moment —
    /// a partial parse would hand `arm_at` a number to clamp instead of the
    /// `None` that means "use the delivery clock".
    #[test]
    fn a_line_with_no_timestamp_has_no_stamp() {
        assert_eq!(line_timestamp_ms("Nytra, the Cyaxan Loner: Keep walking."), None);
    }

    #[test]
    fn a_line_shorter_than_a_timestamp_has_no_stamp() {
        assert_eq!(line_timestamp_ms("2026/08/24"), None);
    }

    /// A zone with ONE fall-back transition, so the ambiguous hour can be
    /// written down instead of waited for. `chrono::Local` is the host's zone
    /// and is UTC on the machines this suite runs on — it has no ambiguous hour
    /// to offer, which is why the parse takes its zone as a parameter.
    ///
    /// 2026-10-25, +02:00 → +01:00 at 01:00 UTC. Local 02:00:00–02:59:59 that
    /// morning therefore happens twice: once at +02:00 (00:00–00:59 UTC) and
    /// again an hour later at +01:00.
    #[derive(Clone, Debug)]
    struct FallBackZone;

    impl FallBackZone {
        const SUMMER: i32 = 2 * 3600;
        const WINTER: i32 = 3600;

        fn summer() -> chrono::FixedOffset {
            chrono::FixedOffset::east_opt(Self::SUMMER).expect("+02:00 is a real offset")
        }

        fn winter() -> chrono::FixedOffset {
            chrono::FixedOffset::east_opt(Self::WINTER).expect("+01:00 is a real offset")
        }

        fn at(hour: u32, minute: u32) -> chrono::NaiveDateTime {
            chrono::NaiveDate::from_ymd_opt(2026, 10, 25)
                .expect("2026-10-25 is a real date")
                .and_hms_opt(hour, minute, 0)
                .expect("a real time of day")
        }
    }

    impl chrono::TimeZone for FallBackZone {
        type Offset = chrono::FixedOffset;

        fn from_offset(_: &chrono::FixedOffset) -> Self {
            FallBackZone
        }

        fn offset_from_local_date(
            &self,
            _: &chrono::NaiveDate,
        ) -> chrono::LocalResult<chrono::FixedOffset> {
            // A whole DAY that carries a transition has no single offset, and
            // nothing in this module asks for one.
            chrono::LocalResult::None
        }

        fn offset_from_local_datetime(
            &self,
            local: &chrono::NaiveDateTime,
        ) -> chrono::LocalResult<chrono::FixedOffset> {
            if *local < Self::at(2, 0) {
                chrono::LocalResult::Single(Self::summer())
            } else if *local < Self::at(3, 0) {
                // The larger offset FIRST: `from_local_datetime` subtracts it,
                // so +02:00 is the earlier of the two instants.
                chrono::LocalResult::Ambiguous(Self::summer(), Self::winter())
            } else {
                chrono::LocalResult::Single(Self::winter())
            }
        }

        fn offset_from_utc_date(&self, _: &chrono::NaiveDate) -> chrono::FixedOffset {
            Self::winter()
        }

        fn offset_from_utc_datetime(&self, utc: &chrono::NaiveDateTime) -> chrono::FixedOffset {
            if *utc < Self::at(1, 0) {
                Self::summer()
            } else {
                Self::winter()
            }
        }
    }

    /// The autumn hour that happens twice. PoE writes LOCAL time with no zone,
    /// so `02:30:00` on that morning names two instants an hour apart and the
    /// stamp cannot say which. The gate takes the EARLIER one and lets
    /// [`MAX_BACKDATE_MS`] decide whether to believe it: guessing the later one
    /// would put the line up to an hour in the FUTURE of a `now` that is
    /// actually after it, and [`arm_at`] answers a future stamp by throwing it
    /// away — the module would sit through the repeated hour answering no voice
    /// line at all.
    #[test]
    fn the_repeated_autumn_hour_resolves_to_the_earlier_of_its_two_instants() {
        let line = "2026/10/25 02:30:00 105432578 cffb0716 [INFO Client 12345] Arith, the Quickshot: Keep walking.";
        // 02:30 local at +02:00 — the first pass through the repeated hour.
        let earlier = FallBackZone::at(0, 30).and_utc().timestamp_millis();

        assert_eq!(
            stamp_ms_in(&FallBackZone, line),
            u64::try_from(earlier).ok(),
        );
    }

    /// The other half of the same rule: an unambiguous stamp in the same zone
    /// resolves normally, so the test above is pinning `.earliest()`'s choice
    /// and not some blanket "always the summer offset".
    #[test]
    fn a_stamp_outside_the_repeated_hour_takes_the_zones_one_answer() {
        let line = "2026/10/25 04:30:00 105432578 cffb0716 [INFO Client 12345] Arith, the Quickshot: Keep walking.";
        // 04:30 local at +01:00 — after the transition, one answer only.
        let only = FallBackZone::at(3, 30).and_utc().timestamp_millis();

        assert_eq!(stamp_ms_in(&FallBackZone, line), u64::try_from(only).ok());
    }

    /// The delay this exists for: the line is three seconds old by the time the
    /// watcher hands it over, so the gate starts three seconds back and both
    /// probes are already due.
    #[test]
    fn a_late_delivery_backdates_the_gate_to_the_line() {
        assert_eq!(arm_at(100_000, Some(97_000)), 97_000);
    }

    /// A stamp from BEFORE the believable band is a clock that disagrees, not a
    /// slow watcher. Backdating an hour would spend both probes at once and
    /// stand down before the player had finished clicking.
    #[test]
    fn a_stamp_older_than_the_backdate_cap_is_not_believed() {
        assert_eq!(arm_at(100_000, Some(100_000 - MAX_BACKDATE_MS - 1)), 100_000);
    }

    /// The boundary itself is inside the band.
    #[test]
    fn a_stamp_exactly_at_the_backdate_cap_is_believed() {
        let line = 100_000 - MAX_BACKDATE_MS;

        assert_eq!(arm_at(100_000, Some(line)), line);
    }

    /// The fatal direction, and the reason the clamp is two-sided: a stamp in
    /// the FUTURE would leave `probe_due` false for as long as the skew lasts,
    /// and the module would answer no voice line at all.
    #[test]
    fn a_stamp_in_the_future_is_not_believed() {
        assert_eq!(arm_at(100_000, Some(100_001)), 100_000);
    }

    #[test]
    fn an_unreadable_stamp_falls_back_to_the_delivery_clock() {
        assert_eq!(arm_at(100_000, None), 100_000);
    }

    // -- rule 5: a held capture ignores the line ---------------------------

    /// A window on screen is being read; a voice line over it can only make the
    /// loop re-detect a panel the player's cursor is over. MEASURED 2026-08-26
    /// 09:41:56 — the arm resumed the paused read, the resumed cadence spent
    /// its first tick on a tooltip-covered frame, and "window gone" was
    /// published for a window that was plainly open.
    #[test]
    fn a_voice_line_is_ignored_while_a_capture_is_held() {
        assert!(capture_held(MercStatus::Live));
        assert!(capture_held(MercStatus::Done), "a paused read still owns the window");
    }

    /// The states that are NOT a held capture, each for its own reason: `Idle`
    /// and `Scanning` have no window, and `Off`/`Unavailable` have no loop —
    /// treating those last two as held would make the module unarmable if it
    /// were ever switched back on without the status being republished first.
    #[test]
    fn every_other_status_leaves_the_line_free_to_arm() {
        for status in [
            MercStatus::Idle,
            MercStatus::Scanning,
            MercStatus::Off,
            MercStatus::Unavailable,
        ] {
            assert!(!capture_held(status), "{status:?} holds no capture");
        }
    }

    // -- the gate's timing --------------------------------------------------

    /// The four numbers, in the order the design needs them, with the reason
    /// each ordering is load-bearing. A test on the CONSTANTS because the rest
    /// of this section is written against their relationships and would go on
    /// passing — silently testing a different design — if one of them moved.
    ///
    /// - `PROBE_DELAY_MS < PROBE_RETRY_MS`, or there is no retry: both probes
    ///   come due together and the lag allowance buys nothing;
    /// - `PROBE_RETRY_MS < LINE_STALE_MS`, or the gate expires before it has
    ///   spent the probes it was armed with, and every ordinary stand-down
    ///   reports as [`StandDown::Stale`] — a focus problem that is not there;
    /// - `LINE_STALE_MS <= MAX_BACKDATE_MS`, or a line whose stamp [`arm_at`]
    ///   still believes can already be stale on arrival: the gate would arm and
    ///   stand down on the same tick, spending no probe and logging a
    ///   stand-down for a window nobody looked for.
    #[test]
    fn the_gates_four_clocks_stay_in_the_order_the_design_needs() {
        assert_eq!((PROBE_DELAY_MS, PROBE_RETRY_MS), (500, 1_500));
        assert_eq!((LINE_STALE_MS, MAX_BACKDATE_MS), (10_000, 10_000));

        assert!(PROBE_DELAY_MS < PROBE_RETRY_MS);
        assert!(PROBE_RETRY_MS < LINE_STALE_MS);
        assert!(LINE_STALE_MS <= MAX_BACKDATE_MS);
        assert_eq!(PROBE_GAP_MS, PROBE_RETRY_MS - PROBE_DELAY_MS);
    }

    const LINE: u64 = 1_000;

    fn heard() -> BurstGate {
        let mut gate = BurstGate::default();
        gate.hear("Arith, the Quickshot".into(), LINE);
        gate
    }

    /// Nothing looks before the delay. The window is not on screen when the
    /// line lands — the click that opens it is what fires the line.
    #[test]
    fn nothing_probes_before_the_delay() {
        let gate = heard();

        assert_eq!(gate.step(LINE), GateStep::Waiting);
        assert_eq!(gate.step(LINE + PROBE_DELAY_MS - 1), GateStep::Waiting);
    }

    #[test]
    fn the_first_probe_is_due_at_the_delay() {
        assert_eq!(heard().step(LINE + PROBE_DELAY_MS), GateStep::Probe);
    }

    /// The retry is measured from the LINE, not from the probe that preceded
    /// it: a probe that ran late must not push the retry later still.
    #[test]
    fn the_retry_waits_until_a_second_and_a_half_after_the_line() {
        let mut gate = heard();
        gate.note_probe(LINE + PROBE_DELAY_MS);

        assert_eq!(gate.step(LINE + PROBE_RETRY_MS - 1), GateStep::Waiting);
        assert_eq!(gate.step(LINE + PROBE_RETRY_MS), GateStep::Probe);
    }

    /// Two probes and no more. A third would be the burst coming back.
    #[test]
    fn a_third_probe_is_never_due() {
        let mut gate = heard();
        gate.note_probe(LINE + PROBE_DELAY_MS);
        gate.note_probe(LINE + PROBE_RETRY_MS);

        assert_eq!(gate.step(LINE + PROBE_RETRY_MS + 10_000), GateStep::Waiting);
    }

    /// …because the gate has already given up by then. The stand-down is what
    /// clears it, and it names the mercenary the player walked past.
    #[test]
    fn two_spent_probes_stand_the_gate_down() {
        let mut gate = heard();
        gate.note_probe(LINE + PROBE_DELAY_MS);
        gate.note_probe(LINE + PROBE_RETRY_MS);

        assert_eq!(
            gate.take_stood_down(LINE + PROBE_RETRY_MS),
            Some(StandDown::Probed { speaker: "Arith, the Quickshot".into() })
        );
        assert_eq!(gate.step(LINE + PROBE_RETRY_MS), GateStep::Resting);
    }

    /// Reported exactly once — the gate is cleared by the report, so a loop
    /// that asks every 100 ms does not print a line every 100 ms.
    #[test]
    fn a_stand_down_is_reported_once() {
        let mut gate = heard();
        gate.note_probe(LINE + PROBE_DELAY_MS);
        gate.note_probe(LINE + PROBE_RETRY_MS);
        gate.take_stood_down(LINE + PROBE_RETRY_MS);

        assert_eq!(gate.take_stood_down(LINE + PROBE_RETRY_MS), None);
    }

    #[test]
    fn a_gate_with_probes_left_has_not_stood_down() {
        let mut gate = heard();
        gate.note_probe(LINE + PROBE_DELAY_MS);

        assert_eq!(gate.take_stood_down(LINE + PROBE_DELAY_MS), None);
    }

    /// A gate that looked ONCE and then lost the foreground reached no verdict
    /// about the screen. Reporting it as `Probed` — "no recruit window after
    /// X" — would claim one, and send the reader after a detection bug when the
    /// problem is that the game was behind another window.
    #[test]
    fn one_probe_and_then_a_stale_chain_is_not_a_probed_verdict() {
        let mut gate = heard();
        gate.note_probe(LINE + PROBE_DELAY_MS);

        assert_eq!(
            gate.take_stood_down(LINE + LINE_STALE_MS),
            Some(StandDown::Stale { speaker: "Arith, the Quickshot".into() })
        );
    }

    /// The probes only fire while the game is in front, so a line heard as the
    /// player alt-tabs never gets its look. Without the stale bound the gate
    /// would sit armed until they came back and then probe for a window that
    /// closed with the alt-tab.
    #[test]
    fn an_unprobed_line_goes_stale() {
        let mut gate = heard();

        assert_eq!(gate.take_stood_down(LINE + LINE_STALE_MS - 1), None);
        assert_eq!(
            gate.take_stood_down(LINE + LINE_STALE_MS),
            Some(StandDown::Stale { speaker: "Arith, the Quickshot".into() })
        );
    }

    /// The two endings are worded apart. "No recruit window" would blame the
    /// OCR for a gate the game never let finish, and send the reader hunting a
    /// detection bug that is not there.
    #[test]
    fn a_probed_stand_down_names_the_mercenary_and_nothing_else() {
        assert_eq!(
            StandDown::Probed { speaker: "Arith, the Quickshot".into() }.line(),
            "Merc: no recruit window after Arith, the Quickshot — standing down"
        );
    }

    #[test]
    fn a_stale_stand_down_says_the_game_was_not_in_front() {
        assert_eq!(
            StandDown::Stale { speaker: "Arith, the Quickshot".into() }.line(),
            "Merc: no recruit window after Arith, the Quickshot — standing down \
             (the game was not in front for its remaining probes)"
        );
    }

    /// A gate that could not probe on time does not get its 500 ms back: it
    /// finds the first probe overdue and looks the instant the game returns.
    #[test]
    fn a_gate_that_missed_its_deadlines_probes_the_moment_the_game_returns() {
        let gate = heard();
        let back = LINE + PROBE_RETRY_MS + 2_000;

        assert_eq!(gate.step(back), GateStep::Probe);
    }

    /// …and then WAITS. Both deadlines are past by the time the player
    /// alt-tabs back, and spending the retry on the same frame is one probe's
    /// worth of evidence at two probes' cost — the retry exists to re-read a
    /// frame that arrived mid-animation, which needs the animation to have had
    /// time to finish.
    #[test]
    fn a_gate_that_missed_its_deadlines_still_leaves_a_second_between_its_probes() {
        let mut gate = heard();
        let back = LINE + PROBE_RETRY_MS + 2_000;
        gate.note_probe(back);

        assert_eq!(gate.step(back), GateStep::Waiting);
        assert_eq!(gate.step(back + PROBE_GAP_MS - 1), GateStep::Waiting);
        assert_eq!(gate.step(back + PROBE_GAP_MS), GateStep::Probe);
    }

    /// The click line lands on top of the approach line, and it is the click
    /// that opens a window — so a second line RE-ARMS both probes from its own
    /// clock rather than being answered out of what the first had left.
    #[test]
    fn a_second_line_re_arms_both_probes_from_its_own_clock() {
        let mut gate = heard();
        gate.note_probe(LINE + PROBE_DELAY_MS);

        assert!(!gate.hear("Arith, the Quickshot".into(), LINE + 400), "not a new arm to log");
        assert_eq!(gate.step(LINE + PROBE_DELAY_MS), GateStep::Waiting);
        assert_eq!(gate.step(LINE + 400 + PROBE_DELAY_MS), GateStep::Probe);
        assert_eq!(
            gate.step(LINE + 400 + PROBE_RETRY_MS),
            GateStep::Probe,
            "and the retry with them",
        );
    }

    /// The re-arm must not hand the probe its remembered band back. `fired`
    /// resets on every line, so a band keyed on THAT would sit on the last
    /// panel's rect for as long as the mercenary kept talking — and the one
    /// failure the remembered band has (the player moved the window) is silent.
    #[test]
    fn chatter_cannot_pin_the_probe_to_the_remembered_band() {
        let mut gate = heard();
        gate.note_probe(LINE + PROBE_DELAY_MS);

        gate.hear("Arith, the Quickshot".into(), LINE + 400);

        assert_eq!(gate.looks(), 1, "the look is spent whichever line bought it");
    }

    /// …and the chain is what runs out. Each line re-arms the probes, so the
    /// stale clock has to run from the FIRST line of the chain or a mercenary
    /// who speaks every second would hold the gate open indefinitely — the
    /// re-armable expiry POE-198's burst had.
    #[test]
    fn an_unbroken_chain_of_lines_still_stands_down_at_the_stale_bound() {
        let mut gate = heard();

        for t in (400..LINE_STALE_MS).step_by(400) {
            gate.hear("Arith, the Quickshot".into(), LINE + t);
        }

        assert_eq!(
            gate.take_stood_down(LINE + LINE_STALE_MS),
            Some(StandDown::Stale { speaker: "Arith, the Quickshot".into() }),
            "ten seconds after the FIRST line, not after the latest one",
        );
    }

    /// A different mercenary is a different click, so it starts its own ten
    /// seconds rather than inheriting a chain it had no part in.
    #[test]
    fn a_different_speaker_starts_the_stale_clock_over() {
        let mut gate = heard();

        gate.hear("Fennik, of Unshakeable Faith".into(), LINE + LINE_STALE_MS - 1);

        assert_eq!(gate.take_stood_down(LINE + LINE_STALE_MS), None);
        assert_eq!(
            gate.take_stood_down(LINE + LINE_STALE_MS - 1 + LINE_STALE_MS),
            Some(StandDown::Stale { speaker: "Fennik, of Unshakeable Faith".into() }),
        );
    }

    /// A live capture takes the voice-line slot away and leaves the Scan now
    /// alone. `capture_held` reads the PUBLISHED status off another thread, so
    /// a line landing between the capturing detect and its publish arms a gate
    /// the loop will never probe — and ten seconds later that gate prints a
    /// stand-down for a window the player is looking at.
    #[test]
    fn a_live_capture_takes_the_probe_slot_and_leaves_the_scan_now() {
        let mut gate = heard();
        gate.request_full_detect(LINE);

        gate.disarm_probe();

        assert_eq!(gate.take_stood_down(LINE + LINE_STALE_MS), None, "no phantom stand-down");
        assert_eq!(gate.step(LINE), GateStep::FullDetect, "the button is still owed");
    }

    #[test]
    fn hearing_a_line_over_a_resting_gate_reports_a_new_arm() {
        let mut gate = BurstGate::default();

        assert!(gate.hear("Arith, the Quickshot".into(), LINE));
    }

    #[test]
    fn a_resting_gate_asks_for_nothing() {
        assert_eq!(BurstGate::default().step(LINE), GateStep::Resting);
        assert_eq!(BurstGate::default().take_stood_down(LINE + 60_000), None);
    }

    /// The strip names who the module heard. It comes off the gate rather than
    /// being remembered, so it cannot outlive the line it belongs to.
    #[test]
    fn the_gate_names_the_speaker_it_is_probing_for() {
        let mut gate = heard();

        assert_eq!(gate.speaker(), Some("Arith, the Quickshot"));

        gate.note_probe(LINE + PROBE_DELAY_MS);
        gate.note_probe(LINE + PROBE_RETRY_MS);
        gate.take_stood_down(LINE + PROBE_RETRY_MS);
        assert_eq!(gate.speaker(), None, "a gate that stood down names nobody");
    }

    #[test]
    fn a_scan_now_heard_nobody() {
        let mut gate = BurstGate::default();
        gate.request_full_detect(LINE);

        assert_eq!(gate.speaker(), None);
    }

    /// The probe band widens on the second look, and this is the number it
    /// keys on.
    #[test]
    fn the_gate_counts_the_looks_it_has_spent() {
        let mut gate = heard();

        assert_eq!(gate.looks(), 0);
        gate.note_probe(LINE + PROBE_DELAY_MS);
        assert_eq!(gate.looks(), 1);
    }

    #[test]
    fn a_resting_gate_has_spent_no_looks() {
        assert_eq!(BurstGate::default().looks(), 0);
    }

    /// The counter is per ARMING, not per session: a gate that stood down and
    /// was armed again gets the remembered band back for its first look. It is
    /// the cheap one — 7% of the screen against the default's 55% — and a
    /// window reopens where it opened last.
    #[test]
    fn a_gate_armed_afresh_starts_its_looks_over() {
        let mut gate = heard();
        gate.note_probe(LINE + PROBE_DELAY_MS);
        gate.note_probe(LINE + PROBE_RETRY_MS);
        gate.take_stood_down(LINE + PROBE_RETRY_MS);

        gate.hear("Arith, the Quickshot".into(), LINE + 30_000);

        assert_eq!(gate.looks(), 0);
    }

    // -- Scan now bypasses the gate ----------------------------------------

    /// The button asks for the detect the probe would have led to. Running the
    /// band first could only turn a person's answer into a stand-down.
    #[test]
    fn scan_now_asks_for_a_full_detect_immediately() {
        let mut gate = BurstGate::default();

        assert!(gate.request_full_detect(LINE));
        assert_eq!(gate.step(LINE), GateStep::FullDetect);
    }

    /// No 500 ms wait, and no probe on the way: the two things the gate does to
    /// a voice line are exactly what the button skips.
    #[test]
    fn scan_now_never_probes() {
        let mut gate = BurstGate::default();
        gate.request_full_detect(LINE);

        for at in [LINE, LINE + PROBE_DELAY_MS, LINE + PROBE_RETRY_MS] {
            assert_eq!(gate.step(at), GateStep::FullDetect);
        }
    }

    /// One detect, not a hunt. Leaving the request armed after it was served
    /// would keep the loop detecting for the whole 60 s grace.
    #[test]
    fn a_served_scan_now_asks_for_nothing_more() {
        let mut gate = BurstGate::default();
        gate.request_full_detect(LINE);

        gate.note_full_detect();

        assert_eq!(gate.step(LINE), GateStep::Resting);
    }

    #[test]
    fn a_second_click_before_the_first_is_served_is_not_a_new_arm() {
        let mut gate = BurstGate::default();
        gate.request_full_detect(LINE);

        assert!(!gate.request_full_detect(LINE + 100));
    }

    /// Scan now is clicked in OUR window, so the game is not in front and the
    /// loop is napping. The request waits for the alt-tab rather than being
    /// spent on a screen that is showing this app.
    #[test]
    fn a_scan_now_waits_for_the_game_rather_than_giving_up() {
        let mut gate = BurstGate::default();
        gate.request_full_detect(LINE);

        assert_eq!(gate.take_stood_down(LINE + MANUAL_ARM_GRACE_MS - 1), None);
        assert_eq!(gate.step(LINE + MANUAL_ARM_GRACE_MS - 1), GateStep::FullDetect);
    }

    #[test]
    fn a_scan_now_that_never_sees_the_game_gives_up() {
        let mut gate = BurstGate::default();
        gate.request_full_detect(LINE);

        assert_eq!(gate.take_stood_down(LINE + MANUAL_ARM_GRACE_MS), Some(StandDown::Manual));
        assert_eq!(gate.step(LINE + MANUAL_ARM_GRACE_MS), GateStep::Resting);
    }

    #[test]
    fn a_scan_now_that_gave_up_says_why() {
        assert_eq!(
            StandDown::Manual.line(),
            "Merc: Scan now gave up — the game never came to the foreground"
        );
    }

    /// A person asking outranks a probe that has not run yet: the full detect
    /// answers the probe's question as a side effect, and running the band
    /// first would only spend a probe on it.
    #[test]
    fn scan_now_outranks_a_probe_that_is_due() {
        let mut gate = heard();
        gate.request_full_detect(LINE + PROBE_DELAY_MS);

        assert_eq!(gate.step(LINE + PROBE_DELAY_MS), GateStep::FullDetect);
    }

    /// The two slots are independent, which is what stops a Scan now silently
    /// eating the voice line's remaining probe.
    #[test]
    fn a_served_scan_now_hands_the_loop_back_to_the_waiting_probe() {
        let mut gate = heard();
        gate.request_full_detect(LINE);

        gate.note_full_detect();

        assert_eq!(gate.step(LINE + PROBE_DELAY_MS), GateStep::Probe);
    }

    /// A captured window ends both promises at once. A Scan now still owed
    /// after the detect that found the window would buy a second one.
    #[test]
    fn a_captured_window_disarms_both_slots() {
        let mut gate = heard();
        gate.request_full_detect(LINE);

        gate.disarm();

        assert_eq!(gate.step(LINE + PROBE_RETRY_MS), GateStep::Resting);
        assert_eq!(gate.take_stood_down(LINE + LINE_STALE_MS), None);
    }

    // -- the announcement ---------------------------------------------------

    #[test]
    fn arming_a_scan_only_announces_itself_over_a_resting_module() {
        assert!(scan_outranks(MercStatus::Idle));
        assert!(!scan_outranks(MercStatus::Live));
        // The one that bit: `done` is a window ON SCREEN. The overlay marks any
        // capture outside live/done as a previous read, so announcing a scan
        // here would mark the verdict the player is reading stale and drop its
        // glyph rows. The loop still runs the detect; only the announcement is
        // withheld.
        assert!(!scan_outranks(MercStatus::Done));
        assert!(!scan_outranks(MercStatus::Scanning), "a re-arm changes nothing on the strip");
        assert!(!scan_outranks(MercStatus::Off));
        assert!(!scan_outranks(MercStatus::Unavailable));
    }
}
