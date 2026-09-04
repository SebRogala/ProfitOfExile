//! The temple module's Tauri surface (POE-171).
//!
//! Five commands, all of them thin: four setters that validate, persist and
//! nudge the SSOT, and one debug dump that runs the whole read path over a real
//! screen (or a saved PNG) and writes every intermediate to disk — including
//! (POE-243) `ocr-lines.json`, every OCR line with its box in the ENGINE's own
//! order, which is what settles whether a missing architect was cropped out or
//! emitted out of order.
//!
//! # Why the setters do not touch the loop
//!
//! `AppState.temple_settings` is the single owner. The loop reads a snapshot of
//! it at the top of every read, so a setter's whole job is to write the owner,
//! persist the delta and publish — the next tick picks the change up on its
//! own. The one exception is [`temple_rearm`], which has nothing to write: it
//! bumps an atomic the loop's read gate watches, and (POE-242) arms the capture
//! itself for `super::trigger::MANUAL_ARM_GRACE_MS`.
//!
//! [`temple_debug_capture`] is NOT behind that arm gate. It is an explicit user
//! action — the command a user runs *because* something else went wrong — and a
//! diagnostic that is unavailable in the state being diagnosed is not one.
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
    // Echoed onto the slice: the page and the overlay render these controls
    // from the slice, so without this write the switch the user just flipped
    // would not move until the next full read — and with the module off,
    // never.
    super::run::publish(&app, |s| s.config = config.clone());
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
    // Same echo as `temple_set_config` — see the note there.
    super::run::publish(&app, |s| s.profile = profile.clone());
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
/// diamond the rect missed because the anchor was off — would otherwise stand
/// until the player moves.
#[tauri::command]
pub fn temple_rearm(app: AppHandle) -> Result<(), String> {
    rearm(&app);
    // POE-242: the button now has two jobs, because the loop it used to poke
    // may not be looking at all. Besides forcing the read gate open, it arms
    // the CAPTURE for `trigger::MANUAL_ARM_GRACE_MS` — the fallback for every
    // panel Client.txt does not announce (Alva in the hideout, a non-English
    // client). Only the command does this, not the shared `rearm` helper: a
    // settings change must not start a capture nobody asked for.
    super::trigger::arm_manual(&app);
    crate::app_log(&app, "Temple: re-read armed".to_string());
    Ok(())
}

