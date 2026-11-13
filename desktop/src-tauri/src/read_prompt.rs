//! The "Detected <mechanic> - press <key> to read" prompt's state (POE-388).
//!
//! Client.txt seams arm a mechanic, a successful read or the context's end
//! clears it, and the SSOT carries the most recently armed active one. Display
//! only: nothing here grabs the screen; a read happens only on the read key
//! (`crate::read_hotkey`). Policy: docs/OVERLAY-GUIDE.md § "Screen reading
//! (GGG policy)".
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::temple::trigger::LineEvent;

/// A mechanic the read key can read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Mechanic {
    DivineFont,
    Incursion,
    Mercenary,
}

/// What one Client.txt seam does to the prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Arm(Mechanic),
    End(Mechanic),
}

/// The SSOT's `readPrompt`: the mechanic to name, and the arm count, so a
/// re-arm of the same mechanic (each Gift-lab font round) is a new prompt.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadPromptSnapshot {
    pub mechanic: Option<Mechanic>,
    pub seq: u64,
}

/// The active mechanics in arm order, and the arm count.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadPromptState {
    active: Vec<Mechanic>,
    seq: u64,
}

impl ReadPromptState {
    /// Shows `mechanic` as a new prompt, even when it was already active.
    pub fn arm(&mut self, mechanic: Mechanic) {
        self.active.retain(|m| *m != mechanic);
        self.active.push(mechanic);
        self.seq += 1;
    }
    /// A read of `mechanic` succeeded.
    pub fn read_done(&mut self, mechanic: Mechanic) {
        self.active.retain(|m| *m != mechanic);
    }
    /// The context of `mechanic` ended.
    pub fn end(&mut self, mechanic: Mechanic) {
        self.active.retain(|m| *m != mechanic);
    }
    /// The player changed area. Only the mercenary prompt ends here: the
    /// temple arms on an area line and the lab ends through its own seams.
    pub fn area_changed(&mut self) {
        self.end(Mechanic::Mercenary);
    }
    pub fn apply(&mut self, change: Change) {
        match change {
            Change::Arm(mechanic) => self.arm(mechanic),
            Change::End(mechanic) => self.end(mechanic),
        }
    }
    pub fn snapshot(&self) -> ReadPromptSnapshot {
        ReadPromptSnapshot {
            mechanic: self.active.last().copied(),
            seq: self.seq,
        }
    }
}

/// What a temple Client.txt line does to the prompt. `event` is `classify`'s
/// answer for the line (`None` for a stale one) and `armed_after` is the temple
/// arm after `apply_line`. The temple's start line and area line always arm,
/// whatever transition they made; a line that left the arm live (Alva's banter
/// inside the temple) ends nothing.
pub fn temple_change(event: Option<LineEvent>, armed_after: bool) -> Option<Change> {
    match event? {
        LineEvent::AlvaStart | LineEvent::EnteredTemple => Some(Change::Arm(Mechanic::Incursion)),
        LineEvent::AlvaEnd | LineEvent::LeftArea => {
            (!armed_after).then_some(Change::End(Mechanic::Incursion))
        }
    }
}

/// Runs `edit` on the state's prompt; true when the snapshot changed.
fn edit_in(state: &crate::AppState, edit: impl FnOnce(&mut ReadPromptState)) -> bool {
    let mut prompt = state.read_prompt.lock().unwrap_or_else(|e| e.into_inner());
    let before = prompt.snapshot();
    edit(&mut prompt);
    prompt.snapshot() != before
}

pub fn apply_in(state: &crate::AppState, change: Change) -> bool {
    edit_in(state, |p| p.apply(change))
}
pub fn read_done_in(state: &crate::AppState, mechanic: Mechanic) -> bool {
    edit_in(state, |p| p.read_done(mechanic))
}
pub fn area_changed_in(state: &crate::AppState) -> bool {
    edit_in(state, ReadPromptState::area_changed)
}

