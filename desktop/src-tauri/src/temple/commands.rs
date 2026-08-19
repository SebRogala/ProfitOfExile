//! The temple module's Tauri surface (POE-171).
//!
//! Five commands, all of them thin: four setters that validate, persist and
//! nudge the SSOT, and one debug dump that runs the whole read path over a real
//! screen (or a saved PNG) and writes every intermediate to disk.
//!
//! # Why the setters do not touch the loop
//!
//! `AppState.temple_settings` is the single owner. The loop reads a snapshot of
//! it at the top of every read, so a setter's whole job is to write the owner,
//! persist the delta and publish — the next tick picks the change up on its
//! own. The one exception is [`temple_rearm`], which has nothing to write: it
//! bumps an atomic the loop's read gate watches.
//!
//! # Logging
//!
//! Every rejection reaches `crate::app_log`. A `Result::Err` alone leaves no
//! trace anywhere a user or a log dump can reach, and `log::` output goes
//! nowhere in a shipped build (`windows_subsystem = "windows"`).
//!
//! That covers the dump too, and literally: every `?` in
//! [`debug_capture_blocking`] logs before it returns, every write that fails
//! logs its `io::Error` and pushes a note, and [`TempleDebugReport::files`]
//! only ever names files that reached the disk. A debug command whose own
//! failures are silent is the one command that must not have them — it is what
//! a user runs *because* something else went wrong.

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use super::lattice::Lattice;
use super::panel::{self, SystemOcr};
use super::reader;
use super::slice::{self, TempleProfileSettings};
use super::strategy::TempleConfig;
use crate::AppState;

/// Where a debug dump lands under the app data directory.
pub const DEBUG_DIR: &str = "temple-debug";

// ------------------------------------------------------------- the setters --

/// Set how many opening stones this incursion dropped.
///
/// The panel does not print the count, so it is the one board fact the user
/// supplies. 0 is legal (every passage from the room is already open) and 2 is
/// the maximum — see [`slice::validate_keys`].
#[tauri::command]
pub fn temple_set_keys(keys: u8, app: AppHandle) -> Result<(), String> {
    if let Err(e) = slice::validate_keys(keys) {
        crate::app_log(&app, format!("Temple: key count rejected — {e}"));
        return Err(e);
    }
    {
        let state = app.state::<AppState>();
        state
            .temple_settings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .keys = keys;
    }
    crate::persist_settings(&app);
    // The slice echoes the key count so the overlay renders its own control
    // from one source; without this write the control would not move until the
    // next full read.
    super::run::publish(&app, |s| s.keys = keys);
    rearm(&app);
    Ok(())
}

/// Set the two config flags — the Atlas passive and the scarab.
///
/// Both change the *rules*, not the reading: `artefacts_of_the_vaal` changes
/// how fast the temple budget is spent and `scarab_of_timelines` takes R5
/// (leave the map) away. So the current board's advice is stale the moment
/// either moves, and this re-arms.
#[tauri::command]
pub fn temple_set_config(config: TempleConfig, app: AppHandle) -> Result<(), String> {
    {
        let state = app.state::<AppState>();
        state
            .temple_settings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .config = config.clone();
    }
    crate::persist_settings(&app);
    crate::app_log(
        &app,
        format!(
            "Temple: config — {} incursions per map, R5 {}",
            config.entrances_per_map(),
            if config.r5_applies() { "available" } else { "off (scarab)" }
        ),
    );
    rearm(&app);
    Ok(())
}

