//! What starts a capture burst, and for how long (POE-198).
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
//! Either arms a [`BurstGate`] for [`BURST_TTL_MS`]. While it is armed and the
//! game is the foreground window the loop detects on its usual cadence; the
//! first detected window disarms it and the pre-existing live behaviour takes
//! over unchanged (re-detect 2 s, hover 400 ms, retire after two misses). A
//! burst that finds nothing expires, says so once, and the loop goes back to
//! doing nothing at all.
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
//! A false trigger costs at most one burst of detect ticks and is invisible
//! (nothing is found, nothing is published). A missed trigger costs the capture
//! entirely — which is what Scan now exists for. The bias is deliberate: the
//! cheap failure is the one this file prefers.
//!
//! # Pure vs glue
//!
//! Everything that decides — the speaker parse, the denylist, the burst state
//! machine — is a plain function over plain data and is tested here on Linux.
//! The `AppHandle` wrappers at the bottom only lock, log and publish.

use std::collections::HashSet;
use std::path::Path;

use tauri::{AppHandle, Manager};

use super::run::{now_ms, publish, status};
use super::{MercStatus, DENYLIST_OVERRIDE_FILE, MODULE_ID};
use crate::AppState;

/// The 25 measured NPC speakers. See `assets/README.md` for provenance.
const SHIPPED_DENYLIST: &str = include_str!("assets/npc-denylist.txt");

/// How long a burst keeps looking, once it has started looking.
///
/// For a Client.txt burst the clock starts when the line is DELIVERED, not when
/// the game returns to the foreground: a player who alt-tabs to read the page
/// and comes back within the window still gets the capture, and one who wanders
/// off does not come back to a loop that has been OCR'ing the screen since.
///
/// MEASURED, and the reason the wording is "delivered" rather than "the line":
/// the module-level `arm` stamps the gate with `now_ms()` — the wall clock at
/// the moment the watcher hands this module the line. The `[INFO Client …]`
/// timestamp the line itself carries is never read; nothing on this path parses
/// it. So the TTL actually begins one watcher hop late: `log_watcher` blocks on
/// a notify event with a 5 s `recv_timeout` fallback, so a line that arrives
/// without a filesystem event costs up to that whole fallback before the burst
/// is armed.
///
/// Accepted, not worked around. Reading the file's own timestamp would mean
/// parsing a local-time string with no zone against a clock that may not agree
/// with it — and getting that wrong shortens or lengthens every burst silently,
/// which is worse than a delay bounded by a constant one file away. The TTL is
/// sized with that hop inside it.
pub const BURST_TTL_MS: u64 = 10_000;

/// How long a Scan-now burst waits for the game before giving up.
///
/// Scan now is clicked in OUR window, which by definition means the game is not
/// the foreground window and the capture loop is napping — so a manual burst
/// whose TTL started at the click would burn its whole window before the player
/// could alt-tab, and every manual scan would expire empty. Its clock therefore
/// starts at the first moment the game IS in front ([`BurstGate::begin`]), and
/// this bounds the wait so a click that is never followed by an alt-tab does
/// not leave the module armed for the rest of the session.
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
// Pure — the burst state machine
// ---------------------------------------------------------------------------

/// What armed a burst. Logged, so the two triggers can be told apart when a
/// capture goes wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BurstSource {
    /// A mercenary's voice line in Client.txt.
    ClientTxt,
    /// The page's Scan now button.
    Manual,
}

impl BurstSource {
    pub fn label(self) -> &'static str {
        match self {
            BurstSource::ClientTxt => "client-txt",
            BurstSource::Manual => "manual",
        }
    }
}

/// One armed burst.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArmedBurst {
    pub source: BurstSource,
    /// The speaker that armed it — `None` for Scan now, which names nobody.
    pub speaker: Option<String>,
    /// When the trigger fired.
    pub armed_at_ms: u64,
    /// When the TTL started running. `None` while a Scan-now burst is still
    /// waiting for the game to come to the foreground; a Client.txt burst is
    /// started at its line.
    pub started_at_ms: Option<u64>,
    /// Whether a detect tick has actually run under this burst. The difference
    /// between "looked and found nothing" and "never got to look", which are
    /// two different things to fix.
    pub looked: bool,
}

impl ArmedBurst {
    /// The burst in words, for the two log lines it produces.
    pub fn describe(&self) -> String {
        match &self.speaker {
            Some(speaker) => format!("{speaker} ({})", self.source.label()),
            None => format!("({})", self.source.label()),
        }
    }

