//! Loot capture — PROOF OF CONCEPT (2026-10-27).
//!
//! The owner farms Catarina and logs each run's drops in a spreadsheet. A
//! hotkey grabs a rectangle centred on the cursor, OCRs the ground labels in it,
//! matches them to the sheet's columns and appends one run row to a CSV in the
//! app-data dir. Read-only towards the game: it reads the screen and the
//! cursor position, and sends nothing to the client.
//!
//! - `Shift+Right mouse button` (game in foreground): capture, flash the
//!   rectangle and the readout for [`FLASH_MS`], append a row when anything
//!   matched.
//! - `Shift+F8`: toggle the preview, where the rectangle follows the cursor.
//!
//! The hotkeys are POLLED with `GetAsyncKeyState` rather than registered, so the
//! key still reaches the game and nothing is consumed. The overlay is one
//! window, `loot-capture` (`routes/overlay/loot`), built on first use and
//! afterwards only moved and resized; its page polls [`loot_capture_view`].
//! It is in `capture.rs`'s exclusion list, so the preview frame never becomes
//! OCR input.
//!
//! Measured offline 2026-10-27: plain `Windows.Media.Ocr` en-US on raw crops of
//! 10 screenshots read all 27 item labels, in upper case. The noise lines seen
//! there were gold amounts, "JUN, VEILED MASTER" and short fragments.

// The hotkey thread, the only caller of the capture half, is Windows-only.
#![cfg_attr(not(windows), allow(dead_code))]

use serde::Serialize;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The sheet's loot columns, in the sheet's order.
pub const COLUMNS: [&str; 11] = [
    "Veiled Exalted",
    "Eye",
    "Allflame",
    "Flask",
    "Staff",
    "Quiver",
    "Body armour",
    "Shield",
    "Helmet",
    "Support",
    "Templar Astrolabe",
];

/// How long the rectangle and the readout stay up after a capture.
const FLASH_MS: u64 = 1500;

/// The capture size at 1080p, scaled by the screen height.
const RECT_W_1080: f64 = 900.0;
const RECT_H_1080: f64 = 750.0;
/// The cursor sits this far down the rectangle: labels stack above the boss.
const CURSOR_Y_FRACTION: f64 = 2.0 / 3.0;

pub const WINDOW_LABEL: &str = "loot-capture";

/// What one OCR line is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineKind {
    /// Index into [`COLUMNS`].
    Column(usize),
    /// Gold, NPC names, fragments: dropped silently.
    Noise,
    /// Looks like a label but matches no column: shown, never logged.
    Unknown(String),
}

/// Upper-case, collapse whitespace, drop apostrophes, strip non-alphanumeric
/// characters from both ends (OCR adds `*` and `.`), drop a leading `SUPERIOR `.
fn normalize(line: &str) -> String {
    let upper: String = line
        .chars()
        .filter(|c| !matches!(c, '\'' | '’' | '`' | '‘'))
        .collect::<String>()
        .to_uppercase();
    let collapsed = upper.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim_matches(|c: char| !c.is_alphanumeric());
    trimmed.strip_prefix("SUPERIOR ").unwrap_or(trimmed).to_string()
}

fn is_noise(norm: &str) -> bool {
    let letters = norm.chars().filter(|c| c.is_alphabetic()).count();
    if letters < 4 {
        return true;
    }
    // "191 GOLD", "4108 Gold".
    let mut words = norm.split(' ');
    let first_is_number = words.next().is_some_and(|w| w.chars().all(|c| c.is_ascii_digit() || c == ','));
    if first_is_number && norm.ends_with("GOLD") {
        return true;
    }
    // "JUN, VEILED MASTER" and any other NPC title line; no item label has a comma.
    norm.contains(',') || norm.contains("VEILED MASTER")
}

