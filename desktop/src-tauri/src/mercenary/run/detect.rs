//! Staged mercenary detection pipeline.
//!
//! `run` owns the press worker, session lifetime and the tooltip read. This
//! child owns one panel read of a pressed frame: location, registration, read
//! and reconciliation stages.

use std::time::Instant;

use tauri::AppHandle;
use tokio::sync::watch;

use super::*;
use super::super::read::ReadResult;

type GrabbedOn = (u32, (i32, i32), [i32; 4]);
type PlacedReadData = (
    ([i32; 4], f32),
    Vec<crate::ocr::OcrLineBox>,
    Result<geometry::MercLayout, geometry::DetectMiss>,
);

pub(crate) struct CaptureLocation {
    started: Instant,
    image: image::DynamicImage,
    cropped: Option<image::DynamicImage>,
    screen: [u32; 2],
    grabbed_on: GrabbedOn,
    crop: Option<[i32; 4]>,
    placement: Option<([i32; 4], f32)>,
    full_frame: bool,
    frame: geometry::Frame,
    layout: geometry::MercLayout,
    used_fallback: bool,
    located_panel: Option<[i32; 4]>,
    remember_origin: Option<[i32; 2]>,
    key: FallbackKey,
    took: u128,
    how: &'static str,
    regions_manual: bool,
}

impl CaptureLocation {
    fn view(&self) -> &image::DynamicImage {
        if self.full_frame {
            &self.image
        } else {
            self.cropped.as_ref().unwrap_or(&self.image)
        }
    }
}

struct RegisteredDetection {
    location: CaptureLocation,
    refit: Refit,
    fit_ms: u128,
}

struct ReadStage {
    registered: RegisteredDetection,
    plan: ReadPlan,
    result: ReadResult,
    first_look_tick: bool,
    pass2_ms: u128,
    icons_ms: u128,
    from_kept: bool,
    replaced_on_sight: bool,
    header_guard: Option<[i32; 4]>,
    current_panel: Option<[i32; 4]>,
}

fn placed_read_with<O, E>(
    placement: Option<([i32; 4], f32)>,
    view: &image::DynamicImage,
    frame: geometry::Frame,
    merc_geometry: &MercGeometry,
    fitted_scale: Option<f32>,
    fitted_pitch: f32,
    recognize: O,
) -> Result<Option<PlacedReadData>, E>
where
    O: FnOnce(&image::DynamicImage) -> Result<Vec<crate::ocr::OcrLineBox>, E>,
{
    let Some((panel, scale)) = placement else {
        return Ok(None);
    };
    let lines = frame.to_screen(recognize(view)?);
    let layout = geometry::placed_layout(
        &lines,
        panel,
        merc_geometry,
        fitted_scale.unwrap_or(scale),
        fitted_pitch,
    );
    Ok(Some(((panel, scale), lines, layout)))
}

trait PublicationSink {
    fn publish_first_look(&mut self, capture: MercCapture);
    fn publish_capture(&mut self, capture: MercCapture, complete: bool);
    fn remember_origin(&mut self, origin: [i32; 2]);
}

trait RegistrationEffects {
    fn publish_screen(&mut self, screen: crate::ssot::ScreenSlice) -> crate::ssot::ScreenRecord;
    fn log_screen_refusal(&mut self, line: String);
    fn persist_settings(&mut self);
}

trait DetectionEffects {
    fn refit_counter(&self) -> u64;
    fn debug_mode(&self) -> bool;
    fn template_generation(&mut self) -> u64;
    fn log(&mut self, line: String);
    fn rederive_for_window(&mut self, geometry: &MercGeometry, scale: f32);
    fn with_template_store<R>(
        &mut self,
        read: impl FnOnce(&crate::mercenary::icons::TemplateStore) -> R,
    ) -> R;
    fn registration_effects(&mut self) -> &mut dyn RegistrationEffects;
    fn publication_sink(&mut self) -> &mut dyn PublicationSink;
}

struct AppRegistrationEffects<'app> {
    app: &'app AppHandle,
}

impl RegistrationEffects for AppRegistrationEffects<'_> {
    fn publish_screen(&mut self, screen: crate::ssot::ScreenSlice) -> crate::ssot::ScreenRecord {
        crate::ssot::publish_screen(self.app, screen)
    }

    fn log_screen_refusal(&mut self, line: String) {
        crate::app_log(self.app, line);
    }

    fn persist_settings(&mut self) {
        crate::persist_settings(self.app);
    }
}

struct AppDetectionEffects<'app> {
    app: &'app AppHandle,
    registration: AppRegistrationEffects<'app>,
    publication: AppPublicationSink<'app>,
}

impl<'app> AppDetectionEffects<'app> {
    fn new(app: &'app AppHandle) -> Self {
        Self {
            app,
            registration: AppRegistrationEffects { app },
            publication: AppPublicationSink { app },
        }
    }
}

impl DetectionEffects for AppDetectionEffects<'_> {
    fn refit_counter(&self) -> u64 {
        refit_counter(self.app)
    }

    fn debug_mode(&self) -> bool {
        debug_mode(self.app)
    }

    fn template_generation(&mut self) -> u64 {
        template_generation(self.app)
    }

    fn log(&mut self, line: String) {
        crate::app_log(self.app, line);
    }

    fn rederive_for_window(&mut self, geometry: &MercGeometry, scale: f32) {
        seed::rederive_for_window(self.app, geometry, scale);
    }

    fn with_template_store<R>(
        &mut self,
        read: impl FnOnce(&crate::mercenary::icons::TemplateStore) -> R,
    ) -> R {
        let state = self.app.state::<AppState>();
        let store = state.merc.templates.lock().unwrap_or_else(|e| e.into_inner());
        read(&store)
    }

    fn registration_effects(&mut self) -> &mut dyn RegistrationEffects {
        &mut self.registration
    }

    fn publication_sink(&mut self) -> &mut dyn PublicationSink {
        &mut self.publication
    }
}

fn register_screen<S: RegistrationEffects + ?Sized>(
    sink: &mut S,
    refusals: &mut OnceLog,
    screen: [u32; 2],
    scale: f32,
    source: ScaleSource,
    measured_at_ms: u64,
    grabbed_on: GrabbedOn,
) -> (crate::ssot::ScreenRecord, crate::ssot::ScreenSlice) {
    let published = published_screen(screen, scale, source, measured_at_ms, grabbed_on);
    let record = sink.publish_screen(published);
    // POE-240: `ssot::accepts` refused this measurement — an OCR reading that
    // only re-states the screen scale already standing. The guard, the wording
    // and the once-per-distinct-refusal dedup are [`screen_refusal_log`]'s;
    // all that is left here is putting the line where a user can read it.
    if let Some(line) = screen_refusal_log(refusals, record, source, published.ui_scale) {
        sink.log_screen_refusal(line);
    }
    // POE-214 WI-B2: remember a frame measurement across restarts, so the next
    // session knows this screen's UI scale before — or without — a recruit
    // window ever opening. What the two-part gate buys, and why an OCR-derived
    // scale is not written, is `ssot::should_remember_screen`'s doc; the short
    // of it is that `persist_settings` is two blocking disk ops and the gate's
    // 0.01 deadband is what keeps them off every tick of an open panel.
    //
    // Still no lock held here: `publish_screen` drops its guard before it
    // returns, and `persist_settings` re-takes the owner mutexes through
    // `settings::from_state` — the same after-the-drop shape as
    // `temple::run::publish_anchor_scale`.
    if crate::ssot::should_remember_screen(record.changed, published.source) {
        sink.persist_settings();
    }
    (record, published)
}

struct ReadStageContext<'session> {
    session: &'session mut Session,
    cursor: Option<(i32, i32)>,
    registered: Option<RegisteredDetection>,
    first_look_tick: bool,
}

fn registered_read<E: DetectionEffects>(
    effects: &mut E,
    session: &mut Session,
    cursor: Option<(i32, i32)>,
    cancel: &watch::Receiver<bool>,
    location: CaptureLocation,
) -> Option<ReadStage> {
    let registered = register_geometry(effects, session, location);
    execute_read_plan(effects, session, cursor, cancel, registered)
}

fn publish_settled<S: PublicationSink>(
    sink: &mut S,
    capture: MercCapture,
    complete: bool,
    remember_origin: Option<[i32; 2]>,
) {
    sink.publish_capture(capture, complete);
    if let Some(origin) = remember_origin {
        sink.remember_origin(origin);
    }
}

struct AppPublicationSink<'app> {
    app: &'app AppHandle,
}

impl PublicationSink for AppPublicationSink<'_> {
    fn publish_first_look(&mut self, capture: MercCapture) {
        publish(self.app, |slice| {
            slice.status = MercStatus::Live;
            slice.burst_speaker = None;
            slice.capture = Some(capture);
            slice.last_error = None;
        });
    }

    fn publish_capture(&mut self, capture: MercCapture, complete: bool) {
        publish(self.app, |slice| {
            slice.status = live_status(complete);
            // Whatever armed this scan has been answered by the window on screen.
            slice.burst_speaker = None;
            slice.capture = Some(capture);
            slice.last_error = None;
        });
    }

    fn remember_origin(&mut self, origin: [i32; 2]) {
        crate::ssot::remember_anchor(self.app, crate::ssot::AnchorModule::MercPanel, origin);
    }
}

fn signature_merge_inputs(
    geometry_changed: bool,
    hovered: Option<(String, u8)>,
    carried: std::collections::HashSet<(String, u8)>,
) -> (Option<(String, u8)>, std::collections::HashSet<(String, u8)>) {
    (
        hovered_for_sigs(geometry_changed, hovered),
        carried_for_sigs(geometry_changed, carried),
    )
}

