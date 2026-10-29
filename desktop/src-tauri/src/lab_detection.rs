//! Run-scoped lab identification (POE-285; facts in docs/GAME-FACTS.md).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LabKind {
    Normal,
    Cruel,
    Merciless,
    Eternal,
    Offering,
    Dedication,
    Gift,
    Tribute,
}

#[derive(Default)]
pub struct LabDetection {
    kind: Option<LabKind>,
    in_lab_area: bool,
    manual_override: bool,
}

impl LabDetection {
    pub fn observe_area(&mut self, level: u32, area: &str) {
        self.in_lab_area = area.contains("Labyrinth");
        if !self.in_lab_area || self.kind.is_some() {
            return;
        }
        self.kind = match level {
            33 => Some(LabKind::Normal),
            55 => Some(LabKind::Cruel),
            68 => Some(LabKind::Merciless),
            75 => Some(LabKind::Eternal),
            83 => Some(LabKind::Offering),
            _ => None,
        };
    }

    pub fn observe_font<'a>(
        &mut self,
        options: impl IntoIterator<Item = &'a str>,
        crafts_remaining: Option<i32>,
    ) {
        if !self.in_lab_area || self.kind != Some(LabKind::Offering) {
            return;
        }
        self.kind = Some(if options.into_iter().any(|option| {
            matches!(
                option,
                "corrupted_transfigured_reroll" | "corrupted_gem_reroll"
            )
        }) {
            LabKind::Dedication
        } else if crafts_remaining.is_some_and(|count| count >= 8) {
            LabKind::Gift
        } else {
            LabKind::Tribute
        });
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn override_mode(&mut self) {
        self.manual_override = true;
    }

    pub fn detected_type(&self) -> Option<&'static str> {
        match self.kind? {
            LabKind::Normal => Some("Normal"),
            LabKind::Cruel => Some("Cruel"),
            LabKind::Merciless => Some("Merciless"),
            LabKind::Eternal => Some("Eternal"),
            LabKind::Offering => None,
            LabKind::Dedication => Some("Dedication"),
            LabKind::Gift => Some("Gift"),
            LabKind::Tribute => Some("Tribute"),
        }
    }

    pub fn automatic_mode(&self) -> Option<&'static str> {
        if self.manual_override {
            return None;
        }
        self.detected_type().map(|kind| {
            if kind == "Dedication" {
                "Dedication"
            } else {
                "Normal"
            }
        })
    }
}

#[cfg(test)]
mod lab_detection_tests {
    use super::*;

    fn offering() -> LabDetection {
        let mut detection = LabDetection::default();
        detection.observe_area(83, "EndGame_Labyrinth_boss_3");
        detection
    }

    macro_rules! level_case {
        ($name:ident, $level:expr, $kind:expr) => {
            #[test]
            fn $name() {
                let mut detection = LabDetection::default();
                detection.observe_area($level, "Labyrinth_boss_1");
                assert_eq!(detection.detected_type(), Some($kind));
            }
        };
    }
    level_case!(level_33_identifies_normal, 33, "Normal");
    level_case!(level_55_identifies_cruel, 55, "Cruel");
    level_case!(level_68_identifies_merciless, 68, "Merciless");
    level_case!(level_75_identifies_eternal, 75, "Eternal");

    #[test]
    fn level_83_waits_for_font() {
        assert_eq!(offering().detected_type(), None);
    }

    #[test]
    fn non_lab_area_does_not_identify_lab() {
        let mut detection = LabDetection::default();
        detection.observe_area(75, "MapWorldsBeach");
        assert_eq!(detection.detected_type(), None);
    }

    #[test]
    fn unknown_level_does_not_identify_lab() {
        let mut detection = LabDetection::default();
        detection.observe_area(84, "Labyrinth_boss_1");
        assert_eq!(detection.detected_type(), None);
    }

    #[test]
    fn corrupted_skill_option_identifies_dedication() {
        let mut detection = offering();
        detection.observe_font(["corrupted_gem_reroll"], Some(2));
        assert_eq!(detection.detected_type(), Some("Dedication"));
    }

    #[test]
    fn corrupted_transfigured_option_takes_precedence_over_count() {
        let mut detection = offering();
        detection.observe_font(["corrupted_transfigured_reroll"], Some(8));
        assert_eq!(detection.detected_type(), Some("Dedication"));
    }

    #[test]
    fn eight_crafts_identifies_gift() {
        let mut detection = offering();
        detection.observe_font(["transform_random"], Some(8));
        assert_eq!(detection.detected_type(), Some("Gift"));
    }

