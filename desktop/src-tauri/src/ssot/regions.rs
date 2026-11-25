//! Exclusive Auto/Manual OCR regions (POE-411, ADR-036): the persisted mode,
//! the nullable capture-relative Manual set, and the one resolver every region
//! reader and preview takes its rects from.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tauri::Manager;

use super::{placements_for, Placements, ScreenSlice};

/// The side panel's own border box, in reference px relative to the Entrance
/// plate centre (`TempleLayout::origin`): `[left, top, right, bottom]`, `+x`
/// right and `+y` down. The panel is drawn above and to the right of the
/// Entrance, so both vertical figures are negative.
///
/// **Measured** as the panel's border rectangle on the three captures whose
/// origin AND scale are both recorded — the committed full frame and the two
/// board fixtures' source screenshots:
///
/// | capture | size | scale | origin | panel border box | in ref px from the origin |
/// |---|---|---|---|---|---|
/// | `screen-live-1920x1080.png` | 1920x1080 | 1.0000 | (960, 713) | x 1171–1655, y 44–418 | +211, −669, +695, −295 |
/// | `2026-08-02_22-22-38` | 1374x862 | 1.0000 | (673, 682) | x 884–1368, y 13–387 | +211, −669, +695, −295 |
/// | `2026-08-07_19-28-36` | 1539x968 | 1.13 recorded | (745, 768) | x 980–1517, y 24–440 | +208, −658, +683, −290 |
/// | …the same capture at the 1.111 its own border implies | | | | | +212, −670, +695, −295 |
///
/// The two scale-1.0 captures agree **to the pixel on all four edges**, a month
/// apart and 546 px of capture width apart. That agreement is the constant.
///
/// The third row does NOT disagree with them — its ANCHOR does. Its panel
/// border measures 537 x 416 px, and `537 / 484 = 1.1095`, `416 / 374 = 1.1123`:
/// at scale **1.111** the box above reproduces that capture to under a pixel on
/// every edge. The 1.13 it is recorded at is an anchor error of ~1.7%, which is
/// `temple::run::DIAMOND_DX_REF`'s anchor-accuracy note and POE-247's subject. So there is
/// no board-to-board spread here — there is one error, the anchor's, and it is
/// what [`PANEL_MARGIN_REF`] absorbs.
///
/// # Why the origin and not the capture's right edge
///
/// Until POE-230 this region was `540 × 430` ref px hung off the capture's
/// top-RIGHT corner. Measured 2026-09-03 on the laptop (dump
/// `temple-debug/1788438639673`, the frame committed as the fixture above): that
/// put the crop at `[1380, 0, 540, 430]`, which cuts the panel in half. The
/// title read `NG WORKSHOP`, and the lower-left architect block — Xopec, whose
/// box on that frame is x 1189–1347 — was **entirely** outside the crop, so the
/// dump reported one architect on a board that has two. The panel is drawn
/// against the LAYOUT, not against the screen; the table above is that fact
/// measured, and the third row is what a screen-edge offset was really tracking.
pub const PANEL_BOX_REF: [f32; 4] = [211.0, -669.0, 695.0, -295.0];

/// How far past [`PANEL_BOX_REF`] the OCR crop reaches on the left, top and
/// bottom — in reference px. The right side is [`PANEL_RIGHT_MARGIN_REF`], which
/// is smaller and says why.
///
/// What a margin has to absorb is **anchor error**, since the box itself is the
/// same on every capture that reproduces its own scale. The recorded band is
/// −4% (POE-247's hint chain answering 0.96 where the peak is 1.00) to +1.7%
/// (the 1539 row of [`PANEL_BOX_REF`]'s table). Against a −669 ref px top offset
/// −4% is 27 px, so 40 clears the worst recorded case on the axis where the
/// offsets are largest, and it is also what the retired screen-edge constants
/// documented (~46 horizontal, ~42 vertical), carried across.
///
/// Nothing sits outside the panel on those three sides on the committed fixture,
/// so the only cost of 40 there is buffer size.
pub const PANEL_MARGIN_REF: f32 = 40.0;

/// The RIGHT margin, in reference px — smaller than [`PANEL_MARGIN_REF`] because
/// it is the one side where the crop reaches into somebody else's text.
///
/// **Measured on the committed fixture.** The map's own info block is drawn
/// BEHIND the panel; its leftmost glyph column sits **4 ref px** past the panel
/// border (+699 against the border's +695), so *every* positive right margin
/// admits a strip of it, and the volume is what the constant buys:
///
/// Offsets below are from the origin like everything else here, so the title's
/// own band — absolute y 70–112 on the fixture — is **−643 … −601**. A "run" is
/// a group of glyph rows merged across gaps of two rows or fewer; counting rows
/// strictly adjacent gives a higher figure for the same ink.
///
/// | right margin | ink px admitted | row runs | runs inside the title's band |
/// |---|---|---|---|
/// | 40 | 1250 | 14 | −652 … −641 |
/// | 20 | 622 | 12 | −643 … −641 |
/// | 16 | 456 | 12 | −643 … −641 |
///
/// The band matters because a run overlapping the title's rows can be grouped
/// into the title LINE, and `rooms::match_room_name` rejects a run-together read
/// by `temple::rooms::RATIO_MAX` (1.45) — a short name like `Chasm` has the
/// least room for an appended fragment. 40 was the worst case for that: its run
/// starts 9 rows higher (−652 against −643), so it overlaps the band by more
/// than the 3-row fragment the other two admit.
///
/// **20, not 16.** The floor is not the ink, it is the Hayoxi block, whose own
/// right edge is at +681: a −4% anchor puts the crop's right edge at
/// `(695 + m) × 0.96`, so `m = 16` retains that text by **1.6 ref px** and
/// `m = 20` by **5.4**. 16 is inside the rounding of the thing it has to
/// protect. 20 halves 40's admitted ink, keeps the title-band overlap to the
/// same 3-row fragment 16 does, and still clears the recorded anchor band.
pub const PANEL_RIGHT_MARGIN_REF: f32 = 20.0;

