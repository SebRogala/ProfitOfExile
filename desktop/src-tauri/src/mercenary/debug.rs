//! The debug capture dump and the template-store commands (POE-165 D10 §1).
//!
//! Every constant this module runs on — the row pitch, the cell offset, the
//! occupancy floor, the six thresholds — was measured on ONE reference image
//! and is provisional. `merc_debug_capture` is how they stop being provisional:
//! it runs the real detect path over a real screen (or a saved PNG), writes
//! every intermediate to disk, and hands back a summary. The first Windows run
//! then produces calibration data instead of a mystery.
//!
//! # What goes where
//!
//! - the RETURN value is a summary the page can render whole;
//! - `report.json` in the dump directory holds everything: the geometry and
//!   thresholds in force, every OCR line with its rect, both passes' row texts,
//!   the resulting capture, and every cell's stddev / badge / icon score —
//!   including the rejected ones;
//! - the LOGS panel gets one compact line, because 40 OCR lines would flush the
//!   50-entry buffer.
//!
//! Nothing here is `cfg`-gated. The platform difference arrives as an `Err`
//! from `capture_screen` / `recognize_lines`, and even then the dump directory
//! is written — a screen grab with no OCR is still evidence.

use std::path::{Path, PathBuf};
use std::time::Instant;

use image::{DynamicImage, GenericImageView};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use super::geometry::{self, OcrLineBox};
use super::read::{build_capture, crop_rgba, pass2_texts, CellDebug};
use super::run::{now_ms, publish};
use super::vocab::MercVocab;
use super::MercCapture;
use crate::AppState;

/// One timed step of a debug capture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugTiming {
    pub label: String,
    pub ms: u64,
}

/// One OCR line, in a serializable shape (`OcrLineBox` is not `Serialize` —
/// it is a geometry input, not a wire type).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugLine {
    pub text: String,
    pub rect: [i32; 4],
}

/// What `merc_debug_capture` returns and the page renders.
///
/// A summary on purpose: the page JSON-prints whatever comes back, and the full
/// detail (every line, every cell, the whole geometry) belongs in `report.json`
/// where it can be read at leisure. `dump_dir` is the pointer to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MercDebugReport {
    pub dump_dir: String,
    /// `"screen"`, or the path of the image that was read instead.
    pub source: String,
    pub screen: [u32; 2],
    pub geometry_source: String,
    pub geometry_error: Option<String>,
    pub ocr_lines: usize,
    /// Lines that matched a merc SKILL name well enough to seed the column.
    pub skill_candidates: usize,
    /// The "Wager" line that discriminates the recruit window from every other
    /// PoE surface listing skill names.
    pub anchor_text: Option<String>,
    pub detected: bool,
    pub rows: usize,
    pub occupied_cells: usize,
    pub scale: Option<f32>,
    pub row_pitch: Option<f32>,
    pub learned_templates: usize,
    pub timings: Vec<DebugTiming>,
    pub files: Vec<String>,
    /// Whatever the run has to say for itself — an OCR error, a detect that
    /// found lines but no panel, a crop that fell off the image.
    pub notes: Vec<String>,
}

/// The dump directory for a capture taken at `unix_ms`.
///
/// One directory per capture, named by the timestamp, so repeated dumps
/// accumulate instead of overwriting: comparing two runs is the whole point
/// when a threshold is being tuned.
pub fn dump_dir(root: &Path, unix_ms: u64) -> PathBuf {
    root.join(super::DEBUG_DIR).join(unix_ms.to_string())
}

pub fn row_file(row: u8) -> String {
    format!("row-{row}.png")
}

pub fn cell_file(row: u8, slot: u8) -> String {
    format!("cell-{row}-{slot}.png")
}

/// The images one dump writes.
pub struct DumpImages<'a> {
    pub screen: &'a DynamicImage,
    /// `(row index, name-band rect)` — the crop pass 2 re-OCRs.
    pub rows: Vec<(u8, [i32; 4])>,
    /// `(row index, slot, cell rect)`.
    pub cells: Vec<(u8, u8, [i32; 4])>,
}