pub fn classify_line(line: &str) -> LineKind {
    let norm = normalize(line);
    if is_noise(&norm) {
        return LineKind::Noise;
    }
    let ends = |suffix: &str| norm == suffix || norm.ends_with(&format!(" {suffix}"));
    let column = if norm == "VEILED EXALTED ORB" {
        Some(0)
    } else if norm.starts_with("ZORATHS EYE") {
        Some(1)
    } else if norm.starts_with("ALLFLAME EMBER") {
        Some(2)
    } else if ends("FLASK") {
        Some(3)
    } else if ends("STAFF") {
        Some(4)
    } else if ends("QUIVER") {
        Some(5)
    } else if norm == "VAAL REGALIA" || norm == "SPIDERSILK ROBE" {
        Some(6)
    } else if ends("SHIELD") {
        Some(7)
    } else if ends("CIRCLET") {
        Some(8)
    } else if ends("SUPPORT") {
        Some(9)
    } else if norm == "TEMPLAR ASTROLABE" {
        Some(10)
    } else {
        None
    };
    match column.or_else(|| fragment_column(&norm)) {
        Some(i) => LineKind::Column(i),
        None => LineKind::Unknown(line.trim().to_string()),
    }
}

/// Stems of the table's distinctive words, for a label OCR read only part of
/// (2026-10-27: "TITANIUM SPIRIT SHIELD" came back as "TANIUM SPIRI"). Each
/// stem occurs in exactly one column's names, so a hit is unambiguous; generic
/// words ("FLASK", "STAFF", "SHIELD", "ORB") are deliberately absent, because a
/// fragment of them says nothing a full-word rule above does not.
const FRAGMENTS: [(&str, usize); 9] = [
    ("ZORATH", 1),
    ("ALLFLAM", 2),
    ("QUIVE", 5),
    ("REGALI", 6),
    ("SPIDERSI", 6),
    ("SPIRI", 7),
    ("CIRCL", 8),
    ("SUPPOR", 9),
    ("ASTROLA", 10),
];

fn fragment_column(norm: &str) -> Option<usize> {
    FRAGMENTS.iter().find(|(stem, _)| norm.contains(stem)).map(|(_, i)| *i)
}

/// One capture's verdict: which columns got a drop, and the unknown lines.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RunMatch {
    pub hits: [bool; COLUMNS.len()],
    pub unknown: Vec<String>,
}

impl RunMatch {
    pub fn from_lines(lines: &[String]) -> Self {
        let mut out = RunMatch::default();
        for line in lines {
            match classify_line(line) {
                LineKind::Column(i) => out.hits[i] = true,
                LineKind::Noise => {}
                LineKind::Unknown(text) => {
                    if !out.unknown.contains(&text) {
                        out.unknown.push(text)
                    }
                }
            }
        }
        out
    }

    pub fn any(&self) -> bool {
        self.hits.iter().any(|h| *h)
    }

    /// The readout lines the overlay shows.
    fn readout(&self, run: Option<u32>) -> Vec<String> {
        let mut lines: Vec<String> = Vec::new();
        if self.any() {
            let names: Vec<&str> =
                COLUMNS.iter().zip(self.hits).filter(|(_, h)| *h).map(|(n, _)| *n).collect();
            match run {
                Some(n) => lines.push(format!("Run {n}: {}", names.join(", "))),
                None => lines.push(format!("{} (not saved)", names.join(", "))),
            }
        } else {
            lines.push("nothing matched".to_string());
        }
        lines.extend(self.unknown.iter().map(|u| format!("unknown: {u}")));
        lines
    }
}

/// The capture rectangle `[x, y, w, h]` in screen-relative physical pixels:
/// centred horizontally on `cursor` with the cursor two thirds down, sized by the screen height, clamped inside the screen.
pub fn capture_rect(cursor: (i32, i32), screen: (u32, u32)) -> [i32; 4] {
    let (sw, sh) = (screen.0 as i32, screen.1 as i32);
    let scale = screen.1 as f64 / 1080.0;
    let w = ((RECT_W_1080 * scale).round() as i32).clamp(1, sw.max(1));
    let h = ((RECT_H_1080 * scale).round() as i32).clamp(1, sh.max(1));
    let x = (cursor.0 - w / 2).clamp(0, (sw - w).max(0));
    let y = (cursor.1 - (h as f64 * CURSOR_Y_FRACTION).round() as i32).clamp(0, (sh - h).max(0));
    [x, y, w, h]
}

/// The CSV row for run `run`; `timestamp` is the last cell.
fn csv_row(run: u32, m: &RunMatch, timestamp: &str) -> String {
    let cells: Vec<&str> = m.hits.iter().map(|h| if *h { "1" } else { "" }).collect();
    format!("{run},{},{timestamp}", cells.join(","))
}