/// Temple scale ÷ shared `ui_scale` — the coefficient between this module's
/// scale unit and [`crate::ssot::ScreenSlice`]'s.
///
/// # Exactly what was measured, and what was not
///
/// **Numerator, measured** (2026-09-03, laptop debug dump
/// `temple-debug/1788438639673`): a 1920x1080 capture anchors at temple scale
/// **1.000**, NCC 0.99999.
///
/// **Denominator, NOMINAL**: `1080 / 1200 = 0.90`, the shared unit's own
/// DEFINITION on a 1080 px screen, giving `k = 1.000 / 0.90 = 1.1111`.
///
/// # Why it is still the nominal denominator after POE-234 WI-2
///
/// WI-2 was to recompute this against a merc-MEASURED `ui_scale` at 1920x1080
/// if the repo held one. It does: `tests/fixtures/merc-recruit-pc-1080p.png` is
/// a 1920x1080 capture whose gold-frame fit lands at **0.8985**
/// (`mercenary::cellfit`'s `GridPoint::beats` and `fit_cell_size` both name that
/// number; the fixture's own test asserts it to ±0.01 of 0.90). That would give
/// `k = 1.000 / 0.8985 = 1.1130`, **0.17% above** the nominal value.
///
/// It is not adopted, because that 0.17% is inside the measurement's own
/// quantisation and so is not evidence: the fit searches pitch on a grid, and
/// the PC fixture's two best grid points are one `P_STEP` apart at pitch 43.729
/// and 43.879 — 0.8985 and 0.9016, i.e. **0.0031 of `ui_scale`, or 0.34% of
/// k**, twice the correction it would make. Both numbers are recorded here so
/// the next reading can be compared against both. Two further caveats stand:
/// the two halves come from DIFFERENT machines (the temple's from the laptop,
/// the fit from the PC fixture), which composes only if both ran the game's
/// default UI-scale slider, and the whole coefficient rests on both units being
/// linear in the capture height, which is what each is documented to be.
///
/// **Good to about 1%, and no better.** `crate::ssot::accepts` tolerates 0.01
/// of `ui_scale` drift, which on a 0.90 denominator is that same ~1%, and it is
/// why a `TempleAnchor` reading gets the band-limited acceptance rule rather
/// than `MercFrame`'s unconditional one. One per cent is one `temple::anchor::SCALE_STEP` at
/// scale 1.0, and a hint one step off the truth still anchors well clear of
/// `temple::anchor::NCC_FLOOR` (measured on `board-ref-1374.png`: the peak 1.00 scores 0.9936
/// and its neighbour 0.99 scores 0.9603), which is what makes converting
/// through it cheaper than searching.
///
/// `temple::run` converts in both directions through `temple::anchor::scale_for_ui_scale` and
/// `temple::anchor::ui_scale_for_scale`, so the arithmetic has one home each way.
pub const TEMPLE_SCALE_PER_UI_SCALE: f32 = 1.1111;

/// The side panel's region, given the Entrance centre and the anchor's scale.
///
/// `[x, y, w, h]`. Keyed on the anchor, like `temple::run::remaining_rect` and
/// `temple::run::diamond_rect` and for the reason [`PANEL_BOX_REF`] measures: the game
/// draws this panel against the layout. A crop that still misses — the panel is
/// only partly captured — degrades to "the panel's text is not read" rather than
/// to a wrong answer: `temple::run::crop_clipped` hands back the readable part,
/// `temple::panel::read_panel` returns an unread title and whichever offers survived,
/// and the advisor warns rather than inventing one.
pub fn panel_rect(origin: (i32, i32), scale: f32) -> [i32; 4] {
    let [left, top, right, bottom] = PANEL_BOX_REF;
    let x0 = origin.0 + ((left - PANEL_MARGIN_REF) * scale).round() as i32;
    let y0 = origin.1 + ((top - PANEL_MARGIN_REF) * scale).round() as i32;
    let x1 = origin.0 + ((right + PANEL_RIGHT_MARGIN_REF) * scale).round() as i32;
    let y1 = origin.1 + ((bottom + PANEL_MARGIN_REF) * scale).round() as i32;
    [x0, y0, x1 - x0, y1 - y0]
}

/// Which geometry the readers use. Auto keeps the placement contract; Manual
/// replaces it with the stored set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OcrRegionsMode {
    #[default]
    Auto,
    Manual,
}

/// The `ocr_regions` settings value. Switching to Auto keeps `manual`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct OcrRegionsSetting {
    #[serde(default)]
    pub mode: OcrRegionsMode,
    #[serde(default)]
    pub manual: Option<ManualRegions>,
}

/// The game monitor a Manual set was seeded or last saved on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenRef {
    pub width: u32,
    pub height: u32,
    pub origin: [i32; 2],
    pub at_ms: u64,
}

/// The Manual set: five capture-relative `[x, y, w, h]` rects, each of which
/// may be unset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManualRegions {
    pub set_on: ScreenRef,
    pub edited: bool,
    #[serde(default)]
    pub lab_gem: Option<[i32; 4]>,
    #[serde(default)]
    pub lab_font: Option<[i32; 4]>,
    #[serde(default)]
    pub lab_gem_slots: Option<[i32; 4]>,
    #[serde(default)]
    pub temple_panel: Option<[i32; 4]>,
    #[serde(default)]
    pub merc_panel: Option<[i32; 4]>,
}

/// One Manual region, as `Resolved::unread` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionKey {
    LabGem,
    LabFont,
    LabGemSlots,
    TemplePanel,
    MercPanel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RegionReadState {
    Read,
    Clipped,
    PastEdge,
    Unset,
}

/// What the readers use: the placements, the per-module origin/panel and
/// ui_scale, and the Lab regions that must not be read.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub placements: Placements,
    pub temple: Option<((i32, i32), f32)>,
    pub merc: Option<([i32; 4], f32)>,
    pub manual: bool,
    pub unread: Vec<RegionKey>,
    pub region_states: BTreeMap<RegionKey, RegionReadState>,
}