/// The report's own file name, in the file list and on disk.
pub const REPORT_FILE: &str = "report.json";

/// Write a dump's images. Returns the file names written, in write order.
///
/// Images first, report second, because the report NAMES the files and carries
/// the time they took: writing it first would mean the on-disk copy is the one
/// copy that cannot say what the dump contains.
///
/// A rect that does not lie wholly inside the image is SKIPPED, not clamped and
/// not fatal: a half-off-screen recruit window is exactly the case a dump is
/// taken for, and losing one cell PNG must not cost the report that explains
/// why the cell was off-image.
pub fn write_images(dir: &Path, input: &DumpImages) -> Result<Vec<String>, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut written = Vec::new();

    input
        .screen
        .save(dir.join("screen.png"))
        .map_err(|e| format!("screen.png: {e}"))?;
    written.push("screen.png".to_string());

    for (row, rect) in &input.rows {
        let name = row_file(*row);
        if let Some(crop) = crop_exact(input.screen, *rect) {
            crop.save(dir.join(&name)).map_err(|e| format!("{name}: {e}"))?;
            written.push(name);
        }
    }
    for (row, slot, rect) in &input.cells {
        let name = cell_file(*row, *slot);
        if let Some(crop) = crop_exact(input.screen, *rect) {
            crop.save(dir.join(&name)).map_err(|e| format!("{name}: {e}"))?;
            written.push(name);
        }
    }
    Ok(written)
}

/// A dump's complete file list: the images, then the report that names them.
///
/// The report is in its own list on purpose — someone reading `report.json`
/// out of a zip should see every file the dump produced, itself included.
pub fn dump_files(mut images: Vec<String>) -> Vec<String> {
    images.push(REPORT_FILE.to_string());
    images
}

/// Write `report.json` into an existing dump directory.
pub fn write_report(dir: &Path, detail: &serde_json::Value) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let json = serde_json::to_string_pretty(detail).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(REPORT_FILE), json).map_err(|e| format!("{REPORT_FILE}: {e}"))
}

/// Crop `rect` when it lies wholly inside the image.
fn crop_exact(img: &DynamicImage, rect: [i32; 4]) -> Option<DynamicImage> {
    let [x, y, w, h] = rect;
    if x < 0 || y < 0 || w <= 0 || h <= 0 {
        return None;
    }
    let (iw, ih) = img.dimensions();
    if (x + w) as u32 > iw || (y + h) as u32 > ih {
        return None;
    }
    Some(img.crop_imm(x as u32, y as u32, w as u32, h as u32))
}

/// The one LOGS line a debug capture leaves behind.
pub fn summary_line(r: &MercDebugReport) -> String {
    format!(
        "Merc debug: {} — {} OCR lines, {} skill candidates, anchor {}, {}, geometry {}",
        r.dump_dir,
        r.ocr_lines,
        r.skill_candidates,
        match &r.anchor_text {
            Some(t) => format!("{t:?}"),
            None => "missing".to_string(),
        },
        if r.detected {
            format!(
                "{} rows at scale {:.3}, {} occupied cells",
                r.rows,
                r.scale.unwrap_or(0.0),
                r.occupied_cells
            )
        } else {
            "no panel detected".to_string()
        },
        r.geometry_source,
    )
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Take a debug capture: the screen, or a saved PNG when `image_path` is given.
///
/// Runs on its own OS thread rather than on the async runtime because the OCR
/// engine is apartment-threaded — the same rule that makes the capture loop a
/// thread module (see `spawn_gem_scan` in lib.rs).
#[tauri::command]
pub async fn merc_debug_capture(
    image_path: Option<String>,
    app: AppHandle,
) -> Result<MercDebugReport, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(debug_capture_blocking(image_path, app));
    });
    rx.await
        .map_err(|_| "merc debug capture thread died before reporting".to_string())?
}