    /// What to log when this burst runs out.
    ///
    /// A burst that never looked says so: "no recruit window" would blame the
    /// OCR for a burst the game never let run, and send the reader hunting for
    /// a detection bug that is not there.
    pub fn expiry_line(&self) -> String {
        if self.looked {
            format!(
                "Merc: OCR burst expired with no recruit window — {}",
                self.describe()
            )
        } else {
            format!(
                "Merc: OCR burst expired without ever looking — game not in the foreground — {}",
                self.describe()
            )
        }
    }

    fn expired(&self, now_ms: u64) -> bool {
        match self.started_at_ms {
            Some(started) => now_ms.saturating_sub(started) >= BURST_TTL_MS,
            None => now_ms.saturating_sub(self.armed_at_ms) >= MANUAL_ARM_GRACE_MS,
        }
    }
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
/// per-row glyphs under it. The burst still does its work — the capture loop
/// resumes the paused read the moment it sees the armed gate
/// (`run::LoopState::resume`) and republishes `live`/`done` from the detect.
/// Only the ANNOUNCEMENT is withheld, because there is nothing to announce that
/// the window on screen does not already say.
pub fn scan_outranks(status: MercStatus) -> bool {
    matches!(status, MercStatus::Idle)
}

/// The gate the capture loop asks "should I be looking at all?".
///
/// Three transitions, and the log lines hang off two of them: arming an idle
/// gate says so once (a chattering mercenary re-arms without a second line),
/// and expiring says so once. A DETECTED window takes the burst instead of
/// expiring it, which is what keeps "burst expired with no recruit window" an
/// honest statement rather than a line printed after every successful capture.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct BurstGate {
    armed: Option<ArmedBurst>,
}

impl BurstGate {
    /// Arm the gate, or re-arm an already-armed one.
    ///
    /// `true` when this started a burst — the caller logs on `true` only.
    /// Re-arming an ACTIVE burst extends it (a mercenary who is still talking
    /// is still standing there) and says nothing.
    pub fn arm(&mut self, source: BurstSource, speaker: Option<String>, now_ms: u64) -> bool {
        let fresh = !self.scanning(now_ms);
        self.armed = Some(ArmedBurst {
            source,
            speaker,
            armed_at_ms: now_ms,
            // A voice line is evidence about the game's screen right now, so its
            // clock runs from the line. Scan now is a click in our own window
            // and evidence of nothing yet, so its clock waits for `begin`.
            started_at_ms: match source {
                BurstSource::ClientTxt => Some(now_ms),
                BurstSource::Manual => None,
            },
            looked: false,
        });
        fresh
    }

    /// The game is the foreground window: start the clock of a burst that was
    /// waiting for it. A burst already running keeps the clock it has — this
    /// must never extend a Client.txt burst, whose whole point is that it is
    /// measured from the line.
    pub fn begin(&mut self, now_ms: u64) {
        if let Some(burst) = self.armed.as_mut() {
            if burst.started_at_ms.is_none() {
                burst.started_at_ms = Some(now_ms);
            }
        }
    }

    /// Record that a detect tick ran under the armed burst.
    pub fn note_looked(&mut self) {
        if let Some(burst) = self.armed.as_mut() {
            burst.looked = true;
        }
    }

    /// Whether a burst is armed and still inside its window.
    pub fn scanning(&self, now_ms: u64) -> bool {
        self.armed.as_ref().is_some_and(|b| !b.expired(now_ms))
    }

    /// Who the LIVE burst heard, for the strip to name.
    ///
    /// Gated on the same expiry [`Self::scanning`] uses, so a name can never
    /// outlive the scan it belongs to: the gate holds an expired burst until
    /// something takes it, and reading the speaker straight off `armed` would
    /// print that dead burst's mercenary. `None` for a Scan-now burst, which
    /// heard nobody.
    pub fn speaker(&self, now_ms: u64) -> Option<&str> {
        self.armed
            .as_ref()
            .filter(|b| !b.expired(now_ms))
            .and_then(|b| b.speaker.as_deref())
    }

    /// Take the burst if its window has closed. Returning it CLEARS it, so the
    /// expiry is reported exactly once.
    pub fn take_expired(&mut self, now_ms: u64) -> Option<ArmedBurst> {
        if self.armed.as_ref().is_some_and(|b| b.expired(now_ms)) {
            return self.armed.take();
        }
        None
    }