/// Resolve every region for `screen` under `setting`, clipped against the
/// capture dimensions when they are known.
pub fn resolve(screen: Option<&ScreenSlice>, setting: &OcrRegionsSetting, capture: Option<(u32, u32)>) -> Resolved {
    let manual = setting.mode == OcrRegionsMode::Manual;
    let mut resolved = Resolved {
        placements: placements_for(screen),
        temple: None,
        merc: None,
        manual,
        unread: Vec::new(),
        region_states: BTreeMap::new(),
    };
    if !manual {
        resolved.temple = resolved.placements.temple.zip(screen)
            .map(|(placed, screen)| (placed.entrance_origin, screen.ui_scale));
        resolved.merc = resolved.placements.merc.zip(screen)
            .map(|(placed, screen)| (placed.panel, screen.ui_scale));
        return resolved;
    }
    resolved.placements.temple = None;
    resolved.placements.merc = None;
    let regions = setting.manual;
    for (key, rect, target) in [
        (RegionKey::LabGem, regions.and_then(|r| r.lab_gem), &mut resolved.placements.lab.gem),
        (RegionKey::LabFont, regions.and_then(|r| r.lab_font), &mut resolved.placements.lab.font),
    ] {
        let (state, crop) = classify(rect, capture, false);
        resolved.region_states.insert(key, state);
        if let Some(rect) = crop.or(rect) { *target = rect; }
    }
    if let Some(strip) = regions.and_then(|r| r.lab_gem_slots).filter(|r| r[2] > 0 && r[3] > 0) {
        let scale = strip[2] as f32 / (2 * crate::GEM_SLOT_PITCH_REF + crate::GEM_SLOT_ICON_REF) as f32;
        let half_icon = super::scale_value(crate::GEM_SLOT_ICON_REF, scale) / 2;
        resolved.placements.lab.gem_slots = super::lab::gem_slot_rects_at(
            (strip[0].saturating_add(half_icon), strip[1].saturating_add(half_icon)),
            crate::GEM_SLOT_PITCH_REF,
            scale,
        );
    }
    let (mut slots_state, _) = classify(regions.and_then(|r| r.lab_gem_slots), capture, true);
    if slots_state == RegionReadState::Read
        && resolved.placements.lab.gem_slots.iter().any(|&slot| classify(Some(slot), capture, true).0 != RegionReadState::Read)
    {
        slots_state = RegionReadState::PastEdge;
    }
    resolved.region_states.insert(RegionKey::LabGemSlots, slots_state);
    for (key, rect) in [
        (RegionKey::TemplePanel, regions.and_then(|r| r.temple_panel)),
        (RegionKey::MercPanel, regions.and_then(|r| r.merc_panel)),
    ] {
        resolved.region_states.insert(key, classify(rect, capture, true).0);
    }
    resolved.unread = resolved.region_states.iter()
        .filter_map(|(&key, &state)| matches!(state, RegionReadState::PastEdge | RegionReadState::Unset).then_some(key))
        .collect();
    if let Some(panel) = regions.and_then(|r| r.temple_panel) {
        let (origin, scale) = origin_scale_for_panel(panel);
        resolved.placements.temple = Some(super::TemplePlacements { entrance_origin: origin });
        if !resolved.unread.contains(&RegionKey::TemplePanel) {
            resolved.temple = Some((origin, scale));
        }
    }
    if let Some(panel) = regions.and_then(|r| r.merc_panel) {
        resolved.placements.merc = Some(super::MercPlacements { panel });
        if !resolved.unread.contains(&RegionKey::MercPanel) {
            resolved.merc = Some((panel, panel[2] as f32 / super::MERC_PANEL_REF[2] as f32));
        }
    }
    resolved
}

fn clip(rect: [i32; 4], capture: Option<(u32, u32)>) -> Option<[i32; 4]> {
    if rect[2] <= 0 || rect[3] <= 0 {
        return None;
    }
    let Some((width, height)) = capture else { return Some(rect) };
    let [x, y, w, h] = rect.map(i64::from);
    let left = x.max(0);
    let top = y.max(0);
    let right = (x + w).min(i64::from(width));
    let bottom = (y + h).min(i64::from(height));
    (right > left && bottom > top).then_some([left as i32, top as i32, (right - left) as i32, (bottom - top) as i32])
}

fn classify(rect: Option<[i32; 4]>, capture: Option<(u32, u32)>, scale_bearing: bool) -> (RegionReadState, Option<[i32; 4]>) {
    let Some(rect) = rect else { return (RegionReadState::Unset, None) };
    match clip(rect, capture) {
        Some(crop) if crop == rect => (RegionReadState::Read, Some(crop)),
        Some(crop) if !scale_bearing => (RegionReadState::Clipped, Some(crop)),
        _ => (RegionReadState::PastEdge, None),
    }
}

pub fn current(state: &crate::AppState, capture: Option<(u32, u32)>) -> Resolved {
    let screen = *state.screen.lock().unwrap_or_else(|e| e.into_inner());
    let setting = *state.ocr_regions.lock().unwrap_or_else(|e| e.into_inner());
    let resolved = resolve(screen.as_ref(), &setting, capture);
    log_edges(&setting, capture, &resolved);
    resolved
}

