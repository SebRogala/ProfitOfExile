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

/// The active mechanics in arm order, each with the `seq` it was armed at,
/// and the arm count.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadPromptState {
    active: Vec<(Mechanic, u64)>,
    seq: u64,
    lab_context: u64,
}

impl ReadPromptState {
    /// Shows `mechanic` as a new prompt, even when it was already active.
    pub fn arm(&mut self, mechanic: Mechanic) {
        self.seq += 1;
        self.active.retain(|(m, _)| *m != mechanic);
        self.active.push((mechanic, self.seq));
    }
    /// The identity of `mechanic`'s live arm, taken when a press is accepted
    /// so its read completes only that arm.
    pub fn armed_id(&self, mechanic: Mechanic) -> Option<u64> {
        self.active.iter().find(|(m, _)| *m == mechanic).map(|(_, id)| *id)
    }
    /// A read of `mechanic`'s arm `id` succeeded. A newer arm of the same
    /// mechanic, made while that read ran, stays shown.
    pub fn read_done(&mut self, mechanic: Mechanic, id: u64) {
        self.active.retain(|entry| *entry != (mechanic, id));
    }
    /// The context of `mechanic` ended.
    pub fn end(&mut self, mechanic: Mechanic) {
        self.active.retain(|(m, _)| *m != mechanic);
        if mechanic == Mechanic::DivineFont {
            self.lab_context = 0;
        }
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
            mechanic: self.active.last().map(|(m, _)| *m),
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

pub fn lab_nav_change(event: &crate::lab_navigation::NavEvent) -> Option<Change> {
    use crate::lab_navigation::NavEvent;
    match event {
        NavEvent::LabFinished => Some(Change::Arm(Mechanic::DivineFont)),
        NavEvent::LabExited => Some(Change::End(Mechanic::DivineFont)),
        _ => None,
    }
}

pub fn lab_event_change(event: &crate::lab_state::LabEvent) -> Option<Change> {
    use crate::lab_state::LabEvent;
    match event {
        LabEvent::FontOpened => Some(Change::Arm(Mechanic::DivineFont)),
        LabEvent::ZoneChanged { .. } => Some(Change::End(Mechanic::DivineFont)),
        _ => None,
    }
}

/// An idle seal may arrive after a portal return armed another font context.
/// Check its identity under the same prompt guard as the replacement arm.
pub fn end_lab_context_in(state: &crate::AppState, ended: u64) -> bool {
    edit_in(state, |p| {
        if p.lab_context == ended {
            p.end(Mechanic::DivineFont);
        }
    })
}

/// Runs `edit` on the state's prompt; true when the snapshot changed.
fn edit_in(state: &crate::AppState, edit: impl FnOnce(&mut ReadPromptState)) -> bool {
    let mut prompt = state.read_prompt.lock().unwrap_or_else(|e| e.into_inner());
    let before = prompt.snapshot();
    edit(&mut prompt);
    prompt.snapshot() != before
}

/// Whether `mechanic`'s module is on: the one check the arm entry and the
/// module toggle go through.
fn module_on(state: &crate::AppState, mechanic: Mechanic) -> bool {
    let id = match mechanic {
        // The lab has no entry in the module registry; its overlay switch is its on/off.
        Mechanic::DivineFont => {
            return *state.lab_overlays_enabled.lock().unwrap_or_else(|e| e.into_inner());
        }
        Mechanic::Incursion => "temple",
        Mechanic::Mercenary => crate::mercenary::MODULE_ID,
    };
    let enabled = state.modules_enabled.lock().unwrap_or_else(|e| e.into_inner()).get(id) == Some(&true);
    enabled
}

/// Applies `change`; an arm of a mechanic whose module is off shows nothing.
pub fn apply_in(state: &crate::AppState, change: Change) -> bool {
    apply_paused_in(state, change, &mut || {})
}
/// [`apply_in`] with `pause` run between the unlocked module check and the
/// prompt edit, where a concurrent module toggle can land.
fn apply_paused_in(state: &crate::AppState, change: Change, pause: &mut dyn FnMut()) -> bool {
    if let Change::Arm(mechanic) = change {
        if !module_on(state, mechanic) {
            return false;
        }
    }
    pause();
    // Divine Font takes font_session before read_prompt, then the module flag.
    // The lifecycle guard keeps take_font_upload from ending the context
    // between the token check and prompt insertion. Module toggles release
    // their flag before taking read_prompt, preserving WI-1's lock order.
    let _font_session = matches!(change, Change::Arm(Mechanic::DivineFont))
        .then(|| state.lab_scan.font_session.lock().unwrap_or_else(|e| e.into_inner()));
    edit_in(state, |p| match change {
        Change::Arm(mechanic) if !module_on(state, mechanic) => {}
        Change::Arm(Mechanic::DivineFont) => {
            let token = state.lab_scan.font_context.load(std::sync::atomic::Ordering::SeqCst);
            if token != 0 {
                p.lab_context = token;
                p.arm(Mechanic::DivineFont);
            }
        }
        _ => p.apply(change),
    })
}
/// Hides the prompt of every mechanic whose module is now off.
pub fn modules_changed_in(state: &crate::AppState) -> bool {
    edit_in(state, |p| {
        for m in [Mechanic::DivineFont, Mechanic::Incursion, Mechanic::Mercenary] {
            if !module_on(state, m) {
                p.end(m);
            }
        }
    })
}
pub fn read_done_in(state: &crate::AppState, mechanic: Mechanic, id: u64) -> bool {
    edit_in(state, |p| p.read_done(mechanic, id))
}
pub fn armed_id_in(state: &crate::AppState, mechanic: Mechanic) -> Option<u64> {
    state.read_prompt.lock().unwrap_or_else(|e| e.into_inner()).armed_id(mechanic)
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
/// A read of the arm `id` (from [`armed_id_in`] at the press) succeeded;
/// `None` (nothing was armed at the press) clears nothing.
pub fn read_done(app: &AppHandle, mechanic: Mechanic, id: Option<u64>) {
    if let Some(id) = id {
        emit_if(app, read_done_in(&app.state::<crate::AppState>(), mechanic, id));
    }
}
pub fn modules_changed(app: &AppHandle) {
    emit_if(app, modules_changed_in(&app.state::<crate::AppState>()));
}
pub fn end_lab_context(app: &AppHandle, ended: u64) {
    emit_if(app, end_lab_context_in(&app.state::<crate::AppState>(), ended));
}
pub fn area_changed(app: &AppHandle) {
    emit_if(app, area_changed_in(&app.state::<crate::AppState>()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::temple::trigger::{apply_line, classify, ArmReason, ArmState, Transition};

    const NOW: u64 = 1_756_800_000_000;

    const FONT_OPEN: &str = "[INFO Client 1] InstanceClientLabyrinthCraftResultOptionsList recieved";

    fn lab_line(prompt: &mut ReadPromptState, sm: &mut crate::lab_state::LabStateMachine, line: &str) {
        if let Some(event) = crate::lab_navigation::parse_nav_event(line, true) {
            if let Some(change) = lab_nav_change(&event) { prompt.apply(change); }
        }
        if let Some(event) = sm.process_line(line) {
            if let Some(change) = lab_event_change(&event) { prompt.apply(change); }
        }
    }

    #[test]
    fn izaros_defeat_shows_the_divine_font_prompt() {
        let mut prompt = ReadPromptState::default();
        lab_line(&mut prompt, &mut crate::lab_state::LabStateMachine::new(),
            "[INFO Client 1] Izaro: I die for the Empire!");
        assert_eq!(prompt.snapshot().mechanic, Some(Mechanic::DivineFont));
    }

    #[test]
    fn each_font_open_line_shows_a_fresh_prompt() {
        let mut prompt = ReadPromptState::default();
        let mut sm = crate::lab_state::LabStateMachine::new();
        lab_line(&mut prompt, &mut sm, FONT_OPEN);
        let first = prompt.armed_id(Mechanic::DivineFont).expect("first font prompt");
        lab_line(&mut prompt, &mut sm, FONT_OPEN);
        assert!(prompt.armed_id(Mechanic::DivineFont).unwrap() > first);
    }

    #[test]
    fn a_zone_change_ends_the_font_prompt() {
        let mut prompt = ReadPromptState::default();
        prompt.arm(Mechanic::DivineFont);
        let mut sm = crate::lab_state::LabStateMachine::new();
        sm.process_line(FONT_OPEN);
        let event = sm.process_line(r#"Generating level 69 area "2_10_town" with seed 1"#).unwrap();
        assert!(matches!(event, crate::lab_state::LabEvent::ZoneChanged { .. }));
        if let Some(change) = lab_event_change(&event) { prompt.apply(change); }
        assert_eq!(prompt.snapshot().mechanic, None);
    }

    #[test]
    fn a_lab_exit_ends_the_font_prompt_without_a_font_open() {
        let mut prompt = ReadPromptState::default();
        prompt.arm(Mechanic::DivineFont);
        lab_line(&mut prompt, &mut crate::lab_state::LabStateMachine::new(),
            "[INFO Client 1] : You have entered Lioneye's Watch.");
        assert_eq!(prompt.snapshot().mechanic, None);
    }

    #[test]
    fn unrelated_lab_lines_leave_the_prompt_unchanged() {
        let mut prompt = ReadPromptState::default();
        prompt.arm(Mechanic::Mercenary);
        let before = prompt.snapshot();
        lab_line(&mut prompt, &mut crate::lab_state::LabStateMachine::new(),
            "[INFO Client 1] Izaro: Ascend with precision.");
        assert_eq!(prompt.snapshot(), before);
    }

    #[test]
    fn a_font_open_with_lab_overlays_off_shows_no_prompt() {
        let state = crate::settings::tests::test_app_state();
        *state.lab_overlays_enabled.lock().unwrap() = false;
        let event = crate::lab_state::LabStateMachine::new().process_line(FONT_OPEN).unwrap();
        apply_in(&state, lab_event_change(&event).expect("font open arms"));
        assert_eq!(shown_in(&state), None);
    }

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
        prompt.read_done(Mechanic::Mercenary, prompt.armed_id(Mechanic::Mercenary).unwrap());
        assert_eq!(prompt.snapshot().mechanic, Some(Mechanic::Incursion));
        prompt.read_done(Mechanic::Incursion, prompt.armed_id(Mechanic::Incursion).unwrap());
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
        let a = temple.prompt.armed_id(Mechanic::Incursion).unwrap();
        temple.prompt.read_done(Mechanic::Incursion, a);
        assert_eq!(temple.shown(), None);
        let transition = temple.line(&temple_line(NOW));
        assert_eq!(transition, Transition::Extended(ArmReason::TempleArea));
        assert_eq!(temple.shown(), Some(Mechanic::Incursion));
    }

    #[test]
    fn a_read_of_an_older_arm_leaves_a_temple_re_arm_made_during_it() {
        let mut temple = Temple::new();
        temple.line(&alva_start(NOW));
        let a = temple.prompt.armed_id(Mechanic::Incursion).unwrap();
        temple.line(&temple_line(NOW));
        temple.prompt.read_done(Mechanic::Incursion, a);
        assert_eq!(temple.shown(), Some(Mechanic::Incursion));
        let b = temple.prompt.armed_id(Mechanic::Incursion).unwrap();
        temple.prompt.read_done(Mechanic::Incursion, b);
        assert_eq!(temple.shown(), None);
    }

    #[test]
    fn a_mercenary_arm_during_a_temple_read_does_not_spoil_its_completion() {
        let mut temple = Temple::new();
        temple.line(&alva_start(NOW));
        let a = temple.prompt.armed_id(Mechanic::Incursion).unwrap();
        temple.prompt.arm(Mechanic::Mercenary);
        temple.prompt.read_done(Mechanic::Incursion, a);
        temple.prompt.end(Mechanic::Mercenary);
        assert_eq!(temple.shown(), None);
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

    fn set_module(state: &crate::AppState, mechanic: Mechanic, on: bool) {
        match mechanic {
            Mechanic::DivineFont => *state.lab_overlays_enabled.lock().unwrap() = on,
            Mechanic::Incursion => {
                state.modules_enabled.lock().unwrap().insert("temple".into(), on);
            }
            Mechanic::Mercenary => {
                state.modules_enabled.lock().unwrap().insert(crate::mercenary::MODULE_ID.into(), on);
            }
        }
    }
    fn shown_in(state: &crate::AppState) -> Option<Mechanic> {
        state.read_prompt.lock().unwrap().snapshot().mechanic
    }

    #[test]
    fn a_divine_font_arm_without_a_context_publishes_no_prompt() {
        let state = crate::settings::tests::test_app_state();
        set_module(&state, Mechanic::DivineFont, true);
        assert!(!apply_in(&state, Change::Arm(Mechanic::DivineFont)));
        assert_eq!(shown_in(&state), None);
    }

    #[test]
    fn each_mechanic_arms_only_while_its_own_module_is_on() {
        let all = [Mechanic::DivineFont, Mechanic::Incursion, Mechanic::Mercenary];
        for mechanic in all {
            let state = crate::settings::tests::test_app_state();
            for other in all {
                set_module(&state, other, other != mechanic);
            }
            assert!(!apply_in(&state, Change::Arm(mechanic)), "{mechanic:?} armed with its module off");
            assert_eq!(shown_in(&state), None);
            set_module(&state, mechanic, true);
            if mechanic == Mechanic::DivineFont {
                crate::lab::font_context::arm_font_context(&state);
            }
            assert!(apply_in(&state, Change::Arm(mechanic)));
            assert_eq!(shown_in(&state), Some(mechanic));
        }
    }

    #[test]
    fn a_temple_line_with_the_temple_module_off_shows_no_prompt() {
        let state = crate::settings::tests::test_app_state();
        set_module(&state, Mechanic::Incursion, false);
        let line = temple_line(NOW);
        let mut arm = ArmState::default();
        let event = classify(&line, NOW);
        apply_line(&mut arm, &line, NOW);
        let change = temple_change(event, arm.arm.is_armed(NOW));
        assert_eq!(change, Some(Change::Arm(Mechanic::Incursion)));
        apply_in(&state, change.unwrap());
        assert_eq!(shown_in(&state), None);
    }

    #[test]
    fn turning_the_temple_module_off_hides_its_shown_prompt_only() {
        let state = crate::settings::tests::test_app_state();
        set_module(&state, Mechanic::Mercenary, true);
        set_module(&state, Mechanic::Incursion, true);
        apply_in(&state, Change::Arm(Mechanic::Mercenary));
        apply_in(&state, Change::Arm(Mechanic::Incursion));
        set_module(&state, Mechanic::Incursion, false);
        assert!(modules_changed_in(&state));
        assert_eq!(shown_in(&state), Some(Mechanic::Mercenary));
    }

    #[test]
    fn a_module_turned_off_during_an_arm_leaves_no_prompt() {
        let state = crate::settings::tests::test_app_state();
        set_module(&state, Mechanic::Incursion, true);
        apply_paused_in(&state, Change::Arm(Mechanic::Incursion), &mut || {
            set_module(&state, Mechanic::Incursion, false);
            modules_changed_in(&state);
        });
        assert_eq!(shown_in(&state), None);
    }

    #[test]
    fn the_app_state_path_reports_a_change_only_when_the_snapshot_moved() {
        let state = crate::settings::tests::test_app_state();
        set_module(&state, Mechanic::Mercenary, true);
        assert!(apply_in(&state, Change::Arm(Mechanic::Mercenary)));
        let merc = armed_id_in(&state, Mechanic::Mercenary).unwrap();
        assert!(!read_done_in(&state, Mechanic::Incursion, merc));
        assert!(area_changed_in(&state));
        assert!(!area_changed_in(&state));
        assert_eq!(state.read_prompt.lock().unwrap().snapshot().mechanic, None);
    }
}
