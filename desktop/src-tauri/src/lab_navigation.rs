use serde::Serialize;
use std::io::{Read as _, Seek, SeekFrom};
use std::path::Path;

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
    DarkshrineActivated,
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

const DARKSHRINE_LINES: &[&str] = &[
    "Be twice blessed.",
    "To the worthy go the spoils.",
    "Discover what lies within.",
    "Movement ceases, tension mounts.",
    "The heart quickens, the blood thickens.",
    "Restore that which was lost.",
    "Courage stands tall.",
    "None shall stop you.",
    "Deliver pain exquisite.",
    "Hit hard. Hit once.",
    "Death doesn't wait.",
];

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
    let marker = "You have entered ";
    let start = line.find(marker)? + marker.len();
    let rest = &line[start..];
    let end = rest.find('.')?;
    if end == 0 {
        return None;
    }
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

    // Darkshrine activation — specific text lines from shrine effects.
    if in_lab {
        for text in DARKSHRINE_LINES {
            if line.contains(text) {
                return Some(NavEvent::DarkshrineActivated);
            }
        }
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

/// Reconstruct current lab state from Client.txt on startup/overlay toggle.
///
/// Two-step read to minimize I/O:
/// 1. Read last 32KB — find last "You have entered X" to determine `in_lab`.
///    If not in lab, return immediately (most common case). 32KB covers
///    hundreds of [WINDOW] focus lines that can push area entries far back.
/// 2. Read last 128KB — find last PlazaEntered, parse events forward from there.
///    128KB covers hours of gameplay.
///
/// Returns `(events_to_replay, in_lab)`. Events start from PlazaEntered.
pub fn replay_recent_log(path: &Path) -> (Vec<NavEvent>, bool) {
    let file_len = match std::fs::metadata(path) {
        Ok(m) => m.len(),
        Err(e) => {
            log::warn!("replay_recent_log: metadata failed: {}", e);
            return (vec![], false);
        }
    };
    if file_len == 0 {
        return (vec![], false);
    }

    // Step 1: read to check in_lab — 32KB covers hundreds of [WINDOW] focus lines
    let probe = match read_tail(path, file_len, 32_768) {
        Some(s) => s,
        None => return (vec![], false),
    };
    let mut in_lab = false;
    for line in probe.lines().rev() {
        if let Some(area) = parse_entered_area(line) {
            in_lab = area == "Aspirants' Plaza" || area == "Aspirant's Plaza"
                || area == "Aspirant's Trial"
                || is_lab_room(area);
            break;
        }
    }
    drop(probe); // release 4KB

    if !in_lab {
        log::info!("replay_recent_log: not in lab, skipping replay");
        return (vec![], false);
    }
    log::info!("replay_recent_log: in_lab=true, loading replay data");

    // Step 2: larger read to find PlazaEntered and replay events (128KB)
    let content = match read_tail(path, file_len, 131_072) {
        Some(s) => s,
        None => return (vec![], true),
    };
    let lines: Vec<&str> = content.lines().collect();

    // Scan backwards for last PlazaEntered
    let mut plaza_idx = 0;
    for (i, line) in lines.iter().enumerate().rev() {
        if let Some(area) = parse_entered_area(line) {
            if area == "Aspirants' Plaza" || area == "Aspirant's Plaza" {
                plaza_idx = i;
                break;
            }
        }
    }

    // Parse events forward from plaza
    let mut parse_in_lab = false;
    let mut events: Vec<NavEvent> = Vec::new();
    for line in &lines[plaza_idx..] {
        if line.is_empty() { continue; }
        if let Some(event) = parse_nav_event(line, parse_in_lab) {
            match &event {
                NavEvent::PlazaEntered => parse_in_lab = true,
                NavEvent::LabExited => parse_in_lab = false,
                _ => {}
            }
            events.push(event);
        }
    }

    (events, true)
}

/// Read the last `max_bytes` of a file as a String, skipping the first
/// partial line when seeking mid-file.
fn read_tail(path: &Path, file_len: u64, max_bytes: u64) -> Option<String> {
    let mut file = std::fs::File::open(path).map_err(|e| {
        log::warn!("read_tail: open failed: {}", e);
    }).ok()?;
    let start = if file_len > max_bytes { file_len - max_bytes } else { 0 };
    file.seek(SeekFrom::Start(start)).map_err(|e| {
        log::warn!("read_tail: seek failed: {}", e);
    }).ok()?;
    let mut buf = String::new();
    file.read_to_string(&mut buf).map_err(|e| {
        log::warn!("read_tail: read failed: {}", e);
    }).ok()?;
    if start > 0 {
        if let Some(idx) = buf.find('\n') {
            buf = buf[idx + 1..].to_string();
        }
    }
    Some(buf)
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
    fn test_darkshrine_activated() {
        for text in DARKSHRINE_LINES {
            let line = format!("2026/04/10 20:40:21 1234 [INFO Client 1] : {}", text);
            assert!(matches!(parse_nav_event(&line, true), Some(NavEvent::DarkshrineActivated)),
                "should detect darkshrine for: {}", text);
            // Not in lab → None
            assert!(parse_nav_event(&line, false).is_none(),
                "should not detect darkshrine outside lab for: {}", text);
        }
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

    #[test]
    fn test_replay_recent_log() {
        use std::io::Write;
        let dir = std::env::temp_dir().join("poe_test_replay");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("Client_test.txt");
        let mut f = std::fs::File::create(&path).unwrap();

        // Simulate: hideout → plaza → lab rooms → still in lab
        writeln!(f, "2026/04/01 19:00:00 1234 [INFO Client 1] : You have entered Highgate.").unwrap();
        writeln!(f, "2026/04/01 20:00:00 1234 [INFO Client 1] : You have entered Aspirants' Plaza.").unwrap();
        writeln!(f, "2026/04/01 20:01:00 1234 [INFO Client 1] Izaro: Ascend with precision.").unwrap();
        writeln!(f, "2026/04/01 20:02:00 1234 [INFO Client 1] : You have entered Estate Walkways.").unwrap();
        writeln!(f, "2026/04/01 20:03:00 1234 [INFO Client 1] : You have entered Domain Crossing.").unwrap();
        drop(f);

        let (events, in_lab) = replay_recent_log(&path);
        assert!(in_lab, "should be in lab");
        // Replay should start from PlazaEntered (trimming the Highgate line)
        assert!(matches!(&events[0], NavEvent::PlazaEntered));
        // Should have: Plaza, LabStarted, RoomChanged(Estate), RoomChanged(Domain)
        assert_eq!(events.len(), 4);
        assert!(matches!(&events[2], NavEvent::RoomChanged { name } if name == "Estate Walkways"));
        assert!(matches!(&events[3], NavEvent::RoomChanged { name } if name == "Domain Crossing"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_replay_after_lab_exit() {
        use std::io::Write;
        let dir = std::env::temp_dir().join("poe_test_replay2");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("Client_test2.txt");
        let mut f = std::fs::File::create(&path).unwrap();

        // Lab run completed, player back in hideout
        writeln!(f, "2026/04/01 20:00:00 1234 [INFO Client 1] : You have entered Aspirants' Plaza.").unwrap();
        writeln!(f, "2026/04/01 20:02:00 1234 [INFO Client 1] : You have entered Estate Walkways.").unwrap();
        writeln!(f, "2026/04/01 20:10:00 1234 [INFO Client 1] : You have entered Highgate.").unwrap();
        drop(f);

        let (events, in_lab) = replay_recent_log(&path);
        assert!(!in_lab, "should not be in lab after exit");
        // Not in lab → nothing to replay
        assert!(events.is_empty(), "should have no events when not in lab");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_replay_skips_truncated_first_line() {
        use std::io::Write;
        let dir = std::env::temp_dir().join("poe_test_replay3");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("Client_test3.txt");
        let mut f = std::fs::File::create(&path).unwrap();

        // Write enough data that max_bytes=100 lands mid-file
        // First 200 bytes of padding, then the real events
        for _ in 0..5 {
            writeln!(f, "2026/04/01 19:00:00 1234 [INFO Client 1] Some random long log line padding text here").unwrap();
        }
        writeln!(f, "2026/04/01 20:00:00 1234 [INFO Client 1] : You have entered Aspirants' Plaza.").unwrap();
        writeln!(f, "2026/04/01 20:02:00 1234 [INFO Client 1] : You have entered Estate Walkways.").unwrap();
        drop(f);

        // Use a small max_bytes that lands in the middle of the padding lines
        let (events, in_lab) = replay_recent_log(&path);
        assert!(in_lab, "should be in lab");
        // Should still find PlazaEntered and RoomChanged — truncated first line is skipped
        assert!(events.iter().any(|e| matches!(e, NavEvent::PlazaEntered)),
            "should find PlazaEntered despite mid-file seek");
        assert!(events.iter().any(|e| matches!(e, NavEvent::RoomChanged { name } if name == "Estate Walkways")),
            "should find RoomChanged despite mid-file seek");
    }

    #[test]
    fn test_replay_nonexistent_file() {
        let path = std::path::Path::new("/tmp/poe_nonexistent_client.txt");
        let (events, in_lab) = replay_recent_log(path);
        assert!(events.is_empty());
        assert!(!in_lab);
    }
}