fn debug_capture_blocking(
    image_path: Option<String>,
    app: AppHandle,
) -> Result<MercDebugReport, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("no app data directory to write a dump into: {e}"))?;
    // Re-read the override on every dump: recalibrating means editing the file
    // and pressing the button again, without restarting the module.
    let (g, geometry_source, geometry_error) = super::load_override(&data_dir);
    let vocab = MercVocab::load()?;
    let dir = dump_dir(&data_dir, now_ms());

    let mut timings = Vec::new();
    let mut notes = Vec::new();

    let started = Instant::now();
    let (img, source) = match &image_path {
        Some(path) => (
            image::open(path).map_err(|e| format!("{path}: {e}"))?,
            path.clone(),
        ),
        None => (crate::capture::capture_screen()?, "screen".to_string()),
    };
    timings.push(timing("capture", started));
    let (iw, ih) = img.dimensions();

    let mut report = MercDebugReport {
        dump_dir: dir.display().to_string(),
        source,
        screen: [iw, ih],
        geometry_source: geometry_source.to_string(),
        geometry_error,
        ocr_lines: 0,
        skill_candidates: 0,
        anchor_text: None,
        detected: false,
        rows: 0,
        occupied_cells: 0,
        scale: None,
        row_pitch: None,
        learned_templates: 0,
        timings,
        files: Vec::new(),
        notes: Vec::new(),
    };

    let started = Instant::now();
    let lines = crate::ocr::recognize_lines(&img);
    report.timings.push(timing("ocr", started));

    let lines = match lines {
        Ok(lines) => lines,
        Err(e) => {
            // The grab is still worth keeping: on Windows it says what was on
            // screen when OCR failed, and off Windows it is the only artifact
            // this path can produce.
            notes.push(format!("OCR failed: {e}"));
            report.notes = notes;
            let dumped = write_images(
                &dir,
                &DumpImages {
                    screen: &img,
                    rows: Vec::new(),
                    cells: Vec::new(),
                },
            )
            .and_then(|files| {
                report.files = dump_files(files);
                write_report(&dir, &serde_json::json!({ "report": &report, "geometry": &g }))
            });
            return Err(match dumped {
                Ok(()) => format!("{e} — partial dump at {}", dir.display()),
                Err(de) => format!("{e} (the partial dump also failed: {de})"),
            });
        }
    };

    report.ocr_lines = lines.len();
    report.skill_candidates = lines
        .iter()
        .filter(|l| {
            let read = vocab.match_skill(&l.text, &g.thresholds);
            read.state != super::ReadState::Unknown
        })
        .count();
    report.anchor_text = lines
        .iter()
        .find(|l| geometry::is_wager_line(&l.text, &g))
        .map(|l| l.text.clone());

    let started = Instant::now();
    let layout = geometry::detect(&lines, &g, &vocab);
    report.timings.push(timing("detect", started));

    let (capture, cells, rows, pass1): (
        Option<MercCapture>,
        Vec<CellDebug>,
        Vec<(u8, [i32; 4])>,
        Vec<String>,
    ) = match &layout {
        None => {
            notes.push(detect_note(&report));
            (None, Vec::new(), Vec::new(), Vec::new())
        }
        Some(layout) => {
            let started = Instant::now();
            let texts = pass2_texts(&img, layout, &g);
            report.timings.push(timing("pass2", started));

            let started = Instant::now();
            let result = {
                let state = app.state::<AppState>();
                let store = state
                    .merc_templates
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                report.learned_templates = store.len();
                build_capture(&img, layout, &texts, now_ms(), &g, &vocab, &store)
            };
            report.timings.push(timing("read", started));

            report.detected = true;
            report.rows = result.capture.rows.len();
            report.scale = Some(layout.scale);
            report.row_pitch = Some(layout.row_pitch);
            report.occupied_cells = result.cells.iter().filter(|c| c.occupied).count();
            let rows = layout
                .rows
                .iter()
                .map(|r| (r.index, r.name_rect))
                .collect::<Vec<_>>();
            let pass1 = layout.rows.iter().map(|r| r.text.clone()).collect();
            (Some(result.capture), result.cells, rows, pass1)
        }
    };

    let pass2: Vec<String> = capture
        .as_ref()
        .map(|c| c.rows.iter().map(|r| r.skill.raw.clone()).collect())
        .unwrap_or_default();
    let cell_files: Vec<(u8, u8, [i32; 4])> =
        cells.iter().map(|c| (c.row, c.slot, c.rect)).collect();
    for (row, slot, rect) in &cell_files {
        if crop_rgba(&img, *rect, &g).is_none() {
            notes.push(format!("cell {row}-{slot} rect {rect:?} falls outside the image"));
        }
    }
    report.notes = notes;

    let started = Instant::now();
    let files = write_images(
        &dir,
        &DumpImages {
            screen: &img,
            rows,
            cells: cell_files,
        },
    )?;
    // Both of these have to be on `report` BEFORE it is serialized, or the
    // on-disk copy — the one that outlives the session — is the only copy that
    // cannot say what the dump holds or what it cost.
    report.timings.push(timing("dump", started));
    report.files = dump_files(files);

    write_report(
        &dir,
        &serde_json::json!({
            "report": &report,
            "geometry": &g,
            "lines": lines.iter().map(to_debug_line).collect::<Vec<_>>(),
            "pass1Texts": pass1,
            "pass2Texts": pass2,
            "capture": &capture,
            "cells": &cells,
        }),
    )?;

    crate::app_log(&app, summary_line(&report));
    Ok(report)
}