    #[test]
    fn twice_blessed_gift_identifies_gift() {
        let mut detection = offering();
        detection.observe_font(["transform_random"], Some(9));
        assert_eq!(detection.detected_type(), Some("Gift"));
    }

    #[test]
    fn seven_crafts_identifies_tribute() {
        let mut detection = offering();
        detection.observe_font(["transform_random"], Some(7));
        assert_eq!(detection.detected_type(), Some("Tribute"));
    }

    #[test]
    fn missing_count_uses_ticket_tribute_fallback() {
        let mut detection = offering();
        detection.observe_font(["transform_random"], None);
        assert_eq!(detection.detected_type(), Some("Tribute"));
    }

    #[test]
    fn later_font_read_keeps_first_type() {
        let mut detection = offering();
        detection.observe_font(["transform_random"], Some(8));
        detection.observe_font(["corrupted_gem_reroll"], Some(2));
        assert_eq!(detection.detected_type(), Some("Gift"));
    }

    #[test]
    fn room_change_keeps_first_type() {
        let mut detection = offering();
        detection.observe_font(["transform_random"], Some(8));
        detection.observe_area(83, "EndGame_Labyrinth_end");
        detection.observe_font(["transform_random"], Some(2));
        assert_eq!(detection.detected_type(), Some("Gift"));
    }

    #[test]
    fn portal_return_keeps_first_type() {
        let mut detection = offering();
        detection.observe_font(["transform_random"], Some(8));
        detection.observe_area(60, "Hideout");
        detection.observe_area(83, "EndGame_Labyrinth_end");
        detection.observe_font(["transform_random"], Some(2));
        assert_eq!(detection.detected_type(), Some("Gift"));
    }

    #[test]
    fn font_outside_lab_does_not_resolve_offering() {
        let mut detection = offering();
        detection.observe_area(60, "Hideout");
        detection.observe_font(["corrupted_gem_reroll"], Some(2));
        assert_eq!(detection.detected_type(), None);
    }

    #[test]
    fn font_without_area_does_not_infer_offering() {
        let mut detection = LabDetection::default();
        detection.observe_font(["corrupted_gem_reroll"], Some(2));
        assert_eq!(detection.detected_type(), None);
    }

    #[test]
    fn font_does_not_reclassify_standard_lab() {
        let mut detection = LabDetection::default();
        detection.observe_area(75, "Labyrinth_boss_3");
        detection.observe_font(["corrupted_gem_reroll"], Some(8));
        assert_eq!(detection.detected_type(), Some("Eternal"));
    }

    #[test]
    fn reset_allows_next_run_to_detect_different_offering() {
        let mut detection = offering();
        detection.observe_font(["transform_random"], Some(8));
        detection.reset();
        detection.observe_area(83, "EndGame_Labyrinth_boss_3");
        detection.observe_font(["corrupted_gem_reroll"], Some(2));
        assert_eq!(detection.detected_type(), Some("Dedication"));
    }

    #[test]
    fn dedication_selects_corrupted_comparator() {
        let mut detection = offering();
        detection.observe_font(["corrupted_gem_reroll"], Some(2));
        assert_eq!(detection.automatic_mode(), Some("Dedication"));
    }

    #[test]
    fn gift_selects_normal_comparator() {
        let mut detection = offering();
        detection.observe_font(["transform_random"], Some(8));
        assert_eq!(detection.automatic_mode(), Some("Normal"));
    }

    #[test]
    fn unresolved_offering_preserves_comparator_selection() {
        assert_eq!(offering().automatic_mode(), None);
    }

    #[test]
    fn manual_override_blocks_automatic_mode() {
        let mut detection = offering();
        detection.override_mode();
        detection.observe_font(["corrupted_gem_reroll"], Some(2));
        assert_eq!(detection.automatic_mode(), None);
    }

    #[test]
    fn manual_override_preserves_observed_lab_type() {
        let mut detection = offering();
        detection.observe_font(["corrupted_gem_reroll"], Some(2));
        detection.override_mode();
        assert_eq!(detection.detected_type(), Some("Dedication"));
    }

    #[test]
    fn reset_releases_manual_override() {
        let mut detection = offering();
        detection.override_mode();
        detection.reset();
        detection.observe_area(75, "Labyrinth_boss_1");
        assert_eq!(detection.automatic_mode(), Some("Normal"));
    }
}