/// Bump the counter `slice::RearmGate` watches.
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
    /// The diamond rect this build used, and the crop taken at it. Since
    /// POE-230 all three regions below are placed from the Entrance origin and
    /// the anchor's scale, so a rect that is wrong here is either a wrong anchor
    /// or a constant that needs re-measuring — the dump is what tells the two
    /// apart. The loop prints the same three on every read (`Temple: rois …`).
    pub diamond_rect: Option<[i32; 4]>,
    /// The side panel's OCR region — see `diamond_rect`.
    pub panel_rect: Option<[i32; 4]>,
    /// The `N Incursions Remaining` OCR region — see `diamond_rect`.
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
    // The display the pixels came off, so the hint below can be checked against
    // the screen the remembered scale was measured on (POE-237). An image FILE
    // has no display: `0` is `crate::capture::Capture`'s unknown, and
    // `ssot::screen_matches` falls back to the dimensions alone for it, which is
    // the right rule for a dump somebody dragged in from another machine.
    let (img, monitor_id, source) = match &image_path {
        Some(path) => (
            image::open(path).map_err(|e| abort(&app, format!("{path}: {e}")))?,
            0,
            path.clone(),
        ),
        None => {
            let grab = crate::capture::capture_screen(&app).map_err(|e| abort(&app, e))?;
            (grab.image, grab.monitor_id, "screen".to_string())
        }
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

    // The same hint the capture loop uses, from the same shared slice (POE-234
    // WI-2): the button's whole point is to reproduce what the loop sees, and it
    // reaches the exhaustive sweep behind it either way, so a hint that misses
    // costs one correlation and never an answer.
    let hint = {
        let state = app.state::<crate::AppState>();
        let screen = *state.screen.lock().unwrap_or_else(|e| e.into_inner());
        super::run::hint_for_capture(screen.as_ref(), (img.width(), img.height()), monitor_id)
    };
    let started = std::time::Instant::now();
    let layout = reader::read_layout_with_hint(&img, hint.as_ref());
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

    // The diamond crop is the whole reason this dump exists: re-measuring the
    // rect needs the pixels it actually looked at, right or wrong.
    let rect = super::run::diamond_rect(layout.origin, layout.scale);
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
        let note = format!("the diamond rect {rect:?} falls outside the capture");
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
    // The loop's own list, not a second copy of it (`run::text_regions`): a
    // third text region must not be able to reach the dump without reaching the
    // read and the outside-the-capture check with it.
    let regions = super::run::text_regions(&layout);
    report.panel_rect = Some(regions[0].1);
    report.remaining_rect = Some(regions[1].1);
    let mut lines: Vec<crate::mercenary::geometry::OcrLineBox> = Vec::new();
    let mut dumped: Vec<DumpRegion> = Vec::new();
    for (name, rect) in regions {
        let Some((crop, origin)) = super::run::crop_clipped(&img, rect) else {
            let note = format!("the {name} text region {rect:?} falls outside the capture");
            crate::app_log(&app, format!("Temple debug: {note}"));
            report.notes.push(note);
            continue;
        };
        let file = format!("{name}.png");
        if let Some(line) = note_write(&mut report, &file, crop.save(dir.join(&file))) {
            crate::app_log(&app, format!("Temple debug: {line}"));
        }
        // The loop's own seam, so the dump's coordinates are the loop's
        // coordinates and not a second conversion that could disagree.
        match panel::crop_lines(&crop, origin) {
            Ok(read) => {
                dumped.push(DumpRegion {
                    region: name.to_string(),
                    rect,
                    origin: [origin.0, origin.1],
                    lines_in_engine_order: read.iter().map(dump_line).collect(),
                });
                lines.extend(read);
            }
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
    report.notes.extend(lines.iter().take(60).map(|l| l.text.clone()));

    // The whole read, next to the summary rather than instead of it: `notes`
    // is what a user pastes into a bug report, this is what answers a question
    // the summary raised. Written before `report.json`, which names it.
    let dump = OcrLinesDump {
        regions: dumped,
        title: panel_reading
            .room
            .identity()
            .map(|id| id.display_name().to_string()),
        title_rect: panel_reading.room_rect,
        blocks: panel_reading.architects.iter().map(dump_block).collect(),
    };
    match serde_json::to_string_pretty(&dump) {
        Ok(json) => {
            if let Some(line) = note_write(
                &mut report,
                OCR_LINES_FILE,
                std::fs::write(dir.join(OCR_LINES_FILE), json),
            ) {
                crate::app_log(&app, format!("Temple debug: {line}"));
            }
        }
        Err(e) => {
            let note = format!("{OCR_LINES_FILE} could not be serialised: {e}");
            crate::app_log(&app, format!("Temple debug: {note}"));
            report.notes.push(note);
        }
    }

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

/// The OCR-line dump's file name (POE-243).
pub const OCR_LINES_FILE: &str = "ocr-lines.json";

/// One OCR line, as the engine gave it and where it sits on screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DumpLine {
    pub text: String,
    /// `[x, y, w, h]` in CAPTURE px — already through the 2× descale and the
    /// crop-origin translate, so it can be compared with `panelRect` and
    /// `remainingRect` in `report.json` without arithmetic.
    pub rect: [i32; 4],
}

/// One text region's lines, in the ENGINE's own order.
///
/// Engine order, deliberately and stated in the field name's doc rather than
/// implied: the whole question POE-243 was opened on is whether the engine
/// emits a wrapped continuation before the line it belongs to, and a dump that
/// had already sorted the lines could not answer it. `panel::reading_order` is
/// what the parser applies afterwards.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DumpRegion {
    /// [`super::slice::PANEL_REGION`] or [`super::slice::REMAINING_REGION`] —
    /// `super::run::text_regions` is what fills it.
    pub region: String,
    /// The ROI this region was cropped at, capture px.
    pub rect: [i32; 4],
    /// The crop's clipped top-left corner — what the line boxes were offset by.
    pub origin: [i32; 2],
    pub lines_in_engine_order: Vec<DumpLine>,
}

/// One architect block the parser built out of those lines.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DumpBlock {
    pub architect_name: String,
    /// `"change"` or `"upgrade"`.
    pub kind: String,
    pub printed_target: String,
    /// The union of the boxes of the lines the block was built from.
    pub rect: Option<[i32; 4]>,
}

