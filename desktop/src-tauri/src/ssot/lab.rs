//! Lab placements derived from the screen slice: the gem name band, the Divine
//! Font panel and the Font's three gem-choice slots.

use super::scale_value;

/// The gem name region for a client rect: the client's full-width top band, one
/// name line tall — see [`crate::INVENTORY_PANEL_W_REF`] for the rule and its
/// provenance. Never taller than the name line, by the owner's ruling; full
/// width because the tooltip follows the hovered item across the inventory's
/// left edge.
pub(super) fn gem_region(client: [i32; 4], scale: f32) -> [i32; 4] {
    [
        client[0],
        client[1],
        client[2].max(0),
        scale_value(crate::GEM_NAME_BAND_H_REF, scale).max(0),
    ]
}

/// The font panel region for a client rect: `FONT_PANEL_W_REF · scale` wide,
/// centred in the space left of the right-docked inventory (whose width is
/// [`crate::INVENTORY_PANEL_W_REF`] · scale), `FONT_PANEL_TOP_REF
/// · scale` below the client's top and `FONT_PANEL_H_REF · scale` tall — see
/// [`crate::FONT_PANEL_W_REF`] for the rule and its provenance. The centring
/// halves in integer px, so an odd gap lands the region one px left of centre.
pub(super) fn font_region(client: [i32; 4], scale: f32) -> [i32; 4] {
    let space_w = client[2] - scale_value(crate::INVENTORY_PANEL_W_REF, scale);
    let w = scale_value(crate::FONT_PANEL_W_REF, scale).max(0);
    [
        client[0] + (space_w - w) / 2,
        client[1] + scale_value(crate::FONT_PANEL_TOP_REF, scale),
        w,
        scale_value(crate::FONT_PANEL_H_REF, scale).max(0),
    ]
}

/// The three Divine Font gem-slot squares, left to right: each
/// `GEM_SLOT_ICON_REF · scale` wide and tall, centred on its slot. The slots
/// share the font panel's anchor — the centre of the space left of the
/// right-docked inventory — offset by `GEM_SLOT_CENTRE_DX_REF` and spaced
/// `GEM_SLOT_PITCH_REF` apart, `GEM_SLOT_Y_REF` below the client's top. See
/// [`crate::GEM_SLOT_PITCH_REF`] for the provenance.
pub(super) fn gem_slot_rects(client: [i32; 4], scale: f32) -> [[i32; 4]; 3] {
    let space_w = client[2] - scale_value(crate::INVENTORY_PANEL_W_REF, scale);
    let anchor_x = client[0] + space_w / 2;
    let y = client[1] + scale_value(crate::GEM_SLOT_Y_REF, scale);
    let size = scale_value(crate::GEM_SLOT_ICON_REF, scale).max(0);
    [-1, 0, 1].map(|i| {
        let x = anchor_x + scale_value(crate::GEM_SLOT_CENTRE_DX_REF + i * crate::GEM_SLOT_PITCH_REF, scale);
        [x - size / 2, y - size / 2, size, size]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn centre(rect: [i32; 4]) -> (i32, i32) {
        (rect[0] + rect[2] / 2, rect[1] + rect[3] / 2)
    }

    /// The frame the constants were measured on (screenshot-0012, 1080p at
    /// ui_scale 0.90): slot centres at x 530 / 628 / 728, y 397.
    #[test]
    fn slots_at_1080p_land_on_the_measured_centres() {
        let slots = gem_slot_rects([0, 0, 1920, 1080], 0.9);

        for (slot, measured_x) in slots.iter().zip([530, 628, 728]) {
            let (x, y) = centre(*slot);
            assert!((x - measured_x).abs() <= 2, "slot {slot:?} centre x {x}, measured {measured_x}");
            assert!((y - 397).abs() <= 2, "slot {slot:?} centre y {y}, measured 397");
        }
    }

    /// The icon matches at 48 px on the measured 1080p frame.
    #[test]
    fn slots_at_1080p_are_48_px_squares() {
        for slot in gem_slot_rects([0, 0, 1920, 1080], 0.9) {
            assert_eq!([slot[2], slot[3]], [48, 48], "slot {slot:?}");
        }
    }

    /// 4K at scale 1.80 is the 1080p layout doubled about the client origin,
    /// give or take one px of rounding on each edge and size.
    #[test]
    fn slots_at_4k_are_the_1080p_slots_doubled() {
        let hd = gem_slot_rects([0, 0, 1920, 1080], 0.9);
        let uhd = gem_slot_rects([0, 0, 3840, 2160], 1.8);

        for (small, big) in hd.iter().zip(uhd.iter()) {
            for i in 0..4 {
                assert!((big[i] - 2 * small[i]).abs() <= 1, "4K {big:?} vs 2 × 1080p {small:?}");
            }
        }
    }

    /// 16:10 is where centring and a fixed x offset disagree: centred, the
    /// middle slot sits at (1920 − 731) / 2 − 2 = 592 at scale 1.0.
    #[test]
    fn the_middle_slot_at_16_10_follows_the_space_left_of_the_inventory() {
        let slots = gem_slot_rects([0, 0, 1920, 1200], 1.0);

        assert_eq!(centre(slots[1]).0, (1920 - 731) / 2 - 2);
    }

    /// Adjacent slots sit one pitch apart: 110 reference px at scale 1.0.
    #[test]
    fn slots_at_scale_1_are_one_pitch_apart() {
        let slots = gem_slot_rects([0, 0, 1920, 1200], 1.0);

        assert_eq!(centre(slots[1]).0 - centre(slots[0]).0, 110);
        assert_eq!(centre(slots[2]).0 - centre(slots[1]).0, 110);
    }

    /// A windowed client moves every slot by its origin and nothing else.
    #[test]
    fn a_client_offset_shifts_every_slot_by_the_origin() {
        let at_origin = gem_slot_rects([0, 0, 1920, 1080], 0.9);
        let offset = gem_slot_rects([120, 40, 1920, 1080], 0.9);

        for (a, b) in at_origin.iter().zip(offset.iter()) {
            assert_eq!(*b, [a[0] + 120, a[1] + 40, a[2], a[3]]);
        }
    }
}
