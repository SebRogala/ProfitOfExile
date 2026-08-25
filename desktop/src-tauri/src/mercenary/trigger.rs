//! What starts a capture burst, and for how long (POE-198).
//!
//! Before this file the capture loop hunted for a recruit window once a second
//! for as long as the module was on. That is a full-screen grab plus a
//! full-screen OCR per second, forever, for a window the player opens a handful
//! of times an hour. The loop now runs OCR only when something has asked it to
//! look, and the ask is an event:
//!
//! - a **Client.txt voice line** whose speaker is shaped like a mercenary's
//!   (`<Name>, the <Epithet>`) and is not a known NPC, or
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
//! match against. `<Name>, the <Epithet>` is the shape, but PoE's own NPCs use
//! it too — measured on Sebastian's Client.txt (2026-08-25): 93 distinct
//! `, the ` speakers over 1263 lines, of which 24 are NPCs (`Varashta, the
//! Winter Sekhema` alone accounts for 416 lines). The positive pattern ALONE
//! was measured to fail; the 24 ship in `assets/npc-denylist.txt` and
//! `<app_data>/merc-npc-denylist.txt` extends them without a rebuild, the same
//! shape as `merc-geometry.json`.
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

/// The 24 measured NPC speakers. See `assets/README.md` for provenance.
const SHIPPED_DENYLIST: &str = include_str!("assets/npc-denylist.txt");

/// How long a burst keeps looking, once it has started looking.
///
/// For a Client.txt burst the clock starts at the LINE, not when the game
/// returns to the foreground: a player who alt-tabs to read the page and comes
/// back within the window still gets the capture, and one who wanders off does
/// not come back to a loop that has been OCR'ing the screen since.
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

/// The infix that makes a Client.txt speaker dialogue rather than chat.
const SPEAKER_INFIX: &str = ", the ";

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
/// free name, `", the "`, then a non-empty epithet (`^\S+, the ` plus the
/// requirement that something follows).
///
/// Shape only — it says nothing about whether the speaker is a mercenary. That
/// is [`NpcDenylist`]'s job, because the names are generated and there is no
/// positive list to match.
pub fn is_dialogue_speaker(speaker: &str) -> bool {
    let Some((name, epithet)) = speaker.split_once(SPEAKER_INFIX) else {
        return false;
    };
    !name.is_empty()
        && !name.contains(char::is_whitespace)
        && !name.starts_with(CHAT_SIGILS)
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
    /// The shipped list — the 24 measured NPCs, and nothing else.
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

/// Arm a burst and log it if it started one.
fn arm(app: &AppHandle, source: BurstSource, speaker: Option<String>) {
    let armed = {
        let state = app.state::<AppState>();
        let mut gate = state.merc_burst.lock().unwrap_or_else(|e| e.into_inner());
        let fresh = gate.arm(source, speaker, now_ms());
        match (fresh, gate.armed.as_ref()) {
            (true, Some(burst)) => Some(burst.describe()),
            _ => None,
        }
    };
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
pub fn scan_now(app: &AppHandle) -> Result<(), String> {
    if !module_enabled(app) {
        return Err("Merc OCR is switched off — turn the module on first".to_string());
    }
    if status(app) == MercStatus::Unavailable {
        return Err("Merc OCR is unavailable on this machine".to_string());
    }
    arm(app, BurstSource::Manual, None);
    // The loop publishes `scanning` on its next iteration anyway; doing it here
    // means the button's own click is what changes the badge, rather than a
    // quarter-second of nothing.
    publish(app, |slice| {
        if slice.status == MercStatus::Idle {
            slice.status = MercStatus::Scanning;
        }
    });
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
    }

    #[test]
    fn the_denylist_matches_an_npc_whatever_its_case_and_padding() {
        assert!(NpcDenylist::shipped().contains("  varashta, THE Winter Sekhema "));
    }

    #[test]
    fn the_denylist_does_not_match_a_mercenary() {
        assert!(!NpcDenylist::shipped().contains("Nytra, the Cyaxan Loner"));
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