pub(super) fn log_edges(setting: &OcrRegionsSetting, capture: Option<(u32, u32)>, resolved: &Resolved) {
    use std::collections::HashSet;
    use std::sync::{LazyLock, Mutex};
    static SAID: LazyLock<Mutex<HashSet<(RegionKey, [i32; 4], (u32, u32))>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
    let (Some(regions), Some(capture)) = (setting.manual, capture) else { return };
    if !resolved.manual { return; }
    for (key, rect, actual) in [
        (RegionKey::LabGem, regions.lab_gem, Some(resolved.placements.lab.gem)),
        (RegionKey::LabFont, regions.lab_font, Some(resolved.placements.lab.font)),
        (RegionKey::LabGemSlots, regions.lab_gem_slots, regions.lab_gem_slots),
        (RegionKey::TemplePanel, regions.temple_panel, regions.temple_panel),
        (RegionKey::MercPanel, regions.merc_panel, regions.merc_panel),
    ] {
        let Some(rect) = rect else { continue };
        let unread = resolved.unread.contains(&key);
        if (unread || actual != Some(rect)) && SAID.lock().unwrap_or_else(|e| e.into_inner()).insert((key, rect, capture)) {
            log::warn!("Manual OCR region {key:?} {rect:?} on {capture:?}: {}", if unread { "not read" } else { "cut at the screen edge" });
        }
    }
}

/// The snapshot's `ocrRegions` projection.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrRegionsSnapshot {
    pub mode: OcrRegionsMode,
    pub manual: Option<ManualRegionsView>,
    pub auto: Option<Placements>,
    pub screen_warning: Option<ScreenWarning>,
}

/// The camelCase snapshot mirror of [`ManualRegions`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualRegionsView {
    pub set_on: ScreenRef,
    pub edited: bool,
    pub lab_gem: Option<[i32; 4]>,
    pub lab_font: Option<[i32; 4]>,
    pub lab_gem_slots: Option<[i32; 4]>,
    pub temple_panel: Option<[i32; 4]>,
    pub merc_panel: Option<[i32; 4]>,
}

/// The Manual set was placed on a different game monitor than the live one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenWarning {
    pub set_on: ScreenRef,
    pub live: ScreenRef,
}

pub fn screen_warning(set_on: ScreenRef, live: ScreenRef) -> Option<ScreenWarning> {
    (!super::same_capture(
        (set_on.width, set_on.height), (set_on.origin[0], set_on.origin[1]),
        (live.width, live.height), (live.origin[0], live.origin[1]),
    )).then_some(ScreenWarning { set_on, live })
}

pub fn recalibrate_allowed(state: &crate::AppState) -> Result<(), String> {
    if state.ocr_regions.lock().unwrap_or_else(|e| e.into_inner()).mode == OcrRegionsMode::Manual {
        Err("Recalibrate refused: OCR regions are Manual".to_string())
    } else {
        Ok(())
    }
}

pub fn set_mode_at(
    state: &crate::AppState,
    path: Option<&std::path::Path>,
    mode: OcrRegionsMode,
    live: Option<ScreenRef>,
) -> Result<(), String> {
    if mode == OcrRegionsMode::Manual && live.is_none() {
        return Err(UNKNOWN_MONITOR.to_string());
    }
    update_setting_at(state, path, |mut setting| {
        if mode == OcrRegionsMode::Manual && setting.manual.is_none() {
            let screen = *state.screen.lock().unwrap_or_else(|e| e.into_inner());
            let auto = screen.as_ref().map(|screen| placements_for(Some(screen)));
            setting.manual = Some(ManualRegions {
                set_on: live.expect("Manual requires a live monitor"),
                edited: false,
                lab_gem: auto.map(|p| p.lab.gem),
                lab_font: auto.map(|p| p.lab.font),
                lab_gem_slots: auto.map(|p| {
                    let slots = p.lab.gem_slots;
                    let left = slots.iter().map(|r| r[0]).min().unwrap();
                    let right = slots.iter().map(|r| r[0] + r[2]).max().unwrap();
                    [left, slots[0][1], right - left, slots[0][3]]
                }),
                temple_panel: auto.and_then(|p| p.temple).zip(screen)
                    .map(|(p, s)| panel_rect(p.entrance_origin, s.ui_scale * TEMPLE_SCALE_PER_UI_SCALE)),
                merc_panel: auto.and_then(|p| p.merc).map(|p| p.panel),
            });
        }
        setting.mode = mode;
        setting
    })
}

pub fn save_manual_regions_at(
    state: &crate::AppState,
    path: Option<&std::path::Path>,
    mut regions: ManualRegions,
    live: Option<ScreenRef>,
) -> Result<(), String> {
    let live = live.ok_or_else(|| UNKNOWN_MONITOR.to_string())?;
    for rect in [regions.lab_gem, regions.lab_font, regions.lab_gem_slots, regions.temple_panel, regions.merc_panel].into_iter().flatten() {
        if classify(Some(rect), Some((live.width, live.height)), true).0 != RegionReadState::Read {
            return Err("OCR region must have positive dimensions and fit inside the game monitor".to_string());
        }
    }
    regions.set_on = live;
    regions.edited = true;
    update_setting_at(state, path, |setting| OcrRegionsSetting { manual: Some(regions), ..setting })
}

const UNKNOWN_MONITOR: &str = "game monitor unknown: focus the game once, then save";

fn update_setting_at(
    state: &crate::AppState,
    path: Option<&std::path::Path>,
    change: impl FnOnce(OcrRegionsSetting) -> OcrRegionsSetting,
) -> Result<(), String> {
    let path = path.ok_or_else(|| "settings path unavailable".to_string())?;
    let candidate = std::cell::Cell::new(None);
    crate::settings::update_at_committing(path, |loaded| loaded.unwrap_or_default(), |existing| {
        let mut target = crate::settings::from_state(state);
        target.ocr_regions = change(target.ocr_regions);
        candidate.set(Some(target.ocr_regions));
        crate::settings::merge_stored_rows(existing, &mut target);
        target
    }, || {
        *state.ocr_regions.lock().unwrap_or_else(|e| e.into_inner()) = candidate.get().expect("saved candidate");
    }).map_err(|error| error.to_string())
}

pub fn live_screen(state: &crate::AppState) -> Option<ScreenRef> {
    state.game_monitor.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|monitor| ScreenRef {
        width: monitor.width,
        height: monitor.height,
        origin: [monitor.x, monitor.y],
        at_ms: crate::clock::unix_ms(),
    })
}

