use serde::Serialize;

/// Navigation events emitted during lab runs.
/// Parsed from Client.txt independently of the font/OCR state machine.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum NavEvent {
    PlazaEntered,
    LabStarted,
    RoomChanged { name: String },
    SectionFinished,
    LabFinished,
    IzaroBattleStarted,
    PortalSpawned,
    LabExited,
}

// Lab room name components (title case as they appear in Client.txt).
const LAB_ROOM_PREFIXES: &[&str] = &[
    "Estate",
    "Domain",
    "Basilica",
    "Mansion",
    "Sepulchre",
    "Sanitorium",
];

const LAB_ROOM_SUFFIXES: &[&str] = &[
    "Walkways",
    "Path",
    "Crossing",
    "Annex",
    "Halls",
    "Passage",
    "Enclosure",
    "Atrium",
];

// Izaro voicelines (from legacy LabCompass, verified in 3.28 Mirage).
const LAB_START_LINES: &[&str] = &[
    "Izaro: Ascend with precision.",
    "Izaro: The Goddess is watching.",
    "Izaro: Justice will prevail.",
];

const IZARO_BATTLE_LINES: &[&str] = &[
    "Izaro: Complex machinations converge to a single act of power.",
    "Izaro: Slowness lends strength to one's enemies.",
    "Izaro: When one defiles the effigy, one defiles the emperor.",
    "Izaro: The essence of an empire must be shared equally amongst all of its citizens.",
    "Izaro: It is the sovereign who empowers the sceptre. Not the other way round.",
    "Izaro: Some things that slumber should never be awoken.",
    "Izaro: An emperor is only as efficient as those he commands.",
    "Izaro: The emperor beckons and the world attends.",
];

const SECTION_FINISH_LINES: &[&str] = &[
    "Izaro: By the Goddess! What ambition!",
    "Izaro: Such resilience!",
    "Izaro: You are inexhaustible!",
    "Izaro: You were born for this!",
];

const LAB_FINISH_LINES: &[&str] = &[
    "Izaro: I die for the Empire!",
    "Izaro: Delight in your gilded dungeon, ascendant.",
    "Izaro: Your destination is more dangerous than the journey, ascendant.",
    "Izaro: Triumphant at last!",
    "Izaro: You are free!",
    "Izaro: The trap of tyranny is inescapable.",
];

const PORTAL_LINE: &str = ": A portal to Izaro appears.";

/// Check if a room name matches the lab room prefix+suffix pattern.
pub fn is_lab_room(name: &str) -> bool {
    let parts: Vec<&str> = name.split_whitespace().collect();
    if parts.len() != 2 {
        return false;
    }
    LAB_ROOM_PREFIXES.contains(&parts[0]) && LAB_ROOM_SUFFIXES.contains(&parts[1])
}

/// Extract the area name from a "You have entered X." log line.
/// Returns None if the line doesn't match the pattern.
pub fn parse_entered_area(line: &str) -> Option<&str> {
    let marker = ": You have entered ";
    let start = line.find(marker)? + marker.len();
    let rest = &line[start..];
    let end = rest.find('.')?;
    Some(&rest[..end])
}

/// Check if a log line contains an Izaro voiceline and return the corresponding event.
fn parse_izaro_line(line: &str) -> Option<NavEvent> {
    for quote in LAB_START_LINES {
        if line.contains(quote) {
            return Some(NavEvent::LabStarted);
        }
    }
    for quote in LAB_FINISH_LINES {
        if line.contains(quote) {
            return Some(NavEvent::LabFinished);
        }
    }
    for quote in SECTION_FINISH_LINES {
        if line.contains(quote) {
            return Some(NavEvent::SectionFinished);
        }
    }
    for quote in IZARO_BATTLE_LINES {
        if line.contains(quote) {
            return Some(NavEvent::IzaroBattleStarted);
        }
    }
    if line.contains(PORTAL_LINE) {
        return Some(NavEvent::PortalSpawned);
    }
    None
}

