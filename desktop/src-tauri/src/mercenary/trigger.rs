//! What Client.txt tells the mercenary module (POE-198, POE-280, POE-390).
//!
//! Since POE-390 nothing here starts a screen read: the merc module reads only
//! on a press of the app-wide read key (`super::press`). Client.txt does two
//! things for it:
//!
//! - an **area change** (`You have entered …`) is handed to the read worker as
//!   [`MercInbox::AreaChanged`], which retires a live capture: the recruit
//!   window went with the area it was in (ruling 4);
//! - a **known Inspect text** — a normalized two-or-more-word Inspect line, or
//!   a one-word Inspect grunt licensed by the same speaker's previous known
//!   multi-word mercenary line — is logged as heard, and nothing else: no
//!   status, no speaker on the slice (ruling 5). The matching is kept for the
//!   "press the read key" prompt (POE-388).
//!
//! # The Inspect match (POE-280, owner decision 2026-09-14)
//!
//! The owner measured the poedb.tw mercenary dialogue table on 2026-09-14:
//! 2,602 source rows reduce to 1,058 unique normalized texts, with 89 tagged
//! `inspect` and the rest `line`. `FirstApproach` is the walk-up and has no
//! recruit window; `Inspect`/`InspectNotable` is the click and is followed by
//! the window, then `BattleStart` and `JoinParty`. A re-open reads
//! `PlayerRetreat` followed by `Inspect`.
//!
//! A normalized Inspect text with two or more words matches whatever the
//! speaker is called. A one-word Inspect text matches only when that speaker's
//! previous line was a known multi-word mercenary text, either tag. The grunt
//! exception is measured: Sister Cassia says `Oh!` 161 times and Sonja says
//! `Eh.`; NPCs also use these one-word texts, while the 12 mercenary cases had
//! the same speaker's known multi-word line immediately before them.
//!
//! Text, not speaker shape, is the key: mercenary names are generated,
//! plain-name mercenaries exist (`Epipi Rapono: Make a call.`, 2026-08-31), and
//! a speaker name cannot tell FirstApproach, fight and Inspect lines apart.
//! Asset and Client.txt text use the same normalization: lowercase, retain
//! ASCII letters, digits and whitespace, collapse whitespace, and trim. Unknown
//! text does not match (owner decision 2026-09-14), so a GGG patch adding or
//! rewording a line needs a deliberate asset regeneration. The asset texts are
//! English; on a client running another language no line matches.
//!
//! # Pure vs glue
//!
//! The speaker parse, text normalization and lookup, the bounded per-speaker
//! history and the area hand-off ([`client_line_effect`]) are plain functions
//! over plain data, tested here on Linux. [`on_client_line`] only sends and
//! logs.

use std::collections::{hash_map::Entry, HashMap, VecDeque};

use tauri::{AppHandle, Manager};

use super::press::MercInbox;
use super::{MercStatus, MODULE_ID};
use crate::client_log::speaker_of;
use crate::AppState;

/// The embedded poedb dialogue corpus. See `assets/README.md` for provenance.
const SHIPPED_VOICE_LINES: &str = include_str!("assets/merc-voicelines.tsv");

/// The two tags in the embedded dialogue corpus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MercVoiceTag {
    Inspect,
    Line,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct KnownVoiceLine {
    tag: MercVoiceTag,
    words: usize,
}

/// The normalized mercenary dialogue corpus embedded in the desktop binary.
#[derive(Debug, Default, Clone)]
pub struct MercVoiceLines {
    entries: HashMap<String, KnownVoiceLine>,
}

impl MercVoiceLines {
    /// Parse the shipped <tag>\t<text> asset.
    pub fn shipped() -> Self {
        Self::parse(SHIPPED_VOICE_LINES)
    }