pub(super) fn snapshot(setting: &OcrRegionsSetting, screen: Option<&ScreenSlice>) -> OcrRegionsSnapshot {
    OcrRegionsSnapshot {
        mode: setting.mode,
        manual: setting.manual.map(|r| ManualRegionsView {
            set_on: r.set_on, edited: r.edited, lab_gem: r.lab_gem, lab_font: r.lab_font,
            lab_gem_slots: r.lab_gem_slots, temple_panel: r.temple_panel, merc_panel: r.merc_panel,
        }),
        auto: screen.map(|s| placements_for(Some(s))),
        screen_warning: None,
    }
}

#[tauri::command]
pub fn set_ocr_regions_mode(app: tauri::AppHandle, mode: OcrRegionsMode) -> Result<(), String> {
    let state = app.state::<crate::AppState>();
    set_mode_at(&state, crate::settings::settings_path_pub(&app).as_deref(), mode, live_screen(&state))?;
    super::emit_ssot(&app);
    Ok(())
}

#[tauri::command]
pub fn set_manual_ocr_regions(app: tauri::AppHandle, regions: ManualRegions) -> Result<(), String> {
    let state = app.state::<crate::AppState>();
    save_manual_regions_at(&state, crate::settings::settings_path_pub(&app).as_deref(), regions, live_screen(&state))?;
    super::emit_ssot(&app);
    Ok(())
}

