//! The detected-mechanic and completed no-match prompt's state.
//!
//! Client.txt seams arm a mechanic, a successful read or the context's end
//! clears it, and the SSOT carries the newest active arm that is presented
//! (a `DivineFont` arm is recorded for the read but never presented) or the
//! no-match presentation. Nothing here grabs the screen; a read happens only on the read key
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

// POE-438, Operator 2026-11-20: the Lab per-craft prompt is hidden for now
// because the lookup replaces it; its arm is kept so the read path is unchanged.
const UNPRESENTED: &[Mechanic] = &[Mechanic::DivineFont];

/// What one Client.txt seam does to the prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Arm(Mechanic),
    End(Mechanic),
}

/// The SSOT presentation; each arm or completed result has a fresh identity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadPromptSnapshot {
    pub mechanic: Option<Mechanic>,
    pub no_match: bool,
    /// A recalibration sweep is running for the newest press.
    pub recalibrating: bool,
    pub seq: u64,
    /// Which Divine Font gem slots the current choice's stored read left
    /// uncertain, left to right; the prompt asks for another press.
    pub gem_uncertain: [bool; 3],
    /// The no-match result offers a recalibration: a read press before it
    /// expires may sweep (POE-459). False on the result of such a press.
    pub recalibrate_offer: bool,
}

/// A queued press's result order and the Client.txt arm generation it saw.
#[derive(Debug, Clone, Copy)]
pub struct PressStamp {
    id: u64,
    arm_generation: u64,
    confirming: bool,
}

impl PressStamp {
    /// The press was accepted while a no-match result offered recalibration.
    pub fn confirming(&self) -> bool {
        self.confirming
    }
}

/// Active arms retain their immutable identities beneath the presentation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadPromptState {
    active: Vec<(Mechanic, u64)>,
    seq: u64,
    lab_context: u64,
    last_client_mechanic: Option<Mechanic>,
    no_match: bool,
    recalibrate_offer: bool,
    /// The identity of the shown no-match result, which its expiry names.
    no_match_id: u64,
    recalibrating: bool,
    gem_uncertain: [bool; 3],
    arm_generation: u64,
    accepted: u64,
    finished: u64,
}