/// One panel read of a pressed frame: OCR it and publish what it holds.
///
/// `frame` is the press's one grab (POE-390); nothing here takes another.
/// `cursor` is the cursor at the press — where it was while the frame was
/// taken — which is what the header-withholding rule below needs it to be.
///
/// `press_seq` is the worker's press counter: each press opens a new
/// full-screen locate opportunity ([`FallbackKey`], [`locate_decision`]).
pub(super) fn detect_selected(
    app: &AppHandle,
    session: &mut Session,
    cursor: Option<(i32, i32)>,
    cancel: &watch::Receiver<bool>,
    evidence: MercProbe,
) -> DetectTick {
    let location = match evidence {
        MercProbe::Panel(location) => *location,
        MercProbe::Tooltip { column_key } => {
            if let Some(key) = column_key {
                if let Some(line) = column_trusted_line(&mut session.column_trusted_said, key) { crate::app_log(app, line.into()); }
            }
            let outcome = match judge_no_layout(session, None, cursor, Instant::now()) {
                NoLayout::Occluded { announce } => {
                    if announce { crate::app_log(app, "Merc: panel occluded (cursor over it) — holding the capture".into()); }
                    DetectOutcome::Occluded
                }
                NoLayout::Counted(outcome) => { miss_effects(app, outcome); outcome }
            };
            return detect_report(Some(outcome), false);
        }
    };
    if let Some(line) = admit_oversize_frame(&mut session.errors, location.screen[0], location.screen[1], crate::ocr::max_image_dimension()) { crate::app_log(app, line); }
    if location.used_fallback { session.fallback.take(location.key); }
    if let Some(origin) = location.remember_origin {
        if location.placement.is_some_and(|(placed, _)| [placed[0], placed[1]] != origin) {
            crate::app_log(app, MERC_GEOMETRY_NOTICE_LINE.to_string());
        }
    }
    let full_frame = location.full_frame;
    let mut effects = AppDetectionEffects::new(app);
    let Some(read) = registered_read(&mut effects, session, cursor, cancel, location) else {
        return detect_report(None, full_frame);
    };
    reconcile_and_publish(app, session, cursor, read)
}

pub(crate) enum MercProbe {
    Panel(Box<CaptureLocation>),
    Tooltip { column_key: Option<FallbackKey> },
}

pub(super) fn probe_frame(
    app: &AppHandle,
    session: &mut Session,
    cursor: Option<(i32, i32)>,
    grab: &crate::capture::Capture,
    press_seq: u64,
) -> Result<Option<MercProbe>, String> {
    let (placement, regions_manual) = merc_placement(app, grab);
    probe_press(session, cursor, grab,
        FallbackKey { refit: refit_counter(app), press: press_seq },
        placement, regions_manual, &mut crate::ocr::recognize_lines)
}

fn merc_placement(app: &AppHandle, grab: &crate::capture::Capture) -> (Option<([i32; 4], f32)>, bool) {
    press_placement(&app.state::<AppState>(), grab)
}

/// The press's merc placement and whether it came from Manual regions.
fn press_placement(state: &AppState, grab: &crate::capture::Capture) -> (Option<([i32; 4], f32)>, bool) {
    let resolved = crate::ssot::regions::current(state, Some((grab.image.width(), grab.image.height())));
    if resolved.manual { return (resolved.merc, true); }
    let screen = *state.screen.lock().unwrap_or_else(|e| e.into_inner());
    if !crate::ssot::screen_matches(&screen, (grab.image.width(), grab.image.height()), grab.origin, grab.client) {
        return (None, false);
    }
    let placement = screen.and_then(|slice| {
        crate::ssot::placements_for(Some(&slice)).merc.map(|placed| (placed.panel, slice.ui_scale))
    });
    (placement, false)
}

/// One press's probe under the placement [`press_placement`] decided.
fn probe_press(
    session: &mut Session,
    cursor: Option<(i32, i32)>,
    grab: &crate::capture::Capture,
    key: FallbackKey,
    placement: Option<([i32; 4], f32)>,
    regions_manual: bool,
    recognize: &mut impl FnMut(&image::DynamicImage) -> Result<Vec<OcrLineBox>, String>,
) -> Result<Option<MercProbe>, String> {
    if regions_manual { session.refit_located = key.refit; }
    probe_regions_with(session, cursor, grab, key, placement, regions_manual, recognize)
}

#[cfg(test)]
fn probe_frame_with(
    session: &Session,
    cursor: Option<(i32, i32)>,
    grab: &crate::capture::Capture,
    key: FallbackKey,
    placement: Option<([i32; 4], f32)>,
    recognize: &mut impl FnMut(&image::DynamicImage) -> Result<Vec<OcrLineBox>, String>,
) -> Result<Option<MercProbe>, String> {
    probe_regions_with(session, cursor, grab, key, placement, false, recognize)
}

fn probe_regions_with(
    session: &Session,
    cursor: Option<(i32, i32)>,
    grab: &crate::capture::Capture,
    key: FallbackKey,
    placement: Option<([i32; 4], f32)>,
    regions_manual: bool,
    recognize: &mut impl FnMut(&image::DynamicImage) -> Result<Vec<OcrLineBox>, String>,
) -> Result<Option<MercProbe>, String> {
    let started = Instant::now();
    // The display travels with the pixels (POE-237): both halves come off one
    // `Capture`, so the id published below always names the monitor the scale
    // was measured on.
    let grabbed_on = (grab.monitor_id, grab.origin, grab.client);
    // One copy per press: the frame is shared with the other readers.
    let img = grab.image.clone();
    let (iw, ih) = {
        use image::GenericImageView;
        img.dimensions()
    };
    let screen = [iw, ih];
    // The caller withholds mismatched geometry; only selected work may forget it.
    let crop = placement.map(|(panel, scale)| geometry::placed_panel_crop(panel, scale, screen));
    let mut full_frame = crop.is_none();
    // Off the key's own counter read, so the verdict and the budget it may
    // spend are about the same Recalibrate. A pending Recalibrate is the only
    // thing that makes a placed miss manual (ruling 2).
    let manual = !regions_manual && refit_requested(session.refit_located, key.refit);

    let cropped = crop.map(|r| img.crop_imm(r[0] as u32, r[1] as u32, r[2] as u32, r[3] as u32));
    let mut view: &image::DynamicImage = cropped.as_ref().unwrap_or(&img);
    let mut frame = match crop {
        Some(r) => geometry::Frame::cropped((r[0], r[1]), screen),
        None => geometry::Frame::full(screen),
    };
    let known_panel = if regions_manual { placement.map(|(panel, _)| panel) }
        else { session.panel.or_else(|| placement.map(|(panel, _)| panel)) };

    // A held fit's pitch is the frame's horizontal slot pitch, reused as the
    // vertical proxy because it is the only frame-verified length the session
    // holds: the slot pitch it holds is `REF_PITCH · scale` in screen px, and
    // 48.67 in reference space is closer to the fixture's 48.4 vertical row
    // pitch than 49.3 is. POE-216 resolves this axis crossing.
    let fitted = if regions_manual { None } else { session.fitted };
    let fitted_pitch = fitted
        .map(|fit| fit.pitch)
        .unwrap_or_else(|| placement.map_or(session.geometry.row_pitch, |(_, scale)| session.geometry.row_pitch * scale));
    // THE PLACED READ, on the crop. `None` on a cold start, which has no rect
    // to crop and reads nothing before the locate below.
    //
    // TRANSLATED THE INSTANT IT COMES BACK. Windows OCR reports boxes in the
    // pixels it was handed, and every rule below this line — the known-panel
    // anchor, the column-x test, the cell rects the hover tick hit-tests the
    // real cursor against — is screen-absolute. See `geometry::Frame`.
    let placed_read = match placed_read_with(
        placement,
        view,
        frame,
        &session.geometry,
        fitted.map(|fit| fit.scale),
        fitted_pitch,
        &mut *recognize,
    ) {
        Ok(placed_read) => placed_read,
        Err(e) => {
            return Err(format!("Merc: OCR failed — {e}"));
        }
    };

    // Whether the full-screen locate runs is [`locate_decision`]'s call, the
    // whole rule in one place. When it does, it re-reads the SAME grab, so it
    // is one more OCR rather than a second screen capture.
    let mut budget = session.fallback.clone();
    let decision = locate_decision(
        placed_read.as_ref().map(|(placed, _, layout)| (*placed, PlacedRead::of(layout))),
        manual,
        regions_manual,
        &mut budget,
        key,
    );
    let (located_by, _lines, layout) = match (decision, placed_read) {
        (LocateDecision::Locate(reason), _) => {
            full_frame = true;
            view = &img;
            frame = geometry::Frame::full(screen);
            let lines = match recognize(view) {
                Ok(lines) => frame.to_screen(lines),
                Err(e) => {
                    return Err(format!("Merc: OCR failed — {e}"));
                }
            };
            let layout =
                geometry::detect_reason(&lines, &session.geometry, &session.vocab, known_panel);
            (Some(reason), lines, layout)
        }
        (LocateDecision::TrustPlacementOverColumn, Some((_, lines, layout))) => {
            (None, lines, layout)
        }
        (LocateDecision::TrustPlacement, Some((_, lines, layout))) => (None, lines, layout),
        // No placement and no locate: this key's cold start is spent, and a
        // counted miss costs no OCR. (`locate_decision` answers the two trust
        // arms only for a placed read, so `(_, None)` is this same case.)
        (LocateDecision::ColdStartSpent | LocateDecision::ManualUnset, _) | (_, None) => {
            return Ok(None);
        }
    };
    let used_fallback = located_by.is_some();

    // Only a manual locate can move the panel or remember an origin: see
    // [`fallback_panel`], which answers nothing for a cold start.
    let mut located_panel = None;
    let mut remember_origin = None;
    if let Some(reason) = located_by {
        let (located_from_fallback, fallback_origin, _) =
            fallback_panel(reason, &layout, &session.geometry);
        located_panel = located_from_fallback;
        remember_origin = fallback_origin;
    }

    let took = started.elapsed().as_millis();
    let how = frame.describe();

    let layout = match layout {
        Ok(layout) => layout,
        Err(_why) => {
            return probe_tooltip(session, grab, cursor, recognize).map(|present| present.then_some(MercProbe::Tooltip {
                column_key: (decision == LocateDecision::TrustPlacementOverColumn).then_some(key),
            }));
        }
    };
    Ok(Some(MercProbe::Panel(Box::new(CaptureLocation {
        started,
        image: img,
        cropped,
        screen,
        grabbed_on,
        crop,
        placement,
        full_frame,
        frame,
        layout,
        used_fallback,
        located_panel,
        remember_origin,
        key,
        took,
        how,
        regions_manual,
    }))))
}

