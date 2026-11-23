use tauri::{AppHandle, Manager};

use crate::AppState;

use super::super::anchor::{self, CheapHint};
use super::super::reader::TempleLayout;
use super::super::slice;
use super::{now_ms, unit_ratio_line, Session};

/// The scale hint this capture's screen slice supplies (POE-234 WI-2).
///
/// The shared slice is the app's one store of the screen's UI scale. This
/// converts it into the temple unit through [`anchor::scale_for_ui_scale`]. The
/// placed origin is added separately by [`cheap_hint_from_screen`]; this helper
/// remains the scale-only geometry validation seam used by commands and tests.
///
/// # Four sources can be behind the number, and one of them drifts
///
/// `None` — no scale hint, and the caller is uncalibrated — in these cases:
///
/// - nothing has measured a screen (fresh install, or the tick right after
///   `ssot::geometry_recalibrate`);
/// - the stored measurement is not of THIS capture. The rule is
///   `ssot::screen_matches`', reused rather than restated so the temple cannot
///   grow its own opinion of what "the same screen" means — in particular that
///   the display is its virtual-desktop origin, never its monitor handle. In the capture loop this branch is nearly unreachable, because
///   `ssot::drop_if_mismatched` runs first on the same pixels and empties the
///   slot; `super::super::commands::temple_debug_capture` is the caller that can be
///   handed an image file of any size;
/// - the stored `ui_scale` cannot describe a screen. `settings::ScreenScaleSetting::is_sane`
///   refuses those at load and both writers measure rather than invent, so this
///   is the conversion being a total function rather than a claim that a zero
///   is reachable — the cost of being sure is one comparison, and the cost of
///   being wrong is a zero-size template.
pub fn hint_for_capture(
    screen: Option<&crate::ssot::ScreenSlice>,
    capture: (u32, u32),
    origin: (i32, i32),
    client: [i32; 4],
) -> Option<anchor::AnchorCalibration> {
    let screen = screen?;
    if !crate::ssot::screen_matches(&Some(*screen), capture, origin, client) {
        return None;
    }
    if !screen.ui_scale.is_finite() || screen.ui_scale <= 0.0 {
        return None;
    }
    Some(anchor::AnchorCalibration {
        screen_w: capture.0,
        screen_h: capture.1,
        scale: anchor::scale_for_ui_scale(screen.ui_scale),
    })
}

/// Build the cheap recheck hint from the current screen slice.
///
/// `ssot::placements` owns the Entrance origin, including any origin the
/// fallback has remembered. The scale crosses units only through
/// [`anchor::scale_for_ui_scale`]. The value is rebuilt for each capture and is
/// never taken from the prior read.
pub(super) fn cheap_hint_from_screen(
    screen: Option<&crate::ssot::ScreenSlice>,
    capture: (u32, u32),
    display_origin: (i32, i32),
    client: [i32; 4],
) -> Option<CheapHint> {
    let screen = screen?;
    let calibration = hint_for_capture(Some(screen), capture, display_origin, client)?;
    let origin = crate::ssot::placements(screen).temple?.entrance_origin;
    Some(CheapHint { calibration, origin })
}

/// The hint the loop should use for this capture, plus whether a screen slice
/// existed, read under the slice's own lock and dropped before any image work.
///
/// Lock-then-drop, like every other reader of an `AppState` mutex on this
/// thread: the anchor search that follows takes seconds, and holding the screen
/// slot across it would block `ssot::publish_screen` on the merc thread for all
/// of them.
pub(super) fn hint_from_slice(
    app: &AppHandle,
    capture: (u32, u32),
    display_origin: (i32, i32),
    client: [i32; 4],
) -> (
    Option<CheapHint>,
    Option<crate::ssot::ScreenScaleSource>,
    bool,
    Option<[i32; 2]>,
) {
    let screen = {
        let state = app.state::<AppState>();
        let slot = state.screen.lock().unwrap_or_else(|e| e.into_inner());
        *slot
    };
    let hint = cheap_hint_from_screen(screen.as_ref(), capture, display_origin, client);
    // The ANCHORED origin, which is a different question from the hint's
    // origin: `ssot::placements` answers the latter with a SEED for any
    // non-null slice, so the hint alone cannot tell "a plate was found here"
    // from "arithmetic put the centre here". `cold_sweep_reason` needs the
    // first — see the note on its `placed_origin`.
    let anchored = screen
        .and_then(|screen| screen.anchors)
        .and_then(|anchors| anchors.temple_entrance);
    (hint, screen.map(|s| s.source), screen.is_some(), anchored)
}