fn emit_if(app: &AppHandle, changed: bool) {
    if changed {
        crate::ssot::emit_ssot(app);
    }
}
pub fn apply(app: &AppHandle, change: Option<Change>) {
    if let Some(change) = change {
        emit_if(app, apply_in(&app.state::<crate::AppState>(), change));
    }
}
pub fn read_done(app: &AppHandle, mechanic: Mechanic) {
    emit_if(app, read_done_in(&app.state::<crate::AppState>(), mechanic));
}
pub fn area_changed(app: &AppHandle) {
    emit_if(app, area_changed_in(&app.state::<crate::AppState>()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::temple::trigger::{apply_line, classify, ArmReason, ArmState, Transition};

    const NOW: u64 = 1_756_800_000_000;

    fn stamped(at_ms: u64, message: &str) -> String {
        use chrono::TimeZone;
        let when = chrono::Local.timestamp_millis_opt(at_ms as i64).single().expect("a plain local hour");
        format!("{} 105432578 cffb0716 [INFO Client 12345] {message}", when.format("%Y/%m/%d %H:%M:%S"))
    }
    fn alva_start(at_ms: u64) -> String {
        stamped(at_ms, "Alva, Master Explorer: Time to go.")
    }
    fn alva_end(at_ms: u64) -> String {
        stamped(at_ms, "Alva, Master Explorer: Good job.")
    }
    fn alva_banter(at_ms: u64) -> String {
        stamped(at_ms, "Alva, Master Explorer: At last... Atzoatl.")
    }
    fn temple_line(at_ms: u64) -> String {
        stamped(at_ms, ": You have entered The Temple of Atzoatl.")
    }
    fn map_line(at_ms: u64) -> String {
        stamped(at_ms, ": You have entered Ancient City.")
    }

    /// The production path of `temple::trigger::on_client_line` without the app.
    struct Temple {
        arm: ArmState,
        prompt: ReadPromptState,
    }
    impl Temple {
        fn new() -> Self {
            Self { arm: ArmState::default(), prompt: ReadPromptState::default() }
        }
        fn line(&mut self, line: &str) -> Transition {
            let event = classify(line, NOW);
            let transition = apply_line(&mut self.arm, line, NOW);
            if let Some(change) = temple_change(event, self.arm.arm.is_armed(NOW)) {
                self.prompt.apply(change);
            }
            transition
        }
        fn shown(&self) -> Option<Mechanic> {
            self.prompt.snapshot().mechanic
        }
    }

    #[test]
    fn each_armed_mechanic_is_the_one_shown() {
        for mechanic in [Mechanic::DivineFont, Mechanic::Incursion, Mechanic::Mercenary] {
            let mut prompt = ReadPromptState::default();
            prompt.arm(mechanic);
            assert_eq!(prompt.snapshot().mechanic, Some(mechanic));
        }
    }

    #[test]
    fn each_font_round_arm_is_a_new_prompt() {
        let mut prompt = ReadPromptState::default();
        prompt.arm(Mechanic::DivineFont);
        let first = prompt.snapshot();
        prompt.arm(Mechanic::DivineFont);
        let second = prompt.snapshot();
        assert_eq!(second.mechanic, Some(Mechanic::DivineFont));
        assert_ne!(first.seq, second.seq);
    }

    #[test]
    fn reading_the_shown_mechanic_shows_the_older_active_one_again() {
        let mut prompt = ReadPromptState::default();
        prompt.arm(Mechanic::Incursion);
        prompt.arm(Mechanic::Mercenary);
        prompt.read_done(Mechanic::Mercenary);
        assert_eq!(prompt.snapshot().mechanic, Some(Mechanic::Incursion));
        prompt.read_done(Mechanic::Incursion);
        assert_eq!(prompt.snapshot().mechanic, None);
    }

    #[test]
    fn a_context_end_hides_its_prompt() {
        let mut prompt = ReadPromptState::default();
        prompt.arm(Mechanic::DivineFont);
        prompt.end(Mechanic::DivineFont);
        assert_eq!(prompt.snapshot().mechanic, None);
    }

    #[test]
    fn an_area_change_ends_only_the_mercenary_prompt() {
        let mut prompt = ReadPromptState::default();
        prompt.arm(Mechanic::DivineFont);
        prompt.arm(Mechanic::Incursion);
        prompt.arm(Mechanic::Mercenary);
        prompt.area_changed();
        assert_eq!(prompt.snapshot().mechanic, Some(Mechanic::Incursion));
        prompt.end(Mechanic::Incursion);
        assert_eq!(prompt.snapshot().mechanic, Some(Mechanic::DivineFont));
    }

    #[test]
    fn the_temple_area_shows_the_prompt_again_after_a_read_under_a_start_arm() {
        let mut temple = Temple::new();
        temple.line(&alva_start(NOW));
        assert_eq!(temple.shown(), Some(Mechanic::Incursion));
        temple.prompt.read_done(Mechanic::Incursion);
        assert_eq!(temple.shown(), None);
        let transition = temple.line(&temple_line(NOW));
        assert_eq!(transition, Transition::Extended(ArmReason::TempleArea));
        assert_eq!(temple.shown(), Some(Mechanic::Incursion));
    }

    #[test]
    fn the_temple_area_with_no_start_shows_the_prompt() {
        let mut temple = Temple::new();
        temple.line(&temple_line(NOW));
        assert_eq!(temple.shown(), Some(Mechanic::Incursion));
    }

    #[test]
    fn alva_banter_inside_the_temple_keeps_the_prompt() {
        let mut temple = Temple::new();
        temple.line(&temple_line(NOW));
        temple.line(&alva_banter(NOW));
        assert_eq!(temple.shown(), Some(Mechanic::Incursion));
    }

    #[test]
    fn leaving_the_area_clears_the_temple_prompt() {
        let mut temple = Temple::new();
        temple.line(&temple_line(NOW));
        temple.line(&map_line(NOW));
        assert_eq!(temple.shown(), None);
    }

    #[test]
    fn an_end_line_map_side_clears_a_start_prompt() {
        let mut temple = Temple::new();
        temple.line(&alva_start(NOW));
        temple.line(&alva_end(NOW));
        assert_eq!(temple.shown(), None);
    }

    #[test]
    fn a_stale_start_line_changes_nothing() {
        let mut temple = Temple::new();
        temple.line(&alva_start(NOW - crate::client_log::LINE_STALE_MS - 60_000));
        assert_eq!(temple.prompt, ReadPromptState::default());
    }

    #[test]
    fn the_app_state_path_reports_a_change_only_when_the_snapshot_moved() {
        let state = crate::settings::tests::test_app_state();
        assert!(apply_in(&state, Change::Arm(Mechanic::Mercenary)));
        assert!(!read_done_in(&state, Mechanic::Incursion));
        assert!(area_changed_in(&state));
        assert!(!area_changed_in(&state));
        assert_eq!(state.read_prompt.lock().unwrap().snapshot().mechanic, None);
    }
}