fn probe_tooltip(session: &Session, grab: &crate::capture::Capture, cursor: Option<(i32, i32)>,
    recognize: &mut impl FnMut(&image::DynamicImage) -> Result<Vec<OcrLineBox>, String>) -> Result<bool, String> {
    let Some((capture, cursor)) = session.current.as_ref().zip(cursor) else { return Ok(false); };
    let Some((ri, si)) = cell_at(capture, cursor) else { return Ok(false); };
    let Some(region) = hover_region(cursor, capture.scale, &session.geometry.thresholds, capture.screen) else { return Ok(false); };
    if region[0] < 0 || region[1] < 0 || region[2] <= 0 || region[3] <= 0
        || (region[0] + region[2]) as u32 > grab.image.width() || (region[1] + region[3]) as u32 > grab.image.height() { return Ok(false); }
    let crop = grab.image.crop_imm(region[0] as u32, region[1] as u32, region[2] as u32, region[3] as u32);
    let processed = crate::capture::preprocess_for_ocr_fast(&crop);
    let lines = recognize(&processed)?;
    let scale = (processed.width() as f32 / crop.width() as f32, processed.height() as f32 / crop.height() as f32);
    Ok(confirm_from_tooltip(&tooltip_lines(&lines, region, scale, cursor), capture.rows[ri].supports[si].tier,
        &session.vocab, &session.geometry.thresholds).is_some())
}

fn register_geometry<E: DetectionEffects>(
    effects: &mut E,
    session: &mut Session,
    location: CaptureLocation,
) -> RegisteredDetection {
    let screen = location.screen;
    let grabbed_on = location.grabbed_on;
    let frame = location.frame;
    let key = location.key;
    let layout = location.layout.clone();
    // A Manual probe owns its scale even if the mode changed before registration.
    if location.regions_manual {
        session.refit_located = key.refit;
        record_registration(session, &layout, false);
        return RegisteredDetection { location, refit: Refit::NotRequested, fit_ms: 0 };
    }
    let view = location.view();
    // Read BEFORE the fit rewrites `layout.scale`: the log line's whole job is
    // to show the two cues side by side.
    let s_ocr = layout.scale;

    // The MANUAL arm of the geometry lifecycle, and it runs FIRST — before the
    // staleness gate below and before the fit — because it needs no cue at all
    // (POE-227). Recalibrate empties the shared screen scale, and a session
    // still holding a registration would republish it — through
    // `apply_held` below and `publish_screen` further down — on this very tick,
    // putting the number the user asked to be forgotten straight back into the
    // slice and into settings.json. Dropping the registration here, before the
    // deadband and before the fit, is what makes the button re-measure: with
    // nothing held, `next_fitted_scale` adopts this tick's fit outright and a
    // tick whose fit declines leaves the layout on the OCR cue it was built
    // with (`geometry::MercLayout` reads `ScaleSource::Ocr` until something
    // registers it). The locate decision above only PEEKED at its own counter
    // read ([`refit_requested`]), which `refit_located` records just below; this
    // re-reads the counter AT THE FIT, which is what ssot's bump-before-write
    // ordering relies on.
    let refit = consume_refit(session, effects.refit_counter());
    match &refit {
        Refit::NotRequested => {}
        Refit::Dropped(held) => effects.log(
            format!(
                "Merc: Recalibrate — dropped the settled frame registration (scale \
                 {:.3}); this tick re-measures the panel",
                held.scale
            ),
        ),
        Refit::NothingHeld => effects.log(
            "Merc: Recalibrate — no frame registration was held; this tick measures \
             the panel"
                .to_string(),
        ),
    }
    session.refit_located = key.refit;

    // The AUTOMATIC arm, and what `s_ocr` above is the other half of: the same
    // read is what keeps a HELD registration honest. The session
    // measures `fitted` once and then carries it across every tick that cannot
    // see the frame, while `s_ocr` is re-measured here on every one of them, so
    // a UI-scale change during a run of declines would otherwise be written
    // back over a correctly scaled layout for as long as the declines last.
    // Dropped BEFORE the deadband reads it rather than at the `apply_held` call
    // below, so a number the OCR already contradicts cannot also seed a
    // `pending` proposal or turn away this tick's fresh fit: with nothing held,
    // a fit that lands registers on the spot (`adopted`, which arms
    // `geometry_changed`) and a fit that declines leaves the layout on the OCR's
    // own scale and rects — `ScaleSource::Ocr`, which arms it the other way.
    if let Some(held) = session.fitted {
        if held_is_stale(held.scale, s_ocr) {
            session.fitted = None;
            if effects.debug_mode() {
                effects.log(
                    format!(
                        "Merc: frame registration dropped — session held scale {:.3} but the \
                         OCR now reads {s_ocr:.3}",
                        held.scale
                    ),
                );
            }
        }
    }

    // POE-214: the OCR line centres put the whole grid 6-12 px left of the gold
    // frame the icons are actually drawn in, so before ANY rect derived from
    // this layout is used, measure the frame and rewrite the layout onto it.
    let stage = Instant::now();
    let refined = cellfit::refine(view, frame, layout, &session.geometry);
    let fit_ms = stage.elapsed().as_millis();
    let mut layout = refined.layout;
    let adopted = match &refined.fit {
        Some(fit) => {
            let fresh = FittedScale::from_fit(fit, layout.column_x0);
            let (settled, adopted) = next_fitted_scale(session.fitted, fresh);
            session.fitted = Some(settled);
            adopted
        }
        None => false,
    };
    // EVERYTHING downstream reads the SESSION's registration, never the raw
    // per-tick fit: the placed row bands, `pass2_texts`,
    // `seed::rederive_for_window` (whose memo key is the window `layout.scale`
    // implies), `build_capture` and the `MercCapture.scale` the SSOT publishes
    // all read this one layout. So a fresh measurement the deadband refused
    // must not reach them, and a tick whose fit declined must not drop them
    // back to the OCR's 6-12 px drift while the session knows where the frame
    // is.
    if let Some(settled) = session.fitted {
        // `Frame` whenever THIS tick measured the frame, even on a tick whose
        // measurement the deadband refused: the cue is still the frame, and the
        // log line below says which number the session is holding. `Held` is
        // for a tick that could not see the frame at all.
        let source = if refined.fit.is_some() { ScaleSource::Frame } else { ScaleSource::Held };
        cellfit::apply_held(&mut layout, &settled, source);
    }
    record_registration(session, &layout, adopted);
    // POE-214 WI-B1: the settled scale is not merc-only knowledge. It is the
    // GAME UI's scale on this screen, and the Lab capture regions want it (as
    // fractions of the slice) exactly as much as the grid rects here do. So it
    // is published from the one place it is settled, off the SESSION's layout
    // rather than the raw fit — the same number `MercCapture.scale` carries a
    // few lines down, on the same tick, so the two can never disagree.
    //
    // No lock is held at this point: the only guard this tick opens is
    // `merc.templates`, around `build_capture` below. Called on every tick that
    // reads a panel — a tick with no recruit window returns well above here —
    // and `ssot::screen_changed` is what keeps a re-measurement of the same
    // screen from waking every overlay's poll.
    // Which cue this reports, why `Held` is a frame measurement and which cues
    // verify are `ssot`'s calls, made once in [`published_screen`]. The
    // timestamp is the tick's own clock read, a few ms before `build_capture`
    // takes its `captured_at_ms` from the same source: this is when the scale
    // was MEASURED, and publishing at the settle rather than after pass 2 keeps
    // a cancelled tick's measurement from being lost.
    let (_screen_record, _published) = register_screen(
        effects.registration_effects(),
        &mut session.screen_refusals,
        screen,
        layout.scale,
        layout.scale_source,
        now_ms(),
        grabbed_on,
    );
    if effects.debug_mode() {
        match (&refined.fit, &refined.declined) {
            (Some(fit), _) => {
                // The settled scale is what the capture is READ at; the fit's
                // own is what this tick measured. They differ on a tick the
                // deadband refused, and that difference is the thing a smoke
                // check needs to see.
                let held = session.fitted.map_or(String::new(), |s| {
                    if s.scale == fit.scale {
                        String::new()
                    } else {
                        format!(" — session holds {:.3}", s.scale)
                    }
                });
                effects.log(
                    format!(
                        "Merc: frame fit scale {:.3} (ocr {:.3}) X0 {:.1} pitch {:.2} dark {}, \
                         {} cells span {}, row-pitch residual {:.1}{held}",
                        fit.scale,
                        s_ocr,
                        fit.x0,
                        fit.pitch,
                        fit.dark_side,
                        fit.cells_used,
                        fit.slot_span,
                        fit.residual_row_pitch,
                    ),
                );
            }
            (None, Some(why)) => {
                let kept = match session.fitted {
                    Some(held) => {
                        format!("holding frame registration (scale {:.3})", held.scale)
                    }
                    None => format!("keeping ocr {s_ocr:.3}"),
                };
                effects.log(format!("Merc: frame fit declined — {why}; {kept}"));
            }
            (None, None) => {}
        }
    }


    RegisteredDetection {
        location: CaptureLocation { layout, ..location },
        refit,
        fit_ms,
    }
}

fn record_registration(session: &mut Session, layout: &geometry::MercLayout, adopted: bool) {
    // Armed, never cleared here: the merge that consumes it clears it, and a
    // second change landing before then would only re-arm what is already
    // armed.
    session.geometry_changed |=
        registration_changed(session.scale_source, layout.scale_source, adopted);
    session.scale_source = layout.scale_source;
}