    /// Parse rows, normalizing their text before using it as the lookup key.
    pub fn parse(raw: &str) -> Self {
        let mut entries: HashMap<String, KnownVoiceLine> = HashMap::new();
        for row in raw.lines() {
            let Some((raw_tag, text)) = row.split_once('\t') else {
                continue;
            };
            let tag = match raw_tag {
                "inspect" => MercVoiceTag::Inspect,
                "line" => MercVoiceTag::Line,
                _ => continue,
            };
            let normalized = normalize_text(text);
            if normalized.is_empty() {
                continue;
            }
            let known = KnownVoiceLine { tag, words: word_count(&normalized) };
            match entries.entry(normalized) {
                Entry::Occupied(mut existing) => {
                    if existing.get().tag == MercVoiceTag::Line && tag == MercVoiceTag::Inspect {
                        existing.insert(known);
                    }
                }
                Entry::Vacant(entry) => {
                    entry.insert(known);
                }
            }
        }
        Self { entries }
    }

    /// Number of unique normalized texts in the corpus.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the test corpus has no entries.
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Number of unique normalized texts tagged inspect.
    pub fn inspect_len(&self) -> usize {
        self.entries
            .values()
            .filter(|line| line.tag == MercVoiceTag::Inspect)
            .count()
    }

    /// Resolve a raw or normalized text to its corpus tag.
    #[cfg(test)]
    pub fn tag(&self, text: &str) -> Option<MercVoiceTag> {
        self.entries.get(&normalize_text(text)).map(|line| line.tag)
    }

    fn lookup(&self, normalized: &str) -> Option<KnownVoiceLine> {
        self.entries.get(normalized).copied()
    }
}

/// Text normalization shared by the asset loader and Client.txt decisions.
pub fn normalize_text(text: &str) -> String {
    let mut normalized = String::with_capacity(text.len());
    let mut pending_space = false;
    for character in text.chars() {
        if character.is_ascii_alphanumeric() {
            if pending_space && !normalized.is_empty() {
                normalized.push(' ');
            }
            normalized.push(character.to_ascii_lowercase());
            pending_space = false;
        } else if character.is_whitespace() {
            pending_space = !normalized.is_empty();
        }
    }
    normalized
}

fn word_count(normalized: &str) -> usize {
    normalized.split_whitespace().count()
}

/// Maximum number of speaker keys retained by MercSpeakerHistory.
pub const MAX_SPEAKER_HISTORY: usize = 256;

/// Whether the previous accepted line for recently heard speakers licenses a
/// one-word grunt.
///
/// Entries are least-recently-used by speaker. A bounded history is enough for
/// the one-line grunt rule, while an NPC-heavy long session cannot grow this
/// watcher-owned state without limit. Eviction only loses a possible grunt
/// trigger; Scan now remains the explicit fallback. Every accepted line still
/// does one hash lookup and, for an existing speaker, an O(256) deque search/remove
/// to move that speaker to most-recently-used; existing speakers allocate no
/// strings, and a new speaker allocates its two owned keys.
#[derive(Debug, Default)]
pub struct MercSpeakerHistory {
    previous: HashMap<String, bool>,
    order: VecDeque<String>,
}

impl MercSpeakerHistory {
    /// Number of speaker keys currently retained.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.previous.len()
    }

    fn previous_was_multiword_merc(&self, speaker: &str) -> bool {
        self.previous.get(speaker).copied().unwrap_or(false)
    }

    fn remember(&mut self, speaker: &str, licenses_grunt: bool) {
        if let Some(previous) = self.previous.get_mut(speaker) {
            *previous = licenses_grunt;
            let index = self
                .order
                .iter()
                .position(|key| key == speaker)
                .expect("speaker key was found");
            let key = self.order.remove(index).expect("speaker key was found");
            self.order.push_back(key);
            return;
        }
        if self.previous.len() >= MAX_SPEAKER_HISTORY {
            if let Some(oldest) = self.order.pop_front() {
                self.previous.remove(&oldest);
            }
        }
        self.order.push_back(speaker.to_owned());
        self.previous.insert(speaker.to_owned(), licenses_grunt);
    }
}