/// Set the four tunable profile fields.
///
/// Rejects a profile the scorer cannot use rather than storing it — see
/// [`TempleProfileSettings::validate`]. A stored NaN would make every later
/// ranking arbitrary with nothing on screen to say why.
#[tauri::command]
pub fn temple_set_profile(profile: TempleProfileSettings, app: AppHandle) -> Result<(), String> {
    if let Err(e) = profile.validate() {
        crate::app_log(&app, format!("Temple: profile rejected — {e}"));
        return Err(e);
    }
    {
        let state = app.state::<AppState>();
        state
            .temple_settings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .profile = profile.clone();
    }
    crate::persist_settings(&app);
    crate::app_log(
        &app,
        format!(
            "Temple: profile — apex {:.2}, path cost {:.2}, reroll {}, R4 carve-out {}",
            profile.apex_score,
            profile.path_cost,
            profile.reroll_until_favourable,
            profile.r4_keep_upgrade_targets
        ),
    );
    rearm(&app);
    Ok(())
}

/// Force the next tick to do a full read, whatever the gate thinks.
///
/// The user's escape hatch: the gate deliberately does not re-read a board that
/// looks identical, so a read that came out wrong — a mis-OCR'd plate, a
/// diamond the estimated rect missed — would otherwise stand until the player
/// moves.
#[tauri::command]
pub fn temple_rearm(app: AppHandle) -> Result<(), String> {
    rearm(&app);
    crate::app_log(&app, "Temple: re-read armed".to_string());
    Ok(())
}

/// Bump the counter `slice::ReadGate` watches.
///
/// An atomic rather than a field of `temple_settings`: it is read on every tick
/// and never read together with the settings, and a counter that shared their
/// mutex would put the loop behind the webview thread once a second.
fn rearm(app: &AppHandle) {
    let state = app.state::<AppState>();
    state.temple_rearm.fetch_add(1, Ordering::SeqCst);
}

// ------------------------------------------------------------- the dump --

/// One timed step of a debug capture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugTiming {
    pub label: String,
    pub ms: u64,
}

/// What `temple_debug_capture` returns and the page renders.
///
/// A summary on purpose: the page JSON-prints whatever comes back, and the full
/// detail belongs in `report.json` where it can be read at leisure. `dumpDir`
/// is the pointer to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TempleDebugReport {
    pub dump_dir: String,
    /// `"screen"`, or the path of the image that was read instead.
    pub source: String,
    pub screen: [u32; 2],
    /// Whether a layout panel anchored at all, and what it scored.
    pub anchored: bool,
    pub scale: Option<f32>,
    pub ncc: Option<f32>,
    pub confidence: Option<String>,
    pub current: Option<String>,
    /// The estimated diamond rect this build used — the number most likely to
    /// be wrong, and the reason this dump writes the crop.
    pub diamond_rect: Option<[i32; 4]>,
    /// The side panel's OCR region, as the loop computes it. Reported for the
    /// same reason as `diamond_rect`: it is derived from the capture's right
    /// edge and a windowed client moves it.
    pub panel_rect: Option<[i32; 4]>,
    /// The `N Incursions Remaining` OCR region. Anchored to the Entrance plate,
    /// so this one follows the board rather than the screen.
    pub remaining_rect: Option<[i32; 4]>,
    /// Why the diamond read failed, when it did.
    pub marker_error: Option<String>,
    pub ocr_lines: usize,
    pub unknown_rooms: Vec<String>,
    pub timings: Vec<DebugTiming>,
    pub files: Vec<String>,
    /// Whatever the run has to say for itself.
    pub notes: Vec<String>,
}

/// The dump directory for a capture taken at `unix_ms`.
///
/// One directory per capture, named by the timestamp, so repeated dumps
/// accumulate instead of overwriting — comparing two runs is the whole point
/// when the diamond offset is being re-measured.
pub fn dump_dir(root: &std::path::Path, unix_ms: u64) -> PathBuf {
    root.join(DEBUG_DIR).join(unix_ms.to_string())
}

/// Run the whole read path and dump every intermediate for a bug report.
///
/// Off the capture thread, like `merc_debug_capture`: the read is blocking and
/// apartment-threaded, so it gets its own thread and the command awaits a
/// oneshot rather than blocking the runtime.
#[tauri::command]
pub async fn temple_debug_capture(
    image_path: Option<String>,
    app: AppHandle,
) -> Result<TempleDebugReport, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let logger = app.clone();
    std::thread::spawn(move || {
        let _ = tx.send(debug_capture_blocking(image_path, app));
    });
    match rx.await {
        Ok(result) => result,
        Err(_) => {
            let msg = "temple debug capture thread died before reporting".to_string();
            crate::app_log(&logger, format!("Temple debug: {msg}"));
            Err(msg)
        }
    }
}