fn execute_read_plan<E: DetectionEffects>(
    effects: &mut E,
    session: &mut Session,
    cursor: Option<(i32, i32)>,
    cancel: &watch::Receiver<bool>,
    registered: RegisteredDetection,
) -> Option<ReadStage> {
    let first_look_tick = session.current.is_none();
    let mut context = ReadStageContext {
        session,
        cursor,
        registered: Some(registered),
        first_look_tick,
    };
    // What follows is this round's read: pass 2 — at most `max_rows` more OCR
    // calls, and only for the rows the plan below reads — and the icon walk.
    // A stop signal that arrived during pass 1 stops here, leaving the capture
    // state as it was; the settled screen measurement already published and
    // persisted during registration remains.
    if *cancel.borrow() {
        return None;
    }
    if context.first_look_tick {
        // FIRST LOOK at a window: the rows go out NOW, before the read below,
        // which is the tick's expensive half (2.2 s on a six-row panel in the
        // 2026-09-06 log) and which the strip used to sit through saying
        // "scanning". Only when nothing is live: a re-read of a window already on
        // the slice would blank its icons for the length of every tick. The
        // header is pass 1's, unguarded — it is replaced by the folded one below
        // on this same tick. See [`MercCapture::partial`].
        let registered = context.registered.as_ref().expect("registered read stage");
        let first_look = first_look(
            &registered.location.layout,
            registered.location.screen,
            now_ms(),
            &context.session.geometry,
            &context.session.vocab,
        );
        effects.publication_sink().publish_first_look(first_look);
    }

    let registered = context.registered.take().expect("registered read stage");
    let session = &mut *context.session;
    let cursor = context.cursor;
    let fit_ms = registered.fit_ms;
    let refit = &registered.refit;
    let started = registered.location.started;
    let took = registered.location.took;
    let how = registered.location.how;
    let crop = registered.location.crop;
    let placement = registered.location.placement;
    let located_panel = registered.location.located_panel;
    let used_fallback = registered.location.used_fallback;
    let layout = &registered.location.layout;
    let view = registered.location.view();
    let frame = registered.location.frame;
    let screen = registered.location.screen;
    // Before ANY use of this frame's header — the fold below, and the
    // completeness check that opens a trade session with it. The cursor was
    // read before the grab, so it says where it was WHILE the frame was taken;
    // the rect the withholding keys on is chosen inside
    // [`publishable_header_for`], never here. The header is pass 1's whatever
    // this round reads, so it is decided before the round.
    let (published_header, header_guard) = publishable_header_for(
        &layout,
        &session.geometry,
        session.header_guard,
        cursor,
        layout.header.clone(),
    );
    // A forget/reset while this capture was live means the user disowned a
    // confirmation; re-applying it here is exactly what the un-poison button
    // was pressed to stop. Before the plan (POE-278): the same change starts
    // the read budget over ([`refills_budget`]), so a kept `Confirmed` cell is
    // read afresh rather than copied onto this round's capture.
    let templates_moved =
        generation_changed(&mut session.template_generation, effects.template_generation());
    if templates_moved {
        session.confirmed.clear();
        session.hover_budget.clear();
        // Including the claim that has not been corroborated yet: the un-poison
        // button disowns a read, and a half-made one is still a read.
        session.pending_confirm = None;
        // The retained slot holds the same disowned confirmations one retire
        // back. Leaving it would let the un-poison button be undone by the next
        // re-detect.
        session.retained = None;
    }

    // IDENTITY FIRST, on this frame's own pass-1 view and BEFORE the plan
    // (POE-278): a round that re-reads nothing builds its capture out of the
    // kept one, and a REMATCH at the same level would never be seen by
    // comparing the kept capture with itself. See [`replaced_on_sight`].
    let replaced_on_sight = replaced_on_sight(
        session.current.as_ref(),
        &layout,
        screen,
        &session.geometry,
        &session.vocab,
    );
    if replaced_on_sight {
        effects.log("Merc: recruit window replaced — reading it fresh".to_string());
        drop_replaced_window(session);
    }

    // The placed rect is the geometry source for every tick that trusts the
    // placement, so none of them moves a live capture's panel. Only a manual
    // locate that lands elsewhere uses the located origin, at the placement's
    // size, for this read ([`fallback_panel`]), and a cold-start full detect
    // derives one from its rows until the SSOT placement is available on the
    // next tick. Decided before the plan, which weighs the kept capture's panel
    // against it.
    let current_panel = located_panel
        .or_else(|| placement.map(|(panel, _)| panel))
        .or_else(|| geometry::panel_bounds(&layout, &session.geometry));

    // WHAT THIS ROUND READS (POE-278, ADR-025): the full read on round 1, then
    // at most `RETRIES` rounds that re-read only what the kept capture leaves
    // unknown, then nothing. See [`round_plan`] and `read::plan_read`.
    let plan = round_plan(
        &mut session.state,
        session.current.as_ref(),
        refills_budget(&refit, templates_moved),
        current_panel,
        &layout,
        &session.geometry,
    );
    let mut pass2_ms = 0;
    let mut seeds_ms = 0;
    // `from_kept`: this round's capture was built out of the kept one — a
    // partial round, or one that read nothing.
    let (mut result, icons_ms, from_kept) = match (&plan, session.current.as_ref()) {
        (ReadPlan::Nothing, Some(kept)) => {
            let stage = Instant::now();
            let result = carry_capture(view, frame, &layout, kept, now_ms(), &session.geometry);
            (result, stage.elapsed().as_millis(), true)
        }
        // `plan_read` answers `Nothing` only for a kept capture, and the loop
        // held that same capture; were that ever not so, this reads in full.
        (plan, kept) => {
            let planned = match plan {
                ReadPlan::Partial { rows, .. } => kept.map(|kept| (rows.as_slice(), kept)),
                ReadPlan::Full | ReadPlan::Nothing => None,
            };
            let stage = Instant::now();
            let texts = match planned {
                Some((rows, _)) => pass2_planned(view, frame, &layout, &session.geometry, rows),
                None => pass2_texts(view, frame, &layout, &session.geometry),
            };
            pass2_ms = stage.elapsed().as_millis();
            // BEFORE the store is read (POE-208 L10). The seeds are rendered
            // art, so they are only valid at the window they were resampled
            // for, and this frame reports the window the panel is actually at.
            // Costs nothing on every tick but the first at a given window — see
            // `seed::window_plan`.
            let stage = Instant::now();
            effects.rederive_for_window(&session.geometry, layout.scale);
            seeds_ms = stage.elapsed().as_millis();
            let stage = Instant::now();
            let result = effects.with_template_store(|store| {
                build_planned(
                    view,
                    frame,
                    &layout,
                    &texts,
                    now_ms(),
                    &session.geometry,
                    &session.vocab,
                    &store,
                    planned,
                )
            });
            (result, stage.elapsed().as_millis(), planned.is_some())
        }
    };
    result.capture.rows_on_screen = result.rows_on_screen;
    result.capture.rows_read = result.rows_read;
    let row_mismatch = row_mismatch_line(result.rows_on_screen, result.rows_read);
    if session.row_mismatch_logged.as_deref() != row_mismatch.as_deref() {
        if let Some(line) = row_mismatch.as_deref() {
            effects.log(line.to_string());
        }
        session.row_mismatch_logged = row_mismatch;
    }
    // Where a slow tick went. The loop's own line says only that the tick was
    // slow; this one says which stage to look at. Always in debug mode, and
    // on every tick the backoff would call slow otherwise.
    let total_ms = started.elapsed().as_millis();
    if effects.debug_mode() || total_ms >= SLOW_TICK.as_millis() {
        let cells: usize = result.capture.rows.iter().map(|row| row.supports.len()).sum();
        effects.log(
            format!(
                "Merc: read stages — grab+ocr {took} ms, fit {fit_ms} ms, pass 2 {pass2_ms} ms, \
                 seeds {seeds_ms} ms, icons {icons_ms} ms ({cells} cells, {} rows) — {total_ms} ms \
                 on the {how} frame",
                result.capture.rows.len()
            ),
        );
    }
    if crop.is_some() && !used_fallback && !session.crop_detect_logged {
        session.crop_detect_logged = true;
        effects.log(
            format!("Merc: detect on the crop frame took {} ms", started.elapsed().as_millis()),
        );
    }
    result.capture.header = published_header;

    Some(ReadStage {
        registered,
        plan,
        result,
        first_look_tick,
        pass2_ms,
        icons_ms,
        from_kept,
        replaced_on_sight,
        header_guard,
        current_panel,
    })
}