/// The pure trigger decision for one Client.txt line, including history update.
///
/// Every line accepted by speaker_of updates history, even when the module is
/// disabled, a capture is held, or the line does not arm. The return value is
/// the speaker that earned a probe gate, or None.
pub fn trigger_line<'a>(
    line: &'a str,
    voice_lines: &MercVoiceLines,
    history: &mut MercSpeakerHistory,
) -> Option<&'a str> {
    let speaker = speaker_of(line)?;
    let text = dialogue_text(line)?;
    let normalized = normalize_text(text);
    let known = voice_lines.lookup(&normalized);
    let arms = match known {
        Some(line) if line.tag == MercVoiceTag::Inspect && line.words >= 2 => true,
        Some(line) if line.tag == MercVoiceTag::Inspect && line.words == 1 => {
            history.previous_was_multiword_merc(speaker)
        }
        _ => false,
    };
    let licenses_grunt = known.is_some_and(|line| line.words >= 2);
    history.remember(speaker, licenses_grunt);
    if arms { Some(speaker) } else { None }
}

fn dialogue_text(line: &str) -> Option<&str> {
    let message = &line[line.find("] ")? + 2..];
    message.split_once(": ").map(|(_, text)| text)
}

// ---------------------------------------------------------------------------
// Pure — status precedence and the area hand-off
// ---------------------------------------------------------------------------

/// Whether a press read may publish `Scanning` over this status (ruling 5).
///
/// Only over the module's resting status. `Live` and `Done` are a window ON
/// SCREEN whose verdict the player is reading: the overlay treats every status
/// but `live`/`done` as a capture that is no longer current, so publishing
/// `scanning` over either would mark a live verdict stale and drop the per-row
/// glyphs under it. `Off` and `Unavailable` have no worker to read.
pub fn scan_outranks(status: MercStatus) -> bool {
    matches!(status, MercStatus::Idle)
}

/// What one Client.txt line hands the merc worker: an area change, or nothing.
pub fn client_line_effect(line: &str) -> Option<MercInbox> {
    crate::client_log::parse_entered_area(line).map(|_| MercInbox::AreaChanged)
}

// ---------------------------------------------------------------------------
// Glue — the AppHandle wrappers
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

/// The Client.txt seam, called for EVERY line the watcher reads.
///
/// An area change goes to the installed read worker (if any). Any other line
/// runs the Inspect match, which updates the per-speaker history whatever the
/// module state; a match with the module on is logged and nothing else.
pub fn on_client_line(
    app: &AppHandle,
    line: &str,
    voice_lines: &MercVoiceLines,
    history: &mut MercSpeakerHistory,
) {
    if let Some(message) = client_line_effect(line) {
        if let Some(tx) = super::press::installed_inbox(app) {
            // A send error means the worker is stopping; nothing to retire.
            let _ = tx.send(message);
        }
        return;
    }
    let Some(speaker) = trigger_line(line, voice_lines, history) else {
        return;
    };
    if module_enabled(app) {
        crate::app_log(app, format!("Merc: heard {speaker} — press the read key to read"));
    }
}