/// Whether a press calibrates: searches the pressed frame with
/// [`anchor::anchor_for_loop`] before any probe (POE-389, Coordinator ruling
/// 2026-11-13; calibrate-first since POE-440).
///
/// True only for a null slice or one with no ANCHORED Entrance origin: there is
/// no placement to trust, so the frame is the only way to find the sheet. A
/// placed miss is the sheet not being open, and is `NoPanel` with no search;
/// Recalibrate is the manual fix for a placement that is wrong.
///
/// `anchored_origin` is the slice's anchored origin, never the hint's: the hint
/// carries a seed for any non-null slice (POE-278), and reading that as a
/// verified placement would never search a screen that merely exists.
pub(super) fn locate_on_miss(screen_present: bool, anchored_origin: Option<[i32; 2]>) -> bool {
    !screen_present || anchored_origin.is_none()
}

/// The origin a successful fallback should remember, or `None` when the sweep
/// agrees with the placed origin inside the frame band.
///
/// A null slice has no competing placement, so the swept origin is still the
/// corroborated answer and must reach `remember_anchor` after a successful read.
pub(super) fn placed_origin_contradiction(
    placed_origin: Option<(i32, i32)>,
    swept_origin: (i32, i32),
) -> Option<(i32, i32)> {
    match placed_origin {
        None => Some(swept_origin),
        Some(placed_origin)
            if placed_origin.0.abs_diff(swept_origin.0)
                > slice::FRAME_ORIGIN_TOLERANCE as u32
                || placed_origin.1.abs_diff(swept_origin.1)
                    > slice::FRAME_ORIGIN_TOLERANCE as u32 => Some(swept_origin),
        Some(_) => None,
    }
}

/// The contradiction line for a sweep that found the panel outside the placed
/// Entrance origin's tolerance band.
pub(super) fn placed_origin_contradiction_line(
    placed_origin: Option<(i32, i32)>,
    swept_origin: (i32, i32),
) -> Option<String> {
    let placed_origin = placed_origin?;
    placed_origin_contradiction(Some(placed_origin), swept_origin).map(|_| {
        format!(
            "temple: placed origin ({},{}) contradicted by sweep ({},{})",
            placed_origin.0, placed_origin.1, swept_origin.0, swept_origin.1
        )
    })
}

/// The single placeholder for the future geometry notice task.
pub(super) const GEOMETRY_NOTICE_LINE: &str =
    "Temple: geometry notice pending — placed origin contradicted by sweep (POE-271)";

/// Invoke anchor memory only at the end of a successful full read.
pub(super) fn remember_fallback_anchor(
    fallback_origin: Option<(i32, i32)>,
    remember: impl FnOnce([i32; 2]),
) {
    if let Some((x, y)) = fallback_origin {
        remember([x, y]);
    }
}

/// The line to log when the loop takes its hint from a scale ANOTHER module
/// measured, or `None` when there is nothing to say.
///
/// Nothing to say covers three cases: no hint at all, a hint the loop already
/// announced (the slice is stable for as long as the screen is, and this runs
/// once a second), and a hint derived from the temple's own published value —
/// converting a number this module put there and reading it back is not news,
/// and saying so once a session per screen would still be one line claiming a
/// cross-module handoff that did not happen.
///
/// The cue is printed as its Rust variant name (`{:?}`) rather than as the
/// kebab-case wire string the Settings card renders: the source vocabulary
/// already has three spellings (the enum, `serde`'s wire strings, and
/// `geometry/view.ts`'s labels) and a fourth, hand-written one here would be the
/// one that drifts.
///
/// Pure over plain data, with the "already said" memory passed in, so the
/// once-per-value rule is testable without an `AppHandle`.
pub(super) fn hint_line(
    said: &mut Option<anchor::AnchorCalibration>,
    hint: Option<anchor::AnchorCalibration>,
    source: Option<crate::ssot::ScreenScaleSource>,
) -> Option<String> {
    let hint = hint?;
    if *said == Some(hint) {
        return None;
    }
    let source = source?;
    if source == crate::ssot::ScreenScaleSource::TempleAnchor {
        // Still remembered: the temple's own value must not be re-announced if
        // merc later replaces it with a number that converts to the same hint.
        *said = Some(hint);
        return None;
    }
    *said = Some(hint);
    Some(format!(
        "Temple: anchoring on the remembered screen scale ({source:?}, ui_scale {:.3}) — \
         temple scale {:.3}, no search",
        anchor::ui_scale_for_scale(hint.scale),
        hint.scale
    ))
}