impl ReadPromptState {
    /// Records `mechanic` as the newest arm, even when it was already active;
    /// `snapshot` presents it unless it is a `DivineFont` arm.
    pub fn arm(&mut self, mechanic: Mechanic) {
        self.last_client_mechanic = Some(mechanic);
        self.arm_generation += 1;
        self.no_match = false;
        self.recalibrate_offer = false;
        self.seq += 1;
        if mechanic == Mechanic::DivineFont {
            self.gem_uncertain = [false; 3];
        }
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
            self.gem_uncertain = [false; 3];
        }
    }
    /// The gem choice's stored read left `uncertain` slots; a change is a
    /// fresh presentation.
    pub fn gem_read(&mut self, uncertain: [bool; 3]) {
        if self.gem_uncertain != uncertain {
            self.gem_uncertain = uncertain;
            self.seq += 1;
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
            mechanic: if self.no_match || self.recalibrating { None } else { self.active.iter().rev().map(|(m, _)| *m).find(|m| !UNPRESENTED.contains(m)) },
            no_match: self.no_match,
            recalibrating: self.recalibrating,
            seq: self.seq,
            gem_uncertain: self.gem_uncertain,
            recalibrate_offer: self.no_match && self.recalibrate_offer,
        }
    }
    pub fn accept_press(&mut self) -> PressStamp {
        self.accepted += 1;
        PressStamp {
            id: self.accepted,
            arm_generation: self.arm_generation,
            confirming: self.no_match && self.recalibrate_offer,
        }
    }
    /// A confirming press found no mechanic and started the recalibration sweep.
    pub fn begin_sweep(&mut self) {
        self.recalibrating = true;
        self.no_match = false;
        self.recalibrate_offer = false;
        self.seq += 1;
    }
    /// The no-match result `id` outlived its offer window: it goes, and with
    /// it the offer, so the next press reads without sweeping.
    pub fn expire_no_match(&mut self, id: u64) {
        if self.no_match && self.no_match_id == id {
            self.no_match = false;
            self.recalibrate_offer = false;
            self.seq += 1;
        }
    }
    /// The shown no-match result's identity, for its expiry.
    pub fn no_match_id(&self) -> Option<u64> {
        self.no_match.then_some(self.no_match_id)
    }
    pub fn finish_read(&mut self, stamp: PressStamp, outcome: crate::read_hotkey::DispatchOutcome) {
        use crate::read_hotkey::DispatchOutcome;
        if std::mem::take(&mut self.recalibrating) { self.seq += 1; }
        if stamp.id <= self.finished { return; }
        self.finished = stamp.id;
        if stamp.arm_generation != self.arm_generation { return; }
        match outcome {
            DispatchOutcome::NoMatch => {
                self.no_match = true;
                // A press that confirmed an offer and still found nothing
                // gets the guidance, not another offer.
                self.recalibrate_offer = !stamp.confirming;
            }
            DispatchOutcome::Selected(_) => {
                self.no_match = false;
                self.recalibrate_offer = false;
            }
            DispatchOutcome::Failed(_) | DispatchOutcome::Suppressed => return,
        }
        self.seq += 1;
        if self.no_match {
            self.no_match_id = self.seq;
        }
    }
    pub fn last_client_mechanic(&self) -> Option<Mechanic> {
        self.last_client_mechanic
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
        state.read_prompt.lock().unwrap_or_else(|e| e.into_inner()).last_client_mechanic = Some(mechanic);
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
/// Callers hold the lab `detected_gems` guard (a gem read's commit, a gem
/// context's arm or end), so an end cannot land between a commit and its
/// request; `read_prompt` takes no lock after it.
pub fn gem_read_in(state: &crate::AppState, uncertain: [bool; 3]) -> bool {
    edit_in(state, |p| p.gem_read(uncertain))
}
pub fn read_done_in(state: &crate::AppState, mechanic: Mechanic, id: u64) -> bool {
    edit_in(state, |p| p.read_done(mechanic, id))
}
pub fn accept_press_in(state: &crate::AppState) -> PressStamp {
    state.read_prompt.lock().unwrap_or_else(|e| e.into_inner()).accept_press()
}
pub fn finish_read_in(state: &crate::AppState, stamp: PressStamp, outcome: crate::read_hotkey::DispatchOutcome) -> bool {
    edit_in(state, |p| p.finish_read(stamp, outcome))
}
pub fn begin_sweep_in(state: &crate::AppState) -> bool {
    edit_in(state, ReadPromptState::begin_sweep)
}
pub fn begin_sweep(app: &AppHandle) {
    emit_if(app, begin_sweep_in(&app.state::<crate::AppState>()));
}
pub fn accept_press(app: &AppHandle) -> PressStamp {
    accept_press_in(&app.state::<crate::AppState>())
}
pub fn finish_read(app: &AppHandle, stamp: PressStamp, outcome: crate::read_hotkey::DispatchOutcome) {
    let state = app.state::<crate::AppState>();
    let changed = finish_read_in(&state, stamp, outcome);
    emit_if(app, changed);
    let shown = state.read_prompt.lock().unwrap_or_else(|e| e.into_inner()).no_match_id();
    if let (true, Some(id)) = (changed, shown) {
        let seconds = state.read_hotkey.lock().unwrap_or_else(|e| e.into_inner()).recalibrate_seconds;
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(u64::from(seconds)));
            emit_if(&app, expire_no_match_in(&app.state::<crate::AppState>(), id));
        });
    }
}
pub fn expire_no_match_in(state: &crate::AppState, id: u64) -> bool {
    edit_in(state, |p| p.expire_no_match(id))
}
#[cfg(test)]
pub fn no_match_id_in(state: &crate::AppState) -> Option<u64> {
    state.read_prompt.lock().unwrap_or_else(|e| e.into_inner()).no_match_id()
}
pub fn armed_id_in(state: &crate::AppState, mechanic: Mechanic) -> Option<u64> {
    state.read_prompt.lock().unwrap_or_else(|e| e.into_inner()).armed_id(mechanic)
}
pub fn last_client_mechanic_in(state: &crate::AppState) -> Option<Mechanic> {
    state.read_prompt.lock().unwrap_or_else(|e| e.into_inner()).last_client_mechanic()
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

    #[test]
    fn no_match_projects_a_result_over_retained_arms() {
        let mut prompt = ReadPromptState::default();
        prompt.arm(Mechanic::Mercenary);
        let arm = prompt.armed_id(Mechanic::Mercenary);
        let stamp = prompt.accept_press();
        prompt.finish_read(stamp, crate::read_hotkey::DispatchOutcome::NoMatch);
        assert_eq!(prompt.snapshot(), ReadPromptSnapshot { mechanic: None, no_match: true, seq: 2, recalibrating: false, gem_uncertain: [false; 3], recalibrate_offer: true });
        assert_eq!(prompt.armed_id(Mechanic::Mercenary), arm);
        assert_eq!(prompt.last_client_mechanic(), Some(Mechanic::Mercenary));
    }

    #[test]
    fn successive_no_matches_have_fresh_presentation_identities() {
        let mut prompt = ReadPromptState::default();
        let first = prompt.accept_press();
        prompt.finish_read(first, crate::read_hotkey::DispatchOutcome::NoMatch);
        let second = prompt.accept_press();
        prompt.finish_read(second, crate::read_hotkey::DispatchOutcome::NoMatch);
        assert_eq!(prompt.snapshot(), ReadPromptSnapshot { mechanic: None, no_match: true, seq: 2, recalibrating: false, gem_uncertain: [false; 3], recalibrate_offer: false });
    }

    #[test]
    fn selected_result_replaces_no_match_without_clearing_retained_arms() {
        let mut prompt = ReadPromptState::default();
        prompt.arm(Mechanic::Mercenary);
        let stamp = prompt.accept_press();
        prompt.finish_read(stamp, crate::read_hotkey::DispatchOutcome::NoMatch);
        let stamp = prompt.accept_press();
        prompt.finish_read(stamp, crate::read_hotkey::DispatchOutcome::Selected(crate::read_hotkey::ReaderId::Mercenary));
        assert_eq!(prompt.snapshot(), ReadPromptSnapshot { mechanic: Some(Mechanic::Mercenary), no_match: false, seq: 3, recalibrating: false, gem_uncertain: [false; 3], recalibrate_offer: false });
        assert_eq!(prompt.last_client_mechanic(), Some(Mechanic::Mercenary));
    }

    #[test]
    fn failed_or_suppressed_reads_leave_the_presentation_unchanged() {
        for outcome in [crate::read_hotkey::DispatchOutcome::Failed("capture failed".into()), crate::read_hotkey::DispatchOutcome::Suppressed] {
            let mut prompt = ReadPromptState::default();
            prompt.arm(Mechanic::Mercenary);
            let before = prompt.snapshot();
            let stamp = prompt.accept_press();
            prompt.finish_read(stamp, outcome);
            assert_eq!(prompt.snapshot(), before);
        }
    }

    #[test]
    fn older_queued_result_cannot_replace_a_newer_finished_result() {
        let mut prompt = ReadPromptState::default();
        let older = prompt.accept_press();
        let newer = prompt.accept_press();
        prompt.finish_read(newer, crate::read_hotkey::DispatchOutcome::Selected(crate::read_hotkey::ReaderId::Temple));
        prompt.finish_read(older, crate::read_hotkey::DispatchOutcome::NoMatch);
        assert_eq!(prompt.snapshot(), ReadPromptSnapshot { mechanic: None, no_match: false, seq: 1, recalibrating: false, gem_uncertain: [false; 3], recalibrate_offer: false });
    }

    #[test]
    fn a_new_client_arm_supersedes_an_older_no_match_completion() {
        let mut prompt = ReadPromptState::default();
        let stamp = prompt.accept_press();
        prompt.arm(Mechanic::Mercenary);
        prompt.finish_read(stamp, crate::read_hotkey::DispatchOutcome::NoMatch);
        assert_eq!(prompt.snapshot(), ReadPromptSnapshot { mechanic: Some(Mechanic::Mercenary), no_match: false, seq: 1, recalibrating: false, gem_uncertain: [false; 3], recalibrate_offer: false });
    }

    #[test]
    fn an_old_selected_completion_preserves_a_newer_arm_of_the_same_mechanic() {
        let mut prompt = ReadPromptState::default();
        prompt.arm(Mechanic::Mercenary);
        let arm = prompt.armed_id(Mechanic::Mercenary).unwrap();
        let stamp = prompt.accept_press();
        prompt.arm(Mechanic::Mercenary);
        prompt.read_done(Mechanic::Mercenary, arm);
        prompt.finish_read(stamp, crate::read_hotkey::DispatchOutcome::Selected(crate::read_hotkey::ReaderId::Mercenary));
        assert_eq!(prompt.snapshot(), ReadPromptSnapshot { mechanic: Some(Mechanic::Mercenary), no_match: false, seq: 2, recalibrating: false, gem_uncertain: [false; 3], recalibrate_offer: false });
        assert_eq!(prompt.armed_id(Mechanic::Mercenary), Some(2));
    }

    #[test]
    fn a_new_client_arm_replaces_no_match() {
        let mut prompt = ReadPromptState::default();
        let stamp = prompt.accept_press();
        prompt.finish_read(stamp, crate::read_hotkey::DispatchOutcome::NoMatch);
        prompt.arm(Mechanic::Mercenary);
        assert_eq!(prompt.snapshot(), ReadPromptSnapshot { mechanic: Some(Mechanic::Mercenary), no_match: false, seq: 2, recalibrating: false, gem_uncertain: [false; 3], recalibrate_offer: false });
    }

    #[test]
    fn completion_preserves_client_recency() {
        let mut prompt = ReadPromptState::default();
        prompt.apply(Change::Arm(Mechanic::Mercenary));
        prompt.read_done(Mechanic::Mercenary, prompt.armed_id(Mechanic::Mercenary).unwrap());
        assert_eq!(prompt.last_client_mechanic(), Some(Mechanic::Mercenary));
        assert_eq!(prompt.snapshot().mechanic, None);
    }
    #[test]
    fn context_end_preserves_client_recency() {
        let mut prompt = ReadPromptState::default();
        prompt.apply(Change::Arm(Mechanic::Mercenary));
        prompt.area_changed();
        assert_eq!(prompt.last_client_mechanic(), Some(Mechanic::Mercenary));
        assert_eq!(prompt.snapshot().mechanic, None);
    }
    #[test]
    fn font_without_context_still_updates_client_recency() {
        let state = crate::settings::tests::test_app_state();
        assert!(!apply_in(&state, Change::Arm(Mechanic::DivineFont)));
        assert_eq!(last_client_mechanic_in(&state), Some(Mechanic::DivineFont));
        assert_eq!(shown_in(&state), None);
    }

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
    fn izaros_defeat_arms_the_divine_font() {
        let mut prompt = ReadPromptState::default();
        lab_line(&mut prompt, &mut crate::lab_state::LabStateMachine::new(),
            "[INFO Client 1] Izaro: I die for the Empire!");
        assert!(prompt.armed_id(Mechanic::DivineFont).is_some());
        assert_eq!(prompt.snapshot().mechanic, None);
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
        assert!(prompt.armed_id(Mechanic::DivineFont).is_some());
        let mut sm = crate::lab_state::LabStateMachine::new();
        sm.process_line(FONT_OPEN);
        let event = sm.process_line(r#"Generating level 69 area "2_10_town" with seed 1"#).unwrap();
        assert!(matches!(event, crate::lab_state::LabEvent::ZoneChanged { .. }));
        if let Some(change) = lab_event_change(&event) { prompt.apply(change); }
        assert_eq!(prompt.armed_id(Mechanic::DivineFont), None);
        assert_eq!(prompt.snapshot().mechanic, None);
    }

    #[test]
    fn a_lab_exit_ends_the_font_prompt_without_a_font_open() {
        let mut prompt = ReadPromptState::default();
        prompt.arm(Mechanic::DivineFont);
        assert!(prompt.armed_id(Mechanic::DivineFont).is_some());
        lab_line(&mut prompt, &mut crate::lab_state::LabStateMachine::new(),
            "[INFO Client 1] : You have entered Lioneye's Watch.");
        assert_eq!(prompt.armed_id(Mechanic::DivineFont), None);
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
    fn each_font_round_arm_is_a_new_prompt() {
        let mut prompt = ReadPromptState::default();
        prompt.arm(Mechanic::DivineFont);
        let first = prompt.armed_id(Mechanic::DivineFont).unwrap();
        prompt.arm(Mechanic::DivineFont);
        let second = prompt.armed_id(Mechanic::DivineFont).unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn reading_the_shown_mercenary_keeps_the_older_temple_arm() {
        let mut prompt = ReadPromptState::default();
        prompt.arm(Mechanic::Incursion);
        let temple = prompt.armed_id(Mechanic::Incursion);
        prompt.arm(Mechanic::Mercenary);
        assert_eq!(prompt.snapshot().mechanic, Some(Mechanic::Mercenary));
        prompt.read_done(Mechanic::Mercenary, prompt.armed_id(Mechanic::Mercenary).unwrap());
        assert_eq!(prompt.armed_id(Mechanic::Mercenary), None);
        assert_eq!(prompt.armed_id(Mechanic::Incursion), temple);
        prompt.read_done(Mechanic::Incursion, temple.unwrap());
        assert_eq!(prompt.armed_id(Mechanic::Incursion), None);
        assert_eq!(prompt.snapshot().mechanic, None);
    }

    #[test]
    fn a_context_end_hides_its_prompt() {
        let mut prompt = ReadPromptState::default();
        prompt.arm(Mechanic::DivineFont);
        assert!(prompt.armed_id(Mechanic::DivineFont).is_some());
        prompt.end(Mechanic::DivineFont);
        assert_eq!(prompt.armed_id(Mechanic::DivineFont), None);
        assert_eq!(prompt.snapshot().mechanic, None);
    }

    #[test]
    fn an_area_change_ends_only_the_mercenary_prompt() {
        let mut prompt = ReadPromptState::default();
        prompt.arm(Mechanic::DivineFont);
        prompt.arm(Mechanic::Incursion);
        prompt.arm(Mechanic::Mercenary);
        prompt.area_changed();
        assert_eq!(prompt.armed_id(Mechanic::Mercenary), None);
        assert!(prompt.armed_id(Mechanic::Incursion).is_some());
        prompt.end(Mechanic::Incursion);
        assert_eq!(prompt.armed_id(Mechanic::Incursion), None);
        assert!(prompt.armed_id(Mechanic::DivineFont).is_some());
        assert_eq!(prompt.snapshot().mechanic, None);
    }

    #[test]
    fn the_temple_area_re_arms_the_read_after_a_read_under_a_start_arm() {
        let mut temple = Temple::new();
        temple.line(&alva_start(NOW));
        let a = temple.prompt.armed_id(Mechanic::Incursion).unwrap();
        temple.prompt.read_done(Mechanic::Incursion, a);
        assert_eq!(temple.prompt.armed_id(Mechanic::Incursion), None);
        let transition = temple.line(&temple_line(NOW));
        assert_eq!(transition, Transition::Extended(ArmReason::TempleArea));
        let b = temple.prompt.armed_id(Mechanic::Incursion).expect("the temple area re-arms the read");
        assert!(b > a);
    }

    #[test]
    fn a_read_of_an_older_arm_leaves_a_temple_re_arm_made_during_it() {
        let mut temple = Temple::new();
        temple.line(&alva_start(NOW));
        let a = temple.prompt.armed_id(Mechanic::Incursion).unwrap();
        temple.line(&temple_line(NOW));
        let b = temple.prompt.armed_id(Mechanic::Incursion).unwrap();
        assert_ne!(a, b);
        temple.prompt.read_done(Mechanic::Incursion, a);
        assert_eq!(temple.prompt.armed_id(Mechanic::Incursion), Some(b));
        temple.prompt.read_done(Mechanic::Incursion, b);
        assert_eq!(temple.prompt.armed_id(Mechanic::Incursion), None);
    }

    #[test]
    fn a_mercenary_arm_during_a_temple_read_does_not_spoil_its_completion() {
        let mut temple = Temple::new();
        temple.line(&alva_start(NOW));
        let a = temple.prompt.armed_id(Mechanic::Incursion).unwrap();
        temple.prompt.arm(Mechanic::Mercenary);
        temple.prompt.read_done(Mechanic::Incursion, a);
        assert_eq!(temple.prompt.armed_id(Mechanic::Incursion), None);
        temple.prompt.end(Mechanic::Mercenary);
        assert_eq!(temple.shown(), None);
    }

    #[test]
    fn alva_banter_inside_the_temple_keeps_the_read_arm() {
        let mut temple = Temple::new();
        temple.line(&temple_line(NOW));
        let arm = temple.prompt.armed_id(Mechanic::Incursion);
        assert!(arm.is_some());
        temple.line(&alva_banter(NOW));
        assert_eq!(temple.prompt.armed_id(Mechanic::Incursion), arm);
    }

    #[test]
    fn leaving_the_area_clears_the_temple_arm() {
        let mut temple = Temple::new();
        temple.line(&temple_line(NOW));
        temple.line(&map_line(NOW));
        assert_eq!(temple.prompt.armed_id(Mechanic::Incursion), None);
    }

    #[test]
    fn an_end_line_map_side_clears_a_start_arm() {
        let mut temple = Temple::new();
        temple.line(&alva_start(NOW));
        temple.line(&alva_end(NOW));
        assert_eq!(temple.prompt.armed_id(Mechanic::Incursion), None);
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
            if mechanic == Mechanic::Mercenary {
                assert_eq!(shown_in(&state), Some(mechanic));
            } else {
                assert!(armed_id_in(&state, mechanic).is_some(), "{mechanic:?} not armed with its module on");
            }
        }
    }

    #[test]
    fn a_temple_line_with_the_temple_module_off_records_no_arm() {
        let state = crate::settings::tests::test_app_state();
        set_module(&state, Mechanic::Incursion, false);
        let line = temple_line(NOW);
        let mut arm = ArmState::default();
        let event = classify(&line, NOW);
        apply_line(&mut arm, &line, NOW);
        let change = temple_change(event, arm.arm.is_armed(NOW));
        assert_eq!(change, Some(Change::Arm(Mechanic::Incursion)));
        apply_in(&state, change.unwrap());
        assert_eq!(armed_id_in(&state, Mechanic::Incursion), None);
    }

    #[test]
    fn turning_the_temple_module_off_ends_its_arm_only() {
        let state = crate::settings::tests::test_app_state();
        set_module(&state, Mechanic::Mercenary, true);
        set_module(&state, Mechanic::Incursion, true);
        apply_in(&state, Change::Arm(Mechanic::Mercenary));
        apply_in(&state, Change::Arm(Mechanic::Incursion));
        let mercenary = armed_id_in(&state, Mechanic::Mercenary);
        set_module(&state, Mechanic::Incursion, false);
        modules_changed_in(&state);
        assert_eq!(armed_id_in(&state, Mechanic::Incursion), None);
        assert_eq!(armed_id_in(&state, Mechanic::Mercenary), mercenary);
        assert_eq!(shown_in(&state), Some(Mechanic::Mercenary));
    }

    #[test]
    fn a_module_turned_off_during_an_arm_leaves_no_prompt() {
        let state = crate::settings::tests::test_app_state();
        set_module(&state, Mechanic::Mercenary, true);
        apply_paused_in(&state, Change::Arm(Mechanic::Mercenary), &mut || {
            set_module(&state, Mechanic::Mercenary, false);
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

    mod keep_happy {
        use super::*;
        use crate::read_hotkey::DispatchOutcome;

        #[test]
        fn an_uncertain_gem_read_is_a_fresh_presentation_of_its_slots() {
            let mut prompt = ReadPromptState::default();
            let before = prompt.snapshot().seq;

            prompt.gem_read([false, true, false]);

            let snapshot = prompt.snapshot();
            assert_eq!(snapshot.gem_uncertain, [false, true, false]);
            assert!(snapshot.seq > before);
        }

        #[test]
        fn the_font_ending_or_opening_again_withdraws_the_uncertain_slots() {
            for change in [Change::End(Mechanic::DivineFont), Change::Arm(Mechanic::DivineFont)] {
                let mut prompt = ReadPromptState::default();
                prompt.gem_read([true, false, false]);

                prompt.apply(change);

                assert_eq!(prompt.snapshot().gem_uncertain, [false; 3], "{change:?}");
            }
        }

        #[test]
        fn a_sweep_presents_recalibrating_until_its_no_match_finishes() {
            let mut prompt = ReadPromptState::default();
            prompt.arm(Mechanic::Mercenary);
            let stamp = prompt.accept_press();
            prompt.begin_sweep();
            assert_eq!(prompt.snapshot(), ReadPromptSnapshot { mechanic: None, no_match: false, recalibrating: true, seq: 2, gem_uncertain: [false; 3], recalibrate_offer: false });
            prompt.finish_read(stamp, DispatchOutcome::NoMatch);
            let done = prompt.snapshot();
            assert!(!done.recalibrating && done.no_match && done.mechanic.is_none(), "{done:?}");
            assert!(done.seq > 2);
        }

        #[test]
        fn a_lab_font_arm_raises_no_prompt_and_keeps_its_read_identity() {
            let state = crate::settings::tests::test_app_state();
            set_module(&state, Mechanic::DivineFont, true);
            crate::lab::font_context::arm_font_context(&state);
            apply_in(&state, Change::Arm(Mechanic::DivineFont));
            assert_eq!(shown_in(&state), None);
            assert!(armed_id_in(&state, Mechanic::DivineFont).is_some());
            assert_eq!(last_client_mechanic_in(&state), Some(Mechanic::DivineFont));
        }

        #[test]
        fn a_temple_arming_line_records_the_read_without_presenting_incursion() {
            for line in [alva_start(NOW), temple_line(NOW)] {
                let mut temple = Temple::new();
                temple.line(&line);
                let arm = temple.prompt.armed_id(Mechanic::Incursion).unwrap_or_else(|| panic!("{line}: no read arm"));
                assert_eq!(temple.prompt.last_client_mechanic(), Some(Mechanic::Incursion), "{line}");
                assert_eq!(temple.shown(), None, "{line}");
                temple.prompt.read_done(Mechanic::Incursion, arm + 1);
                assert_eq!(temple.prompt.armed_id(Mechanic::Incursion), Some(arm), "{line}: another identity consumed the arm");
                temple.prompt.read_done(Mechanic::Incursion, arm);
                assert_eq!(temple.prompt.armed_id(Mechanic::Incursion), None, "{line}");
            }
        }

        #[test]
        fn a_mercenary_arm_still_raises_its_prompt_over_a_live_lab_arm() {
            let state = crate::settings::tests::test_app_state();
            set_module(&state, Mechanic::Mercenary, true);
            set_module(&state, Mechanic::DivineFont, true);
            crate::lab::font_context::arm_font_context(&state);
            apply_in(&state, Change::Arm(Mechanic::Mercenary));
            apply_in(&state, Change::Arm(Mechanic::DivineFont));
            assert!(armed_id_in(&state, Mechanic::DivineFont).is_some());
            assert_eq!(shown_in(&state), Some(Mechanic::Mercenary));
        }
    }

    mod keep_regression {
        use super::*;

        /// POE-460, Operator-reported: a Temple arm leaked "Detected Incursion - press <key>".
        #[test]
        fn only_a_mercenary_arm_is_presented() {
            for (mechanic, shown) in [
                (Mechanic::Mercenary, Some(Mechanic::Mercenary)),
                (Mechanic::Incursion, None),
                (Mechanic::DivineFont, None),
            ] {
                let mut prompt = ReadPromptState::default();
                prompt.arm(mechanic);
                assert_eq!(prompt.snapshot().mechanic, shown, "{mechanic:?}");
            }
        }
    }

    mod keep_critical {
        use super::*;
        use crate::read_hotkey::{DispatchOutcome, ReaderId};

        /// Silent consequence: a status stuck on "running a recalibration sweep".
        #[test]
        fn every_finish_path_clears_the_sweep_status() {
            let finish: [fn(&mut ReadPromptState); 4] = [
                |p| { let s = p.accept_press(); p.begin_sweep(); p.finish_read(s, DispatchOutcome::Failed("x".into())); },
                |p| { let s = p.accept_press(); p.begin_sweep(); p.finish_read(s, DispatchOutcome::Suppressed); },
                |p| {
                    let old = p.accept_press(); let new = p.accept_press();
                    p.finish_read(new, DispatchOutcome::NoMatch); p.begin_sweep();
                    p.finish_read(old, DispatchOutcome::Selected(ReaderId::Temple));
                },
                |p| { let s = p.accept_press(); p.begin_sweep(); p.arm(Mechanic::Mercenary); p.finish_read(s, DispatchOutcome::NoMatch); },
            ];
            for run in finish {
                let mut prompt = ReadPromptState::default();
                run(&mut prompt);
                assert!(!prompt.snapshot().recalibrating, "{:?}", prompt.snapshot());
            }
        }
    }
}