/// The Entrance origin and shared ui_scale a temple panel rect implies — the
/// inverse of the temple panel rule.
pub fn origin_scale_for_panel(rect: [i32; 4]) -> ((i32, i32), f32) {
    let [left, top, right, _] = PANEL_BOX_REF;
    let scale = rect[2] as f32 / (right - left + PANEL_MARGIN_REF + PANEL_RIGHT_MARGIN_REF);
    let origin = (
        rect[0].saturating_sub(((left - PANEL_MARGIN_REF) * scale).round() as i32),
        rect[1].saturating_sub(((top - PANEL_MARGIN_REF) * scale).round() as i32),
    );
    (origin, scale / TEMPLE_SCALE_PER_UI_SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssot::screen_from_geometry;

    const SET_ON_3440: ScreenRef = ScreenRef { width: 3440, height: 1440, origin: [0, 0], at_ms: 1_732_400_000_000 };

    fn manual(regions: ManualRegions) -> OcrRegionsSetting {
        OcrRegionsSetting { mode: OcrRegionsMode::Manual, manual: Some(regions) }
    }

    fn unset_on(set_on: ScreenRef) -> ManualRegions {
        ManualRegions {
            set_on,
            edited: true,
            lab_gem: None,
            lab_font: None,
            lab_gem_slots: None,
            temple_panel: None,
            merc_panel: None,
        }
    }

    /// The POE-472 whole-capture fallback slice: a 2560x1440 capture whose
    /// client is the whole monitor, so Auto scales it at ui 1.2.
    fn whole_capture_2560x1440() -> ScreenSlice {
        screen_from_geometry(2560, 1440, 1, (0, 0), [0, 0, 2560, 1440], 0)
    }

    /// The live game monitor the command cores are handed, stamped later than
    /// [`SET_ON_3440`] so a test can tell a fresh stamp from a kept one.
    const LIVE_3440: ScreenRef = ScreenRef { width: 3440, height: 1440, origin: [0, 0], at_ms: 1_732_500_000_000 };
    const LIVE_2560: ScreenRef = ScreenRef { width: 2560, height: 1440, origin: [0, 0], at_ms: 1_732_600_000_000 };

    fn game_monitor(live: ScreenRef) -> crate::capture::GameMonitor {
        crate::capture::GameMonitor { id: 1, x: live.origin[0], y: live.origin[1], width: live.width, height: live.height }
    }

    /// A test `AppState` on `live`'s game monitor, holding `setting`.
    fn state_on(live: Option<ScreenRef>, screen: Option<ScreenSlice>, setting: OcrRegionsSetting) -> crate::AppState {
        let state = crate::settings::tests::test_app_state();
        *state.game_monitor.lock().unwrap() = live.map(game_monitor);
        *state.screen.lock().unwrap() = screen;
        *state.ocr_regions.lock().unwrap() = setting;
        state
    }

    /// A fresh settings path under the temp dir; the caller removes the dir.
    fn settings_path(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("poe_test_regions_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        (dir.join("settings.json"), dir)
    }

    /// Write `state` to `path` the way every whole-file save does.
    fn persist(state: &crate::AppState, path: &std::path::Path) {
        crate::settings::update_at(path, |loaded| loaded.unwrap_or_default(), |_| crate::settings::from_state(state))
            .unwrap();
    }

    fn slice_3440x1440() -> ScreenSlice {
        screen_from_geometry(3440, 1440, 1, (0, 0), [0, 0, 3440, 1440], 0)
    }

    fn snapshot_ocr_regions(state: &crate::AppState) -> serde_json::Value {
        serde_json::to_value(crate::ssot::build_snapshot(state)).unwrap()["ocrRegions"].clone()
    }

    fn screen_json(screen: &serde_json::Value) -> (serde_json::Value, serde_json::Value, serde_json::Value) {
        (screen["width"].clone(), screen["height"].clone(), screen["origin"].clone())
    }

    mod commands {
        use super::*;

        mod keep_happy {
            use super::*;

            enum Expect {
                /// The snapshot after the switch: mode, `manual.labFont`, the
                /// `setOn` monitor, `edited`, and whether `auto` is projected.
                Snapshot { lab_font: serde_json::Value, five_null: bool, auto_null: bool },
                /// The setting the switch leaves in `AppState`.
                State(OcrRegionsSetting),
            }

            /// WI-3 example 1 and the 2026-11-25 seeding rulings: the first
            /// Manual switch seeds from Auto (all five null with no Auto slice),
            /// a stored set comes back with its own stamp, and Auto keeps it.
            #[test]
            fn set_mode_at_leaves_the_set_each_switch_names() {
                let stored = ManualRegions { lab_font: Some([1820, 293, 888, 908]), ..unset_on(SET_ON_3440) };
                let cases = [
                    (
                        "Auto on 3440x1440, no stored set: Manual seeds the Auto Font",
                        Some(slice_3440x1440()),
                        OcrRegionsSetting::default(),
                        OcrRegionsMode::Manual,
                        Expect::Snapshot { lab_font: serde_json::json!([837, 293, 888, 908]), five_null: false, auto_null: false },
                    ),
                    (
                        "known monitor, no Auto slice: Manual seeds all five null",
                        None,
                        OcrRegionsSetting::default(),
                        OcrRegionsMode::Manual,
                        Expect::Snapshot { lab_font: serde_json::Value::Null, five_null: true, auto_null: true },
                    ),
                    (
                        "Auto with a stored set: Manual restores it with its original stamp",
                        Some(slice_3440x1440()),
                        OcrRegionsSetting { mode: OcrRegionsMode::Auto, manual: Some(stored) },
                        OcrRegionsMode::Manual,
                        Expect::State(OcrRegionsSetting { mode: OcrRegionsMode::Manual, manual: Some(stored) }),
                    ),
                    (
                        "Manual with a stored set: Auto keeps the set",
                        Some(slice_3440x1440()),
                        manual(stored),
                        OcrRegionsMode::Auto,
                        Expect::State(OcrRegionsSetting { mode: OcrRegionsMode::Auto, manual: Some(stored) }),
                    ),
                ];

                for (case, screen, setting, mode, expect) in cases {
                    let (path, dir) = settings_path("set_mode");
                    let state = state_on(Some(LIVE_3440), screen, setting);

                    let result = set_mode_at(&state, Some(&path), mode, Some(LIVE_3440));
                    let regions = snapshot_ocr_regions(&state);
                    let after = *state.ocr_regions.lock().unwrap();
                    let _ = std::fs::remove_dir_all(&dir);

                    assert_eq!(result, Ok(()), "{case}");
                    match expect {
                        Expect::Snapshot { lab_font, five_null, auto_null } => {
                            assert_eq!(regions["mode"], "manual", "{case}: {regions}");
                            let set = &regions["manual"];
                            assert_eq!(set["labFont"], lab_font, "{case}: {regions}");
                            assert_eq!(
                                screen_json(&set["setOn"]),
                                (serde_json::json!(3440), serde_json::json!(1440), serde_json::json!([0, 0])),
                                "{case}: {regions}"
                            );
                            assert_eq!(set["edited"], false, "{case}: a seed is not an edit: {regions}");
                            assert_eq!(regions["screenWarning"], serde_json::Value::Null, "{case}: {regions}");
                            assert_eq!(regions["auto"].is_null(), auto_null, "{case}: {regions}");
                            if five_null {
                                for key in ["labGem", "labFont", "labGemSlots", "templePanel", "mercPanel"] {
                                    assert_eq!(set[key], serde_json::Value::Null, "{case}: {key} in {regions}");
                                }
                            }
                        }
                        Expect::State(want) => assert_eq!(after, want, "{case}"),
                    }
                }
            }

            /// WI-3 example 2 (AC-3): Recalibrate is refused in Manual and
            /// allowed again once the mode is back to Auto.
            #[test]
            fn recalibrate_is_refused_only_while_ocr_regions_are_manual() {
                let stored = ManualRegions { lab_font: Some([1820, 293, 888, 908]), ..unset_on(SET_ON_3440) };
                let cases = [
                    ("Manual", manual(stored), Err("Recalibrate refused: OCR regions are Manual".to_string())),
                    ("Auto with a stored set", OcrRegionsSetting { mode: OcrRegionsMode::Auto, manual: Some(stored) }, Ok(())),
                ];

                for (case, setting, want) in cases {
                    let state = state_on(Some(LIVE_3440), Some(slice_3440x1440()), setting);

                    assert_eq!(recalibrate_allowed(&state), want, "{case}");
                }
            }

            /// WI-3 example 3 (AC-4): a set saved on 3440x1440 warns on a
            /// 2560x1440 game monitor and stops warning back on 3440x1440.
            #[test]
            fn the_snapshot_screen_warning_follows_the_live_game_monitor() {
                let (path, dir) = settings_path("screen_warning");
                // Stamped on 2560x1440 before this save, so only the save's
                // fresh stamp can make the 2560x1440 monitor the mismatch.
                let earlier = ScreenRef { at_ms: 1_732_300_000_000, ..LIVE_2560 };
                let state = state_on(Some(LIVE_3440), Some(slice_3440x1440()), manual(unset_on(earlier)));
                let placed = ManualRegions { lab_font: Some([1820, 293, 888, 908]), ..unset_on(earlier) };

                let saved = save_manual_regions_at(&state, Some(&path), placed, Some(LIVE_3440));
                *state.game_monitor.lock().unwrap() = Some(game_monitor(LIVE_2560));
                let elsewhere = snapshot_ocr_regions(&state)["screenWarning"].clone();
                *state.game_monitor.lock().unwrap() = Some(game_monitor(LIVE_3440));
                let back = snapshot_ocr_regions(&state)["screenWarning"].clone();
                let _ = std::fs::remove_dir_all(&dir);

                assert_eq!(saved, Ok(()));
                assert_eq!(
                    (screen_json(&elsewhere["setOn"]), screen_json(&elsewhere["live"])),
                    (
                        (serde_json::json!(3440), serde_json::json!(1440), serde_json::json!([0, 0])),
                        (serde_json::json!(2560), serde_json::json!(1440), serde_json::json!([0, 0])),
                    ),
                    "warning on 2560x1440: {elsewhere}"
                );
                assert_eq!(back, serde_json::Value::Null, "back on 3440x1440");
            }
        }

        mod keep_critical {
            use super::*;

            enum Call {
                Save(ManualRegions, Option<ScreenRef>),
                Mode(OcrRegionsMode, Option<ScreenRef>),
            }

            const UNKNOWN_MONITOR: &str = "game monitor unknown: focus the game once, then save";

            /// Silent consequence: a saved crop past the image (or a set
            /// stamped against no monitor) reads the wrong pixels on every
            /// press with nothing on screen to say so; a refused call must
            /// leave both the running set and the settings file as they were.
            #[test]
            fn a_refused_save_or_switch_leaves_the_state_and_the_file_byte_identical() {
                let stored = ManualRegions { lab_font: Some([1820, 293, 888, 908]), ..unset_on(SET_ON_3440) };
                let cases = [
                    (
                        "Font right edge at 3888 on a 3440-wide capture",
                        manual(stored),
                        Call::Save(ManualRegions { lab_font: Some([3000, 293, 888, 908]), ..stored }, Some(LIVE_3440)),
                        None,
                    ),
                    (
                        "a zero-width gem tooltip",
                        manual(stored),
                        Call::Save(ManualRegions { lab_gem: Some([0, 0, 0, 64]), ..stored }, Some(LIVE_3440)),
                        None,
                    ),
                    (
                        "a save with the game monitor unknown",
                        manual(stored),
                        Call::Save(stored, None),
                        Some(UNKNOWN_MONITOR),
                    ),
                    (
                        "a first Manual switch with the game monitor unknown",
                        OcrRegionsSetting::default(),
                        Call::Mode(OcrRegionsMode::Manual, None),
                        Some(UNKNOWN_MONITOR),
                    ),
                ];

                for (case, setting, call, message) in cases {
                    let (path, dir) = settings_path("refused");
                    let live = match call {
                        Call::Save(_, live) | Call::Mode(_, live) => live,
                    };
                    let state = state_on(live, Some(slice_3440x1440()), setting);
                    persist(&state, &path);
                    let before = std::fs::read(&path).unwrap();

                    let result = match call {
                        Call::Save(regions, live) => save_manual_regions_at(&state, Some(&path), regions, live),
                        Call::Mode(mode, live) => set_mode_at(&state, Some(&path), mode, live),
                    };
                    let after = std::fs::read(&path).unwrap();
                    let _ = std::fs::remove_dir_all(&dir);

                    match message {
                        Some(message) => assert_eq!(result, Err(message.to_string()), "{case}"),
                        None => assert!(result.is_err(), "{case}: {result:?}"),
                    }
                    assert_eq!(*state.ocr_regions.lock().unwrap(), setting, "{case}: AppState set");
                    assert!(after == before, "{case}: the settings file changed");
                }
            }

            /// Silent consequence: a save the file refused but `AppState`
            /// took makes the app read with a set the next launch has lost,
            /// and the player is told nothing.
            #[test]
            fn an_unreadable_settings_file_refuses_the_save_with_the_old_set_still_in_state() {
                let (path, dir) = settings_path("unreadable");
                let stored = ManualRegions { lab_font: Some([1820, 293, 888, 908]), ..unset_on(SET_ON_3440) };
                let state = state_on(Some(LIVE_3440), Some(slice_3440x1440()), manual(stored));
                std::fs::write(&path, "{ not json").unwrap();
                let moved = ManualRegions { lab_font: Some([1700, 293, 888, 908]), ..stored };

                let result = save_manual_regions_at(&state, Some(&path), moved, Some(LIVE_3440));
                let _ = std::fs::remove_dir_all(&dir);

                assert!(result.is_err(), "{result:?}");
                assert_eq!(*state.ocr_regions.lock().unwrap(), manual(stored));
            }
        }
    }

    mod keep_happy {
        use super::*;

        // keep:happy — AC-1: the controller layout's centre-right Font panel.
        #[test]
        fn a_manual_font_on_a_3440x1440_slice_resolves_to_the_stored_rect_not_the_auto_seed() {
            let slice = screen_from_geometry(3440, 1440, 1, (0, 0), [0, 0, 3440, 1440], 0);
            let setting = manual(ManualRegions { lab_font: Some([1820, 293, 888, 908]), ..unset_on(SET_ON_3440) });

            let resolved = resolve(Some(&slice), &setting, Some((3440, 1440)));

            assert_eq!(resolved.placements.lab.font, [1820, 293, 888, 908], "Manual Font");
            let auto = resolve(Some(&slice), &OcrRegionsSetting::default(), Some((3440, 1440)));
            assert_eq!(auto.placements.lab.font, [837, 293, 888, 908], "Auto Font on the same slice");
        }

        // keep:happy — AC-2: the ultrawide slot strip splits into the Auto
        // slots of the true 2560x1080 client.
        #[test]
        fn a_manual_slot_strip_splits_into_the_three_true_client_slots() {
            let set_on = ScreenRef { width: 2560, height: 1440, origin: [0, 0], at_ms: 1_732_400_000_000 };
            let setting = manual(ManualRegions { lab_gem_slots: Some([826, 373, 246, 48]), ..unset_on(set_on) });

            let slots = resolve(Some(&whole_capture_2560x1440()), &setting, Some((2560, 1440))).placements.lab.gem_slots;

            let expected = [[826, 373, 48, 48], [925, 373, 48, 48], [1024, 373, 48, 48]];
            for (slot, want) in slots.iter().zip(expected.iter()) {
                for i in 0..4 {
                    assert!((slot[i] - want[i]).abs() <= 1, "slots {slots:?}, expected {expected:?} within 1 px");
                }
            }
        }
    }

    mod keep_critical {
        use super::*;

        enum Expect {
            LabFontClippedTo([i32; 4]),
            TempleUnread,
            Unread(RegionKey),
            Read(RegionKey),
        }

        /// The serialized `region_states` of a set where only `key` is set
        /// (`state`) and the other four regions are null.
        fn states_with(key: &str, state: &str) -> serde_json::Value {
            let mut states = serde_json::json!({
                "lab_gem": "unset",
                "lab_font": "unset",
                "lab_gem_slots": "unset",
                "temple_panel": "unset",
                "merc_panel": "unset",
            });
            states[key] = serde_json::json!(state);
            states
        }

        /// Silent consequence: a crop past the image edge, or a scale derived
        /// from a strip clipped at the edge, reads the wrong pixels on every
        /// press with nothing on screen to say so. The read state is what the
        /// Settings rows label: a region labelled read but skipped, or skipped
        /// but labelled read, misleads the player just as silently.
        #[test]
        fn a_set_saved_on_3440x1440_clips_free_crops_and_skips_scale_bearing_rects_on_a_2560x1440_capture() {
            let slice = whole_capture_2560x1440();
            let cases = [
                (
                    "Font crossing x = 2560 is cut at the edge",
                    ManualRegions { lab_font: Some([1820, 293, 888, 908]), ..unset_on(SET_ON_3440) },
                    Expect::LabFontClippedTo([1820, 293, 740, 908]),
                    states_with("lab_font", "clipped"),
                ),
                (
                    "a temple panel wholly past x = 2560 is not read",
                    ManualRegions { temple_panel: Some([2600, 300, 544, 454]), ..unset_on(SET_ON_3440) },
                    Expect::TempleUnread,
                    states_with("temple_panel", "past-edge"),
                ),
                (
                    "a slot strip crossing x = 2560 is not read",
                    ManualRegions { lab_gem_slots: Some([2400, 497, 328, 64]), ..unset_on(SET_ON_3440) },
                    Expect::Unread(RegionKey::LabGemSlots),
                    states_with("lab_gem_slots", "past-edge"),
                ),
                (
                    "a Font wholly past x = 2560 is past the edge, not clipped",
                    ManualRegions { lab_font: Some([2600, 293, 888, 908]), ..unset_on(SET_ON_3440) },
                    Expect::Unread(RegionKey::LabFont),
                    states_with("lab_font", "past-edge"),
                ),
                (
                    "a contained slot strip whose derived slots end at y = 1453 is past the edge",
                    ManualRegions { lab_gem_slots: Some([100, 1400, 273, 30]), ..unset_on(SET_ON_3440) },
                    Expect::Unread(RegionKey::LabGemSlots),
                    states_with("lab_gem_slots", "past-edge"),
                ),
                (
                    "a merc panel inside the capture is read",
                    ManualRegions { merc_panel: Some([1406, 738, 666, 572]), ..unset_on(SET_ON_3440) },
                    Expect::Read(RegionKey::MercPanel),
                    states_with("merc_panel", "read"),
                ),
            ];

            for (case, regions, expect, states) in cases {
                let resolved = resolve(Some(&slice), &manual(regions), Some((2560, 1440)));
                assert_eq!(serde_json::to_value(&resolved.region_states).unwrap(), states, "{case}: region states");
                let unread: std::collections::BTreeSet<String> = resolved.unread.iter()
                    .map(|key| serde_json::to_value(key).unwrap().as_str().unwrap().to_string())
                    .collect();
                let past_edge_or_unset: std::collections::BTreeSet<String> = states.as_object().unwrap().iter()
                    .filter(|(_, state)| *state == "past-edge" || *state == "unset")
                    .map(|(key, _)| key.clone())
                    .collect();
                assert_eq!(unread, past_edge_or_unset, "{case}: unread is exactly the past-edge and unset regions");
                match expect {
                    Expect::LabFontClippedTo(rect) => {
                        assert_eq!(resolved.placements.lab.font, rect, "{case}");
                        assert!(!resolved.unread.contains(&RegionKey::LabFont), "{case}: unread {:?}", resolved.unread);
                    }
                    Expect::TempleUnread => assert_eq!(resolved.temple, None, "{case}"),
                    Expect::Unread(key) => {
                        assert!(resolved.unread.contains(&key), "{case}: unread {:?}", resolved.unread)
                    }
                    Expect::Read(key) => {
                        assert!(!resolved.unread.contains(&key), "{case}: unread {:?}", resolved.unread)
                    }
                }
            }
        }

        /// Silent consequence: a contained strip whose width-derived square
        /// slots run past the capture's bottom edge would have each slot crop
        /// read past the image on every press, yielding silently wrong gem
        /// icons. Without a known capture nothing is clipped, so it stays read.
        #[test]
        fn a_contained_slot_strip_whose_derived_slots_end_past_the_capture_is_not_read() {
            let slice = whole_capture_2560x1440();
            let set_on = ScreenRef { width: 2560, height: 1440, origin: [0, 0], at_ms: 1_732_400_000_000 };
            let setting = manual(ManualRegions { lab_gem_slots: Some([100, 1400, 273, 30]), ..unset_on(set_on) });
            let cases = [
                ("2560x1440 capture: slots end at y = 1453 > 1440", Some((2560, 1440)), true),
                ("no capture: resolution stays unclipped", None, false),
            ];

            for (case, capture, want_unread) in cases {
                let resolved = resolve(Some(&slice), &setting, capture);
                assert_eq!(
                    resolved.unread.contains(&RegionKey::LabGemSlots),
                    want_unread,
                    "{case}: unread {:?}, slots {:?}",
                    resolved.unread,
                    resolved.placements.lab.gem_slots,
                );
            }
        }
    }

    mod keep_regression {
        use super::*;

        /// POE-472(b), Operator-reported: on the whole-capture fallback slice
        /// (ui 1.2) Auto places the Font at [397, 293, 888, 908], off the
        /// 2560x1080 client's panel; the Manual Font must be read as stored.
        #[test]
        fn a_manual_font_on_the_whole_capture_fallback_slice_resolves_to_the_stored_rect() {
            let set_on = ScreenRef { width: 2560, height: 1440, origin: [0, 0], at_ms: 1_732_400_000_000 };
            let setting = manual(ManualRegions { lab_font: Some([618, 220, 666, 681]), ..unset_on(set_on) });

            let resolved = resolve(Some(&whole_capture_2560x1440()), &setting, Some((2560, 1440)));

            assert_eq!(resolved.placements.lab.font, [618, 220, 666, 681]);
        }
    }
}
