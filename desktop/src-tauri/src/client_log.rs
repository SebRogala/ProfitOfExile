//! Client.txt line grammar and the shared line-clock policy: the pure parses
//! every feature reads a log line with (speaker, area entry, area change, the
//! line's own timestamp), the catch-up tail read, and the clock constants the
//! mercenary and temple triggers both measure against. `MANUAL_ARM_GRACE_MS`
//! is policy rather than grammar; it lives here because merc and temple share
//! it.

use serde::Serialize;
use std::io::{Read as _, Seek, SeekFrom};
use std::path::Path;

/// How long an unprobed qualifying Inspect line stays worth probing.
///
/// Both probes fire only while the game is the FOREGROUND window, so a line
/// heard as the player alt-tabs never gets its look. This is the give-up: an
/// Inspect line is evidence about the screen at the moment it was spoken, and
/// ten seconds later it is evidence about nothing. Without it the gate would sit
/// armed until the player came back — an hour later, if that is when they come
/// back — and then probe for a window that closed with the alt-tab.
///
/// Ten seconds because that is what POE-198's burst was sized at, for the same
/// reason: it is the outside of "the player is still standing where they were".
/// What has changed is what the ten seconds BUY. They used to be ten seconds of
/// full-screen OCR; they are now the shelf life of a right to two placed-crop looks.
///
/// Measured from the FIRST line of an unbroken chain by the same speaker, not
/// from the latest one. Every qualifying Inspect line re-arms both probes
/// (`BurstGate::hear` in `mercenary/trigger.rs`), so a clock measured from
/// the latest one would never run out while the mercenary kept talking —
/// POE-198's re-armable expiry, back in the gate's clothes. The chain breaks when the gate goes back to
/// resting (a stand-down, a capture, or a disarm) or when a DIFFERENT speaker
/// is heard, and the next line then starts its own ten seconds.
pub const LINE_STALE_MS: u64 = 10_000;

/// The furthest back a line's own timestamp may move the gate's clock.
///
/// The gate is measured from the LINE, not from the watcher's delivery of it —
/// `log_watcher` blocks on a notify event with a 5 s `recv_timeout` fallback, so
/// a line that arrives without a filesystem event reaches this module up to that
/// whole fallback late, and a 500 ms probe measured from delivery would be five
/// and a half seconds after the click. [`line_timestamp_ms`] reads the
/// `2026/08/24 22:51:12` stamp the line carries and [`arm_at`] backdates the
/// gate to it.
///
/// The clamp is what makes that safe. The stamp is LOCAL time with no zone,
/// read against a UTC clock that may not agree with it: a machine whose local
/// offset resolves wrong produces a line timestamp minutes or hours away from
/// `now`. Backdating by an hour would fire both probes at once and stand down
/// before the player finished clicking; backdating FORWARD would leave the
/// probes never due and the module silently dead. So a stamp outside
/// `[now - this, now]` is not believed, and the gate falls back to the delivery
/// clock — the behaviour POE-198 shipped, which is wrong by at most one watcher
/// hop.
pub const MAX_BACKDATE_MS: u64 = 10_000;

/// How long a Scan-now request waits for the game before giving up.
///
/// Scan now is clicked in OUR window, which by definition means the game is not
/// the foreground window and the capture loop is napping. So the request is not
/// a deadline to meet but a debt to settle: it is owed ONE detect, and it gets
/// it at the first moment the game IS in front. This bounds the wait, so a
/// click that is never followed by an alt-tab does not leave the module holding
/// a detect for the rest of the session.
pub const MANUAL_ARM_GRACE_MS: u64 = 60_000;

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

// ---------------------------------------------------------------------------
// Pure — the line's own clock
// ---------------------------------------------------------------------------

/// The `2026/08/24 22:51:12` stamp every Client.txt line starts with, as epoch
/// milliseconds, or `None` when the line does not carry one this can read.
///
/// LOCAL time with no zone — that is the format PoE writes — so the conversion
/// goes through the machine's own offset. Two cases have no single answer and
/// both resolve to "do not believe this stamp": the hour a spring-forward
/// deletes does not exist (`None`), and the hour an autumn fall-back repeats
/// happens twice (the EARLIER of the two, which is at most an hour early and is
/// caught by [`MAX_BACKDATE_MS`] rather than by guessing).
///
/// Only the first 19 characters are read. The rest of the prefix
/// (`105432578 cffb0716 [INFO Client 12345]`) is a monotonic tick count and a
/// thread id, neither of which is a clock this can compare against `now`.
pub fn line_timestamp_ms(line: &str) -> Option<u64> {
    stamp_ms_in(&chrono::Local, line)
}

