use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum LabState {
    /// Not in lab, or no font interaction detected
    Idle,
    /// Font UI is open — screen reader should activate
    FontReady,
    /// Actively reading gem tooltips from screen
    PickingGems,
    /// Font interaction complete
    Done,
}

#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub enum LabEvent {
    FontOpened,
    FontClosed,
    ZoneChanged { area: String },
    GameFocused,
    GameBlurred,
}

pub struct LabStateMachine {
    state: LabState,
}

impl LabStateMachine {
    pub fn new() -> Self {
        Self {
            state: LabState::Idle,
        }
    }

    #[allow(dead_code)]
    pub fn state(&self) -> &LabState {
        &self.state
    }

    /// Process a log line and return any state transition event.
    pub fn process_line(&mut self, line: &str) -> Option<LabEvent> {
        // Game focus events — fire regardless of lab state
        if line.contains("[WINDOW] Gained focus") {
            return Some(LabEvent::GameFocused);
        }
        if line.contains("[WINDOW] Lost focus") {
            return Some(LabEvent::GameBlurred);
        }

        // Font opened — the reliable trigger
        if line.contains("InstanceClientLabyrinthCraftResultOptionsList recieved") {
            let prev = self.state.clone();
            self.state = LabState::FontReady;
            log::info!("Lab state: {:?} -> FontReady", prev);
            return Some(LabEvent::FontOpened);
        }

        // Zone change while picking — stop screen reading
        if matches!(self.state, LabState::FontReady | LabState::PickingGems) {
            if let Some(area) = parse_area_change(line) {
                log::info!("Lab state: {:?} -> Done (zone change: {})", self.state, area);
                self.state = LabState::Done;
                return Some(LabEvent::ZoneChanged {
                    area: area.to_string(),
                });
            }
        }

        // Auto-transition Done -> Idle
        if self.state == LabState::Done {
            self.state = LabState::Idle;
        }

        None
    }

    /// Transition from FontReady to PickingGems (called when screen reader starts).
    pub fn start_picking(&mut self) {
        if self.state == LabState::FontReady {
            self.state = LabState::PickingGems;
            log::info!("Lab state: FontReady -> PickingGems");
        }
    }

    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.state = LabState::Idle;
    }
}