fn csv_header() -> String {
    format!("Run #,{},timestamp", COLUMNS.join(","))
}

/// Append one run to `path`, writing the header first on a new file. Returns
/// the run number. Appending never truncates, so the earlier rows survive a
/// crash mid-write; only the hotkey thread writes this file.
fn append_run(path: &std::path::Path, m: &RunMatch, timestamp: &str) -> Result<u32, String> {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    }
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let rows = existing.lines().skip(1).filter(|l| !l.trim().is_empty()).count() as u32;
    let mut text = String::new();
    if existing.is_empty() {
        text.push_str(&csv_header());
        text.push('\n');
    } else if !existing.ends_with('\n') {
        text.push('\n');
    }
    let run = rows + 1;
    text.push_str(&csv_row(run, m, timestamp));
    text.push('\n');
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("open {}: {e}", path.display()))?;
    file.write_all(text.as_bytes()).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(run)
}

/// What the overlay page draws. Owned by the hotkey thread, read by the page.
struct ViewState {
    flash_until: Option<Instant>,
    preview: bool,
    lines: Vec<String>,
}

static VIEW: Mutex<ViewState> = Mutex::new(ViewState { flash_until: None, preview: false, lines: Vec::new() });

#[derive(Debug, Clone, Serialize)]
pub struct LootCaptureView {
    pub visible: bool,
    pub preview: bool,
    pub lines: Vec<String>,
}

/// Polled by `routes/overlay/loot`.
#[tauri::command]
pub fn loot_capture_view() -> LootCaptureView {
    let view = VIEW.lock().unwrap_or_else(|e| e.into_inner());
    let flashing = view.flash_until.is_some_and(|t| Instant::now() < t);
    LootCaptureView {
        visible: flashing || view.preview,
        preview: view.preview,
        lines: if flashing { view.lines.clone() } else { Vec::new() },
    }
}

fn set_flash(lines: Vec<String>) {
    let mut view = VIEW.lock().unwrap_or_else(|e| e.into_inner());
    view.flash_until = Some(Instant::now() + Duration::from_millis(FLASH_MS));
    view.lines = lines;
}

#[cfg(windows)]
pub use platform::spawn_hotkey_poller;