/// [`line_timestamp_ms`] against an EXPLICIT zone.
///
/// Split out for one reason: the two answers that depend on the zone — the
/// deleted spring-forward hour and the repeated autumn one — cannot be
/// exercised through `chrono::Local`, whose transitions are the host's and are
/// UTC (no transitions at all) on the machines this suite runs on. A test zone
/// with one known fall-back is what pins `.earliest()`; see
/// `the_repeated_autumn_hour_resolves_to_the_earlier_of_its_two_instants`.
fn stamp_ms_in<Tz: chrono::TimeZone>(tz: &Tz, line: &str) -> Option<u64> {
    let stamp = line.get(..19)?;
    let naive = chrono::NaiveDateTime::parse_from_str(stamp, "%Y/%m/%d %H:%M:%S").ok()?;
    let resolved = tz.from_local_datetime(&naive).earliest()?;
    u64::try_from(resolved.timestamp_millis()).ok()
}

/// The moment the gate's clock starts, given the wall clock now and whatever
/// the line's own stamp said.
///
/// The line wins when it is BEHIND `now` by no more than [`MAX_BACKDATE_MS`],
/// which is the whole of the believable range: a watcher hop is a delay, never
/// a head start, so a stamp in the future is a clock that disagrees and a stamp
/// hours old is the same disagreement the other way. Everything outside that
/// band falls back to `now` — the delivery clock POE-198 shipped.
///
/// Pure and separate from the parse so the clamp is testable without a
/// timezone: the parse is what depends on the host's offset, and this is what
/// decides.
pub fn arm_at(now_ms: u64, line_ms: Option<u64>) -> u64 {
    match line_ms {
        Some(line) if line <= now_ms && now_ms - line <= MAX_BACKDATE_MS => line,
        _ => now_ms,
    }
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

/// How much of the log's tail a catch-up pass reads.
///
/// 32KB covers hundreds of `[WINDOW]` focus lines, which are what push area
/// entries far back in a file that is otherwise quiet.
pub const CATCH_UP_TAIL_BYTES: u64 = 32_768;

/// The last [`CATCH_UP_TAIL_BYTES`] of the log as text, first partial line
/// dropped. `None` for a missing, unreadable or empty file.
///
/// The size every catch-up pass shares. The watcher start reads it once and
/// hands the same text to the lab replay
/// ([`crate::lab_navigation::replay_recent_tail`]), the temple seed
/// (`temple::trigger::catch_up`, POE-242) and the loot attribution, so the
/// three cannot disagree about how far back "recent" reaches.
pub fn recent_log_tail(path: &Path) -> Option<String> {
    let file_len = std::fs::metadata(path)
        .map_err(|e| log::warn!("recent_log_tail: metadata failed: {}", e))
        .ok()?
        .len();
    if file_len == 0 {
        return None;
    }
    read_tail(path, file_len, CATCH_UP_TAIL_BYTES)
}

/// Read the last `max_bytes` of a file as a String, skipping the first
/// partial line when seeking mid-file.
pub(crate) fn read_tail(path: &Path, file_len: u64, max_bytes: u64) -> Option<String> {
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

/// A Client.txt larger than [`CATCH_UP_TAIL_BYTES`] for the catch-up tests:
/// 40 KB of older `[WINDOW]` focus lines, then `decisive`, then 8 KB of newer
/// focus lines. Every `decisive` line therefore sits INSIDE the catch-up
/// window but OUTSIDE the last 4 KB, so a pass that reads a smaller tail than
/// the shared one misses it.
#[cfg(test)]
pub(crate) fn write_catch_up_fixture(name: &str, decisive: &[&str]) -> std::path::PathBuf {
    use std::io::Write;
    let dir = std::env::temp_dir().join(format!("poe_test_catch_up_{name}"));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("Client.txt");
    let mut f = std::fs::File::create(&path).unwrap();
    let focus = "2026/04/01 19:00:00 1234 [INFO Client 1] [WINDOW] Gained focus";
    for _ in 0..(40_960 / (focus.len() + 1)) {
        writeln!(f, "{focus}").unwrap();
    }
    for line in decisive {
        writeln!(f, "{line}").unwrap();
    }
    for _ in 0..(8_192 / (focus.len() + 1)) {
        writeln!(f, "{focus}").unwrap();
    }
    path
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

    // -- the line's own clock ----------------------------------------------

    /// The gate's deadlines are 500 ms and 1.5 s. A watcher hop is up to 5 s
    /// (`log_watcher`'s `recv_timeout` fallback), so a gate measured from
    /// delivery would probe six seconds after the click that opened the window.
    /// The line carries the moment it was spoken; this is that read.
    #[test]
    fn a_client_line_is_stamped_with_the_moment_it_was_spoken() {
        use chrono::TimeZone;
        let expected = chrono::Local
            .with_ymd_and_hms(2026, 8, 24, 22, 51, 12)
            .earliest()
            .expect("22:51 on that date exists in every zone with a fixed offset");

        assert_eq!(
            line_timestamp_ms(MERC_LINE),
            u64::try_from(expected.timestamp_millis()).ok()
        );
    }

    /// A line whose prefix is not a timestamp must not resolve to SOME moment —
    /// a partial parse would hand `arm_at` a number to clamp instead of the
    /// `None` that means "use the delivery clock".
    #[test]
    fn a_line_with_no_timestamp_has_no_stamp() {
        assert_eq!(line_timestamp_ms("Nytra, the Cyaxan Loner: Keep walking."), None);
    }

    #[test]
    fn a_line_shorter_than_a_timestamp_has_no_stamp() {
        assert_eq!(line_timestamp_ms("2026/08/24"), None);
    }

    /// A zone with ONE fall-back transition, so the ambiguous hour can be
    /// written down instead of waited for. `chrono::Local` is the host's zone
    /// and is UTC on the machines this suite runs on — it has no ambiguous hour
    /// to offer, which is why the parse takes its zone as a parameter.
    ///
    /// 2026-10-25, +02:00 → +01:00 at 01:00 UTC. Local 02:00:00–02:59:59 that
    /// morning therefore happens twice: once at +02:00 (00:00–00:59 UTC) and
    /// again an hour later at +01:00.
    #[derive(Clone, Debug)]
    struct FallBackZone;

    impl FallBackZone {
        const SUMMER: i32 = 2 * 3600;
        const WINTER: i32 = 3600;

        fn summer() -> chrono::FixedOffset {
            chrono::FixedOffset::east_opt(Self::SUMMER).expect("+02:00 is a real offset")
        }

        fn winter() -> chrono::FixedOffset {
            chrono::FixedOffset::east_opt(Self::WINTER).expect("+01:00 is a real offset")
        }

        fn at(hour: u32, minute: u32) -> chrono::NaiveDateTime {
            chrono::NaiveDate::from_ymd_opt(2026, 10, 25)
                .expect("2026-10-25 is a real date")
                .and_hms_opt(hour, minute, 0)
                .expect("a real time of day")
        }
    }

    impl chrono::TimeZone for FallBackZone {
        type Offset = chrono::FixedOffset;

        fn from_offset(_: &chrono::FixedOffset) -> Self {
            FallBackZone
        }

        fn offset_from_local_date(
            &self,
            _: &chrono::NaiveDate,
        ) -> chrono::LocalResult<chrono::FixedOffset> {
            // A whole DAY that carries a transition has no single offset, and
            // nothing in this module asks for one.
            chrono::LocalResult::None
        }

        fn offset_from_local_datetime(
            &self,
            local: &chrono::NaiveDateTime,
        ) -> chrono::LocalResult<chrono::FixedOffset> {
            if *local < Self::at(2, 0) {
                chrono::LocalResult::Single(Self::summer())
            } else if *local < Self::at(3, 0) {
                // The larger offset FIRST: `from_local_datetime` subtracts it,
                // so +02:00 is the earlier of the two instants.
                chrono::LocalResult::Ambiguous(Self::summer(), Self::winter())
            } else {
                chrono::LocalResult::Single(Self::winter())
            }
        }

        fn offset_from_utc_date(&self, _: &chrono::NaiveDate) -> chrono::FixedOffset {
            Self::winter()
        }

        fn offset_from_utc_datetime(&self, utc: &chrono::NaiveDateTime) -> chrono::FixedOffset {
            if *utc < Self::at(1, 0) {
                Self::summer()
            } else {
                Self::winter()
            }
        }
    }

    /// The autumn hour that happens twice. PoE writes LOCAL time with no zone,
    /// so `02:30:00` on that morning names two instants an hour apart and the
    /// stamp cannot say which. The gate takes the EARLIER one and lets
    /// [`MAX_BACKDATE_MS`] decide whether to believe it: guessing the later one
    /// would put the line up to an hour in the FUTURE of a `now` that is
    /// actually after it, and [`arm_at`] answers a future stamp by throwing it
    /// away — the module would sit through the repeated hour answering no voice
    /// line at all.
    #[test]
    fn the_repeated_autumn_hour_resolves_to_the_earlier_of_its_two_instants() {
        let line = "2026/10/25 02:30:00 105432578 cffb0716 [INFO Client 12345] Arith, the Quickshot: Keep walking.";
        // 02:30 local at +02:00 — the first pass through the repeated hour.
        let earlier = FallBackZone::at(0, 30).and_utc().timestamp_millis();

        assert_eq!(
            stamp_ms_in(&FallBackZone, line),
            u64::try_from(earlier).ok(),
        );
    }

    /// The other half of the same rule: an unambiguous stamp in the same zone
    /// resolves normally, so the test above is pinning `.earliest()`'s choice
    /// and not some blanket "always the summer offset".
    #[test]
    fn a_stamp_outside_the_repeated_hour_takes_the_zones_one_answer() {
        let line = "2026/10/25 04:30:00 105432578 cffb0716 [INFO Client 12345] Arith, the Quickshot: Keep walking.";
        // 04:30 local at +01:00 — after the transition, one answer only.
        let only = FallBackZone::at(3, 30).and_utc().timestamp_millis();

        assert_eq!(stamp_ms_in(&FallBackZone, line), u64::try_from(only).ok());
    }

    /// The delay this exists for: the line is three seconds old by the time the
    /// watcher hands it over, so the gate starts three seconds back and both
    /// probes are already due.
    #[test]
    fn a_late_delivery_backdates_the_gate_to_the_line() {
        assert_eq!(arm_at(100_000, Some(97_000)), 97_000);
    }

    /// A stamp from BEFORE the believable band is a clock that disagrees, not a
    /// slow watcher. Backdating an hour would spend both probes at once and
    /// stand down before the player had finished clicking.
    #[test]
    fn a_stamp_older_than_the_backdate_cap_is_not_believed() {
        assert_eq!(arm_at(100_000, Some(100_000 - MAX_BACKDATE_MS - 1)), 100_000);
    }

    /// The boundary itself is inside the band.
    #[test]
    fn a_stamp_exactly_at_the_backdate_cap_is_believed() {
        let line = 100_000 - MAX_BACKDATE_MS;

        assert_eq!(arm_at(100_000, Some(line)), line);
    }

    /// The fatal direction, and the reason the clamp is two-sided: a stamp in
    /// the FUTURE would leave `probe_due` false for as long as the skew lasts,
    /// and the module would answer no voice line at all.
    #[test]
    fn a_stamp_in_the_future_is_not_believed() {
        assert_eq!(arm_at(100_000, Some(100_001)), 100_000);
    }

    #[test]
    fn an_unreadable_stamp_falls_back_to_the_delivery_clock() {
        assert_eq!(arm_at(100_000, None), 100_000);
    }

    #[test]
    fn test_parse_entered_area() {
        let line = "2026/04/01 20:00:00 12345678 [INFO Client 1234] : You have entered Estate Walkways.";
        assert_eq!(parse_entered_area(line), Some("Estate Walkways"));

        let line2 = "2026/04/01 20:00:00 12345678 [INFO Client 1234] : You have entered Aspirants' Plaza.";
        assert_eq!(parse_entered_area(line2), Some("Aspirants' Plaza"));

        assert_eq!(parse_entered_area("some random log line"), None);
    }

    /// A freshly rotated Client.txt: the file is there and holds nothing. The
    /// callers treat `None` as "no answer" and leave their state alone, so
    /// handing them `Some("")` would read as "the log says you are nowhere".
    #[test]
    fn test_recent_log_tail_is_none_for_an_empty_file() {
        let dir = std::env::temp_dir().join("poe_test_tail_empty");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("Client_empty.txt");
        std::fs::File::create(&path).unwrap();

        assert_eq!(recent_log_tail(&path), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The seek lands mid-line in any log worth reading, and half a line is not
    /// a line: `temple::trigger::catch_up` reads the FIRST line of this buffer
    /// as an area candidate like any other, and a fragment of `: You have
    /// entered The Temple of Atzoatl.` would still parse.
    ///
    /// Every line here is exactly 100 bytes and the file is 34 000, so the
    /// `CATCH_UP_TAIL_BYTES` boundary falls 32 bytes into a line — the whole
    /// point of the arithmetic. A dropped truncation step leaves a 67-byte
    /// first line.
    #[test]
    fn test_recent_log_tail_drops_the_truncated_first_line() {
        use std::io::Write;
        let dir = std::env::temp_dir().join("poe_test_tail_truncated");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("Client_truncated.txt");
        let mut f = std::fs::File::create(&path).unwrap();
        for i in 0..340u32 {
            let line = format!("{i:04} 2026/04/01 19:00:00 1234 [INFO Client 1] padding");
            writeln!(f, "{line:<99}").unwrap();
        }
        drop(f);

        let tail = recent_log_tail(&path).expect("a tail for a non-empty file");

        assert_eq!(
            tail.lines().next().map(str::len),
            Some(99),
            "the first line must be a whole one, not the tail of one",
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

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
    fn parse_area_change_handles_malformed_input() {
        // No quotes
        assert_eq!(parse_area_change("Generating level 75 area no_quotes"), None);
        // Only one quote
        assert_eq!(parse_area_change(r#"Generating level 75 area "unclosed"#), None);
        // No Generating level marker
        assert_eq!(parse_area_change(r#"some other line "with quotes""#), None);
    }
}
