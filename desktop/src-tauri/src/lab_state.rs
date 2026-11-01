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
pub enum LabEvent {
    FontOpened,
    #[allow(dead_code)]
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

        // Fires on font OPEN and again on CRAFT click (same line both times), so
        // open→stash→reopen→CRAFT yields 3 events. Never a craft count.
        // See desktop/src/lib/README.md → Game UI Context.
        if line.contains("InstanceClientLabyrinthCraftResultOptionsList recieved") {
            let prev = self.state.clone();
            self.state = LabState::FontReady;
            log::info!("Lab state: {:?} -> FontReady", prev);
            return Some(LabEvent::FontOpened);
        }

        // Zone change while picking — stop screen reading
        if matches!(self.state, LabState::FontReady | LabState::PickingGems) {
            if let Some(change) = parse_area_change(line) {
                log::info!("Lab state: {:?} -> Done (zone change: {})", self.state, change.area);
                self.state = LabState::Done;
                return Some(LabEvent::ZoneChanged {
                    area: change.area.to_string(),
                });
            }
        }

        // Auto-transition Done -> Idle
        if self.state == LabState::Done {
            self.state = LabState::Idle;
        }

        None
    }

    /// Set state to PickingGems directly.
    #[cfg(test)]
    pub fn set_picking(&mut self) {
        log::info!("Lab state: {:?} -> PickingGems", self.state);
        self.state = LabState::PickingGems;
    }

    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.state = LabState::Idle;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AreaKind {
    Combat,
    Town,
    Hideout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AreaChange<'a> {
    pub level: u32,
    pub area: &'a str,
    pub seed: Option<u32>,
    pub kind: AreaKind,
}

pub fn parse_area_change(line: &str) -> Option<AreaChange<'_>> {
    // A missing/malformed seed must not suppress the zone change that stops lab OCR.
    let marker = "Generating level ";
    let idx = line.find(marker)?;
    let rest = &line[idx + marker.len()..];
    let (level, area) = rest.split_once(" area \"")?;
    let level = level.parse().ok()?;
    let area_end = area.find('"')?;
    let seed = area[area_end + 1..].strip_prefix(" with seed ")
        .and_then(|suffix| suffix.split_whitespace().next())
        .and_then(|seed| seed.parse().ok());
    let area = &area[..area_end];
    let kind = if area.starts_with("Hideout") {
        AreaKind::Hideout
    } else if area.ends_with("_town") {
        AreaKind::Town
    } else {
        AreaKind::Combat
    };
    Some(AreaChange { level, area, seed, kind })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boss_area_retains_seed_attribution() {
        let parsed = parse_area_change(r#"Generating level 83 area "BetrayalMastermindFight" with seed 2667568808"#);
        assert_eq!(serde_json::to_value(parsed).unwrap(), serde_json::json!({
            "level": 83, "area": "BetrayalMastermindFight", "seed": 2667568808_u64,
            "kind": "combat"
        }));
    }

    #[test]
    fn seedless_area_preserves_zone_attribution() {
        assert_eq!(serde_json::to_value(parse_area_change(r#"Generating level 75 area "EndGame_Labyrinth_boss_1""#)).unwrap(),
            serde_json::json!({"level": 75, "area": "EndGame_Labyrinth_boss_1", "seed": null, "kind": "combat"}));
    }

    #[test]
    fn towns_are_classified_at_parse_boundary() {
        let parsed = parse_area_change(r#"Generating level 69 area "2_10_town" with seed 1"#);
        assert_eq!(serde_json::to_value(parsed).unwrap(), serde_json::json!({
            "level": 69, "area": "2_10_town", "seed": 1, "kind": "town"
        }));
    }

    #[test]
    fn hideouts_are_classified_at_parse_boundary() {
        let parsed = parse_area_change(r#"Generating level 60 area "HideoutLuxurious" with seed 0"#);
        assert_eq!(serde_json::to_value(parsed).unwrap(), serde_json::json!({
            "level": 60, "area": "HideoutLuxurious", "seed": 0, "kind": "hideout"
        }));
    }

    #[test]
    fn malformed_seed_preserves_area_without_inventing_seed() {
        assert_eq!(parse_area_change(r#"Generating level 83 area "BetrayalMastermindFight" with seed nope"#),
            Some(AreaChange { level: 83, area: "BetrayalMastermindFight", seed: None, kind: AreaKind::Combat }));
    }

    #[test]
    fn overflowing_seed_preserves_area_without_inventing_seed() {
        assert_eq!(parse_area_change(r#"Generating level 83 area "BetrayalMastermindFight" with seed 4294967296"#),
            Some(AreaChange { level: 83, area: "BetrayalMastermindFight", seed: None, kind: AreaKind::Combat }));
    }

    #[test]
    fn maximum_seed_is_retained() {
        assert_eq!(parse_area_change(r#"Generating level 83 area "BetrayalMastermindFight" with seed 4294967295"#),
            Some(AreaChange { level: 83, area: "BetrayalMastermindFight", seed: Some(u32::MAX), kind: AreaKind::Combat }));
    }

    #[test]
    fn seedless_zone_change_still_stops_lab_reading() {
        let mut sm = LabStateMachine::new();
        sm.set_picking();
        let event = sm.process_line(r#"Generating level 69 area "2_10_town""#);
        assert!(matches!(event, Some(LabEvent::ZoneChanged { area }) if area == "2_10_town"));
        assert_eq!(*sm.state(), LabState::Done);
    }

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
        sm.set_picking();
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
        sm.set_picking();
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
        assert_eq!(parse_area_change(line), Some(AreaChange {
            level: 69, area: "2_10_town", seed: Some(1), kind: AreaKind::Town,
        }));
    }

    #[test]
    fn parse_area_change_rejects_invalid_level() {
        assert_eq!(parse_area_change(r#"Generating level nope area "Labyrinth""#), None);
    }

    #[test]
    fn parse_area_change_retains_offering_level() {
        assert_eq!(
            parse_area_change(r#"Generating level 83 area "EndGame_Labyrinth_boss_3" with seed 1"#),
            Some(AreaChange { level: 83, area: "EndGame_Labyrinth_boss_3", seed: Some(1), kind: AreaKind::Combat }),
        );
    }

    #[test]
    fn done_auto_transitions_to_idle() {
        let mut sm = LabStateMachine::new();
        // Get to Done state
        sm.process_line("[INFO Client] InstanceClientLabyrinthCraftResultOptionsList recieved");
        sm.set_picking();
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
        sm.set_picking();
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