/// Publish what this capture anchored onto the shared screen slice (POE-234
/// WI-2) — the temple's whole WRITE of `crate::ssot::ScreenSlice`.
///
/// Called on every tick that produced a layout, which is every tick whose
/// anchor cleared [`anchor::NCC_FLOOR`] — the temple's half of the README's
/// "VERIFIED by the consuming module on first use". Two gates stand between an
/// anchor and the shared slice: [`screen_from_anchor`]'s `k` check here, and
/// `ssot::accepts` inside [`crate::ssot::publish_screen`], which refuses a
/// temple reading that only re-states a standing merc measurement within the
/// drift band. So calling this every tick is cheap by construction — a refusal
/// at either gate and an unchanged value all stop here. Returns whether the
/// anchor produced a publishable screen slice; a withheld `k` measurement
/// returns false so a null-slice fallback can release its key for one retry. A
/// second withheld result keeps that key spent until the board key changes.
///
/// Same shape as `mercenary::run`'s publish, deliberately: `publish_screen`
/// drops the screen guard before it returns, and `persist_settings` re-takes the
/// owner mutexes through `settings::from_state`, so no lock is held across
/// either.
///
/// **The early return on a withheld measurement is ahead of the persist, and
/// nothing tests that ordering** — both halves need an `AppHandle`, so there is
/// no seam to assert it through. What the tests do cover is the decision the
/// return is taken on ([`screen_from_anchor`], pure) and the rule the persist is
/// gated by (`ssot::should_remember_screen`, pure); the two-line composition
/// between them is read, not asserted.
///
/// # The one case where the two writers can push against each other
///
/// A merc frame fit ALWAYS replaces, and a temple anchor replaces whenever it is
/// outside the band. Both cannot happen at once any more — [`screen_from_anchor`]
/// refuses to publish anything the capture's own height does not corroborate,
/// and a temple scale within `K_TOLERANCE` of that is within the band of any
/// merc reading that is too — so the loops cannot overwrite each other tick by
/// tick. What is left is one publish apiece on a real change of screen.
pub(super) fn publish_anchor_scale(
    app: &AppHandle,
    session: &mut Session,
    layout: &TempleLayout,
    hint: Option<anchor::AnchorCalibration>,
    capture: (u32, u32),
    monitor_id: u32,
    origin: (i32, i32),
    client: [i32; 4],
) -> bool {
    let next = match screen_from_anchor(layout.scale, hint, capture, monitor_id, origin, client, now_ms())
    {
        Ok(next) => next,
        Err(line) => {
            // Once per distinct line, not once per tick: the condition holds for
            // as long as the panel is on screen at that scale, and this loop
            // ticks every `DETECT_INTERVAL`.
            if session.k_said.as_deref() != Some(line.as_str()) {
                session.k_said = Some(line.clone());
                crate::app_log(app, line);
            }
            return false;
        }
    };
    let record = crate::ssot::publish_screen(app, next);
    if record.changed {
        crate::app_log(
            app,
            format!(
                "screen scale from temple anchor: ui_scale {:.3} (temple scale {:.3}, k {:.4})",
                next.ui_scale,
                layout.scale,
                anchor::TEMPLE_SCALE_PER_UI_SCALE
            ),
        );
    }
    // WI-B2's rule, unchanged: a measurement is written to disk, an estimate is
    // not, and the deadband inside `changed` is what keeps a 650 ms loop off it.
    if crate::ssot::should_remember_screen(record.changed, next.source) {
        crate::persist_settings(app);
    }
    true
}

