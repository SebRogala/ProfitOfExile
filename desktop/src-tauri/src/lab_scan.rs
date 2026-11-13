//! Lab reading state shared by the two lab screen readers
//! (`lab::font_reader`, `lab::gem_reader`), the Client.txt event handlers
//! that arm and disarm them, and the commands that read their results.
//!
//! Nothing here reads the screen: each read key press hands the readers one
//! frame from `read_hotkey::spawn_read_hotkey`'s dispatcher thread
//! ([OVERLAY-GUIDE § Screen reading](../../../docs/OVERLAY-GUIDE.md#screen-reading-ggg-policy)).
//! The rename of this module is deferred (POE-391 C9).

use std::sync::atomic::AtomicU64;
use std::sync::Mutex;

use crate::FontSessionData;

pub struct LabScanState {
    pub detection: Mutex<crate::lab_detection::LabDetection>,
    pub detected_gems: Mutex<Vec<String>>,
    /// The one token issuer for both lab readers. Never reset and never
    /// stored to: only `font_session::next_scan_generation` advances it, so
    /// every arm of either context gets a value no earlier arm had, and a
    /// press captured before a disarm can never equal a later context.
    pub read_token_seq: AtomicU64,
    /// The armed font context's token, 0 = unarmed. Written only while the
    /// `font_session` guard is held (`lab::font_context`); the font reader's
    /// `arm` reads it lock-free and every write path re-checks it under that
    /// guard.
    pub font_context: AtomicU64,
    /// Bumped under the `font_session` guard by every font arm, disarm and
    /// read that saw the panel. Never reset. An idle timer carries the value
    /// it was spawned with and acts only if it still matches.
    pub font_idle_seq: AtomicU64,
    /// The armed gem context's token, 0 = unarmed. Written only while the
    /// `detected_gems` guard is held, the guard `commit_gem_read` checks it
    /// under.
    pub gem_context: AtomicU64,
    /// Bumped under the `detected_gems` guard by every gem arm and every gem
    /// commit; never reset. A manual mode change compares it across its
    /// generation bump to tell a choice that changed meanwhile from one
    /// finished before it.
    pub choice_seq: AtomicU64,
    /// The latest committed gem read, uncertain slots and their candidates
    /// included (POE-397 C2/C3). Taken only inside the `detected_gems` guard
    /// (order `detected_gems` → `last_gem_read`) and never held while taking
    /// another lock; cleared wherever the held gems are cleared.
    pub(crate) last_gem_read: Mutex<Option<crate::lab::gem_reader::LabGemRead>>,
    /// The tooltip vocabulary for the gem reader, installed by the background
    /// preload (`lab::gem_vocab`). Acquired alone.
    pub gem_vocab: Mutex<Option<crate::lab::gem_vocab::GemVocab>>,
    /// Bumped by every lab-mode change, under the `lab_settings` guard. A
    /// vocabulary or a gem commit carrying an older value is rejected.
    pub gem_vocab_generation: AtomicU64,
    /// Monotonic count of `FontOpened` events. The craft ledger gates every
    /// count change on it: the panel's count cannot change without a CRAFT
    /// click, and a CRAFT click always fires this event, so a count change with
    /// no new event is a misread. Never reset — the ledger stores the value it
    /// accepted at, and a counter going backwards would re-open accepted rounds.
    pub font_opened_seq: AtomicU64,
    /// Font session data — accumulated rounds, shared between the font reader
    /// and the event handlers.
    pub font_session: Mutex<FontSessionData>,
    /// The alert crafts the latest current font read listed (POE-328).
    /// Replaced by each font read, cleared by `FontOpened` and by every session
    /// reset, read by `font_alerts::get_font_alerts`. Acquired alone by the
    /// reader; `lab::font_context` takes it inside `font_session`.
    pub font_alert_seen: Mutex<crate::font_alerts::AlertSeen>,
}

impl LabScanState {
    pub fn new() -> Self {
        Self {
            detection: Mutex::new(crate::lab_detection::LabDetection::default()),
            detected_gems: Mutex::new(Vec::new()),
            read_token_seq: AtomicU64::new(0),
            font_context: AtomicU64::new(0),
            font_idle_seq: AtomicU64::new(0),
            gem_context: AtomicU64::new(0),
            choice_seq: AtomicU64::new(0),
            last_gem_read: Mutex::new(None),
            gem_vocab: Mutex::new(None),
            gem_vocab_generation: AtomicU64::new(0),
            font_opened_seq: AtomicU64::new(0),
            font_session: Mutex::new(FontSessionData::default()),
            font_alert_seen: Mutex::new(crate::font_alerts::AlertSeen::default()),
        }
    }
}