#[cfg(not(windows))]
pub fn spawn_hotkey_poller(_app: tauri::AppHandle) {}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::sync::atomic::Ordering;
    use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize};
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_F8, VK_RBUTTON, VK_SHIFT};
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    const POLL_MS: u64 = 30;

    fn key_down(vk: i32) -> bool {
        (unsafe { GetAsyncKeyState(vk) } as u16 & 0x8000) != 0
    }

    fn cursor() -> Option<(i32, i32)> {
        let mut p = POINT::default();
        unsafe { GetCursorPos(&mut p) }.ok()?;
        Some((p.x, p.y))
    }

    /// The game's display (origin, size), else the primary's.
    fn screen(app: &AppHandle) -> Option<((i32, i32), (u32, u32))> {
        let game = *app.state::<crate::AppState>().game_monitor.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(g) = game {
            return Some(((g.x, g.y), (g.width, g.height)));
        }
        let m = app.primary_monitor().ok().flatten()?;
        Some(((m.position().x, m.position().y), (m.size().width, m.size().height)))
    }

    /// Build the window on first use, then only move and resize it. `rect` is
    /// screen-ABSOLUTE physical px.
    fn place_window(app: &AppHandle, rect: [i32; 4]) {
        let position = PhysicalPosition::new(rect[0], rect[1]);
        let size = PhysicalSize::new(rect[2] as u32, rect[3] as u32);
        if let Some(win) = app.get_webview_window(WINDOW_LABEL) {
            if let Err(e) = win.set_position(position).and_then(|_| win.set_size(size)) {
                crate::app_log(app, format!("loot: moving the overlay failed: {e}"));
            }
            return;
        }
        let built = tauri::WebviewWindowBuilder::new(
            app,
            WINDOW_LABEL,
            tauri::WebviewUrl::App("overlay/loot".into()),
        )
        .transparent(true)
        .decorations(false)
        .always_on_top(true)
        .resizable(false)
        .shadow(false)
        .skip_taskbar(true)
        .focused(false)
        .inner_size(rect[2] as f64, rect[3] as f64)
        .build();
        let win = match built {
            Ok(w) => w,
            Err(e) => {
                crate::app_log(app, format!("loot: building the overlay failed: {e}"));
                return;
            }
        };
        // Exact physical geometry after construction, with the WebView2
        // transparency resize nudge (overlay guide, guard 3 and the
        // runtime-earned observations).
        let placed = win
            .set_position(position)
            .and_then(|_| win.set_size(PhysicalSize::new(size.width + 1, size.height + 1)))
            .and_then(|_| win.set_size(size))
            .and_then(|_| win.set_position(position));
        if let Err(e) = placed {
            crate::app_log(app, format!("loot: placing the overlay failed: {e}"));
        }
        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = crate::apply_overlay_clickthrough(WINDOW_LABEL.to_string(), app2.clone()).await {
                crate::app_log(&app2, format!("loot: click-through setup failed: {e}"));
                if let Some(w) = app2.get_webview_window(WINDOW_LABEL) {
                    let _ = w.destroy();
                }
            }
        });
    }

    /// DEBUG AID (preview box vs grabbed pixels, 2026-10-27): every geometry
    /// input in one line, so a unit mismatch between the preview's monitor
    /// (`screen`), the grab, the cursor and the window shows up as two numbers
    /// that disagree.
    fn log_geometry(app: &AppHandle, cur: (i32, i32), shot: &crate::capture::Capture, rect: [i32; 4]) {
        let preview_screen = screen(app);
        let preview_rect = preview_screen.map(|((px, py), dims)| {
            let r = capture_rect((cur.0 - px, cur.1 - py), dims);
            [r[0] + px, r[1] + py, r[2], r[3]]
        });
        let window = app.get_webview_window(WINDOW_LABEL).map(|w| {
            format!(
                "outer {:?} {:?}, inner {:?}, scale {:?}",
                w.outer_position().map(|p| (p.x, p.y)).ok(),
                w.outer_size().map(|s| (s.width, s.height)).ok(),
                w.inner_size().map(|s| (s.width, s.height)).ok(),
                w.scale_factor().ok()
            )
        });
        let monitor = app.monitor_from_point(cur.0 as f64, cur.1 as f64).ok().flatten().map(|m| {
            format!("{:?} {:?} scale {}", m.position(), m.size(), m.scale_factor())
        });
        crate::app_log(
            app,
            format!(
                "loot: geometry cursor {cur:?}; grab origin {:?} image {}x{}; crop (abs) {:?}; \
                 preview monitor {preview_screen:?} -> rect {preview_rect:?}; tauri monitor at cursor \
                 {monitor:?}; window {window:?}",
                shot.origin,
                shot.image.width(),
                shot.image.height(),
                [rect[0] + shot.origin.0, rect[1] + shot.origin.1, rect[2], rect[3]],
            ),
        );
    }

    /// DEBUG AID: the exact pixels handed to OCR, overwritten per capture.
    fn save_crop(app: &AppHandle, crop: &image::DynamicImage) {
        let Ok(dir) = app.path().app_data_dir() else { return };
        let path = dir.join("loot-runs").join("last-capture.png");
        let saved = std::fs::create_dir_all(dir.join("loot-runs"))
            .map_err(|e| e.to_string())
            .and_then(|_| crop.save(&path).map_err(|e| e.to_string()));
        if let Err(e) = saved {
            crate::app_log(app, format!("loot: saving {} failed: {e}", path.display()));
        }
    }

    fn capture_once(app: &AppHandle) {
        let Some(cur) = cursor() else {
            crate::app_log(app, "loot: cursor position unreadable".to_string());
            return;
        };
        let started = Instant::now();
        let shot = match crate::capture::capture_screen(app) {
            Ok(s) => s,
            Err(e) => {
                crate::app_log(app, format!("loot: capture failed: {e}"));
                return;
            }
        };
        let grab_ms = started.elapsed().as_millis();
        let (ox, oy) = shot.origin;
        let rect = capture_rect((cur.0 - ox, cur.1 - oy), (shot.image.width(), shot.image.height()));
        place_window(app, [rect[0] + ox, rect[1] + oy, rect[2], rect[3]]);
        set_flash(vec!["reading…".to_string()]);
        let place_ms = started.elapsed().as_millis() - grab_ms;

        let stage = Instant::now();
        let crop = shot.image.crop_imm(rect[0] as u32, rect[1] as u32, rect[2] as u32, rect[3] as u32);
        // Two reads, matches unioned: the raw crop (what read all 27 labels
        // offline) and the app's bilinear 2x preprocessing (`capture.rs`). One
        // live raw read lost both ends of a label that sat fully inside the box
        // (2026-10-27), and the gem band showed a kernel can blind one header
        // while another reads it. A capture is one keypress per run, so the
        // second OCR costs nothing the player waits on in a loop.
        let crop_ms = stage.elapsed().as_millis();
        let stage = Instant::now();
        let raw = match crate::ocr::recognize_text(&crop) {
            Ok(l) => l,
            Err(e) => {
                crate::app_log(app, format!("loot: OCR failed: {e}"));
                set_flash(vec![format!("OCR failed: {e}")]);
                return;
            }
        };
        let raw_ocr_ms = stage.elapsed().as_millis();
        let stage = Instant::now();
        let scaled = crate::capture::preprocess_for_ocr_fast(&crop);
        let preprocess_ms = stage.elapsed().as_millis();
        let stage = Instant::now();
        let upscaled = crate::ocr::recognize_text(&scaled)
            .unwrap_or_else(|e| {
                crate::app_log(app, format!("loot: OCR of the 2x crop failed: {e}"));
                Vec::new()
            });
        let scaled_ocr_ms = stage.elapsed().as_millis();
        crate::app_log(
            app,
            format!(
                "loot: rect {:?} on screen at ({ox},{oy}), {} ms, OCR raw: {:?}, OCR 2x: {:?}",
                rect,
                started.elapsed().as_millis(),
                raw,
                upscaled
            ),
        );
        let lines: Vec<String> = raw.into_iter().chain(upscaled).collect();
        log_geometry(app, cur, &shot, rect);
        let stage = Instant::now();
        save_crop(app, &crop);
        let save_ms = stage.elapsed().as_millis();

        let m = RunMatch::from_lines(&lines);
        let mut run = None;
        if m.any() {
            match app.path().app_data_dir() {
                Ok(dir) => {
                    let path = dir.join("loot-runs").join("catarina.csv");
                    let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                    match append_run(&path, &m, &ts) {
                        Ok(n) => {
                            run = Some(n);
                            crate::app_log(app, format!("loot: run {n} appended to {}", path.display()));
                        }
                        Err(e) => crate::app_log(app, format!("loot: CSV write failed: {e}")),
                    }
                }
                Err(e) => crate::app_log(app, format!("loot: no app-data dir: {e}")),
            }
        }
        set_flash(m.readout(run));
        crate::app_log(
            app,
            format!(
                "loot: timing grab {grab_ms} ms, place window {place_ms}, crop {crop_ms}, OCR raw {raw_ocr_ms}, \
                 2x preprocess {preprocess_ms}, OCR 2x {scaled_ocr_ms}, PNG save {save_ms}, total {} ms ({} build)",
                started.elapsed().as_millis(),
                if cfg!(debug_assertions) { "debug" } else { "release" }
            ),
        );
    }

    pub fn spawn_hotkey_poller(app: AppHandle) {
        std::thread::spawn(move || {
            crate::app_log(&app, "loot: hotkey poller started (Shift+RMB capture, Shift+F8 preview)".to_string());
            let mut capture_was_down = false;
            let mut preview_was_down = false;
            let mut last_preview_cursor: Option<(i32, i32)> = None;
            loop {
                std::thread::sleep(Duration::from_millis(POLL_MS));
                let shift = key_down(VK_SHIFT.0 as i32);
                let capture_down = shift && key_down(VK_RBUTTON.0 as i32);
                let capture_pressed = capture_down && !capture_was_down;
                capture_was_down = capture_down;
                let preview_down = shift && key_down(VK_F8.0 as i32);
                let preview_pressed = preview_down && !preview_was_down;
                preview_was_down = preview_down;
                let in_game = app.state::<crate::AppState>().game_in_foreground.load(Ordering::SeqCst);

                if preview_pressed && in_game {
                    let on = {
                        let mut view = VIEW.lock().unwrap_or_else(|e| e.into_inner());
                        view.preview = !view.preview;
                        view.preview
                    };
                    last_preview_cursor = None;
                    crate::app_log(&app, format!("loot: preview {}", if on { "on" } else { "off" }));
                }
                if capture_pressed && in_game {
                    capture_once(&app);
                }

                let preview = VIEW.lock().unwrap_or_else(|e| e.into_inner()).preview;
                if preview {
                    let Some(cur) = cursor() else { continue };
                    if last_preview_cursor == Some(cur) {
                        continue;
                    }
                    last_preview_cursor = Some(cur);
                    if let Some(((ox, oy), dims)) = screen(&app) {
                        let r = capture_rect((cur.0 - ox, cur.1 - oy), dims);
                        place_window(&app, [r[0] + ox, r[1] + oy, r[2], r[3]]);
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn ocr_lines_from_a_catarina_drop_map_to_their_sheet_columns() {
        let m = RunMatch::from_lines(&lines(&[
            "ZORATH'S EYE OF THE ENDLESS",
            "VEILED EXALTED ORB*",
            "SILVER FLASK.",
            "SUPERIOR VAAL REGALIA",
            "TITANIUM SPIRIT SHIELD",
            "ALLFLAME EMBER OF ANARCHY",
            "HUBRIS CIRCLET",
            "BROADHEAD ARROW QUIVER",
            "EZOMYTE STAFF",
            "SPELL ECHO SUPPORT",
            "TEMPLAR ASTROLABE",
        ]));
        assert_eq!(m.hits, [true; 11]);
        assert!(m.unknown.is_empty(), "unexpected unknowns: {:?}", m.unknown);
    }

    #[test]
    fn gold_npc_names_and_fragments_are_dropped_as_noise() {
        let m = RunMatch::from_lines(&lines(&["191 GOLD", "4108 Gold", "JUN, VEILED MASTER", "Ii", "*."]));
        assert_eq!(m, RunMatch::default());
    }

    #[test]
    fn an_unmatched_label_is_reported_unknown_and_marks_no_column() {
        let m = RunMatch::from_lines(&lines(&["CHAOS ORB", "VAAL REGALIA"]));
        assert_eq!(m.unknown, vec!["CHAOS ORB".to_string()]);
        assert_eq!(m.hits.iter().filter(|h| **h).count(), 1);
        assert!(m.hits[6]);
    }

    #[test]
    fn a_label_read_only_in_part_still_maps_by_its_distinctive_stem() {
        assert_eq!(classify_line("TANIUM SPIRI"), LineKind::Column(7));
    }

    #[test]
    fn a_fragment_of_a_generic_word_stays_unknown() {
        assert_eq!(classify_line("TITANIUM SHIEL"), LineKind::Unknown("TITANIUM SHIEL".to_string()));
    }

    #[test]
    fn capture_rect_scales_with_screen_height_and_clamps_at_the_edge() {
        assert_eq!(capture_rect((1920, 1080), (3840, 2160)), [1020, 80, 1800, 1500]);
        assert_eq!(capture_rect((10, 1070), (1920, 1080)), [0, 330, 900, 750]);
    }

    #[test]
    fn appended_runs_number_from_one_under_the_header() {
        let dir = std::env::temp_dir().join(format!("loot-capture-test-{}", uuid::Uuid::new_v4()));
        let path = dir.join("catarina.csv");
        let mut eye = RunMatch::default();
        eye.hits[1] = true;
        let mut flask = RunMatch::default();
        flask.hits[3] = true;
        assert_eq!(append_run(&path, &eye, "t1"), Ok(1));
        assert_eq!(append_run(&path, &flask, "t2"), Ok(2));
        let text = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            text,
            "Run #,Veiled Exalted,Eye,Allflame,Flask,Staff,Quiver,Body armour,Shield,Helmet,Support,Templar Astrolabe,timestamp\n\
             1,,1,,,,,,,,,,t1\n\
             2,,,,1,,,,,,,,t2\n"
        );
    }
}
