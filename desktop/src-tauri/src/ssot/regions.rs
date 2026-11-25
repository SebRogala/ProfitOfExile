//! Exclusive Auto/Manual OCR regions (POE-411, ADR-036): the persisted mode,
//! the nullable capture-relative Manual set, and the one resolver every region
//! reader and preview takes its rects from.

use serde::{Deserialize, Serialize};

use super::{placements_for, Placements, ScreenSlice};

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKey {
    LabGem,
    LabFont,
    LabGemSlots,
    TemplePanel,
    MercPanel,
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
}

/// Resolve every region for `screen` under `setting`, clipped against the
/// capture dimensions when they are known.
pub fn resolve(screen: Option<&ScreenSlice>, setting: &OcrRegionsSetting, capture: Option<(u32, u32)>) -> Resolved {
    let _ = (setting, capture);
    Resolved {
        placements: placements_for(screen),
        temple: None,
        merc: None,
        manual: false,
        unread: Vec::new(),
    }
}

/// The Entrance origin and shared ui_scale a temple panel rect implies — the
/// inverse of the temple panel rule.
pub fn origin_scale_for_panel(rect: [i32; 4]) -> ((i32, i32), f32) {
    let _ = rect;
    ((0, 0), 0.0)
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
        }

        /// Silent consequence: a crop past the image edge, or a scale derived
        /// from a strip clipped at the edge, reads the wrong pixels on every
        /// press with nothing on screen to say so.
        #[test]
        fn a_set_saved_on_3440x1440_clips_free_crops_and_skips_scale_bearing_rects_on_a_2560x1440_capture() {
            let slice = whole_capture_2560x1440();
            let cases = [
                (
                    "Font crossing x = 2560 is cut at the edge",
                    ManualRegions { lab_font: Some([1820, 293, 888, 908]), ..unset_on(SET_ON_3440) },
                    Expect::LabFontClippedTo([1820, 293, 740, 908]),
                ),
                (
                    "a temple panel wholly past x = 2560 is not read",
                    ManualRegions { temple_panel: Some([2600, 300, 544, 454]), ..unset_on(SET_ON_3440) },
                    Expect::TempleUnread,
                ),
                (
                    "a slot strip crossing x = 2560 is not read",
                    ManualRegions { lab_gem_slots: Some([2400, 497, 328, 64]), ..unset_on(SET_ON_3440) },
                    Expect::Unread(RegionKey::LabGemSlots),
                ),
            ];

            for (case, regions, expect) in cases {
                let resolved = resolve(Some(&slice), &manual(regions), Some((2560, 1440)));
                match expect {
                    Expect::LabFontClippedTo(rect) => {
                        assert_eq!(resolved.placements.lab.font, rect, "{case}");
                        assert!(!resolved.unread.contains(&RegionKey::LabFont), "{case}: unread {:?}", resolved.unread);
                    }
                    Expect::TempleUnread => assert_eq!(resolved.temple, None, "{case}"),
                    Expect::Unread(key) => {
                        assert!(resolved.unread.contains(&key), "{case}: unread {:?}", resolved.unread)
                    }
                }
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