/// Log `msg` and hand it back as the command's `Err`.
///
/// The dump's one failure idiom: a bare `?` returns a string the page prints
/// once and nothing keeps, which is useless in a bug report — the point of this
/// command is that the failure is still readable an hour later.
fn abort(app: &AppHandle, msg: String) -> String {
    crate::app_log(app, format!("Temple debug: {msg}"));
    msg
}

fn timing(label: &str, started: std::time::Instant) -> DebugTiming {
    DebugTiming {
        label: label.to_string(),
        ms: started.elapsed().as_millis() as u64,
    }
}

fn debug_capture_blocking(
    image_path: Option<String>,
    app: AppHandle,
) -> Result<TempleDebugReport, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| abort(&app, format!("no app data directory to write a dump into: {e}")))?;
    let dir = dump_dir(&data_dir, super::run::now_ms());
    std::fs::create_dir_all(&dir).map_err(|e| abort(&app, format!("cannot create {dir:?}: {e}")))?;

    let started = std::time::Instant::now();
    let (img, source) = match &image_path {
        Some(path) => (
            image::open(path).map_err(|e| abort(&app, format!("{path}: {e}")))?,
            path.clone(),
        ),
        None => (
            crate::capture::capture_screen().map_err(|e| abort(&app, e))?,
            "screen".to_string(),
        ),
    };
    // The report is built before the first write, so every write in this
    // function goes through the one owner of the `files` claim (`note_write`).
    let mut report = TempleDebugReport {
        dump_dir: dir.to_string_lossy().to_string(),
        source,
        screen: [img.width(), img.height()],
        anchored: false,
        scale: None,
        ncc: None,
        confidence: None,
        current: None,
        diamond_rect: None,
        panel_rect: None,
        remaining_rect: None,
        marker_error: None,
        ocr_lines: 0,
        unknown_rooms: Vec::new(),
        timings: vec![timing("capture", started)],
        files: Vec::new(),
        notes: Vec::new(),
    };

    if let Some(line) = note_write(&mut report, "screen.png", img.save(dir.join("screen.png"))) {
        crate::app_log(&app, format!("Temple debug: {line}"));
    }

    let settings = super::run::settings_snapshot(&app);
    let started = std::time::Instant::now();
    let layout = reader::read_layout_with_hint(&img, settings.calibration.as_ref());
    report.timings.push(timing("anchor+doors", started));

    let layout = match layout {
        Ok(layout) => layout,
        Err(e) => {
            report.notes.push(e.to_string());
            write_report(&app, &dir, &mut report);
            crate::app_log(&app, format!("Temple debug: no layout — {e}"));
            return Ok(report);
        }
    };
    report.anchored = true;
    report.scale = Some(layout.scale);
    report.ncc = Some(layout.ncc);
    report.confidence = Some(format!("{:?}", layout.confidence));
    report.current = layout.current.map(|s| s.as_str().to_string());

    // The diamond crop is the whole reason this dump exists: the rect is an
    // estimate from two screenshots, and re-measuring it needs the pixels it
    // actually looked at, right or wrong.
    let rect = super::run::diamond_rect((img.width(), img.height()), layout.scale);
    report.diamond_rect = Some(rect);
    let [dx, dy, dw, dh] = rect;
    if dx >= 0 && dy >= 0 && (dx + dw) as u32 <= img.width() && (dy + dh) as u32 <= img.height() {
        // The one crop this dump exists to hand back — a silent loss makes the
        // whole run worthless without saying so.
        let crop = img.crop_imm(dx as u32, dy as u32, dw as u32, dh as u32);
        if let Some(line) = note_write(&mut report, "diamond.png", crop.save(dir.join("diamond.png")))
        {
            crate::app_log(&app, format!("Temple debug: {line}"));
        }
    } else {
        let note = format!("the estimated diamond rect {rect:?} falls outside the capture");
        crate::app_log(&app, format!("Temple debug: {note}"));
        report.notes.push(note);
    }
    if let Err(e) = super::run::read_markers(&img, &layout) {
        report.marker_error = Some(e);
    }

    // The same two bounded crops the loop reads, in the same order — a dump
    // that OCR'd the whole frame would report lines the loop never sees, which
    // is the opposite of what a bug report needs. Each crop is written out
    // alongside, because a wrong ROI is exactly the kind of thing this command
    // is run to find.
    let started = std::time::Instant::now();
    let regions = [
        ("panel", super::run::panel_rect((img.width(), img.height()), layout.scale)),
        ("remaining", super::run::remaining_rect(layout.origin, layout.scale)),
    ];
    report.panel_rect = Some(regions[0].1);
    report.remaining_rect = Some(regions[1].1);
    let mut lines: Vec<String> = Vec::new();
    for (name, rect) in regions {
        let Some(crop) = super::run::crop_clipped(&img, rect) else {
            let note = format!("the {name} text region {rect:?} falls outside the capture");
            crate::app_log(&app, format!("Temple debug: {note}"));
            report.notes.push(note);
            continue;
        };
        let file = format!("{name}.png");
        if let Some(line) = note_write(&mut report, &file, crop.save(dir.join(&file))) {
            crate::app_log(&app, format!("Temple debug: {line}"));
        }
        let prepared = crate::capture::preprocess_for_ocr(&crop);
        match crate::ocr::recognize_lines(&prepared) {
            Ok(read) => lines.extend(read.into_iter().map(|l| l.text)),
            Err(e) => {
                let note = format!("{name} OCR failed — {e}");
                crate::app_log(&app, format!("Temple debug: {note}"));
                report.notes.push(note);
            }
        }
    }
    report.timings.push(timing("text ocr", started));
    report.ocr_lines = lines.len();

    let started = std::time::Instant::now();
    let lattice = Lattice::new(layout.origin, layout.scale);
    let rooms = panel::read_board(&SystemOcr, &img, &lattice, &|| false);
    report.timings.push(timing("plate ocr", started));
    report.unknown_rooms = slice::unknown_rooms(&rooms);

    let panel_reading = panel::read_panel(&lines);
    let plates: Vec<String> = rooms
        .iter()
        .map(|r| {
            format!(
                "{}: {}",
                r.slot.as_str(),
                r.identity
                    .identity()
                    .map_or("<unread>", |id| id.display_name())
            )
        })
        .collect();
    report.notes.push(format!("plates — {}", plates.join("; ")));
    report.notes.push(format!(
        "panel — {} architect block(s), incursions remaining {:?}",
        panel_reading.architects.len(),
        panel_reading.incursions_remaining
    ));
    report.notes.extend(lines.iter().take(60).cloned());

    write_report(&app, &dir, &mut report);
    crate::app_log(
        &app,
        format!(
            "Temple debug: {} OCR lines, {} unread plates, dump at {}",
            report.ocr_lines,
            report.unknown_rooms.len(),
            report.dump_dir
        ),
    );
    Ok(report)
}

