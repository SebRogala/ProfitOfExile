//! Staged mercenary detection pipeline.
//!
//! `run` owns cadence, session lifetime and hover work. This child owns one
//! detection's capture/location, registration, read and reconciliation stages.

use std::time::Instant;

use tauri::AppHandle;
use tokio::sync::watch;

use super::*;
use super::super::read;

type GrabbedOn = (u32, (i32, i32), [i32; 4]);
type PlacedReadData = (
    ([i32; 4], f32),
    Vec<geometry::OcrLineBox>,
    Result<geometry::MercLayout, geometry::DetectMiss>,
);

struct CaptureLocation {
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
    result: read::ReadResult,
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
    probed_lines: Option<Vec<geometry::OcrLineBox>>,
    view: &image::DynamicImage,
    frame: geometry::Frame,
    merc_geometry: &MercGeometry,
    fitted_scale: Option<f32>,
    fitted_pitch: f32,
    recognize: O,
) -> Result<Option<PlacedReadData>, E>
where
    O: FnOnce(&image::DynamicImage) -> Result<Vec<geometry::OcrLineBox>, E>,
{
    let Some((panel, scale)) = placement else {
        return Ok(None);
    };
    let lines = match probed_lines {
        Some(lines) => lines,
        None => frame.to_screen(recognize(view)?),
    };
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

fn register_screen<S: RegistrationEffects>(
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

struct ReadStageContext<'app, 'session> {
    app: &'app AppHandle,
    session: &'session mut Session,
    cursor: Option<(i32, i32)>,
    registered: Option<RegisteredDetection>,
    first_look_tick: bool,
}

fn registered_then_read<C, T, R>(
    context: &mut C,
    register: impl FnOnce(&mut C) -> T,
    read: impl FnOnce(&mut C, T) -> Option<R>,
) -> Option<R> {
    let registered = register(context);
    read(context, registered)
}

fn read_stage_gate<C, R>(
    cancel: &watch::Receiver<bool>,
    context: &mut C,
    first_look: impl FnOnce(&mut C) -> Option<MercCapture>,
    publish_first_look: impl FnOnce(MercCapture),
    execute_read: impl FnOnce(&mut C) -> R,
) -> Option<R> {
    if *cancel.borrow() {
        return None;
    }
    if let Some(first_look) = first_look(context) {
        publish_first_look(first_look);
    }
    Some(execute_read(context))
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

/// One detect tick: grab the screen, OCR it, and publish what it holds.
///
/// `cursor` is the loop's ONE read for this iteration, taken before the hover
/// confirm and so before this grab — which is what the header-withholding rule
/// below needs it to be. It is up to one hover-OCR older than the frame it
/// judges, and that is the honest trade against a second read: two reads inside
/// one iteration let the hold decision and the withholding decision disagree
/// about where the cursor is, and a disagreement there publishes a tooltip's
/// text as the mercenary's name.
///
/// `grabbed` is the frame a probe has already taken this iteration. Passing it
/// in is what makes a probe hit cost one grab rather than two, and — the part
/// that matters — what makes the detect read the SAME pixels the probe accepted
/// on. Re-grabbing would leave a window that closed in the millisecond between
/// them looking like a probe that lied.
///
/// `gate` is the step this tick was served for — a Scan now
/// (`GateStep::FullDetect`) is one half of what makes a tick manual
/// ([`manual_tick`]), and so of whether a placed miss may buy the full-screen
/// locate ([`locate_decision`]).
pub(super) fn detect_tick(
    app: &AppHandle,
    session: &mut Session,
    cursor: Option<(i32, i32)>,
    cancel: &watch::Receiver<bool>,
    grabbed: Option<crate::capture::Capture>,
    probed_lines: Option<Vec<geometry::OcrLineBox>>,
    tick_started: Option<Instant>,
    gate: trigger::GateStep,
) -> DetectTick {
    let location = match capture_location(
        app,
        session,
        cursor,
        grabbed,
        probed_lines,
        tick_started,
        gate,
    ) {
        Ok(location) => location,
        Err(tick) => return tick,
    };
    let full_frame = location.full_frame;
    let Some(read) = registered_then_read(
        session,
        |session| register_geometry(app, session, location),
        |session, registered| execute_read_plan(app, session, cursor, cancel, registered),
    ) else {
        return detect_report(None, full_frame);
    };
    reconcile_and_publish(app, session, cursor, read)
}

fn capture_location(
    app: &AppHandle,
    session: &mut Session,
    cursor: Option<(i32, i32)>,
    grabbed: Option<crate::capture::Capture>,
    probed_lines: Option<Vec<geometry::OcrLineBox>>,
    tick_started: Option<Instant>,
    gate: trigger::GateStep,
) -> Result<CaptureLocation, DetectTick> {

    let started = tick_started.unwrap_or_else(Instant::now);
    let grab = match grabbed {
        Some(grab) => grab,
        None => match crate::capture::capture_screen(app) {
            Ok(grab) => grab,
            Err(e) => {
                fail(app, session, format!("Merc: screen capture failed — {e}"));
                return Err(detect_report(Some(miss(app, session, true)), true));
            }
        },
    };
    // The display travels with the pixels, whether this tick grabbed them or a
    // probe did (POE-237): both halves come off one `Capture`, so the id
    // published below always names the monitor the scale was measured on.
    let grabbed_on = (grab.monitor_id, grab.origin, grab.client);
    let img = grab.image;
    let (iw, ih) = {
        use image::GenericImageView;
        img.dimensions()
    };
    let screen = [iw, ih];
    // Before ANY remembered geometry is read (POE-227). `iw`/`ih` are the whole
    // grab's dimensions even on a cropped tick — the platform layer captures a
    // monitor, and only the OCR view is narrowed below — so this is the screen's
    // real size on every path. A scale remembered from another monitor is
    // dropped here, which is the first moment anything in the app can tell.
    crate::ssot::drop_if_mismatched(app, (iw, ih), grabbed_on.1, grabbed_on.2);

    let placement = merc_placement(app);
    let crop = placement.map(|(panel, scale)| geometry::placed_panel_crop(panel, scale, screen));
    let mut full_frame = crop.is_none();
    let key = fallback_key(app);
    // Off the key's own counter read, so the verdict and the budget it may
    // spend are about the same Recalibrate.
    let manual = manual_tick(gate, session.refit_located, key.refit);

    let cropped = crop.map(|r| img.crop_imm(r[0] as u32, r[1] as u32, r[2] as u32, r[3] as u32));
    let mut view: &image::DynamicImage = cropped.as_ref().unwrap_or(&img);
    let mut frame = match crop {
        Some(r) => geometry::Frame::cropped((r[0], r[1]), screen),
        None => geometry::Frame::full(screen),
    };
    let known_panel = session.panel.or_else(|| placement.map(|(panel, _)| panel));

    // A held fit's pitch is the frame's horizontal slot pitch, reused as the
    // vertical proxy because it is the only frame-verified length the session
    // holds: the slot pitch it holds is `REF_PITCH · scale` in screen px, and
    // 48.67 in reference space is closer to the fixture's 48.4 vertical row
    // pitch than 49.3 is. POE-216 resolves this axis crossing.
    let fitted_pitch = session
        .fitted
        .map(|fit| fit.pitch)
        .unwrap_or_else(|| placement.map_or(session.geometry.row_pitch, |(_, scale)| session.geometry.row_pitch * scale));
    // THE PLACED READ, on the crop: the probe's own lines when a probe took
    // them this iteration, so a probed tick pays no second OCR whatever the
    // probe saw. `None` on a cold start, which has no rect to crop and reads
    // nothing before the locate below.
    //
    // TRANSLATED THE INSTANT IT COMES BACK. Windows OCR reports boxes in the
    // pixels it was handed, and every rule below this line — the known-panel
    // anchor, the column-x test, the cell rects the hover tick hit-tests the
    // real cursor against — is screen-absolute. See `geometry::Frame`.
    let placed_read = match placed_read_with(
        placement,
        probed_lines,
        view,
        frame,
        &session.geometry,
        session.fitted.map(|fit| fit.scale),
        fitted_pitch,
        crate::ocr::recognize_lines,
    ) {
        Ok(placed_read) => placed_read,
        Err(e) => {
            fail(app, session, format!("Merc: OCR failed — {e}"));
            return Err(detect_report(Some(miss(app, session, true)), full_frame));
        }
    };

    // Whether the full-screen locate runs is [`locate_decision`]'s call, the
    // whole rule in one place. When it does, it re-reads the SAME grab, so it
    // is one more OCR rather than a second screen capture.
    let decision = locate_decision(
        placed_read.as_ref().map(|(placed, _, layout)| (*placed, PlacedRead::of(layout))),
        manual,
        &mut session.fallback,
        key,
    );
    let (located_by, lines, layout) = match (decision, placed_read) {
        (LocateDecision::Locate(reason), _) => {
            full_frame = true;
            view = &img;
            frame = geometry::Frame::full(screen);
            let lines = match crate::ocr::recognize_lines(view) {
                Ok(lines) => frame.to_screen(lines),
                Err(e) => {
                    fail(app, session, format!("Merc: OCR failed — {e}"));
                    return Err(detect_report(Some(miss(app, session, true)), full_frame));
                }
            };
            let layout =
                geometry::detect_reason(&lines, &session.geometry, &session.vocab, known_panel);
            (Some(reason), lines, layout)
        }
        (LocateDecision::TrustPlacementOverColumn, Some((_, lines, layout))) => {
            if let Some(line) = column_trusted_line(&mut session.column_trusted_said, key) {
                crate::app_log(app, line.to_string());
            }
            (None, lines, layout)
        }
        (LocateDecision::TrustPlacement, Some((_, lines, layout))) => (None, lines, layout),
        // No placement and no locate: this key's cold start is spent, and a
        // counted miss costs no OCR. (`locate_decision` answers the two trust
        // arms only for a placed read, so `(_, None)` is this same case.)
        (LocateDecision::ColdStartSpent, _) | (_, None) => {
            return Err(detect_report(Some(miss(app, session, false)), full_frame));
        }
    };
    let used_fallback = located_by.is_some();

    // Only a manual locate can move the panel or remember an origin: see
    // [`fallback_panel`], which answers nothing for a cold start.
    let mut located_panel = None;
    let mut remember_origin = None;
    if let Some(reason) = located_by {
        let (located_from_fallback, fallback_origin, contradiction) =
            fallback_panel(reason, &layout, &session.geometry);
        located_panel = located_from_fallback;
        remember_origin = fallback_origin;
        if let Some(line) = contradiction {
            crate::app_log(app, line);
            crate::app_log(app, MERC_GEOMETRY_NOTICE_LINE.to_string());
        }
    }

    let took = started.elapsed().as_millis();
    let how = frame.describe();

    let layout = match layout {
        Ok(layout) => layout,
        Err(why) => {
            // Every number the 2026-08-26 smoke wanted and did not have, on the
            // frame that lost the window: which rect the anchor was weighed
            // against, which frame the OCR ran on, how many skill names came back,
            // where their column sat, and which step of `detect_reason` returned.
            // Debug-gated because a miss is the ordinary state of a loop watching
            // an empty screen.
            if debug_mode(app) {
                crate::app_log(
                    app,
                    format!(
                        "Merc: no layout on the {how} frame — {why}; placed panel {:?}, cursor {:?}",
                        placement.map(|(panel, _)| panel).or(session.panel), cursor
                    ),
                );
            }
            // A tooltip the player just opened sits ON the panel and hides the rows
            // the detect needs. The cursor is the proof — the game opens one only
            // under it — so this tick is not evidence the window closed.
            // No layout means no rect from THIS frame; the session's is all there
            // is. See [`cursor_on_panel`].
            let in_panel = cursor_on_panel(
                placement.map(|(panel, _)| panel),
                session.panel,
                cursor,
            );
            let live = session.state.live;
            if session.occlusion.on_occluded(live, in_panel, Instant::now()) == MissKind::Occluded {
                if session.occlusion.announce() {
                    crate::app_log(
                        app,
                        "Merc: panel occluded (cursor over it) — holding the capture".to_string(),
                    );
                }
                return Err(detect_report(Some(DetectOutcome::Occluded), full_frame));
            }
            // Logged once per focus session: a loop that never detects would
            // otherwise leave no trace of having looked at all.
            if !session.miss_logged {
                session.miss_logged = true;
                let skills = lines
                    .iter()
                    .filter(|l| {
                        session.vocab.match_skill(&l.text, &session.geometry.thresholds).state
                            != ReadState::Unknown
                    })
                    .count();
                crate::app_log(
                    app,
                    format!(
                        "Merc: looked, no recruit window — {} OCR lines, {} skill candidates \
                         ({how} frame, {took} ms)",
                        lines.len(),
                        skills
                    ),
                );
            }
            return Err(detect_report(Some(miss(app, session, false)), full_frame));
        }
    };
    Ok(CaptureLocation {
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
    })
}

fn register_geometry(
    app: &AppHandle,
    session: &mut Session,
    location: CaptureLocation,
) -> RegisteredDetection {
    let screen = location.screen;
    let grabbed_on = location.grabbed_on;
    let frame = location.frame;
    let key = location.key;
    let layout = location.layout.clone();
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
    // read ([`manual_tick`]), which `refit_located` records just below; this
    // re-reads the counter AT THE FIT, which is what ssot's bump-before-write
    // ordering relies on.
    let refit = consume_refit(session, refit_counter(app));
    match &refit {
        Refit::NotRequested => {}
        Refit::Dropped(held) => crate::app_log(
            app,
            format!(
                "Merc: Recalibrate — dropped the settled frame registration (scale \
                 {:.3}); this tick re-measures the panel",
                held.scale
            ),
        ),
        Refit::NothingHeld => crate::app_log(
            app,
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
            if debug_mode(app) {
                crate::app_log(
                    app,
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
    // Armed, never cleared here: the merge that consumes it clears it, and a
    // second change landing before then would only re-arm what is already
    // armed.
    session.geometry_changed |=
        registration_changed(session.scale_source, layout.scale_source, adopted);
    session.scale_source = layout.scale_source;
    // POE-214 WI-B1: the settled scale is not merc-only knowledge. It is the
    // GAME UI's scale on this screen, and the Lab capture regions want it (as
    // fractions of the slice) exactly as much as the grid rects here do. So it
    // is published from the one place it is settled, off the SESSION's layout
    // rather than the raw fit — the same number `MercCapture.scale` carries a
    // few lines down, on the same tick, so the two can never disagree.
    //
    // No lock is held at this point: the only guard this tick opens is
    // `merc_templates`, around `build_capture` below. Called on every tick that
    // reads a panel — a tick with no recruit window returns well above here —
    // and `ssot::screen_changed` is what keeps a re-measurement of the same
    // screen from waking every overlay's poll.
    // Which cue this reports, why `Held` is a frame measurement and which cues
    // verify are `ssot`'s calls, made once in [`published_screen`]. The
    // timestamp is the tick's own clock read, a few ms before `build_capture`
    // takes its `captured_at_ms` from the same source: this is when the scale
    // was MEASURED, and publishing at the settle rather than after pass 2 keeps
    // a cancelled tick's measurement from being lost.
    let mut registration = AppRegistrationEffects { app };
    let (_screen_record, _published) = register_screen(
        &mut registration,
        &mut session.screen_refusals,
        screen,
        layout.scale,
        layout.scale_source,
        now_ms(),
        grabbed_on,
    );
    if debug_mode(app) {
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
                crate::app_log(
                    app,
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
                crate::app_log(app, format!("Merc: frame fit declined — {why}; {kept}"));
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

fn execute_read_plan(
    app: &AppHandle,
    session: &mut Session,
    cursor: Option<(i32, i32)>,
    cancel: &watch::Receiver<bool>,
    registered: RegisteredDetection,
) -> Option<ReadStage> {
    let first_look_tick = session.current.is_none();
    let mut context = ReadStageContext {
        app,
        session,
        cursor,
        registered: Some(registered),
        first_look_tick,
    };
    let mut publication = AppPublicationSink { app };
    // What follows is this round's read: pass 2 — at most `max_rows` more OCR
    // calls, and only for the rows the plan below reads — and the icon walk.
    // A stop signal that arrived during pass 1 stops here, leaving the capture
    // state as it was; the settled screen measurement already published and
    // persisted during registration remains.
    read_stage_gate(
        cancel,
        &mut context,
        |context| {
            // FIRST LOOK at a window: the rows go out NOW, before the read below,
            // which is the tick's expensive half (2.2 s on a six-row panel in the
            // 2026-09-06 log) and which the strip used to sit through saying
            // "scanning". Only when nothing is live: a re-read of a window already on
            // the slice would blank its icons for the length of every tick. The
            // header is pass 1's, unguarded — it is replaced by the folded one below
            // on this same tick. See [`MercCapture::partial`].
            context.first_look_tick.then(|| {
                let registered = context.registered.as_ref().expect("registered read stage");
                first_look(
                    &registered.location.layout,
                    registered.location.screen,
                    now_ms(),
                    &context.session.geometry,
                    &context.session.vocab,
                )
            })
        },
        |capture| publication.publish_first_look(capture),
        |context| {
    let registered = context.registered.take().expect("registered read stage");
    let app = context.app;
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
        generation_changed(&mut session.template_generation, template_generation(app));
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
        crate::app_log(app, "Merc: recruit window replaced — reading it fresh".to_string());
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
            seed::rederive_for_window(app, &session.geometry, layout.scale);
            seeds_ms = stage.elapsed().as_millis();
            let stage = Instant::now();
            let result = {
                let state = app.state::<AppState>();
                let store = state.merc_templates.lock().unwrap_or_else(|e| e.into_inner());
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
            };
            (result, stage.elapsed().as_millis(), planned.is_some())
        }
    };
    result.capture.rows_on_screen = result.rows_on_screen;
    result.capture.rows_read = result.rows_read;
    let row_mismatch = row_mismatch_line(result.rows_on_screen, result.rows_read);
    if session.row_mismatch_logged.as_deref() != row_mismatch.as_deref() {
        if let Some(line) = row_mismatch.as_deref() {
            crate::app_log(app, line.to_string());
        }
        session.row_mismatch_logged = row_mismatch;
    }
    // Where a slow tick went. The loop's own line says only that the tick was
    // slow; this one says which stage to look at. Always in debug mode, and
    // on every tick the backoff would call slow otherwise.
    let total_ms = started.elapsed().as_millis();
    if debug_mode(app) || total_ms >= SLOW_TICK.as_millis() {
        let cells: usize = result.capture.rows.iter().map(|row| row.supports.len()).sum();
        crate::app_log(
            app,
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
        crate::app_log(
            app,
            format!("Merc: detect on the crop frame took {} ms", started.elapsed().as_millis()),
        );
    }
    result.capture.header = published_header;


            ReadStage {
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
            }
        },
    )
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
        crate::app_log(
            app,
            format!(
                "Merc: capture complete — OCR paused (liveness every {} s)",
                LIVENESS_INTERVAL.as_secs()
            ),
        );
        // The settle edge OPENS a trade session if this capture has none yet
        // (POE-202). Here rather than at the first detect because a half-read
        // panel builds a query for a mercenary nobody has, and each of those
        // would cost one of three searches.
        //
        // `get_or_insert_with`, never a fresh session: `note_complete` is a
        // rising edge, but `LoopState::resume` drops `complete` whenever a Scan
        // now arms over a finished window (a voice line cannot reach it:
        // `trigger::capture_held`), so one capture crosses this edge as often
        // as the player triggers a re-read. A new session per edge would hand
        // that capture a new 3-search budget each time, which is unbounded
        // searching dressed up as a ceiling. ONE session per capture: opened
        // here, cleared only by the retire in [`miss`].
        //
        // A capture whose rounds ran out incomplete opens none here. If a hover
        // completes it later, the next liveness tick's carried capture is
        // complete and crosses this edge then.
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
        let spent = session.state.note_round();
        crate::app_log(
            app,
            round_line(
                session.state.rounds,
                &plan,
                pass2_ms,
                icons_ms,
                complete,
                session.state.rounds_left(),
            ),
        );
        if spent && !complete {
            crate::app_log(app, rounds_spent_line(&result.capture));
        }
    }
    let mut publication = AppPublicationSink { app };
    publish_settled(&mut publication, result.capture, complete, remember_origin);
    detect_report(Some(outcome), full_frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    struct RecordingPublication {
        events: Vec<&'static str>,
        capture: Option<MercCapture>,
        complete: Option<bool>,
        origin: Option<[i32; 2]>,
    }

    impl PublicationSink for RecordingPublication {
        fn publish_first_look(&mut self, capture: MercCapture) {
            self.events.push("first-look");
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

    #[test]
    fn placed_capture_reuses_supplied_probe_lines() {
        let image = image::DynamicImage::new_rgba8(200, 200);
        let frame = geometry::Frame::full([200, 200]);
        let geometry = MercGeometry::default();
        let probe_lines = vec![geometry::OcrLineBox {
            text: "Ice Shot".into(),
            x: 20,
            y: 40,
            w: 60,
            h: 16,
        }];
        let ocr_calls = std::cell::Cell::new(0);

        let placed = placed_read_with(
            Some(([0, 0, 200, 200], 1.0)),
            Some(probe_lines.clone()),
            &image,
            frame,
            &geometry,
            None,
            geometry.row_pitch,
            |_| {
                ocr_calls.set(ocr_calls.get() + 1);
                Err("probe lines should avoid a second OCR")
            },
        )
        .expect("supplied lines do not invoke OCR")
        .expect("a placed capture has a placed read");

        assert_eq!(ocr_calls.get(), 0);
        assert_eq!(placed.1, probe_lines);
    }

    #[test]
    fn cancelled_read_preserves_registered_screen() {
        let (_tx, cancel) = watch::channel(true);
        let mut registration = RecordingRegistration::default();
        let mut refusals = OnceLog::default();
        let grabbed_on = (131_074, (-1920, 0), [11, 22, 333, 444]);
        let later_publication = RefCell::new(RecordingPublication::default());
        let mut registration_context = ();

        let read = registered_then_read(
            &mut registration_context,
            |_| {
                let (_, screen) = register_screen(
                    &mut registration,
                    &mut refusals,
                    [1920, 1080],
                    1.0,
                    ScaleSource::Frame,
                    1_724_000_000_000,
                    grabbed_on,
                );
                screen
            },
            |_, _screen| {
                let mut context = ();
                read_stage_gate(
                    &cancel,
                    &mut context,
                    |_| Some(test_capture([1920, 1080])),
                    |capture| later_publication.borrow_mut().publish_first_look(capture),
                    |_| {
                        let mut publication = later_publication.borrow_mut();
                        publication.events.push("full-read");
                        publish_settled(&mut *publication, test_capture([1920, 1080]), true, None);
                    },
                )
            },
        );

        let screen = registration
            .screen
            .expect("registered screen was published");
        assert_eq!(
            (screen.monitor_id, screen.origin, screen.client),
            grabbed_on
        );
        assert_eq!((screen.width, screen.height), (1920, 1080));
        let persisted = registration.persisted.expect("frame screen was persisted");
        assert_eq!((persisted.monitor_id, persisted.origin, persisted.client), grabbed_on);
        assert_eq!((persisted.width, persisted.height), (1920, 1080));
        assert_eq!(registration.events, vec!["publish", "persist"]);
        assert!(read.is_none());
        assert!(later_publication.into_inner().events.is_empty());
    }

    #[test]
    fn first_look_precedes_the_full_read() {
        let (_tx, cancel) = watch::channel(false);
        let publication = RefCell::new(RecordingPublication::default());

        let read = read_stage_gate(
            &cancel,
            &mut (),
            |_| Some(test_capture([1920, 1080])),
            |capture| publication.borrow_mut().publish_first_look(capture),
            |_| {
                publication.borrow_mut().events.push("full-read");
                7u8
            },
        );

        assert_eq!(read, Some(7));
        assert_eq!(publication.into_inner().events, vec!["first-look", "full-read"]);
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
}