/// Parse a Client.txt log line for navigation events.
/// `in_lab` indicates whether the player is currently in the labyrinth.
pub fn parse_nav_event(line: &str, in_lab: bool) -> Option<NavEvent> {
    // Check Izaro voicelines first (can fire from any lab state).
    if let Some(event) = parse_izaro_line(line) {
        return Some(event);
    }

    // Check area transitions.
    if let Some(area) = parse_entered_area(line) {
        if area == "Aspirants' Plaza" || area == "Aspirant's Plaza" {
            return Some(NavEvent::PlazaEntered);
        }
        if area == "Aspirant's Trial" {
            return Some(NavEvent::RoomChanged {
                name: area.to_string(),
            });
        }
        if is_lab_room(area) {
            return Some(NavEvent::RoomChanged {
                name: area.to_string(),
            });
        }
        // Non-lab area while in lab → lab exit.
        if in_lab {
            return Some(NavEvent::LabExited);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_lab_room() {
        assert!(is_lab_room("Estate Walkways"));
        assert!(is_lab_room("Domain Crossing"));
        assert!(is_lab_room("Basilica Atrium"));
        assert!(is_lab_room("Mansion Annex"));
        assert!(is_lab_room("Sepulchre Halls"));
        assert!(is_lab_room("Sanitorium Passage"));
        assert!(!is_lab_room("Hideout"));
        assert!(!is_lab_room("Aspirant's Trial"));
        assert!(!is_lab_room("Aspirants' Plaza"));
        assert!(!is_lab_room("The Blood Aqueduct"));
        assert!(!is_lab_room("Estate")); // prefix only
    }

    #[test]
    fn test_parse_entered_area() {
        let line = "2026/04/01 20:00:00 12345678 [INFO Client 1234] : You have entered Estate Walkways.";
        assert_eq!(parse_entered_area(line), Some("Estate Walkways"));

        let line2 = "2026/04/01 20:00:00 12345678 [INFO Client 1234] : You have entered Aspirants' Plaza.";
        assert_eq!(parse_entered_area(line2), Some("Aspirants' Plaza"));

        assert_eq!(parse_entered_area("some random log line"), None);
    }

    #[test]
    fn test_parse_izaro_lines() {
        for quote in LAB_START_LINES {
            let line = format!("2026/04/01 20:00:00 12345678 [INFO Client 1234] {}", quote);
            assert!(matches!(parse_izaro_line(&line), Some(NavEvent::LabStarted)));
        }
        for quote in LAB_FINISH_LINES {
            let line = format!("2026/04/01 20:00:00 12345678 [INFO Client 1234] {}", quote);
            assert!(matches!(parse_izaro_line(&line), Some(NavEvent::LabFinished)));
        }
        for quote in SECTION_FINISH_LINES {
            let line = format!("2026/04/01 20:00:00 12345678 [INFO Client 1234] {}", quote);
            assert!(matches!(parse_izaro_line(&line), Some(NavEvent::SectionFinished)));
        }
        for quote in IZARO_BATTLE_LINES {
            let line = format!("2026/04/01 20:00:00 12345678 [INFO Client 1234] {}", quote);
            assert!(matches!(parse_izaro_line(&line), Some(NavEvent::IzaroBattleStarted)));
        }
        let portal = "2026/04/01 20:00:00 12345678 [INFO Client 1234] : A portal to Izaro appears.";
        assert!(matches!(parse_izaro_line(portal), Some(NavEvent::PortalSpawned)));
    }

    #[test]
    fn test_nav_event_room_changed() {
        let line = "2026/04/01 20:00:00 12345678 [INFO Client 1234] : You have entered Estate Walkways.";
        let event = parse_nav_event(line, true);
        assert!(matches!(event, Some(NavEvent::RoomChanged { ref name }) if name == "Estate Walkways"));
    }

    #[test]
    fn test_nav_event_plaza() {
        let line = "2026/04/01 20:00:00 12345678 [INFO Client 1234] : You have entered Aspirants' Plaza.";
        assert!(matches!(parse_nav_event(line, false), Some(NavEvent::PlazaEntered)));
    }

    #[test]
    fn test_nav_event_lab_exit() {
        let line = "2026/04/01 20:00:00 12345678 [INFO Client 1234] : You have entered Highgate.";
        // In lab → LabExited
        assert!(matches!(parse_nav_event(line, true), Some(NavEvent::LabExited)));
        // Not in lab → None (ignore non-lab areas)
        assert!(parse_nav_event(line, false).is_none());
    }

    #[test]
    fn test_nav_event_aspirants_trial() {
        let line = "2026/04/01 20:00:00 12345678 [INFO Client 1234] : You have entered Aspirant's Trial.";
        let event = parse_nav_event(line, true);
        assert!(matches!(event, Some(NavEvent::RoomChanged { ref name }) if name == "Aspirant's Trial"));
    }
}