/// Why a detect over these lines found nothing — the two D2 preconditions, in
/// the operator's terms.
fn detect_note(report: &MercDebugReport) -> String {
    match (report.skill_candidates, &report.anchor_text) {
        (0..=1, _) => format!(
            "no panel: {} skill-name candidates, 2 needed",
            report.skill_candidates
        ),
        (_, None) => "no panel: skill names found, but no \"Wager\" anchor line".to_string(),
        _ => "no panel: candidates and anchor present, so the row clustering or the anchor's \
              position relative to row 1 rejected it — see the line rects"
            .to_string(),
    }
}

fn to_debug_line(l: &OcrLineBox) -> DebugLine {
    DebugLine {
        text: l.text.clone(),
        rect: [l.x, l.y, l.w, l.h],
    }
}

fn timing(label: &str, started: Instant) -> DebugTiming {
    DebugTiming {
        label: label.to_string(),
        ms: started.elapsed().as_millis() as u64,
    }
}

/// Forget one learned icon template — the un-poison path for a mistimed hover
/// (D10 §1). `tier` is `Option` because the page parses it out of a store key;
/// a key it could not parse must fail loudly rather than forget something else.
#[tauri::command]
pub fn merc_forget_template(family: String, tier: Option<u8>, app: AppHandle) -> Result<(), String> {
    let Some(tier) = tier else {
        return Err(format!("template key for {family:?} carries no tier"));
    };
    let dir = templates_dir(&app)?;
    let learned = {
        let state = app.state::<AppState>();
        let mut store = state
            .merc_templates
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if !store.forget(&family, tier) {
            return Err(format!("no learned template for {family} (tier {tier})"));
        }
        store.save(&dir)?;
        store.learned_keys()
    };
    bump_generation(&app);
    crate::app_log(&app, format!("Merc: forgot template {family} (tier {tier})"));
    publish(&app, |slice| slice.learned_families = learned);
    Ok(())
}

/// Drop every learned template (D10 §1).
///
/// The template PNGs of the forgotten samples stay on disk; `index.json` is
/// rewritten empty, and the store loads from the index, so they are inert. They
/// are left rather than deleted because a wrongly-reset store is otherwise
/// unrecoverable, and the directory is a debug surface anyway.
#[tauri::command]
pub fn merc_reset_templates(app: AppHandle) -> Result<(), String> {
    let dir = templates_dir(&app)?;
    let count = {
        let state = app.state::<AppState>();
        let mut store = state
            .merc_templates
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let count = store.len();
        store.reset();
        store.save(&dir)?;
        count
    };
    bump_generation(&app);
    crate::app_log(&app, format!("Merc: reset {count} learned templates"));
    publish(&app, |slice| slice.learned_families = Vec::new());
    Ok(())
}