/// The screen slice one temple anchor may publish, or the line saying why it may
/// not.
///
/// # Why an anchor above [`anchor::NCC_FLOOR`] is not automatically publishable
///
/// [`anchor::sweep_range`]'s ceiling is SOFT: the fine pass refines one nominate
/// step past the top nominee, so a capture whose true scale is above the ceiling
/// does not fail, it anchors APPROXIMATELY. Measured 2026-09-03 on a synthetic
/// plate at scale 2.10 against a 2.00 ceiling: the sweep answered **2.05 at NCC
/// 0.9390**, well above the floor. Before this gate that number would have been
/// converted through `k`, published as the screen's geometry, persisted, and
/// then used by POE-233 to place the lab OCR rects — a 2.5% error in a module
/// that never looked at a temple.
///
/// So the anchor has to be corroborated by something that is not the anchor, and
/// there are two such things. In order:
///
/// 1. **The standing hint**, when the slice holds one. It is a measurement of
///    this screen that did not come from this board, so an anchor within one
///    [`anchor::SCALE_STEP`] of it is corroborated by the strongest evidence
///    available — and, since the hint is what the anchor was searched at, this is
///    the ordinary case. The publish then goes on to `ssot::accepts`, which
///    refuses it as a restatement of the value it agrees with. That is the whole
///    point: a merc-frame-measured screen with a non-default UI slider reaches
///    the acceptance rule and is turned down there, instead of being stopped
///    here by an arithmetic that knows nothing about the slider.
/// 2. **The game client's own height**, when the slice is empty and there is nothing
///    else to ask. At the game's DEFAULT UI scale the temple scale is
///    `k * (height / 1200)` by both units' definitions, so a scale more than
///    [`super::K_TOLERANCE`] from it is one the screen does not account for.
///
/// # What this makes the temple, stated plainly
///
/// **With this gate the temple can only ever publish a scale within 1% of the
/// nominal one, or of a measurement already standing. It corroborates and
/// persists a verified seed; it does not teach the slice a new number.**
///
/// Everything else follows from that sentence. An empty slice is filled only
/// with a value the client height predicts — which is what a temple-only
/// machine at the default slider needs, and it is a real measurement rather
/// than an assumption, because the anchor had to clear [`anchor::NCC_FLOOR`] to
/// get here. A slice that already holds something is only ever confirmed. And
/// the two cases the temple therefore cannot report are the two it cannot tell
/// apart anyway: a soft-ceiling approximation, and a genuine off-nominal UI
/// slider on a machine no other module has measured. The first must not be
/// published; the second is a real number the temple is choosing not to be the
/// sole source of. Merc's gold frame measures the slider case directly, and
/// after it does, the temple corroborates it through arm 1 for the rest of the
/// session.
///
/// A withheld measurement leaves a consumer failing closed, which the README's
/// placement rule already requires; a wrong one mis-scales every rect derived
/// from it with nothing on screen to say so.
///
/// Pure, so every arm is testable without a screen or an `AppHandle`.
pub(super) fn screen_from_anchor(
    scale: f32,
    hint: Option<anchor::AnchorCalibration>,
    capture: (u32, u32),
    monitor_id: u32,
    origin: (i32, i32),
    client: [i32; 4],
    measured_at_ms: u64,
) -> Result<crate::ssot::ScreenSlice, String> {
    let withheld = match hint {
        Some(hint) => hint_disagreement_line(scale, hint),
        None => unit_ratio_line(scale, client[2].max(0) as u32, client[3].max(0) as u32),
    };
    match withheld {
        Some(line) => Err(line),
        None => Ok(anchored_screen(scale, capture, monitor_id, origin, client, measured_at_ms)),
    }
}

/// The line to log when an anchor disagrees with the hint it was searched
/// against, or `None` when the two corroborate each other.
///
/// One [`anchor::SCALE_STEP`] is the finest disagreement this module's scale grid
/// can express. Inside it the anchor confirms the standing measurement; outside
/// it, the anchor came from elsewhere — the table row or an explicit fallback —
/// and the temple does not overrule a measurement of this screen with a board it
/// read (see [`screen_from_anchor`]'s second section).
///
/// Pure, and separate from the height check, because the two withhold for
/// genuinely different reasons and a user reading `app.log` needs to know which.
pub(super) fn hint_disagreement_line(scale: f32, hint: anchor::AnchorCalibration) -> Option<String> {
    if (scale - hint.scale).abs() <= anchor::SCALE_STEP {
        return None;
    }
    Some(format!(
        "temple anchor not corroborated by the remembered screen scale: anchored at \
         {scale:.3} against a hint of {:.3} (ui_scale {:.3}) — the measurement was \
         withheld, and the shared screen scale is left to whatever else measures this \
         screen",
        hint.scale,
        anchor::ui_scale_for_scale(hint.scale),
    ))
}

/// The screen slice one temple anchor publishes.
///
/// Pure and separate from [`publish_anchor_scale`] so both derived fields are
/// pinned by tests: the unit conversion (`ssot`'s unit is not this module's, and
/// [`anchor::ui_scale_for_scale`] is the one place that crosses between them),
/// and whether the cue VERIFIES the screen — `ssot::verifies_the_screen`'s call,
/// not a literal here, for the same reason `mercenary::run::published_screen`
/// asks rather than answers.
///
/// Whether it MAY be published is [`screen_from_anchor`]'s question, not this
/// one: this builds the value, that decides the screen corroborates it.
///
/// `monitor_id` and `origin` come from the same `crate::capture::Capture` as the
/// pixels and are copied through untouched (POE-237): they are what lets
/// `ssot::screen_matches` tell a second 1920x1080 monitor from the remembered
/// one, so they must never be re-derived from anything else.
pub fn anchored_screen(
    scale: f32,
    capture: (u32, u32),
    monitor_id: u32,
    origin: (i32, i32),
    client: [i32; 4],
    measured_at_ms: u64,
) -> crate::ssot::ScreenSlice {
    let source = crate::ssot::ScreenScaleSource::TempleAnchor;
    crate::ssot::ScreenSlice {
        width: capture.0,
        height: capture.1,
        ui_scale: anchor::ui_scale_for_scale(scale),
        source,
        measured_at_ms,
        verified_this_session: crate::ssot::verifies_the_screen(source),
        monitor_id,
        origin,
        client,
        anchors: None,
    }
}