    /// Disarm — a recruit window was found, so the burst is over.
    pub fn take(&mut self) -> Option<ArmedBurst> {
        self.armed.take()
    }
}

// ---------------------------------------------------------------------------
// Glue — the same three operations against `AppState`
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

/// Arm a burst, publish the scan it started, and log it if it started one.
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
fn arm(app: &AppHandle, source: BurstSource, speaker: Option<String>) {
    let armed = {
        let state = app.state::<AppState>();
        let mut gate = state.merc_burst.lock().unwrap_or_else(|e| e.into_inner());
        let fresh = gate.arm(source, speaker.clone(), now_ms());
        match (fresh, gate.armed.as_ref()) {
            (true, Some(burst)) => Some(burst.describe()),
            _ => None,
        }
    };
    publish(app, |slice| {
        if scan_outranks(slice.status) {
            slice.status = MercStatus::Scanning;
            slice.burst_speaker = speaker;
        }
    });
    if let Some(what) = armed {
        crate::app_log(app, format!("Merc: OCR burst armed — {what}"));
    }
}

/// The Client.txt seam: one line in, a burst maybe armed.
///
/// Called for EVERY line the watcher reads, so the order of the checks is the
/// cost order — two string searches before any lock.
pub fn on_client_line(app: &AppHandle, line: &str, denylist: &NpcDenylist) {
    let Some(speaker) = trigger_speaker(line, denylist) else {
        return;
    };
    if !module_enabled(app) {
        return;
    }
    arm(app, BurstSource::ClientTxt, Some(speaker.to_string()));
}

/// Whether the loop should be running detect ticks right now.
pub fn scanning(app: &AppHandle, now_ms: u64) -> bool {
    let state = app.state::<AppState>();
    let scanning = state
        .merc_burst
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .scanning(now_ms);
    scanning
}

/// Who the armed burst heard, or `None` when nothing is armed or it heard
/// nobody. See [`BurstGate::speaker`].
pub fn speaker(app: &AppHandle, now_ms: u64) -> Option<String> {
    let state = app.state::<AppState>();
    let speaker = state
        .merc_burst
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .speaker(now_ms)
        .map(str::to_string);
    speaker
}

/// Report an expired burst, once. `Some` only when it found nothing.
pub fn take_expired(app: &AppHandle, now_ms: u64) -> Option<ArmedBurst> {
    let state = app.state::<AppState>();
    let expired = state
        .merc_burst
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take_expired(now_ms);
    expired
}

/// The game is in front and the loop is about to work: start the clock of a
/// burst that was waiting for exactly that.
pub fn begin(app: &AppHandle, now_ms: u64) {
    let state = app.state::<AppState>();
    state
        .merc_burst
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .begin(now_ms);
}

/// A detect tick ran — the burst got its look.
pub fn note_looked(app: &AppHandle) {
    let state = app.state::<AppState>();
    state
        .merc_burst
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .note_looked();
}

/// A recruit window was captured — the burst is over.
pub fn disarm(app: &AppHandle) {
    let state = app.state::<AppState>();
    state
        .merc_burst
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
}