/// Tell the capture loop the store was edited by hand.
///
/// Dropping a template is only half the un-poison: the loop also holds the
/// CONFIRMATION in memory and re-applies it to every later capture, so without
/// this the forgotten cell keeps showing the identity the user just disowned
/// until the recruit window closes.
fn bump_generation(app: &AppHandle) {
    let state = app.state::<AppState>();
    state
        .merc_template_generation
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
}

fn templates_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join(super::ICONS_DIR))
        .map_err(|e| format!("no app data directory: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "poe-merc-dump-{}-{}-{:?}",
            tag,
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn screen() -> DynamicImage {
        let mut img = RgbaImage::from_pixel(200, 120, Rgba([10, 10, 12, 255]));
        for y in 0..40 {
            for x in 0..60 {
                img.put_pixel(x, y, Rgba([200, 180, 90, 255]));
            }
        }
        DynamicImage::ImageRgba8(img)
    }

    fn report() -> MercDebugReport {
        MercDebugReport {
            dump_dir: "/tmp/merc-debug/1".into(),
            source: "screen".into(),
            screen: [200, 120],
            geometry_source: "default".into(),
            geometry_error: None,
            ocr_lines: 7,
            skill_candidates: 2,
            anchor_text: Some("Wager: 1 028".into()),
            detected: true,
            rows: 6,
            occupied_cells: 12,
            scale: Some(0.986),
            row_pitch: Some(48.6),
            learned_templates: 3,
            timings: vec![DebugTiming { label: "ocr".into(), ms: 412 }],
            files: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// One directory per capture, named by the timestamp: two dumps must not
    /// overwrite each other, because comparing them is how a threshold gets
    /// tuned.
    #[test]
    fn each_dump_gets_its_own_timestamped_directory() {
        let root = Path::new("/data");

        let a = dump_dir(root, 1_700_000_000_000);
        let b = dump_dir(root, 1_700_000_000_001);

        assert_eq!(a, Path::new("/data/merc-debug/1700000000000"));
        assert_ne!(a, b);
    }

    /// The dump's file names are the page's and the operator's index into it —
    /// `cell-2-3.png` IS the coordinate of the cell it shows.
    #[test]
    fn dump_file_names_carry_the_row_and_slot_they_show() {
        assert_eq!(row_file(4), "row-4.png");
        assert_eq!(cell_file(2, 3), "cell-2-3.png");
    }

    /// The whole dump: the screen, one crop per row, one per cell — written
    /// under the given directory, which may not exist yet.
    #[test]
    fn a_dump_writes_the_screen_and_one_crop_per_row_and_cell() {
        let dir = temp_dir("full");
        let img = screen();

        let files = write_images(
            &dir,
            &DumpImages {
                screen: &img,
                rows: vec![(0, [0, 0, 60, 20])],
                cells: vec![(0, 0, [0, 0, 40, 40]), (0, 1, [40, 0, 40, 40])],
            },
        )
        .expect("dump writes");

        assert_eq!(
            files,
            vec![
                "screen.png".to_string(),
                "row-0.png".to_string(),
                "cell-0-0.png".to_string(),
                "cell-0-1.png".to_string(),
            ]
        );
        let row = image::open(dir.join("row-0.png")).expect("the row crop is a readable png");
        assert_eq!((row.width(), row.height()), (60, 20));
        let cell = image::open(dir.join("cell-0-1.png")).expect("the cell crop is readable");
        assert_eq!((cell.width(), cell.height()), (40, 40));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The images are written first and the report last, so the report can name
    /// them — including itself. A file list that omitted `report.json` would
    /// leave a reader of the dump guessing at what it is holding.
    #[test]
    fn the_reports_file_list_names_the_images_and_itself() {
        let files = dump_files(vec!["screen.png".to_string(), "cell-0-0.png".to_string()]);

        assert_eq!(
            files,
            vec![
                "screen.png".to_string(),
                "cell-0-0.png".to_string(),
                "report.json".to_string(),
            ]
        );
    }

    /// `report.json` is the calibration artifact — it must come back as JSON
    /// carrying the numbers, not as a debug-formatted blob.
    #[test]
    fn the_report_json_round_trips_the_summary() {
        let dir = temp_dir("json");
        let img = screen();

        write_images(
            &dir,
            &DumpImages {
                screen: &img,
                rows: Vec::new(),
                cells: Vec::new(),
            },
        )
        .expect("dump writes");
        let mut r = report();
        r.files = dump_files(vec!["screen.png".to_string()]);
        write_report(&dir, &serde_json::json!({ "report": r, "lines": [] }))
            .expect("report writes");

        let raw = std::fs::read_to_string(dir.join("report.json")).expect("report written");
        let parsed: serde_json::Value = serde_json::from_str(&raw).expect("report is JSON");
        // f32 → JSON is not exact; what the test pins is that the NUMBER
        // survives as a number, not that it round-trips bit for bit.
        let pitch = parsed["report"]["rowPitch"].as_f64().expect("row pitch is a number");
        assert!((pitch - 48.6).abs() < 0.001, "got {pitch}");
        assert_eq!(parsed["report"]["anchorText"], "Wager: 1 028");
        assert_eq!(parsed["report"]["timings"][0]["label"], "ocr");
        assert_eq!(parsed["report"]["occupiedCells"], 12);
        assert_eq!(
            parsed["report"]["files"],
            serde_json::json!(["screen.png", "report.json"]),
            "the on-disk report must list the files the dump wrote",
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A half-off-screen recruit window is exactly what a dump gets taken for:
    /// the crop that cannot be made is skipped, and the report that explains it
    /// is still written.
    #[test]
    fn a_crop_that_falls_outside_the_image_is_skipped_not_fatal() {
        let dir = temp_dir("offimage");
        let img = screen();

        let files = write_images(
            &dir,
            &DumpImages {
                screen: &img,
                rows: Vec::new(),
                cells: vec![(0, 0, [180, 100, 44, 44]), (0, 1, [-4, 0, 44, 44])],
            },
        )
        .expect("an off-image crop must not fail the dump");

        assert_eq!(files, vec!["screen.png".to_string()]);
        assert!(!dir.join("cell-0-0.png").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The LOGS line has to answer "did it see the window?" on its own — the
    /// dump path is there for the follow-up, not for the first read.
    #[test]
    fn the_log_summary_states_the_detection_and_where_the_dump_is() {
        let line = summary_line(&report());

        assert!(line.contains("/tmp/merc-debug/1"), "got {line}");
        assert!(line.contains("7 OCR lines"), "got {line}");
        assert!(line.contains("6 rows at scale 0.986"), "got {line}");
        assert!(line.contains("12 occupied cells"), "got {line}");
    }

    /// A failed detect must say so in the same line — "0 rows" would read as a
    /// detected but empty panel.
    #[test]
    fn the_log_summary_says_when_no_panel_was_detected() {
        let mut r = report();
        r.detected = false;
        r.anchor_text = None;

        let line = summary_line(&r);

        assert!(line.contains("no panel detected"), "got {line}");
        assert!(line.contains("anchor missing"), "got {line}");
    }

    /// The note names the precondition that actually failed, in D2's order:
    /// too few skill candidates is the first gate, the anchor the second.
    #[test]
    fn the_detect_note_names_the_precondition_that_failed() {
        let mut r = report();
        r.detected = false;
        r.skill_candidates = 1;
        assert!(detect_note(&r).contains("2 needed"), "{}", detect_note(&r));

        r.skill_candidates = 4;
        r.anchor_text = None;
        assert!(detect_note(&r).contains("Wager"), "{}", detect_note(&r));

        r.anchor_text = Some("Wager: 1 028".into());
        let note = detect_note(&r);
        assert!(
            note.contains("row clustering"),
            "with both preconditions met the note must point elsewhere, got {note}",
        );
    }
}