fn reconcile_and_publish(
    app: &AppHandle,
    session: &mut Session,
    cursor: Option<(i32, i32)>,
    stage: ReadStage,
) -> DetectTick {
    let ReadStage {
        registered,
        plan,
        mut result,
        first_look_tick,
        pass2_ms,
        icons_ms,
        from_kept,
        replaced_on_sight,
        header_guard,
        current_panel,
    } = stage;
    let location = &registered.location;
    let layout = &location.layout;
    let full_frame = location.full_frame;
    let used_fallback = location.used_fallback;
    let remember_origin = location.remember_origin;
    let took = location.took;
    let how = location.how;
    let replaced_after_read = if from_kept {
        // Built out of the kept capture: the identity question was asked of
        // pass 1 above, and the header only fills what the kept one leaves
        // unresolved — a resolved field is not re-litigated by a round that
        // read nothing new about it.
        if let Some(kept) = session.current.as_ref() {
            result.capture.header = fold_unresolved_header(&kept.header, &result.capture.header);
        }
        false
    } else {
        // Nothing live means this is the first look at a panel since the last
        // retire — the moment the retained slot exists for. Before the header
        // fold, because `apply_confirmed` below reads what this restores.
        if first_look_tick {
            if let Some(line) = restore_retained(session, &result.capture).log_line() {
                crate::app_log(app, line);
            }
        }

        // IDENTITY FIRST, then everything the loop remembered. The header
        // merge and the remembered confirmations are both statements about ONE
        // recruit window, and a REMATCH swaps the mercenary behind a panel that
        // looks the same — with the liveness pause the loop can take ~20 s to
        // notice a window that closed, so "a capture exists" is not evidence it
        // is the same one. A different panel therefore drops the lot rather
        // than merging into it. A full read asks again with its pass-2 names.
        let (header, replaced) = fold_header(session.current.as_ref(), &result.capture);
        result.capture.header = header;
        if replaced {
            crate::app_log(app, "Merc: recruit window replaced — reading it fresh".to_string());
            drop_replaced_window(session);
        }
        replaced
    };
    let replaced = replaced_on_sight || replaced_after_read;
    // AFTER the identity check: a confirmation belongs to the window it was
    // made on, and re-applying the old window's cells to a new mercenary's rows
    // is the same inheritance bug one layer down.
    apply_confirmed(&mut result.capture, &session.confirmed);

    // A replaced panel is a NEW window however the state machine reads: the
    // loop was live for the panel that is gone, so `on_detect` would call this
    // a refresh and the log would never say a different mercenary is on screen.
    let outcome = match session.state.on_detect(true) {
        _ if replaced => DetectOutcome::Captured,
        outcome => outcome,
    };
    if outcome == DetectOutcome::Captured {
        crate::app_log(
            app,
            format!(
                "Merc: recruit window detected ({} rows, scale {:.3} via {}) — {how} frame, \
                 {took} ms",
                result.capture.rows.len(),
                result.capture.scale,
                layout.scale_source.label()
            ),
        );
        // The window opening is the moment another device's hover is worth
        // having (POE-210). Off-tick, single-flight with the module-start pull,
        // and throttled to one attempt per minute — so the churn this branch
        // also fires on (a retired window re-detected, a panel whose mercenary
        // was REPLACED) costs nothing. A landed corpus merges under the
        // template mutex and bumps the generation, which is what clears the
        // confirmations this session is holding.
        sync::spawn_repull(app);
    }
    // THE ONE PLACE A CROP IS STAMPED WITH ITS REGISTRATION (POE-215 D3).
    // `read::build_planned` cuts the crops but is not told which cue registered
    // the layout it was handed; this line is where the two meet, and after it
    // every copy of the crop — the cache, a `PendingConfirm`, the template that
    // is finally learned — carries the source of the tick it was cut on. A
    // copied cell cut nothing, and its cached crop keeps the source it had.
    let cut_at = layout.scale_source;
    let fresh: SigCache = result
        .sigs
        .into_iter()
        .map(|(key, (sig, raw))| (key, (sig, raw, cut_at)))
        .collect();
    // One read of the flag for both halves: on a re-registering tick neither
    // the hovered cell's cold crop nor a copied cell's cached crop is of the
    // registration the layout now has. See [`carried_for_sigs`].
    let geometry_changed = std::mem::take(&mut session.geometry_changed);
    let (hovered, carried) = signature_merge_inputs(
        geometry_changed,
        hovered_key(&result.capture, cursor),
        result.carried,
    );
    session.sigs = merge_sigs(
        std::mem::take(&mut session.sigs),
        fresh,
        hovered,
        &carried,
    );
    // The capture the pending claim was made against is being replaced right
    // here — this is the one place that can tell whether its row survived.
    session.pending_confirm =
        drop_pending_off_capture(session.pending_confirm.take(), &result.capture);
    session.panel = current_panel;
    // Publish the same settled panel rect the next detect will use. The
    // preview reads this capture field; it must not infer a rect from rows.
    result.capture.panel = session.panel;
    session.current = Some(result.capture.clone());
    session.revision += 1;
    // The placed path gets fixed row bands from the SSOT panel, so partial OCR
    // cannot shrink its header guard. A full fallback may see fewer rows under
    // a tooltip; retain the previous guard there and grow only.
    let previous_header_guard = session.header_guard;
    session.header_guard = if used_fallback {
        grow_rect(previous_header_guard, header_guard)
    } else {
        header_guard
    };
    session.occlusion.on_hit();

    // The header as the player will see it, once per CHANGE. Every tick would
    // be a line every 2 s saying the same three fields; nothing would be a
    // header that silently went wrong (2026-08-26) with no record of when. The
    // gate is the rendered line, so a wager the loop does not print cannot
    // trigger a duplicate.
    if let Some(line) = header_log_line(&result.capture.header, &session.header_logged) {
        crate::app_log(app, line.clone());
        session.header_logged = Some(line);
    }

    // Nothing left for a DETECT to find: the cadence drops to the liveness
    // check (2026-08-25 smoke). The hover tick stays on — it is the only path
    // that can correct a confident wrong match.
    let complete = capture_complete(&result.capture);
    if session.state.note_complete(complete) {
        crate::app_log(app, "Merc: capture complete".to_string());
        // The settle edge OPENS a trade session if this capture has none yet
        // (POE-202). Here rather than at the first detect because a half-read
        // panel builds a query for a mercenary nobody has, and each of those
        // would cost one of three searches.
        //
        // `get_or_insert_with`, never a fresh session: one capture can cross
        // this edge more than once (a re-read that loses a field, then regains
        // it), and a new session per edge would hand that capture a new
        // 3-search budget each time. ONE session per capture: opened here,
        // cleared only by a retire ([`retire_state`]).
        //
        // An incomplete capture opens none here. If a hover completes it, the
        // next press's carried capture is complete and crosses this edge then.
        session.trade.get_or_insert_with(MercTradeSession::new);
    }
    // THE ROUND, counted after the read so the line says what happened. A
    // panel the full read found REPLACED is the new window's round 1; a round
    // that read nothing is not a round and writes no line (ADR-025 clause 3).
    // `!from_kept` is the full read the match above falls back to.
    if plan.reads() || !from_kept {
        if replaced_after_read {
            session.state.refill_rounds();
        }
        session.state.note_round();
        crate::app_log(
            app,
            round_line(session.state.rounds, &plan, pass2_ms, icons_ms, complete),
        );
    }
    let mut publication = AppPublicationSink { app };
    publish_settled(&mut publication, result.capture, complete, remember_origin);
    detect_report(Some(outcome), full_frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingPublication {
        events: Vec<&'static str>,
        capture: Option<MercCapture>,
        first_look_capture: Option<MercCapture>,
        complete: Option<bool>,
        origin: Option<[i32; 2]>,
    }

    impl PublicationSink for RecordingPublication {
        fn publish_first_look(&mut self, capture: MercCapture) {
            self.events.push("first-look");
            self.first_look_capture = Some(capture.clone());
            self.capture = Some(capture);
        }

        fn publish_capture(&mut self, capture: MercCapture, complete: bool) {
            self.events.push("publish");
            self.capture = Some(capture);
            self.complete = Some(complete);
        }

        fn remember_origin(&mut self, origin: [i32; 2]) {
            self.events.push("remember");
            self.origin = Some(origin);
        }
    }

    #[derive(Default)]
    struct RecordingRegistration {
        events: Vec<&'static str>,
        screen: Option<crate::ssot::ScreenSlice>,
        persisted: Option<crate::ssot::ScreenSlice>,
        current: Option<crate::ssot::ScreenSlice>,
    }

    impl RegistrationEffects for RecordingRegistration {
        fn publish_screen(
            &mut self,
            screen: crate::ssot::ScreenSlice,
        ) -> crate::ssot::ScreenRecord {
            self.events.push("publish");
            self.screen = Some(screen);
            crate::ssot::record_screen(&mut self.current, screen)
        }

        fn log_screen_refusal(&mut self, _line: String) {
            self.events.push("refusal");
        }

        fn persist_settings(&mut self) {
            self.events.push("persist");
            self.persisted = self.screen;
        }
    }

    struct RecordingDetection {
        refit_counter: u64,
        debug_mode: bool,
        template_generation: u64,
        store: crate::mercenary::icons::TemplateStore,
        registration: RecordingRegistration,
        publication: RecordingPublication,
    }

    impl Default for RecordingDetection {
        fn default() -> Self {
            Self {
                refit_counter: 0,
                debug_mode: false,
                template_generation: 0,
                store: crate::mercenary::icons::TemplateStore::new(),
                registration: RecordingRegistration::default(),
                publication: RecordingPublication::default(),
            }
        }
    }

    impl DetectionEffects for RecordingDetection {
        fn refit_counter(&self) -> u64 {
            self.refit_counter
        }

        fn debug_mode(&self) -> bool {
            self.debug_mode
        }

        fn template_generation(&mut self) -> u64 {
            self.publication.events.push("read-start");
            self.template_generation
        }

        fn log(&mut self, _line: String) {}

        fn rederive_for_window(&mut self, _geometry: &MercGeometry, _scale: f32) {}

        fn with_template_store<R>(
            &mut self,
            read: impl FnOnce(&crate::mercenary::icons::TemplateStore) -> R,
        ) -> R {
            self.publication.events.push("full-read");
            read(&self.store)
        }

        fn registration_effects(&mut self) -> &mut dyn RegistrationEffects {
            &mut self.registration
        }

        fn publication_sink(&mut self) -> &mut dyn PublicationSink {
            &mut self.publication
        }
    }

    fn test_capture(screen: [u32; 2]) -> MercCapture {
        MercCapture {
            captured_at_ms: 42,
            live: true,
            scale: 1.25,
            screen,
            panel: Some([140, 80, 560, 320]),
            header: MercHeader::default(),
            rows: Vec::new(),
            rows_on_screen: 0,
            rows_read: 0,
            partial: false,
        }
    }

    fn test_session() -> Session {
        Session {
            geometry: MercGeometry::default(),
            vocab: MercVocab::load().expect("the compiled merc vocabulary parses"),
            state: LoopState::default(),
            errors: OnceLog::default(),
            screen_refusals: OnceLog::default(),
            current: None,
            sigs: SigCache::new(),
            pending_confirm: None,
            confirmed: HashMap::new(),
            hover_budget: HoverBudget::default(),
            template_generation: 0,
            saves: None,
            miss_logged: false,
            panel: None,
            header_guard: None,
            fallback: FallbackBudget::default(),
            column_trusted_said: None,
            crop_detect_logged: false,
            occlusion: OcclusionRun::default(),
            header_logged: None,
            row_mismatch_logged: None,
            retained: None,
            trade: None,
            fitted: Some(FittedScale {
                scale: 1.0,
                cell_px: 40,
                x0_offset: 0.0,
                pitch: 49.0,
                source: FitSource::Grid,
                pending: None,
            }),
            scale_source: ScaleSource::Ocr,
            refit_seen: 0,
            refit_located: 0,
            geometry_changed: false,
            revision: 0,
        }
    }

    fn test_location() -> CaptureLocation {
        let geometry = MercGeometry::default();
        let layout = geometry::MercLayout {
            scale: 1.0,
            scale_source: ScaleSource::Ocr,
            column_x0: 100,
            row_pitch: geometry.row_pitch,
            rows: vec![geometry::MercLayoutRow {
                index: 0,
                centre_y: 100.0,
                skill_icon: [60, 80, 40, 40],
                name_rect: [100, 90, 120, 20],
                text: "Ice Shot".into(),
                cells: (0..geometry.max_slots)
                    .map(|slot| [140 + slot as i32 * 49, 80, 40, 40])
                    .collect(),
            }],
            header: MercHeader::default(),
        };
        CaptureLocation {
            started: Instant::now(),
            image: image::DynamicImage::new_rgba8(1920, 1080),
            cropped: None,
            screen: [1920, 1080],
            grabbed_on: (131_074, (-1920, 0), [11, 22, 333, 444]),
            crop: None,
            placement: None,
            full_frame: true,
            frame: geometry::Frame::full([1920, 1080]),
            layout,
            used_fallback: false,
            located_panel: None,
            remember_origin: None,
            key: FallbackKey { refit: 0, press: 0 },
            took: 4,
            how: "full",
            regions_manual: false,
        }
    }

    fn probe_capture() -> crate::capture::Capture {
        let crop = image::load_from_memory(include_bytes!("../../../tests/fixtures/merc-recruit-pc-1080p.png")).unwrap();
        // Derived full frame: the committed 500×280 crop at its documented origin.
        let mut image = image::DynamicImage::new_rgb8(cellfit::PC_SCREEN[0], cellfit::PC_SCREEN[1]);
        image::imageops::overlay(&mut image, &crop, cellfit::PC_ORIGIN.0 as i64, cellfit::PC_ORIGIN.1 as i64);
        crate::capture::Capture {
            image,
            monitor_id: 9, origin: (0,0), client: [0,0,1920,1080], timing: Default::default(),
        }
    }

    #[test]
    fn cold_probe_recognizes_the_real_pc_ocr_fixture_without_spending_fallback() {
        let mut session = test_session();
        session.fitted = None;
        let key = FallbackKey { refit: 0, press: 1 };
        let mut ocrs = 0;
        let probe = probe_frame_with(&session, None, &probe_capture(), key, None, &mut |_| {
            ocrs += 1;
            Ok(cellfit::pc_lines())
        }).unwrap();
        let Some(MercProbe::Panel(location)) = probe else { panic!("expected recognized merc panel"); };
        assert_eq!(location.layout.rows.iter().map(|row| row.text.as_str()).collect::<Vec<_>>(),
            vec!["Withering Step", "Chaotic Burst", "Chaotic Shot", "Caustic Arrow", "Trarthan Agility", "Grace"]);
        assert_eq!(ocrs, 1);
        assert_eq!(session.fallback.uses, 0);
        assert!(!session.state.live);
        let mut effects = RecordingDetection::default();
        let (_tx, cancel) = watch::channel(false);
        let read = registered_read(&mut effects, &mut session, None, &cancel, *location).unwrap();
        assert_eq!(read.result.capture.rows[0].skill.name.as_deref(), Some("Withering Step"));
        assert_eq!(ocrs, 1, "selected transaction reuses presence evidence");
        assert_eq!(effects.publication.first_look_capture.unwrap().rows.len(), 6);
    }

    #[test]
    fn paddle_spike_merc_cold_probe_on_the_pc_fixture() {
        use crate::ocr_paddle::spike;
        let name = "merc_cold_probe_pc_1080p";
        if spike::off(name) {
            return;
        }
        let mut session = test_session();
        session.fitted = None;
        let clock = spike::Clock::default();
        let t = std::time::Instant::now();
        let probe = probe_frame_with(&session, None, &probe_capture(), FallbackKey { refit: 0, press: 1 }, None,
            &mut |img| clock.time(|| crate::ocr::recognize_lines(img)));
        let total = t.elapsed().as_secs_f64() * 1000.0;
        let want = vec!["Withering Step", "Chaotic Burst", "Chaotic Shot", "Caustic Arrow", "Trarthan Agility", "Grace"];
        let rows: Option<Vec<String>> = match &probe {
            Ok(Some(MercProbe::Panel(location))) => Some(location.layout.rows.iter().map(|r| r.text.clone()).collect()),
            _ => None,
        };
        let pass = rows.as_ref().map(|r| r.iter().map(String::as_str).collect::<Vec<_>>() == want).unwrap_or(false);
        let lines = crate::ocr::recognize_lines(&probe_capture().image).map(|l| l.into_iter().map(|l| l.text).collect::<Vec<_>>());
        spike::report(name, pass, &clock, total, &format!("rows={rows:?}; probe_is_err={}; ocr_lines={lines:?}", probe.is_err()));
        assert!(pass);
    }

    /// POE-507 spike diagnostic: why the real-pixel probe misses. The
    /// committed crop starts at y 585, so the `Wager: 8 831` line the
    /// hand-authored `pc_lines` put at y 580 is not in the pixels.
    #[test]
    fn paddle_spike_merc_detect_reason_with_and_without_wager() {
        use crate::ocr_paddle::spike;
        if spike::off("merc_detect_diag") {
            return;
        }
        let session = test_session();
        let lines = crate::ocr::recognize_lines(&probe_capture().image).unwrap();
        eprintln!("PADDLE-DIAG|merc lines={lines:?}");
        let bare = geometry::detect_reason(&lines, &session.geometry, &session.vocab, None);
        eprintln!("PADDLE-DIAG|merc bare={:?}", bare.as_ref().map(|l| l.rows.iter().map(|r| r.text.clone()).collect::<Vec<_>>()));
        let mut with = lines.clone();
        with.insert(0, geometry::test_line("Wager: 8 831", 700, 580));
        let wager = geometry::detect_reason(&with, &session.geometry, &session.vocab, None);
        eprintln!("PADDLE-DIAG|merc with_wager={:?}", wager.as_ref().map(|l| l.rows.iter().map(|r| r.text.clone()).collect::<Vec<_>>()));
        let canned = geometry::detect_reason(&cellfit::pc_lines()[1..], &session.geometry, &session.vocab, None);
        eprintln!("PADDLE-DIAG|merc canned_without_wager={:?}", canned.as_ref().map(|l| l.rows.len()));
    }

    #[test]
    fn losing_probe_preserves_the_held_capture_and_registration() {
        let mut session = test_session();
        let capture = test_capture([1920,1080]);
        session.current = Some(capture.clone());
        session.panel = capture.panel;
        session.state.on_detect(true);
        let fitted = session.fitted;
        let before = (session.state.live,session.state.misses,session.state.complete,session.state.rounds);
        let probe = probe_frame_with(&session, None, &probe_capture(), FallbackKey { refit: 1, press: 1 }, None,
            &mut |_| Ok(vec![OcrLineBox { text: "Inventory".into(), x: 10,y:10,w:80,h:16 }])).unwrap();
        assert!(probe.is_none());
        assert_eq!(session.current, Some(capture));
        assert_eq!((session.state.live,session.state.misses,session.state.complete,session.state.rounds), before);
        assert_eq!(session.fitted, fitted);
        assert_eq!(session.fallback.uses, 0);
        assert_eq!(session.refit_located, 0);
        assert_eq!(session.revision, 0);
        assert!(session.retained.is_none());
    }

    #[test]
    fn a_held_panel_with_cursor_on_it_needs_current_tooltip_pixels() {
        let (session, cursor) = held_pc_session();
        let mut ocrs = 0;
        let probe = probe_frame_with(&session, Some(cursor), &probe_capture(), FallbackKey { refit: 0, press: 1 }, None,
            &mut |_| { ocrs += 1; Ok(Vec::new()) }).unwrap();
        assert!(probe.is_none());
        assert_eq!(ocrs, 2, "the miss checks current tooltip evidence on the same frame");
        assert!(session.state.live);
        assert_eq!(session.state.misses, 0);
    }

    fn held_pc_session() -> (Session, (i32,i32)) {
        let mut session = test_session();
        let layout = geometry::detect_reason(&cellfit::pc_lines(), &session.geometry, &session.vocab, None).unwrap();
        let mut capture = first_look(&layout, [1920,1080], 42, &session.geometry, &session.vocab);
        // Pass 1 has no fitted support cells; this is a previously fitted cell
        // inside the PC fixture's panel, retained while its tooltip covers it.
        let rect = [1000,600,44,44];
        capture.rows[0].supports.push(MercSupportRead {
            slot:0, rect, family:None, tier:Some(2), ids:Vec::new(), name:None,
            score:0.0, state:ReadState::Unknown, candidates:Vec::new(),
        });
        let cursor = (rect[0]+rect[2]/2,rect[1]+rect[3]/2);
        capture.panel = Some([724,554,500,430]);
        session.panel = capture.panel;
        session.current = Some(capture);
        session.state.on_detect(true);
        (session,cursor)
    }

    #[test]
    fn current_tooltip_evidence_selects_the_held_merc_panel_without_retirement() {
        let (session,cursor) = held_pc_session();
        let mut ocrs = 0;
        let probe = probe_frame_with(&session, Some(cursor), &probe_capture(), FallbackKey { refit:0,press:1 }, None,
            &mut |_| {
                ocrs += 1;
                Ok(if ocrs == 1 { Vec::new() } else {
                    vec![OcrLineBox { text:"Chain (Tier 2)".into(), x:40,y:40,w:200,h:24 }]
                })
            }).unwrap();
        assert!(matches!(probe, Some(MercProbe::Tooltip { .. })));
        assert!(session.state.live);
        assert_eq!(session.state.misses, 0);
        assert!(session.pending_confirm.is_none(), "recognition does not confirm or learn a cell");
    }

    #[test]
    fn production_read_with_existing_capture_skips_first_look_publication() {
        let (_tx, cancel) = watch::channel(false);
        let mut effects = RecordingDetection::default();
        let mut session = test_session();
        session.current = Some(test_capture([1920, 1080]));

        let read = registered_read(
            &mut effects,
            &mut session,
            None,
            &cancel,
            test_location(),
        );

        assert!(read.is_some());
        assert_eq!(effects.publication.events, vec!["read-start", "full-read"]);
        assert!(effects.publication.first_look_capture.is_none());
    }

    #[test]
    fn cancelled_read_preserves_registered_screen() {
        let (_tx, cancel) = watch::channel(true);
        let mut effects = RecordingDetection::default();
        let mut session = test_session();
        let read = registered_read(
            &mut effects,
            &mut session,
            None,
            &cancel,
            test_location(),
        );

        let screen = effects
            .registration
            .screen
            .expect("registered screen was published");
        assert_eq!((screen.width, screen.height), (1920, 1080));
        assert_eq!(
            (screen.monitor_id, screen.origin, screen.client),
            (131_074, (-1920, 0), [11, 22, 333, 444])
        );
        let persisted = effects
            .registration
            .persisted
            .expect("frame screen was persisted");
        assert_eq!((persisted.width, persisted.height), (1920, 1080));
        assert_eq!(
            (persisted.monitor_id, persisted.origin, persisted.client),
            (131_074, (-1920, 0), [11, 22, 333, 444])
        );
        assert_eq!(effects.registration.events, vec!["publish", "persist"]);
        assert!(read.is_none());
        assert!(effects.publication.events.is_empty());
    }

    #[test]
    fn first_look_precedes_the_full_read() {
        let (_tx, cancel) = watch::channel(false);
        let mut effects = RecordingDetection::default();
        let mut session = test_session();
        let read = registered_read(
            &mut effects,
            &mut session,
            None,
            &cancel,
            test_location(),
        );

        assert!(read.is_some());
        assert_eq!(
            effects.publication.events,
            vec!["first-look", "read-start", "full-read"]
        );
        let first_look = effects
            .publication
            .first_look_capture
            .expect("production first-look publication was observed");
        assert_eq!(first_look.screen, [1920, 1080]);
        assert_eq!(first_look.rows_on_screen, 1);
        assert!(first_look.partial);
        let screen = effects
            .registration
            .screen
            .expect("production registration published the screen");
        assert_eq!(
            (screen.monitor_id, screen.origin, screen.client),
            (131_074, (-1920, 0), [11, 22, 333, 444])
        );
    }

    #[test]
    fn geometry_change_drops_copied_signature_inputs() {
        let hovered = Some(("ice-shot".to_string(), 0));
        let carried = std::collections::HashSet::from([("ice-shot".to_string(), 1)]);

        let (hovered_after, carried_after) = signature_merge_inputs(true, hovered, carried);

        assert_eq!(hovered_after, None);
        assert!(carried_after.is_empty());
    }

    #[test]
    fn settled_publication_keeps_monitor_geometry_before_origin() {
        let mut publication = RecordingPublication::default();

        publish_settled(
            &mut publication,
            test_capture([3840, 2160]),
            true,
            Some([-1920, 0]),
        );

        assert_eq!(publication.events, vec!["publish", "remember"]);
        assert_eq!(publication.capture.expect("capture published").screen, [3840, 2160]);
        assert_eq!(publication.complete, Some(true));
        assert_eq!(publication.origin, Some([-1920, 0]));
    }

    #[test]
    fn registered_screen_registration_carries_capture_identity() {
        let mut registration = RecordingRegistration::default();
        let mut refusals = OnceLog::default();
        let grabbed_on = (131_074, (-1920, 0), [11, 22, 333, 444]);

        register_screen(
            &mut registration,
            &mut refusals,
            [1920, 1080],
            1.0,
            ScaleSource::Frame,
            1_724_000_000_000,
            grabbed_on,
        );

        let screen = registration.screen.expect("registered screen was published");
        assert_eq!((screen.monitor_id, screen.origin, screen.client), grabbed_on);
    }

    #[test]
    fn unchanged_frame_registration_does_not_persist() {
        let grabbed_on = (131_074, (-1920, 0), [11, 22, 333, 444]);
        let standing = published_screen(
            [1920, 1080],
            1.0,
            ScaleSource::Frame,
            1_724_000_000_000,
            grabbed_on,
        );
        let mut registration = RecordingRegistration {
            current: Some(standing),
            ..RecordingRegistration::default()
        };
        let mut refusals = OnceLog::default();

        register_screen(
            &mut registration,
            &mut refusals,
            [1920, 1080],
            1.0,
            ScaleSource::Frame,
            1_724_000_000_000,
            grabbed_on,
        );

        assert_eq!(registration.events, vec!["publish"]);
        assert!(registration.persisted.is_none());
    }

    #[test]
    fn ocr_registration_does_not_persist() {
        let mut registration = RecordingRegistration::default();
        let mut refusals = OnceLog::default();

        register_screen(
            &mut registration,
            &mut refusals,
            [1920, 1080],
            1.0,
            ScaleSource::Ocr,
            1_724_000_000_000,
            (131_074, (-1920, 0), [11, 22, 333, 444]),
        );

        assert_eq!(registration.events, vec!["publish"]);
        assert!(registration.persisted.is_none());
    }

    #[test]
    fn refused_ocr_registration_logs_without_persisting() {
        let grabbed_on = (131_074, (-1920, 0), [11, 22, 333, 444]);
        let standing = published_screen(
            [1920, 1080],
            1.0,
            ScaleSource::Frame,
            1_724_000_000_000,
            grabbed_on,
        );
        let mut registration = RecordingRegistration {
            current: Some(standing),
            ..RecordingRegistration::default()
        };
        let mut refusals = OnceLog::default();

        register_screen(
            &mut registration,
            &mut refusals,
            [1920, 1080],
            1.0,
            ScaleSource::Ocr,
            1_724_000_000_000,
            grabbed_on,
        );

        assert_eq!(registration.events, vec!["publish", "refusal"]);
        assert!(registration.persisted.is_none());
    }

    #[test]
    fn successful_fallback_publishes_before_remembering_its_origin() {
        let mut publication = RecordingPublication::default();
        publish_settled(
            &mut publication,
            test_capture([3840, 2160]),
            true,
            Some([745, 561]),
        );
        assert_eq!(publication.events, vec!["publish", "remember"]);
        assert_eq!(publication.origin, Some([745, 561]));

        let mut publication = RecordingPublication::default();
        publish_settled(&mut publication, test_capture([3840, 2160]), true, None);
        assert_eq!(publication.events, vec!["publish"]);
        assert_eq!(publication.origin, None);
    }

    // ------------------------------------------------- Manual merc panel --

    /// The WI-6 acceptance panel: Manual `merc_panel` at scale 1.0 on 2560x1440.
    const MANUAL_PANEL: [i32; 4] = [1450, 615, 555, 477];
    /// A Manual `merc_panel` at scale 1.2 (666 / `MERC_PANEL_REF` width 555).
    const MANUAL_PANEL_1_2: [i32; 4] = [1406, 738, 666, 572];
    const MANUAL_SCALE_1_2: f32 = 1.2;
    const SCREEN_2560: [u32; 2] = [2560, 1440];

    /// An `AppState` in Manual whose only set region is `merc_panel`, holding
    /// `screen` as its Auto slice.
    fn manual_merc_state(panel: [i32; 4], screen: Option<crate::ssot::ScreenSlice>) -> crate::AppState {
        let state = crate::settings::tests::test_app_state();
        *state.screen.lock().unwrap() = screen;
        *state.ocr_regions.lock().unwrap() = crate::ssot::OcrRegionsSetting {
            mode: crate::ssot::OcrRegionsMode::Manual,
            manual: Some(crate::ssot::ManualRegions {
                set_on: crate::ssot::ScreenRef { width: 2560, height: 1440, origin: [0, 0], at_ms: 1_732_400_000_000 },
                edited: true,
                lab_gem: None,
                lab_font: None,
                lab_gem_slots: None,
                temple_panel: None,
                merc_panel: Some(panel),
            }),
        };
        state
    }

    /// A black 2560x1440 grab of monitor 1 at (0, 0).
    fn capture_2560() -> crate::capture::Capture {
        crate::capture::Capture {
            image: image::DynamicImage::new_rgb8(SCREEN_2560[0], SCREEN_2560[1]),
            monitor_id: 1, origin: (0, 0), client: [0, 0, 2560, 1440], timing: Default::default(),
        }
    }

    /// [`capture_2560`] with a distinct pixel at each corner of `rect`, so the
    /// crop the probe OCRs can be told from any other rect of the same size.
    fn marked_capture_2560(rect: [i32; 4]) -> crate::capture::Capture {
        let mut grab = capture_2560();
        let rgb = grab.image.as_mut_rgb8().expect("an rgb8 grab");
        let [x, y, w, h] = rect.map(|v| v as u32);
        rgb.put_pixel(x, y, image::Rgb([255, 0, 0]));
        rgb.put_pixel(x + w - 1, y, image::Rgb([0, 255, 0]));
        rgb.put_pixel(x, y + h - 1, image::Rgb([0, 0, 255]));
        rgb.put_pixel(x + w - 1, y + h - 1, image::Rgb([255, 255, 0]));
        grab
    }

    /// The PC panel's OCR lines (`[724,554,500,430]` at ui 0.90) carried onto
    /// `panel` at `scale`, so a placed read at that panel and scale hits.
    fn pc_lines_at(panel: [i32; 4], scale: f32) -> Vec<OcrLineBox> {
        let ratio = scale / 0.90;
        cellfit::pc_lines()
            .into_iter()
            .map(|line| {
                let centre_y = line.y + line.h / 2;
                geometry::test_line(
                    &line.text,
                    panel[0] + ((line.x - 724) as f32 * ratio).round() as i32,
                    panel[1] + ((centre_y - 554) as f32 * ratio).round() as i32,
                )
            })
            .collect()
    }

    /// The Auto fit a session held before the player switched to Manual.
    fn held_fit_0_9() -> FittedScale {
        FittedScale { scale: 0.9, cell_px: 36, x0_offset: 0.0, pitch: 43.8, source: FitSource::Grid, pending: None }
    }

    /// Probe one press of `grab` under `state`'s placement, recording every
    /// image handed to OCR; OCR answers `lines` for the first image and
    /// nothing after.
    fn probe_under(
        state: &crate::AppState,
        session: &mut Session,
        grab: &crate::capture::Capture,
        key: FallbackKey,
        lines: Vec<OcrLineBox>,
    ) -> (Option<MercProbe>, Vec<image::DynamicImage>) {
        let (placement, regions_manual) = press_placement(state, grab);
        let mut seen = Vec::new();
        let mut first = Some(lines);
        let probe = probe_press(session, None, grab, key, placement, regions_manual, &mut |image| {
            seen.push(image.clone());
            Ok(first.take().unwrap_or_default())
        })
        .expect("the probe's fake OCR does not fail");
        (probe, seen)
    }

    /// The first OCR'd image is exactly `grab` cut at `rect`.
    fn assert_ocr_read_crop(seen: &[image::DynamicImage], grab: &crate::capture::Capture, rect: [i32; 4]) {
        let first = seen.first().expect("the press ran OCR");
        assert_eq!(
            (first.width(), first.height()),
            (rect[2] as u32, rect[3] as u32),
            "the first OCR image is not the Manual panel crop {rect:?}"
        );
        let want = grab.image.crop_imm(rect[0] as u32, rect[1] as u32, rect[2] as u32, rect[3] as u32);
        assert!(first.as_bytes() == want.as_bytes(), "the first OCR image is not cut at {rect:?}");
    }

    fn publish_calls(effects: &RecordingDetection) -> usize {
        effects.registration.events.iter().filter(|&&event| event == "publish").count()
    }

    mod keep_happy {
        use super::*;

        /// WI-6 example: a Manual merc panel on a 2560x1440 capture with no
        /// screen slice is read through `placed_panel_crop` of that panel at
        /// its own scale 1.0.
        #[test]
        fn a_manual_merc_panel_on_a_null_slice_is_read_through_its_placed_crop() {
            let crop = geometry::placed_panel_crop(MANUAL_PANEL, 1.0, SCREEN_2560);
            let state = manual_merc_state(MANUAL_PANEL, None);
            let grab = marked_capture_2560(crop);
            let mut session = test_session();

            let (_probe, seen) = probe_under(&state, &mut session, &grab, FallbackKey { refit: 0, press: 1 }, Vec::new());

            assert_ocr_read_crop(&seen, &grab, crop);
        }
    }

    mod keep_critical {
        use super::*;

        /// A panel held from an earlier read does not move a Manual crop.
        /// Consequence if it fails: rows are read from a panel the player moved
        /// away from — a silently wrong verdict.
        #[test]
        fn a_stale_session_panel_does_not_override_the_manual_merc_panel() {
            let crop = geometry::placed_panel_crop(MANUAL_PANEL, 1.0, SCREEN_2560);
            let state = manual_merc_state(MANUAL_PANEL, None);
            let grab = marked_capture_2560(crop);
            let mut session = test_session();
            session.panel = Some([698, 615, 555, 477]);

            let (_probe, seen) = probe_under(&state, &mut session, &grab, FallbackKey { refit: 0, press: 1 }, Vec::new());

            assert_ocr_read_crop(&seen, &grab, crop);
        }

        /// R-A: a Recalibrate pressed while in Auto and not yet acted on buys
        /// no locate in Manual, and is marked seen so it cannot fire after a
        /// later switch back to Auto. Consequence if it fails: a full-screen
        /// locate moves the panel off the player's manual rect and banks it
        /// through `remember_anchor` — a silently wrong read that also rewrites
        /// the Auto geometry (irreversible for the stored slice).
        #[test]
        fn a_pending_recalibrate_does_not_locate_a_manual_placed_miss() {
            let grab = capture_2560();
            let mut session = test_session();
            session.refit_located = 0;
            let key = FallbackKey { refit: 1, press: 1 };
            let mut ocrs = 0;

            let probe = probe_press(&mut session, None, &grab, key, Some((MANUAL_PANEL, 1.0)), true, &mut |_| {
                ocrs += 1;
                Ok(Vec::new())
            })
            .expect("the probe's fake OCR does not fail");

            assert!(probe.is_none(), "an empty placed read is a miss");
            assert_eq!(ocrs, 1, "only the placed crop is read: no full-screen locate (TrustPlacement, not ManualMiss)");
            assert!(session.fallback.clone().take(key), "the key's locate budget is untaken");
            assert_eq!(session.refit_located, 1, "the pending Recalibrate is marked seen");
        }

        /// R-B: a Manual panel's own scale wins over the Auto fit the session
        /// still holds, and the held fit is left as it was. Consequence if it
        /// fails: rows and cells are cut at the Auto fit's scale — a silently
        /// wrong verdict.
        #[test]
        fn a_manual_merc_scale_wins_over_a_held_auto_fit() {
            let grab = capture_2560();
            let mut session = test_session();
            session.fitted = Some(held_fit_0_9());
            let key = FallbackKey { refit: 0, press: 1 };
            let mut lines = Some(pc_lines_at(MANUAL_PANEL_1_2, MANUAL_SCALE_1_2));

            let probe = probe_press(&mut session, None, &grab, key, Some((MANUAL_PANEL_1_2, MANUAL_SCALE_1_2)), true,
                &mut |_| Ok(lines.take().unwrap_or_default()))
                .expect("the probe's fake OCR does not fail");
            let Some(MercProbe::Panel(location)) = probe else {
                panic!("the Manual panel's placed read at scale 1.2 must hit");
            };
            let mut effects = RecordingDetection::default();
            let registered = register_geometry(&mut effects, &mut session, *location);

            let layout = &registered.location.layout;
            assert!((layout.scale - MANUAL_SCALE_1_2).abs() < 1e-4, "layout scale {} is not the Manual 1.2", layout.scale);
            assert!(
                (layout.row_pitch - 49.3 * MANUAL_SCALE_1_2).abs() < 1e-3,
                "row pitch {} is not 49.3 x 1.2",
                layout.row_pitch
            );
            assert_eq!(session.fitted, Some(held_fit_0_9()), "the held Auto fit is unchanged");
        }

        /// A Manual press publishes no screen measurement over the Auto slice.
        /// Consequence if it fails: the manual scale silently overwrites the
        /// Auto measurement — and with it the Auto ghost, "Reset to Auto" and
        /// every read after a switch back to Auto — and is persisted to
        /// settings.json (irreversible for the stored slice).
        #[test]
        fn a_manual_merc_press_does_not_publish_the_screen() {
            let mut slice = crate::ssot::screen_from_geometry(2560, 1440, 1, (0, 0), [0, 0, 2560, 1440], 0);
            slice.ui_scale = 0.9;
            let state = manual_merc_state(MANUAL_PANEL_1_2, Some(slice));
            let grab = capture_2560();
            let mut session = test_session();

            let (probe, _seen) = probe_under(&state, &mut session, &grab, FallbackKey { refit: 0, press: 1 },
                pc_lines_at(MANUAL_PANEL_1_2, MANUAL_SCALE_1_2));
            let Some(MercProbe::Panel(location)) = probe else {
                panic!("the Manual panel's placed read at scale 1.2 must hit");
            };
            let mut effects = RecordingDetection::default();
            register_geometry(&mut effects, &mut session, *location);

            assert_eq!(publish_calls(&effects), 0, "registration events {:?}", effects.registration.events);
        }

        /// R-B round 5: registration obeys the mode the probe carried on
        /// `CaptureLocation`, never the mode at registration time. The location
        /// is the one a Manual probe builds (`regions_manual: true`, placement
        /// scale 1.2); the fake effects stand for an app already switched back
        /// to Auto. Consequence if it fails: one press measured at the manual
        /// scale is published as a `MercOcr` slice and persisted, silently
        /// overwriting the Auto measurement in settings.json (irreversible for
        /// the stored slice).
        #[test]
        fn a_mode_flip_after_a_manual_probe_does_not_publish_the_screen() {
            let g = MercGeometry::default();
            let pitch = g.row_pitch * MANUAL_SCALE_1_2;
            let layout = geometry::placed_layout(
                &pc_lines_at(MANUAL_PANEL_1_2, MANUAL_SCALE_1_2),
                MANUAL_PANEL_1_2,
                &g,
                MANUAL_SCALE_1_2,
                pitch,
            )
            .expect("the Manual panel's placed layout at scale 1.2");
            let crop = geometry::placed_panel_crop(MANUAL_PANEL_1_2, MANUAL_SCALE_1_2, SCREEN_2560);
            let image = image::DynamicImage::new_rgb8(SCREEN_2560[0], SCREEN_2560[1]);
            let cropped = image.crop_imm(crop[0] as u32, crop[1] as u32, crop[2] as u32, crop[3] as u32);
            let location = CaptureLocation {
                image,
                cropped: Some(cropped),
                screen: SCREEN_2560,
                grabbed_on: (1, (0, 0), [0, 0, 2560, 1440]),
                crop: Some(crop),
                placement: Some((MANUAL_PANEL_1_2, MANUAL_SCALE_1_2)),
                full_frame: false,
                frame: geometry::Frame::cropped((crop[0], crop[1]), SCREEN_2560),
                layout,
                regions_manual: true,
                ..test_location()
            };
            let mut session = test_session();
            session.fitted = Some(held_fit_0_9());
            let mut effects = RecordingDetection::default();

            let registered = register_geometry(&mut effects, &mut session, location);

            assert_eq!(publish_calls(&effects), 0, "registration events {:?}", effects.registration.events);
            assert_eq!(session.fitted, Some(held_fit_0_9()), "the held Auto fit is unchanged");
            assert!(
                (registered.location.layout.scale - MANUAL_SCALE_1_2).abs() < 1e-4,
                "layout scale {} is not the Manual 1.2",
                registered.location.layout.scale
            );
        }
    }
}