/// What `ocr-lines.json` holds.
///
/// The whole read, not `report.json`'s first-60-texts summary: every line with
/// its box, per region, plus what the parser made of them. A bug report about
/// a missing architect is answerable from this file alone — the lines are
/// there, in the order they arrived, with the geometry that decides how they
/// group.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrLinesDump {
    pub regions: Vec<DumpRegion>,
    /// The panel title the read settled on, and the line it came off.
    pub title: Option<String>,
    pub title_rect: Option<[i32; 4]>,
    pub blocks: Vec<DumpBlock>,
}

/// One OCR line, for the dump.
fn dump_line(line: &crate::mercenary::geometry::OcrLineBox) -> DumpLine {
    DumpLine {
        text: line.text.clone(),
        rect: [line.x, line.y, line.w, line.h],
    }
}

/// One parsed architect block, for the dump.
fn dump_block(offer: &panel::ArchitectOffer) -> DumpBlock {
    DumpBlock {
        architect_name: offer.architect_name.clone(),
        kind: match offer.kind {
            super::rooms::OfferKind::Change => "change".to_string(),
            super::rooms::OfferKind::Upgrade => "upgrade".to_string(),
        },
        printed_target: offer.printed_target.clone(),
        rect: offer.rect,
    }
}

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

    /// The dump's own wire shape, round-tripped.
    ///
    /// `ocr-lines.json` is read by a person and by whatever reads a bug report
    /// next, so its keys are a contract: camelCase like every other wire struct
    /// in this module, `linesInEngineOrder` saying which order it is in, and
    /// the offer kind as the same `"change"`/`"upgrade"` strings `OfferView`
    /// publishes rather than a Rust enum.
    ///
    /// Fails if a `rename_all` is dropped, a field is renamed, or `dump_block`
    /// starts serialising `OfferKind` directly — all three of which leave the
    /// file readable and the reader looking for a key that is not there.
    #[test]
    fn the_ocr_line_dump_round_trips_through_its_camel_case_keys() {
        let dump = OcrLinesDump {
            regions: vec![DumpRegion {
                region: super::slice::PANEL_REGION.to_string(),
                rect: [1288, 92, 400, 300],
                origin: [1288, 92],
                lines_in_engine_order: vec![DumpLine {
                    text: "Empowerment)".to_string(),
                    rect: [1300, 256, 96, 20],
                }],
            }],
            title: Some("Armourer's Workshop".to_string()),
            title_rect: Some([1300, 100, 152, 20]),
            blocks: vec![
                dump_block(&panel::ArchitectOffer {
                    architect_name: "Atmohua".to_string(),
                    kind: super::super::rooms::OfferKind::Change,
                    printed_target: "Shrine of Empowerment".to_string(),
                    target: super::super::rooms::match_room_name("Shrine of Empowerment"),
                    rect: Some([1300, 210, 224, 66]),
                }),
                dump_block(&panel::ArchitectOffer {
                    architect_name: "Quipolatl".to_string(),
                    kind: super::super::rooms::OfferKind::Upgrade,
                    printed_target: "Armoury".to_string(),
                    target: super::super::rooms::match_room_name("Armoury"),
                    rect: None,
                }),
            ],
        };

        let json = serde_json::to_string(&dump).expect("the dump serialises");

        assert!(
            json.contains(r#""linesInEngineOrder":[{"text":"Empowerment)","rect":[1300,256,96,20]}]"#),
            "the engine-order lines carry their boxes under camelCase keys: {json}",
        );
        assert!(
            json.contains(r#""titleRect":[1300,100,152,20]"#),
            "the title's own box is published: {json}",
        );
        assert!(
            json.contains(r#""kind":"change""#) && json.contains(r#""kind":"upgrade""#),
            "both offer kinds are the wire strings, not enum variants: {json}",
        );
        assert!(
            json.contains(r#""architectName":"Atmohua""#) && json.contains(r#""rect":null"#),
            "a block with no box publishes null rather than a zero box: {json}",
        );
        assert_eq!(
            serde_json::from_str::<OcrLinesDump>(&json).expect("and decodes again"),
            dump,
        );
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