/// Scan now is gone: the read key replaced it (POE-390). The command itself is
/// removed separately (WI-2).
pub fn scan_now(_app: &AppHandle) -> Result<(), String> {
    Err("Scan now is replaced by the read key".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const THRASSEL_APPROACH: &str = concat!(
        "2026/09/14 05:15:54 215134062 cffb065b [INFO Client 38536] ",
        "Thrassel, the Azadin Sadist: Your body is the canvas. My blade, the brush. ",
    );
    const THRASSEL_INSPECT: &str = concat!(
        "2026/09/14 05:15:57 215137234 cffb065b [INFO Client 38536] ",
        "Thrassel, the Azadin Sadist: My Azadi brethren ensured I never lacked for... ",
        "raw material. Care to do the same?",
    );
    const THARVIK_APPROACH: &str = concat!(
        "2026/09/14 05:27:29 215829171 cffb065b [INFO Client 38536] ",
        "Tharvik, the Keitan Inquisitor: You stain the ground with your presence!",
    );
    const THARVIK_INSPECT: &str = concat!(
        "2026/09/14 05:27:31 215830984 cffb065b [INFO Client 38536] ",
        "Tharvik, the Keitan Inquisitor: Born of Keita. Baptised in Innocence. ",
        "Made righteous by both.",
    );
    const EPIPI_INSPECT: &str = concat!(
        "2026/08/31 15:28:54 1248915531 cffb065b [INFO Client 39456] ",
        "Epipi Rapono: Make a call.",
    );
    const KREZIN_INSPECT: &str = concat!(
        "2026/08/01 17:26:56 684137921 cffb065b [INFO Client 51688] ",
        "Krezin, the Bloodstained: I eagerly await your decision. ",
    );
    const RYXELLE_LINE: &str = concat!(
        "2026/09/14 05:08:58 214718453 cffb065b [INFO Client 38536] ",
        "Ryxelle, the Ninth: You are nothing!",
    );
    const SISTER_CASSIA_GRUNT: &str = concat!(
        "2026/01/31 21:48:37 221172203 cff945bb [INFO Client 23972] ",
        "Sister Cassia: Oh!",
    );
    const SONJA_GRUNT: &str = concat!(
        "2026/04/11 22:46:11 1614078937 cffb06dd [INFO Client 3192] ",
        "Sonja, the Farmer: Eh.",
    );
    const KAI_LINE: &str = concat!(
        "2026/07/25 00:36:23 18709515 cffb0658 [INFO Client 13808] ",
        "Kai, the Slack-Jawed: Ni nah guak! Guoah eyai?",
    );
    const KAI_GRUNT: &str = concat!(
        "2026/07/25 00:36:25 18711531 cffb0658 [INFO Client 13808] ",
        "Kai, the Slack-Jawed: Eh?",
    );

    fn shipped_lines() -> MercVoiceLines {
        MercVoiceLines::shipped()
    }

    fn fresh_history() -> MercSpeakerHistory {
        MercSpeakerHistory::default()
    }

    fn synthetic_line(speaker: &str, text: &str) -> String {
        format!(
            "2026/08/31 15:28:55 1248915531 cffb065b [INFO Client 39456] {speaker}: {text}"
        )
    }

    #[test]
    fn the_shipped_asset_parses_inspect_texts() {
        let lines = shipped_lines();

        assert!(lines.inspect_len() > 0, "the embedded corpus parsed no Inspect texts");
        assert_eq!(lines.tag("Make a call."), Some(MercVoiceTag::Inspect));
    }

    #[test]
    fn the_shipped_asset_parses_line_texts() {
        let lines = shipped_lines();

        assert!(!lines.is_empty(), "the embedded corpus parsed into no texts");
        assert_eq!(lines.tag("Your loss."), Some(MercVoiceTag::Line));
    }

    #[test]
    fn every_shipped_asset_row_resolves_to_its_tag() {
        let lines = shipped_lines();

        for (row_number, row) in SHIPPED_VOICE_LINES.lines().enumerate() {
            assert_eq!(
                row.matches('\t').count(),
                1,
                "asset row {row_number} must contain exactly one tab"
            );
            let (raw_tag, text) = row.split_once('\t').expect("asset row has a tab");
            let expected = match raw_tag {
                "inspect" => MercVoiceTag::Inspect,
                "line" => MercVoiceTag::Line,
                _ => panic!("asset row {row_number} has an unknown tag: {raw_tag}"),
            };
            assert_eq!(
                lines.tag(text),
                Some(expected),
                "asset row {row_number} did not resolve: {text}"
            );
        }
    }

    #[test]
    fn normalization_matches_asset_and_client_variants() {
        assert_eq!(
            normalize_text(" I {eagerly} await your decision. \t"),
            normalize_text("I eagerly await your decision."),
        );
        assert_eq!(normalize_text("Eh?"), normalize_text("Eh."));
        assert_eq!(
            normalize_text("Make your choice – and quickly."),
            "make your choice and quickly",
        );
    }

    #[test]
    fn a_walk_up_text_does_not_arm() {
        let mut history = fresh_history();

        assert_eq!(trigger_line(THRASSEL_APPROACH, &shipped_lines(), &mut history), None);
    }

    #[test]
    fn a_known_inspect_text_arms_for_a_titled_speaker() {
        let mut history = fresh_history();

        assert_eq!(
            trigger_line(THRASSEL_INSPECT, &shipped_lines(), &mut history),
            Some("Thrassel, the Azadin Sadist"),
        );
    }

    #[test]
    fn a_known_line_text_does_not_arm() {
        let mut history = fresh_history();

        assert_eq!(trigger_line(RYXELLE_LINE, &shipped_lines(), &mut history), None);
    }

    #[test]
    fn a_known_inspect_text_arms_for_a_plain_name_speaker() {
        let mut history = fresh_history();

        assert_eq!(
            trigger_line(EPIPI_INSPECT, &shipped_lines(), &mut history),
            Some("Epipi Rapono"),
        );
    }

    #[test]
    fn punctuation_and_emphasis_normalize_to_the_same_inspect_text() {
        let mut history = fresh_history();

        assert_eq!(
            trigger_line(KREZIN_INSPECT, &shipped_lines(), &mut history),
            Some("Krezin, the Bloodstained"),
        );
    }

    #[test]
    fn a_first_approach_line_does_not_arm() {
        let mut history = fresh_history();

        assert_eq!(trigger_line(THARVIK_APPROACH, &shipped_lines(), &mut history), None);
    }

    #[test]
    fn a_known_inspect_text_arms_for_tharvik() {
        let mut history = fresh_history();

        assert_eq!(
            trigger_line(THARVIK_INSPECT, &shipped_lines(), &mut history),
            Some("Tharvik, the Keitan Inquisitor"),
        );
    }

    #[test]
    fn the_first_sister_cassia_grunt_does_not_arm() {
        let mut history = fresh_history();

        assert_eq!(trigger_line(SISTER_CASSIA_GRUNT, &shipped_lines(), &mut history), None);
    }

    #[test]
    fn a_second_sister_cassia_grunt_still_does_not_arm() {
        let mut history = fresh_history();

        assert_eq!(trigger_line(SISTER_CASSIA_GRUNT, &shipped_lines(), &mut history), None);
        assert_eq!(trigger_line(SISTER_CASSIA_GRUNT, &shipped_lines(), &mut history), None);
    }

    #[test]
    fn a_sonja_grunt_without_a_previous_line_does_not_arm() {
        let mut history = fresh_history();

        assert_eq!(trigger_line(SONJA_GRUNT, &shipped_lines(), &mut history), None);
    }

    #[test]
    fn a_grunt_arms_after_the_same_speakers_multiword_merc_line() {
        let mut history = fresh_history();
        let lines = shipped_lines();

        trigger_line(KAI_LINE, &lines, &mut history);
        assert_eq!(
            trigger_line(KAI_GRUNT, &lines, &mut history),
            Some("Kai, the Slack-Jawed"),
        );
    }

    #[test]
    fn another_speakers_grunt_does_not_clear_kai_license() {
        let mut history = fresh_history();
        let lines = shipped_lines();

        assert_eq!(trigger_line(KAI_LINE, &lines, &mut history), None);
        assert_eq!(trigger_line(SISTER_CASSIA_GRUNT, &lines, &mut history), None);
        assert_eq!(
            trigger_line(KAI_GRUNT, &lines, &mut history),
            Some("Kai, the Slack-Jawed"),
        );
    }

    #[test]
    fn a_different_speakers_line_does_not_license_a_grunt() {
        let mut history = fresh_history();
        let lines = shipped_lines();

        assert_eq!(
            trigger_line(THARVIK_INSPECT, &lines, &mut history),
            Some("Tharvik, the Keitan Inquisitor"),
        );
        let other_grunt = synthetic_line("Sister Cassia", "Eh?");
        assert_eq!(trigger_line(&other_grunt, &lines, &mut history), None);
    }

    #[test]
    fn an_unknown_line_replaces_a_same_speakers_grunt_license() {
        let mut history = fresh_history();
        let lines = shipped_lines();
        let known_line = synthetic_line("Kai", "Make a call.");
        let unknown_line = synthetic_line("Kai", "zzzzz never listed text.");
        let grunt = synthetic_line("Kai", "Eh?");

        assert_eq!(trigger_line(&known_line, &lines, &mut history), Some("Kai"));
        assert_eq!(trigger_line(&unknown_line, &lines, &mut history), None);
        assert_eq!(trigger_line(&grunt, &lines, &mut history), None);
    }

    #[test]
    fn a_whisper_with_an_inspect_text_does_not_arm() {
        let mut history = fresh_history();
        let line = concat!(
            "2026/08/31 15:28:55 1248915531 cffb065b [INFO Client 39456] ",
            "@From <Guild> Name: Make a call.",
        );

        assert_eq!(trigger_line(line, &shipped_lines(), &mut history), None);
        assert_eq!(history.len(), 0);
    }

    #[test]
    fn a_quoted_inspect_text_does_not_arm() {
        let mut history = fresh_history();
        let line = concat!(
            "2026/08/31 15:28:55 1248915531 cffb065b [INFO Client 39456] ",
            "Vertolka: Epipi Rapono: Make a call.",
        );

        assert_eq!(trigger_line(line, &shipped_lines(), &mut history), None);
    }

    #[test]
    fn speaker_history_never_exceeds_its_bound() {
        let lines = shipped_lines();
        let mut history = fresh_history();

        for index in 0..=MAX_SPEAKER_HISTORY {
            let speaker = format!("Speaker{index}");
            let line = synthetic_line(&speaker, "Make a call.");
            trigger_line(&line, &lines, &mut history);
            assert!(history.len() <= MAX_SPEAKER_HISTORY);
        }
        assert_eq!(history.len(), MAX_SPEAKER_HISTORY);
    }

    #[test]
    fn speaker_history_evicts_the_oldest_speaker() {
        let lines = shipped_lines();
        let mut history = fresh_history();

        for index in 0..MAX_SPEAKER_HISTORY {
            let speaker = format!("Speaker{index}");
            let line = synthetic_line(&speaker, "Make a call.");
            trigger_line(&line, &lines, &mut history);
        }
        let newest = synthetic_line("Newest", "Make a call.");
        trigger_line(&newest, &lines, &mut history);
        let evicted = synthetic_line("Speaker0", "Eh?");

        assert_eq!(history.len(), MAX_SPEAKER_HISTORY);
        assert_eq!(trigger_line(&evicted, &lines, &mut history), None);
    }

    #[test]
    fn re_hearing_the_oldest_speaker_reorders_lru_eviction() {
        let lines = shipped_lines();
        let mut history = fresh_history();

        for index in 0..MAX_SPEAKER_HISTORY {
            let speaker = format!("Speaker{index}");
            let line = synthetic_line(&speaker, "Make a call.");
            trigger_line(&line, &lines, &mut history);
        }
        let reheard = synthetic_line("Speaker0", "Make a call.");
        assert_eq!(trigger_line(&reheard, &lines, &mut history), Some("Speaker0"));
        let newest = synthetic_line("Newest", "Make a call.");
        assert_eq!(trigger_line(&newest, &lines, &mut history), Some("Newest"));

        let retained_grunt = synthetic_line("Speaker0", "Eh?");
        let evicted_grunt = synthetic_line("Speaker1", "Eh?");
        assert_eq!(
            trigger_line(&retained_grunt, &lines, &mut history),
            Some("Speaker0")
        );
        assert_eq!(trigger_line(&evicted_grunt, &lines, &mut history), None);
    }
    // -- rule 5: a held capture ignores the line ---------------------------


    // -- the area hand-off --------------------------------------------------

    const ENTERED_AREA: &str =
        "2026/09/14 05:20:01 215380000 cffb065b [INFO Client 38536] : You have entered Sarn Encampment.";

    #[test]
    fn an_entered_area_line_hands_the_worker_an_area_change() {
        assert!(matches!(client_line_effect(ENTERED_AREA), Some(MercInbox::AreaChanged)));
    }

    #[test]
    fn an_inspect_line_hands_the_worker_nothing() {
        assert!(client_line_effect(THRASSEL_INSPECT).is_none());
    }

    // -- the announcement ---------------------------------------------------

    #[test]
    fn a_press_read_only_announces_itself_over_a_resting_module() {
        assert!(scan_outranks(MercStatus::Idle));
        assert!(!scan_outranks(MercStatus::Live));
        // `done` is a window ON SCREEN. The overlay marks any capture outside
        // live/done as a previous read, so announcing a scan here would mark
        // the verdict the player is reading stale and drop its glyph rows.
        assert!(!scan_outranks(MercStatus::Done));
        assert!(!scan_outranks(MercStatus::Scanning));
        assert!(!scan_outranks(MercStatus::Off));
        assert!(!scan_outranks(MercStatus::Unavailable));
    }
}