fn parse_area_change(line: &str) -> Option<&str> {
    // Match: Generating level NN area "AREA_NAME"
    let marker = "Generating level ";
    let idx = line.find(marker)?;
    let rest = &line[idx + marker.len()..];
    let area_start = rest.find('"')? + 1;
    let area_end = rest[area_start..].find('"')? + area_start;
    Some(&rest[area_start..area_end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_trigger_transitions_to_font_ready() {
        let mut sm = LabStateMachine::new();
        let event = sm.process_line(
            "2026/03/23 19:40:28 197017734 1183f92e [INFO Client 39848] InstanceClientLabyrinthCraftResultOptionsList recieved",
        );
        assert_eq!(*sm.state(), LabState::FontReady);
        assert!(matches!(event, Some(LabEvent::FontOpened)));
    }

    #[test]
    fn zone_change_while_picking_transitions_to_done() {
        let mut sm = LabStateMachine::new();
        sm.process_line(
            "[INFO Client] InstanceClientLabyrinthCraftResultOptionsList recieved",
        );
        sm.start_picking();
        assert_eq!(*sm.state(), LabState::PickingGems);

        let event = sm.process_line(
            r#"[DEBUG Client 39848] Generating level 69 area "2_10_town" with seed 1"#,
        );
        assert_eq!(*sm.state(), LabState::Done);
        assert!(matches!(event, Some(LabEvent::ZoneChanged { .. })));
    }

    #[test]
    fn second_font_trigger_resets_to_font_ready() {
        let mut sm = LabStateMachine::new();
        sm.process_line(
            "[INFO Client] InstanceClientLabyrinthCraftResultOptionsList recieved",
        );
        sm.start_picking();
        assert_eq!(*sm.state(), LabState::PickingGems);

        // Another font usage
        let event = sm.process_line(
            "[INFO Client] InstanceClientLabyrinthCraftResultOptionsList recieved",
        );
        assert_eq!(*sm.state(), LabState::FontReady);
        assert!(matches!(event, Some(LabEvent::FontOpened)));
    }

    #[test]
    fn idle_ignores_zone_changes() {
        let mut sm = LabStateMachine::new();
        let event = sm.process_line(
            r#"[DEBUG Client] Generating level 75 area "EndGame_Labyrinth_boss_1" with seed 123"#,
        );
        assert_eq!(*sm.state(), LabState::Idle);
        assert!(event.is_none());
    }

    #[test]
    fn parse_area_change_extracts_name() {
        let line = r#"[DEBUG Client 39848] Generating level 69 area "2_10_town" with seed 1"#;
        assert_eq!(parse_area_change(line), Some("2_10_town"));
    }

    #[test]
    fn done_auto_transitions_to_idle() {
        let mut sm = LabStateMachine::new();
        // Get to Done state
        sm.process_line("[INFO Client] InstanceClientLabyrinthCraftResultOptionsList recieved");
        sm.start_picking();
        sm.process_line(r#"[DEBUG Client] Generating level 69 area "2_10_town" with seed 1"#);
        assert_eq!(*sm.state(), LabState::Done);

        // Any unrelated line should auto-transition to Idle
        let event = sm.process_line("[INFO Client] some unrelated log line");
        assert_eq!(*sm.state(), LabState::Idle);
        assert!(event.is_none());
    }

    #[test]
    fn zone_change_while_font_ready_transitions_to_done() {
        let mut sm = LabStateMachine::new();
        sm.process_line("[INFO Client] InstanceClientLabyrinthCraftResultOptionsList recieved");
        assert_eq!(*sm.state(), LabState::FontReady);

        // Player leaves without picking — zone change while FontReady
        let event = sm.process_line(
            r#"[DEBUG Client] Generating level 69 area "2_10_town" with seed 1"#,
        );
        assert_eq!(*sm.state(), LabState::Done);
        assert!(matches!(event, Some(LabEvent::ZoneChanged { .. })));
    }

    #[test]
    fn game_focus_events() {
        let mut sm = LabStateMachine::new();
        let event = sm.process_line(
            "2026/03/28 10:00:00 [DEBUG Client 12345] [WINDOW] Gained focus",
        );
        assert!(matches!(event, Some(LabEvent::GameFocused)));
        // Focus events don't change lab state
        assert_eq!(*sm.state(), LabState::Idle);

        let event = sm.process_line(
            "2026/03/28 10:00:01 [DEBUG Client 12345] [WINDOW] Lost focus",
        );
        assert!(matches!(event, Some(LabEvent::GameBlurred)));
        assert_eq!(*sm.state(), LabState::Idle);
    }

    #[test]
    fn focus_events_fire_during_picking() {
        let mut sm = LabStateMachine::new();
        sm.process_line("[INFO Client] InstanceClientLabyrinthCraftResultOptionsList recieved");
        sm.start_picking();
        assert_eq!(*sm.state(), LabState::PickingGems);

        // Focus events should fire even during PickingGems (no state change)
        let event = sm.process_line("[DEBUG Client] [WINDOW] Lost focus");
        assert!(matches!(event, Some(LabEvent::GameBlurred)));
        assert_eq!(*sm.state(), LabState::PickingGems);
    }

    #[test]
    fn parse_area_change_handles_malformed_input() {
        // No quotes
        assert_eq!(parse_area_change("Generating level 75 area no_quotes"), None);
        // Only one quote
        assert_eq!(parse_area_change(r#"Generating level 75 area "unclosed"#), None);
        // No Generating level marker
        assert_eq!(parse_area_change(r#"some other line "with quotes""#), None);
    }
}