/// Scan now: arm one burst on demand (POE-198 AC 3).
///
/// Refuses loudly rather than arming a gate nothing will read: with the module
/// off there is no loop, and where capture is unavailable there is no OCR. A
/// button that silently does nothing is the failure this refusal replaces.
///
/// The burst it arms does NOT start counting here — clicking this button put
/// our own window in front, so the loop is napping on the focus gate. See
/// [`MANUAL_ARM_GRACE_MS`].
///
/// **One case is deliberately silent: a scan asked for over a `Done` capture.**
/// [`scan_outranks`] withholds the `scanning` announcement while a window is on
/// screen, so a player who presses this without alt-tabbing sees no status
/// change — the strip goes on saying `done` until the resumed read publishes,
/// which needs the game in front like every other detect. It is accepted rather
/// than papered over: the alternative is announcing a scan over a verdict the
/// player is reading, which marks it stale and drops its glyph rows (the
/// regression this rule exists to prevent). The button did arm the burst, and
/// the capture on screen is the answer it would have given.
pub fn scan_now(app: &AppHandle) -> Result<(), String> {
    if !module_enabled(app) {
        return Err("Merc OCR is switched off — turn the module on first".to_string());
    }
    if status(app) == MercStatus::Unavailable {
        return Err("Merc OCR is unavailable on this machine".to_string());
    }
    // `arm` publishes the scan itself, so the button's own click is what
    // changes the badge rather than the loop's next iteration.
    arm(app, BurstSource::Manual, None);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scan announces itself over the two RESTING statuses and nothing else.
    /// Over `Live` it would replace a window on screen with a hunt for one.
    /// The strip names who it heard, so the name has to come off the burst
    /// that is actually running. A Scan-now burst heard nobody and must not
    /// borrow the last voice line's mercenary.
    #[test]
    fn the_gate_names_the_speaker_of_the_burst_that_is_running() {
        let mut gate = BurstGate::default();
        gate.arm(BurstSource::ClientTxt, Some("Fennik, of Unshakeable Faith".into()), 1_000);

        assert_eq!(gate.speaker(1_000), Some("Fennik, of Unshakeable Faith"));

        gate.arm(BurstSource::Manual, None, 2_000);
        assert_eq!(gate.speaker(2_000), None, "Scan now heard nobody");
    }

    /// The gate holds an expired burst until something takes it, so reading the
    /// speaker straight off `armed` would print a dead burst's mercenary beside
    /// a scan that is over.
    #[test]
    fn an_expired_burst_names_nobody() {
        let mut gate = BurstGate::default();
        gate.arm(BurstSource::ClientTxt, Some("Fennik, of Unshakeable Faith".into()), 1_000);

        assert_eq!(gate.speaker(1_000 + BURST_TTL_MS), None);
    }

    #[test]
    fn arming_a_scan_only_announces_itself_over_a_resting_module() {
        assert!(scan_outranks(MercStatus::Idle));
        assert!(!scan_outranks(MercStatus::Live));
        // The one that bit: `done` is a window ON SCREEN. The overlay marks any
        // capture outside live/done as a previous read, so announcing a scan
        // here would mark the verdict the player is reading stale and drop its
        // glyph rows. The loop still resumes the read; only the announcement is
        // withheld.
        assert!(!scan_outranks(MercStatus::Done));
        assert!(!scan_outranks(MercStatus::Scanning), "a re-arm changes nothing on the strip");
        assert!(!scan_outranks(MercStatus::Off));
        assert!(!scan_outranks(MercStatus::Unavailable));
    }

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

    #[test]
    fn an_armed_burst_scans_for_the_whole_ttl() {
        let mut gate = BurstGate::default();
        gate.arm(BurstSource::ClientTxt, Some("Nytra, the Lout".into()), 1_000);

        assert!(gate.scanning(1_000));
        assert!(gate.scanning(1_000 + BURST_TTL_MS - 1));
    }

    #[test]
    fn a_burst_stops_scanning_at_the_ttl() {
        let mut gate = BurstGate::default();
        gate.arm(BurstSource::ClientTxt, Some("Nytra, the Lout".into()), 1_000);

        assert!(!gate.scanning(1_000 + BURST_TTL_MS));
    }

    #[test]
    fn an_idle_gate_scans_nothing() {
        assert!(!BurstGate::default().scanning(1_000));
    }

    #[test]
    fn arming_an_idle_gate_reports_a_new_burst() {
        let mut gate = BurstGate::default();
        assert!(gate.arm(BurstSource::Manual, None, 1_000));
    }

    #[test]
    fn re_arming_an_active_burst_reports_no_new_burst() {
        let mut gate = BurstGate::default();
        gate.arm(BurstSource::ClientTxt, Some("Nytra, the Lout".into()), 1_000);

        assert!(!gate.arm(BurstSource::ClientTxt, Some("Nytra, the Lout".into()), 2_000));
    }

    #[test]
    fn re_arming_an_active_burst_extends_its_window() {
        let mut gate = BurstGate::default();
        gate.arm(BurstSource::ClientTxt, Some("Nytra, the Lout".into()), 1_000);
        gate.arm(BurstSource::ClientTxt, Some("Nytra, the Lout".into()), 2_000);

        assert!(gate.scanning(1_000 + BURST_TTL_MS));
    }

    #[test]
    fn arming_after_an_expiry_reports_a_new_burst() {
        let mut gate = BurstGate::default();
        gate.arm(BurstSource::ClientTxt, Some("Nytra, the Lout".into()), 1_000);

        assert!(gate.arm(BurstSource::Manual, None, 1_000 + BURST_TTL_MS));
    }

    #[test]
    fn an_expired_burst_is_reported_once() {
        let mut gate = BurstGate::default();
        gate.arm(BurstSource::ClientTxt, Some("Nytra, the Lout".into()), 1_000);
        let now = 1_000 + BURST_TTL_MS;

        assert_eq!(
            gate.take_expired(now).map(|b| b.describe()),
            Some("Nytra, the Lout (client-txt)".to_string())
        );
        assert_eq!(gate.take_expired(now), None);
    }

    #[test]
    fn a_live_burst_is_not_reported_as_expired() {
        let mut gate = BurstGate::default();
        gate.arm(BurstSource::ClientTxt, Some("Nytra, the Lout".into()), 1_000);

        assert_eq!(gate.take_expired(1_999), None);
    }

    #[test]
    fn a_captured_window_leaves_no_burst_to_expire() {
        let mut gate = BurstGate::default();
        gate.arm(BurstSource::ClientTxt, Some("Nytra, the Lout".into()), 1_000);

        gate.take();

        assert!(!gate.scanning(1_000));
        assert_eq!(gate.take_expired(1_000 + BURST_TTL_MS), None);
    }

    #[test]
    fn a_manual_burst_names_its_source_and_nobody_else() {
        let mut gate = BurstGate::default();
        gate.arm(BurstSource::Manual, None, 1_000);
        gate.begin(1_000);

        assert_eq!(
            gate.take_expired(1_000 + BURST_TTL_MS).map(|b| b.describe()),
            Some("(manual)".to_string())
        );
    }

    /// Scan now is clicked in OUR window, so the game is not in front and the
    /// loop is napping. A TTL that started at the click would be gone before
    /// the player finished alt-tabbing.
    #[test]
    fn a_manual_burst_does_not_start_its_ttl_until_the_game_is_in_front() {
        let mut gate = BurstGate::default();
        gate.arm(BurstSource::Manual, None, 1_000);

        assert!(gate.scanning(1_000 + BURST_TTL_MS + 5_000));
    }

    #[test]
    fn a_manual_burst_runs_its_full_ttl_from_the_moment_the_game_is_in_front() {
        let mut gate = BurstGate::default();
        gate.arm(BurstSource::Manual, None, 1_000);

        gate.begin(20_000);

        assert!(gate.scanning(20_000 + BURST_TTL_MS - 1));
        assert!(!gate.scanning(20_000 + BURST_TTL_MS));
    }

    /// The other half of the same rule: a voice line IS evidence about the
    /// screen at that moment, so its window is measured from the line and the
    /// game coming back later does not buy it a fresh one.
    #[test]
    fn a_client_txt_burst_keeps_the_ttl_it_started_at_its_line() {
        let mut gate = BurstGate::default();
        gate.arm(BurstSource::ClientTxt, Some("Nytra, the Lout".into()), 1_000);

        gate.begin(5_000);

        assert!(!gate.scanning(1_000 + BURST_TTL_MS));
    }

    #[test]
    fn a_manual_burst_that_never_sees_the_game_gives_up() {
        let mut gate = BurstGate::default();
        gate.arm(BurstSource::Manual, None, 1_000);

        assert!(!gate.scanning(1_000 + MANUAL_ARM_GRACE_MS));
        assert!(gate.take_expired(1_000 + MANUAL_ARM_GRACE_MS).is_some());
    }

    #[test]
    fn a_burst_that_never_looked_does_not_blame_the_recruit_window() {
        let mut gate = BurstGate::default();
        gate.arm(BurstSource::ClientTxt, Some("Nytra, the Lout".into()), 1_000);

        let expired = gate.take_expired(1_000 + BURST_TTL_MS).expect("expired");

        assert_eq!(
            expired.expiry_line(),
            "Merc: OCR burst expired without ever looking — game not in the foreground — Nytra, the Lout (client-txt)"
        );
    }

    #[test]
    fn a_burst_that_looked_and_found_nothing_says_so() {
        let mut gate = BurstGate::default();
        gate.arm(BurstSource::ClientTxt, Some("Nytra, the Lout".into()), 1_000);

        gate.note_looked();
        let expired = gate.take_expired(1_000 + BURST_TTL_MS).expect("expired");

        assert_eq!(
            expired.expiry_line(),
            "Merc: OCR burst expired with no recruit window — Nytra, the Lout (client-txt)"
        );
    }
}