/// The report's own file name, in the file list and on disk.
pub const REPORT_FILE: &str = "report.json";

/// Fold one write's outcome into the report, and hand back the line to log.
///
/// The single owner of the rule that [`TempleDebugReport::files`] is a **claim
/// about the disk**: a name goes in only when the bytes landed, and a failure
/// carries the `io::Error`'s own message into `notes` rather than a shrug. Both
/// halves matter in a bug report — a dump whose `files` lists a PNG that is not
/// there sends the next reader looking for it.
///
/// Pure apart from the report it mutates: the caller does the logging, which is
/// what lets this be tested without an `AppHandle`.
fn note_write<E: std::fmt::Display>(
    report: &mut TempleDebugReport,
    file: &str,
    result: Result<(), E>,
) -> Option<String> {
    match result {
        Ok(()) => {
            report.files.push(file.to_string());
            None
        }
        Err(e) => {
            let note = format!("{file} could not be written: {e}");
            report.notes.push(note.clone());
            Some(note)
        }
    }
}

/// Write `report.json` last, because it NAMES the files written before it.
///
/// The copy that lands on disk names itself; the copy the command RETURNS only
/// gains the name once the bytes are there, which is [`note_write`]'s rule. The
/// two differing is the whole point — a report that could not be written must
/// not come back claiming it was.
fn write_report(app: &AppHandle, dir: &std::path::Path, report: &mut TempleDebugReport) {
    let mut on_disk = report.clone();
    on_disk.files.push(REPORT_FILE.to_string());
    let json = match serde_json::to_string_pretty(&on_disk) {
        Ok(json) => json,
        Err(e) => {
            let note = format!("the report could not be serialised: {e}");
            crate::app_log(app, format!("Temple debug: {note}"));
            report.notes.push(note);
            return;
        }
    };
    if let Some(line) = note_write(report, REPORT_FILE, std::fs::write(dir.join(REPORT_FILE), json))
    {
        crate::app_log(app, format!("Temple debug: {line}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_report() -> TempleDebugReport {
        TempleDebugReport {
            dump_dir: "/tmp/poe/1".to_string(),
            source: "screen".to_string(),
            screen: [1374, 862],
            anchored: false,
            scale: None,
            ncc: None,
            confidence: None,
            current: None,
            diamond_rect: None,
            panel_rect: None,
            remaining_rect: None,
            marker_error: None,
            ocr_lines: 0,
            unknown_rooms: Vec::new(),
            timings: Vec::new(),
            files: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// A write that failed must not leave its file named in `files`, and its
    /// `io::Error` must survive into both the notes and the log line.
    ///
    /// Fails if `files` is appended to before the write is known to have
    /// succeeded — a dump listing a PNG that is not on disk sends the next
    /// reader looking for it — or if the error message is swallowed, which is
    /// the whole difference between "the crop is missing" and "the crop is
    /// missing because the disk is full".
    #[test]
    fn a_failed_write_is_not_claimed_as_a_file_and_keeps_its_reason() {
        let mut report = empty_report();

        let line = note_write(
            &mut report,
            "diamond.png",
            Err::<(), _>("No space left on device"),
        )
        .expect("a failed write must produce a log line");

        assert!(report.files.is_empty(), "files is a claim about the disk");
        assert!(
            line.contains("diamond.png") && line.contains("No space left on device"),
            "the line must name the file and the reason, got {line:?}",
        );
        assert_eq!(
            report.notes,
            vec![line],
            "the same text reaches the returned report, not just the log",
        );
    }

    /// The success half: the file is named, and nothing is logged. Fails if
    /// every write logs, which would bury the failures among them.
    #[test]
    fn a_successful_write_is_named_and_says_nothing() {
        let mut report = empty_report();

        assert_eq!(note_write(&mut report, "screen.png", Ok::<(), String>(())), None);

        assert_eq!(report.files, vec!["screen.png".to_string()]);
        assert!(report.notes.is_empty());
    }

    /// Dumps accumulate rather than overwrite — comparing two runs is the
    /// point when the diamond offset is being re-measured. Fails if the
    /// directory stops being keyed by timestamp.
    #[test]
    fn each_dump_gets_its_own_directory() {
        let root = std::path::Path::new("/tmp/poe");

        assert_ne!(dump_dir(root, 1), dump_dir(root, 2));
        assert!(dump_dir(root, 1).ends_with("1"));
        assert!(dump_dir(root, 1).starts_with(root.join(DEBUG_DIR)));
    }
}
