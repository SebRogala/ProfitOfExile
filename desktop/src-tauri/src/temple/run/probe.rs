use std::time::{Duration, Instant};

use super::{cheap_hint_from_screen, locate_on_miss, TempleProbe};
use super::super::{anchor, ReadError};

/// Where a press reads the temple: Auto's screen slice, or Manual's origin and
/// ui_scale (`None` when the temple panel is unset or unreadable).
#[derive(Debug, Clone, Copy)]
pub(crate) enum TempleGeometry {
    Auto(Option<crate::ssot::ScreenSlice>),
    Manual(Option<((i32, i32), f32)>),
}

impl TempleGeometry {
    pub(crate) fn current(state: &crate::AppState, capture: (u32, u32)) -> Self {
        let resolved = crate::ssot::regions::current(state, Some(capture));
        if resolved.manual {
            Self::Manual(resolved.temple)
        } else {
            Self::Auto(*state.screen.lock().unwrap_or_else(|e| e.into_inner()))
        }
    }

    fn matching(&self, grab: &crate::capture::Capture) -> Self {
        match *self {
            Self::Auto(screen) => Self::Auto(screen.filter(|s| crate::ssot::screen_matches(
                &Some(*s), (grab.image.width(), grab.image.height()), grab.origin, grab.client,
            ))),
            manual => manual,
        }
    }
}

/// The press's calibration of the Entrance (POE-440): the whole-frame
/// [`anchor::anchor_for_loop`] locate on the pressed frame, run only while the
/// matching slice has no anchored Entrance origin ([`locate_on_miss`]).
/// `Ok(None)` when the slice is calibrated or the plate is not on the frame;
/// a found plate is `located` evidence the probe selects without a second
/// correlation.
pub fn calibrate_frame(
    grab: &crate::capture::Capture,
    geometry: &TempleGeometry,
    stop: &dyn Fn() -> bool,
) -> Result<Option<TempleProbe>, ReadError> {
    let geometry = geometry.matching(grab);
    if stop() || !locate_on_miss(&geometry) { return Ok(None); }
    let size = (grab.image.width(), grab.image.height());
    let cheap_hint = cheap_hint_from_screen(&geometry, size, grab.origin, grab.client);
    let hint = cheap_hint.map(|h| h.calibration);
    let locating = Instant::now();
    let found = match anchor::anchor_for_loop(
        &grab.image,
        hint.as_ref(),
        (grab.client[2].max(0) as u32, grab.client[3].max(0) as u32),
        stop,
    ) {
        Ok(found) => found,
        Err(ReadError::AnchorNotFound { .. }) => return Ok(None),
    };
    if stop() { return Ok(None); }
    Ok(Some(TempleProbe { found, located: true, hint, placed_origin: cheap_hint.map(|h| h.origin), cheap_ncc: f32::NAN,
        cheap_time: Duration::ZERO, locate_time: locating.elapsed() }))
}

/// Whether this frame has a readable Manual panel or an anchored Auto slice.
pub fn is_calibrated(grab: &crate::capture::Capture, geometry: &TempleGeometry) -> bool {
    match geometry.matching(grab) {
        TempleGeometry::Manual(panel) => panel.is_some(),
        geometry => !locate_on_miss(&geometry),
    }
}

/// The press's presence test: one windowed recheck at the placed Entrance
/// origin. A miss is `Ok(None)`; nothing here searches the frame (POE-440).
pub fn probe_frame(
    grab: &crate::capture::Capture,
    geometry: &TempleGeometry,
    stop: &dyn Fn() -> bool,
) -> Result<Option<TempleProbe>, ReadError> {
    if stop() { return Ok(None); }
    let size = (grab.image.width(), grab.image.height());
    let cheap_hint = cheap_hint_from_screen(geometry, size, grab.origin, grab.client);
    let hint = cheap_hint.map(|h| h.calibration);
    let placed_origin = cheap_hint.map(|h| h.origin);
    let started = Instant::now();
    let cheap = anchor::detect_cheap(&grab.image, cheap_hint.as_ref());
    let cheap_time = started.elapsed();
    let cheap_ncc = cheap.ncc();
    let anchor::CheapDetect::Anchored(found) = cheap else { return Ok(None) };
    if stop() { return Ok(None); }
    Ok(Some(TempleProbe { found, located: false, hint, placed_origin, cheap_ncc, cheap_time, locate_time: Duration::ZERO }))
}
