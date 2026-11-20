//! The temple's reader on the app-wide read key (POE-389).
//!
//! The router prepares the installed worker and snapshots Client.txt context,
//! geometry and epoch. Recognition retains an anchor from the pressed frame;
//! only a selected match reaches the worker, with no invented Client.txt reason.
//! Policy: docs/OVERLAY-GUIDE.md § "Screen reading (GGG policy)".

use std::sync::mpsc::Sender;
use std::sync::Arc;

use tauri::{AppHandle, Manager};

use super::trigger::{ArmReason, ArmState};
use crate::capture::Capture;
use crate::read_hotkey::{PendingRead, Press, ScreenReader};

/// One pressed frame (or the grab's error), with the Client.txt reason the
/// press was armed under.
pub struct PressedFrame {
    pub frame: Result<Arc<Capture>, String>,
    pub reason: Option<ArmReason>,
    pub probe: Option<super::run::TempleProbe>,
    pub epoch: u64,
    /// The Incursion prompt arm live when the press was accepted; its read
    /// clears only that arm.
    pub prompt: Option<u64>,
}

/// What the temple reader needs from the app at press time.
pub trait PressContext {
    fn epoch(&self) -> u64 { 0 }
    fn recent(&self) -> Option<crate::read_prompt::Mechanic> { None }
    fn screen(&self) -> Option<crate::ssot::ScreenSlice> { None }
    /// The temple worker's inbox, `None` while the module is off or has no OCR
    /// engine.
    fn press_sender(&self) -> Option<Sender<PressedFrame>>;
    /// The Client.txt arm, copied out.
    fn arm_state(&self) -> ArmState;
    /// The Incursion prompt's live arm id, `None` when it shows none.
    fn prompt_arm(&self) -> Option<u64> {
        None
    }
    /// The screen owner reconciled against the pressed frame (POE-440).
    fn reconcile_screen(&self, _frame: &Capture) -> Option<crate::ssot::ScreenSlice> {
        self.screen()
    }
    /// Banks a calibration's Entrance origin into the screen it was found on;
    /// `false` when nothing was written.
    fn bank_entrance(&self, _origin: [i32; 2], _frame: &Capture, _ui_scale: f32) -> bool {
        false
    }
    /// Whether this frame's screen setup already spent its one sweep.
    fn sweep_spent(&self, _frame: &Capture) -> bool {
        false
    }
    /// This frame's sweep key under the live Recalibrate generation.
    fn sweep_key(&self, frame: &Capture) -> SweepKey {
        sweep_key(frame, 0)
    }
    /// Records that `key`'s screen setup spent its one sweep.
    fn spend_sweep(&self, _key: SweepKey) {}
    /// Drops a spent sweep recorded for a screen setup other than this frame's.
    fn forget_other_setup(&self, _frame: &Capture) {}
}

/// One screen setup for the sweep budget: capture size, display origin, client
/// size and the Recalibrate generation. A change in any allows one more sweep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SweepKey {
    capture: (u32, u32),
    display: (i32, i32),
    client_size: (i32, i32),
    rearm: u64,
}

pub(crate) fn sweep_key(frame: &Capture, rearm: u64) -> SweepKey {
    SweepKey {
        capture: (frame.image.width(), frame.image.height()),
        display: frame.origin,
        client_size: (frame.client[2], frame.client[3]),
        rearm,
    }
}

/// Clears `spent` when it belongs to a screen setup other than `frame`'s.
pub(crate) fn forget_other_setup(spent: &mut Option<SweepKey>, frame: &Capture) {
    if spent.is_some_and(|key| key != sweep_key(frame, key.rearm)) {
        *spent = None;
    }
}

impl PressContext for AppHandle {
    fn epoch(&self) -> u64 {
        self.state::<crate::AppState>().temple_state.epoch.load(std::sync::atomic::Ordering::SeqCst)
    }
    fn recent(&self) -> Option<crate::read_prompt::Mechanic> {
        crate::read_prompt::last_client_mechanic_in(&self.state::<crate::AppState>())
    }
    fn screen(&self) -> Option<crate::ssot::ScreenSlice> {
        *self.state::<crate::AppState>().screen.lock().unwrap_or_else(|e| e.into_inner())
    }
    fn press_sender(&self) -> Option<Sender<PressedFrame>> {
        let state = self.state::<crate::AppState>();
        if state.modules_enabled.lock().unwrap_or_else(|e| e.into_inner()).get("temple") != Some(&true) {
            return None;
        }
        let tx = state.temple_state.press.lock().unwrap_or_else(|e| e.into_inner()).clone();
        tx
    }
    fn arm_state(&self) -> ArmState {
        super::trigger::arm_state(self)
    }
    fn prompt_arm(&self) -> Option<u64> {
        crate::read_prompt::armed_id_in(&self.state::<crate::AppState>(), crate::read_prompt::Mechanic::Incursion)
    }
    fn reconcile_screen(&self, frame: &Capture) -> Option<crate::ssot::ScreenSlice> {
        crate::ssot::drop_if_mismatched(self, (frame.image.width(), frame.image.height()), frame.origin, frame.client);
        if let Some(screen) = self.screen() { return Some(screen); }
        crate::ssot::publish_screen(self, crate::ssot::screen_from_capture(frame));
        self.screen()
    }
    fn bank_entrance(&self, origin: [i32; 2], frame: &Capture, ui_scale: f32) -> bool {
        crate::ssot::remember_anchor_if_current(self, crate::ssot::AnchorModule::TempleEntrance, origin, frame, ui_scale)
    }
    fn sweep_spent(&self, frame: &Capture) -> bool {
        let key = PressContext::sweep_key(self, frame);
        let spent = *self.state::<crate::AppState>().temple_state.sweep_spent.lock().unwrap_or_else(|e| e.into_inner()) == Some(key);
        spent
    }
    fn sweep_key(&self, frame: &Capture) -> SweepKey {
        sweep_key(frame, self.state::<crate::AppState>().temple_state.rearm.load(std::sync::atomic::Ordering::SeqCst))
    }
    fn spend_sweep(&self, key: SweepKey) {
        *self.state::<crate::AppState>().temple_state.sweep_spent.lock().unwrap_or_else(|e| e.into_inner()) = Some(key);
    }
    fn forget_other_setup(&self, frame: &Capture) {
        forget_other_setup(&mut self.state::<crate::AppState>().temple_state.sweep_spent.lock().unwrap_or_else(|e| e.into_inner()), frame);
    }
}

/// The reason a press reads the temple at `now_ms`, or `None` when Client.txt
/// puts no temple in scope.
pub fn press_reason(arm: ArmState, now_ms: u64) -> Option<ArmReason> {
    arm.arm.reason().filter(|_| arm.arm.is_armed(now_ms))
}

/// The temple's entry in `crate::read_hotkey::READERS`.
pub struct TempleReader;
pub static TEMPLE_READER: TempleReader = TempleReader;

/// Prepared temple context and its retained recognition evidence.
pub struct TemplePending {
    tx: Sender<PressedFrame>,
    reason: Option<ArmReason>,
    prompt: Option<u64>,
    screen: Option<crate::ssot::ScreenSlice>,
    probe: Option<super::run::TempleProbe>,
    epoch: u64,
    reconciled: bool,
}

impl<C: PressContext> ScreenReader<C> for TempleReader {
    fn id(&self) -> crate::read_hotkey::ReaderId { crate::read_hotkey::ReaderId::Temple }
    fn recent(&self, ctx: &C) -> Option<crate::read_prompt::Mechanic> { ctx.recent() }
    fn prepare(&self, ctx: &C, _press: &Press) -> Option<Box<dyn PendingRead<C>>> {
        let tx = ctx.press_sender()?;
        let reason = press_reason(ctx.arm_state(), super::run::now_ms());
        Some(Box::new(TemplePending { tx, reason, prompt: ctx.prompt_arm(), screen: ctx.screen(), probe: None, epoch: ctx.epoch(), reconciled: false }))
    }
}

impl TemplePending {
    /// Reconciles the screen owner once per press and drops another setup's
    /// spent sweep.
    fn reconcile<C: PressContext>(&mut self, frame: &Capture, ctx: &C) {
        if self.reconciled { return; }
        self.reconciled = true;
        self.screen = ctx.reconcile_screen(frame);
        ctx.forget_other_setup(frame);
    }
}

impl<C: PressContext> PendingRead<C> for TemplePending {
    fn needs_calibration(&mut self, frame: &Arc<Capture>, ctx: &C) -> bool {
        let stopped = ctx.epoch() != self.epoch || ctx.press_sender().is_none();
        !stopped && !super::run::is_calibrated(frame, self.screen) && !ctx.sweep_spent(frame)
    }
    fn calibrate(&mut self, frame: &Arc<Capture>, ctx: &C) -> Option<String> {
        let started = std::time::Instant::now();
        self.reconcile(frame, ctx);
        if super::run::is_calibrated(frame, self.screen) { return None; }
        let stop = || ctx.epoch() != self.epoch || ctx.press_sender().is_none();
        let key = ctx.sweep_key(frame);
        let found = super::run::calibrate_frame(frame, self.screen, &stop);
        if stop() {
            return Some(format!("Temple: calibration — cancelled, {} ms", started.elapsed().as_millis()));
        }
        ctx.spend_sweep(key);
        let found = match found {
            Ok(found) => found,
            Err(e) => return Some(format!("Temple: calibration — failed ({e:?}), {} ms", started.elapsed().as_millis())),
        };
        let Some(found) = found else {
            return Some(format!("Temple: calibration — no Entrance plate on this frame, {} ms", started.elapsed().as_millis()));
        };
        if ctx.sweep_key(frame) != key {
            return Some(format!("Temple: calibration — superseded by Recalibrate, {} ms", started.elapsed().as_millis()));
        }
        let (x, y) = found.origin();
        if let Some(screen) = self.screen { ctx.bank_entrance([x, y], frame, screen.ui_scale); }
        self.probe = Some(found);
        Some(format!("Temple: calibration — Entrance found at ({x}, {y}), {} ms", started.elapsed().as_millis()))
    }
    fn probe(&mut self, frame: &Arc<Capture>, ctx: &C) -> Result<bool, String> {
        self.reconcile(frame, ctx);
        if self.probe.is_some() {
            if ctx.epoch() != self.epoch || ctx.press_sender().is_none() { self.probe = None; return Ok(false); }
            return Ok(true);
        }
        self.probe = super::run::probe_frame(frame, self.screen, &|| ctx.epoch() != self.epoch || ctx.press_sender().is_none()).map_err(|e| format!("{e:?}"))?;
        Ok(self.probe.is_some())
    }
    fn consume(self: Box<Self>, frame: Result<&Arc<Capture>, &str>, _ctx: &C) {
        let frame = frame.map(Arc::clone).map_err(str::to_string);
        if self.tx.send(PressedFrame { frame, reason: self.reason, prompt: self.prompt, probe: self.probe, epoch: self.epoch }).is_err() {
            log::warn!("temple: read worker gone, pressed frame dropped");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::GrabTiming;
    use crate::read_hotkey::{run_dispatcher, run_sampler, ReadHotkeySettings, Sample};
    use crate::temple::run::worker_loop;
    use crate::temple::trigger::TempleArm;
    use std::sync::mpsc::{self, Receiver};
    use std::sync::Mutex;

    /// A fake app: the worker inbox the module would install, the arm and the
    /// screen owner.
    struct World {
        sender: Mutex<Option<Sender<PressedFrame>>>,
        arm: Mutex<ArmState>,
        prompt: Mutex<Option<u64>>,
        slot: Mutex<Option<crate::ssot::ScreenSlice>>,
        /// A slice that replaces the owner right after reconciliation, standing
        /// in for a geometry recalibration during the sweep.
        replaced_by: Mutex<Option<crate::ssot::ScreenSlice>>,
        sweep: Mutex<Option<SweepKey>>,
        rearm: Mutex<u64>,
        rearm_during_sweep: Mutex<Option<()>>,
        sweeping: Mutex<bool>,
    }
    impl PressContext for World {
        fn screen(&self) -> Option<crate::ssot::ScreenSlice> {
            *self.slot.lock().unwrap()
        }
        fn press_sender(&self) -> Option<Sender<PressedFrame>> {
            self.sender.lock().unwrap().clone()
        }
        fn arm_state(&self) -> ArmState {
            *self.arm.lock().unwrap()
        }
        fn prompt_arm(&self) -> Option<u64> {
            *self.prompt.lock().unwrap()
        }
        /// Mirrors `ssot::drop_if_mismatched` + `screen_from_capture`.
        fn reconcile_screen(&self, frame: &Capture) -> Option<crate::ssot::ScreenSlice> {
            let mut slot = self.slot.lock().unwrap();
            crate::ssot::reconcile_screen(&mut slot, (frame.image.width(), frame.image.height()), frame.origin, frame.client);
            if slot.is_none() { *slot = Some(crate::ssot::screen_from_capture(frame)); }
            let reconciled = *slot;
            if let Some(next) = self.replaced_by.lock().unwrap().take() { *slot = Some(next); }
            reconciled
        }
        /// The same operation `AppHandle` runs through `remember_anchor_if_current`.
        fn bank_entrance(&self, origin: [i32; 2], frame: &Capture, ui_scale: f32) -> bool {
            crate::ssot::set_anchor_if_current(
                &mut self.slot.lock().unwrap(), (frame.image.width(), frame.image.height()), frame.origin, frame.client,
                ui_scale, crate::ssot::AnchorModule::TempleEntrance, origin,
            )
        }
        fn sweep_spent(&self, frame: &Capture) -> bool {
            *self.sweep.lock().unwrap() == Some(sweep_key(frame, *self.rearm.lock().unwrap()))
        }
        fn sweep_key(&self, frame: &Capture) -> SweepKey {
            let key = sweep_key(frame, *self.rearm.lock().unwrap());
            if self.rearm_during_sweep.lock().unwrap().take().is_some() { *self.sweeping.lock().unwrap() = true; }
            key
        }
        fn spend_sweep(&self, key: SweepKey) {
            *self.sweep.lock().unwrap() = Some(key);
        }
        fn forget_other_setup(&self, frame: &Capture) {
            forget_other_setup(&mut self.sweep.lock().unwrap(), frame);
        }
        /// Stands in for a Recalibrate pressed while the sweep runs: the first
        /// stop check after the sweep took its key bumps the generation once.
        fn epoch(&self) -> u64 {
            if std::mem::take(&mut *self.sweeping.lock().unwrap()) { *self.rearm.lock().unwrap() += 1; }
            0
        }
    }

    /// The 1920x1080 slice at UI scale 0.90, whose seed is the live origin.
    fn slice_1080(entrance: Option<[i32; 2]>) -> crate::ssot::ScreenSlice {
        crate::ssot::ScreenSlice {
            width: 1920, height: 1080, ui_scale: 0.90,
            source: crate::ssot::ScreenScaleSource::Remembered, measured_at_ms: 0,
            verified_this_session: false, monitor_id: 1, origin: (0,0), client: [0,0,1920,1080],
            anchors: entrance.map(|e| crate::ssot::Anchors { temple_entrance: Some(e), merc_panel: None }),
        }
    }

    fn armed(reason: ArmReason) -> ArmState {
        ArmState { arm: TempleArm::Armed { until_ms: None, reason }, ..ArmState::default() }
    }

    /// A world with the worker running under `arm`, and the worker's inbox.
    fn world(arm: ArmState) -> (World, Receiver<PressedFrame>) {
        let (tx, rx) = mpsc::channel();
        (World { sender: Mutex::new(Some(tx)), arm: Mutex::new(arm), prompt: Mutex::new(None),
            slot: Mutex::new(Some(slice_1080(None))), replaced_by: Mutex::new(None),
            sweep: Mutex::new(None), rearm: Mutex::new(0), rearm_during_sweep: Mutex::new(None), sweeping: Mutex::new(false) }, rx)
    }

    #[test]
    fn a_pressed_frame_carries_the_prompt_arm_live_at_the_press() {
        let (world, rx) = world(armed(ArmReason::AlvaStart));
        *world.prompt.lock().unwrap() = Some(4);
        let armed = crate::read_hotkey::prepare_press(Press { cursor: None }, Default::default(), &[&TEMPLE_READER], &world);
        *world.prompt.lock().unwrap() = Some(5);
        crate::read_hotkey::dispatch_press(armed, &world, &mut |_| {}, &mut || Ok(capture(1)), &mut |_| true);
        assert_eq!(rx.try_recv().unwrap().prompt, Some(4));
    }

    fn capture(monitor_id: u32) -> Capture {
        Capture {
            image: image::load_from_memory(include_bytes!("../../tests/fixtures/temple/screen-live-1920x1080.png")).unwrap(),
            monitor_id,
            origin: (0, 0),
            client: [0, 0, 1920, 1080],
            timing: GrabTiming::default(),
        }
    }

    fn sample(keys: &[u16]) -> Sample {
        Sample { down: keys.iter().copied().collect(), foreground: true, settings: ReadHotkeySettings::default(), dismiss: Vec::new() }
    }
    /// Ctrl+A, the default read key.
    const PRESSED: &[u16] = &[17, 65];

    /// The read key's sampler and dispatcher over `samples` with only the
    /// temple reader registered; returns the grab count.
    fn press_through_the_read_key(world: &World, samples: Vec<Sample>) -> u32 {
        let readers: &[&dyn ScreenReader<World>] = &[&TEMPLE_READER];
        let (queue, presses) = mpsc::channel();
        let mut samples = samples.into_iter();
        run_sampler(world, readers, &mut || samples.next(), &mut || None, &queue, &mut || None, &mut |_| 0);
        drop(queue);
        let mut grabs = 0;
        run_dispatcher(world, presses, &mut |_| {}, &mut || {
            grabs += 1;
            Ok(capture(grabs))
        }, &mut |_| true);
        grabs
    }

    /// Ends the world's inbox and drains it through the production receive
    /// path, recording each read's monitor id (or error) and reason.
    fn read_by_the_worker(world: World, rx: Receiver<PressedFrame>) -> Vec<(Result<u32, String>, Option<ArmReason>)> {
        drop(world);
        let mut reads = Vec::new();
        worker_loop(&rx, &|| false, &mut |pressed| {
            reads.push((pressed.frame.map(|c| c.monitor_id), pressed.reason));
        });
        reads
    }

    fn three_presses() -> Vec<Sample> {
        vec![sample(PRESSED), sample(&[]), sample(PRESSED), sample(&[]), sample(PRESSED)]
    }

    // ------------------------------------------------ the read key's grabs --

    #[test]
    fn idle_samples_under_a_temple_arm_grab_nothing_and_read_nothing() {
        let (world, rx) = world(armed(ArmReason::AlvaStart));

        let grabs = press_through_the_read_key(&world, vec![sample(&[]), sample(&[17]), sample(&[65])]);

        assert_eq!(grabs, 0);
        assert!(read_by_the_worker(world, rx).is_empty());
    }

    #[test]
    fn three_presses_under_a_temple_arm_grab_three_frames_and_read_each_once() {
        let (world, rx) = world(armed(ArmReason::AlvaStart));

        let grabs = press_through_the_read_key(&world, three_presses());

        assert_eq!(grabs, 3);
        let reasons = Some(ArmReason::AlvaStart);
        assert_eq!(read_by_the_worker(world, rx), vec![(Ok(1), reasons), (Ok(2), reasons), (Ok(3), reasons)]);
    }

    #[test]
    fn a_press_with_the_module_off_captures_without_consuming() {
        let (world, _rx) = world(armed(ArmReason::TempleArea));
        *world.sender.lock().unwrap() = None;

        assert_eq!(press_through_the_read_key(&world, vec![sample(PRESSED)]), 1);
    }

    #[test]
    fn an_unarmed_temple_frame_is_read_with_no_invented_client_reason() {
        let (world, rx) = world(ArmState::default());

        let grabs = press_through_the_read_key(&world, vec![sample(PRESSED)]);

        assert_eq!(grabs, 1);
        assert_eq!(read_by_the_worker(world, rx), vec![(Ok(1), None)]);
    }

    /// An arm whose deadline has passed is no temple context. Fails if
    /// `press_reason` drops its `is_armed` check.
    #[test]
    fn a_press_under_an_expired_arm_still_captures() {
        let expired = TempleArm::Armed { until_ms: Some(1), reason: ArmReason::AlvaStart };
        let (world, _rx) = world(ArmState { arm: expired, ..ArmState::default() });

        assert_eq!(press_through_the_read_key(&world, vec![sample(PRESSED)]), 1);
    }

    #[test]
    fn a_press_under_the_temple_area_reads_one_frame_carrying_that_reason() {
        let (world, rx) = world(armed(ArmReason::TempleArea));

        let grabs = press_through_the_read_key(&world, vec![sample(PRESSED)]);

        assert_eq!(grabs, 1);
        assert_eq!(read_by_the_worker(world, rx), vec![(Ok(1), Some(ArmReason::TempleArea))]);
    }

    #[test]
    fn an_expired_arm_is_not_temple_context() {
        let arm = ArmState {
            arm: TempleArm::Armed { until_ms: Some(10), reason: ArmReason::AlvaStart },
            ..ArmState::default()
        };
        assert_eq!(press_reason(arm, 10), None);
    }

    // ------------------------------------------------- the worker's receive --

    fn frame(monitor_id: u32) -> PressedFrame {
        PressedFrame { frame: Ok(Arc::new(capture(monitor_id))), reason: Some(ArmReason::AlvaStart), prompt: None, probe: None, epoch: 0 }
    }

    fn drain(rx: &Receiver<PressedFrame>, stop: &dyn Fn() -> bool) -> Vec<Result<u32, String>> {
        let mut reads = Vec::new();
        worker_loop(rx, stop, &mut |pressed| reads.push(pressed.frame.map(|c| c.monitor_id)));
        reads
    }

    #[test]
    fn a_worker_sent_nothing_reads_nothing() {
        let (tx, rx) = mpsc::channel();
        drop(tx);

        assert!(drain(&rx, &|| false).is_empty());
    }

    #[test]
    fn a_worker_reads_each_sent_frame_once_in_order() {
        let (tx, rx) = mpsc::channel();
        for id in [7, 8, 9] {
            tx.send(frame(id)).unwrap();
        }
        drop(tx);

        assert_eq!(drain(&rx, &|| false), vec![Ok(7), Ok(8), Ok(9)]);
    }

    #[test]
    fn a_grab_error_is_read_once_as_the_error() {
        let (tx, rx) = mpsc::channel();
        tx.send(PressedFrame { frame: Err("no monitor".to_string()), reason: Some(ArmReason::AlvaStart), prompt: None, probe: None, epoch: 0 }).unwrap();
        drop(tx);

        assert_eq!(drain(&rx, &|| false), vec![Err("no monitor".to_string())]);
    }

    #[test]
    fn a_stopped_worker_returns_without_reading_a_queued_frame() {
        let (tx, rx) = mpsc::channel();
        tx.send(frame(1)).unwrap();

        assert!(drain(&rx, &|| true).is_empty());
    }

    // ------------------------------------------- calibrate once, then probe --

    fn merc_capture() -> Capture {
        Capture {
            // The 500x280 recruit-panel crop on a 1920x1080 frame: a
            // calibrated screen with a mercenary, not a temple, on it.
            image: {
                let mut screen = image::DynamicImage::new_rgb8(1920, 1080);
                let panel = image::load_from_memory(include_bytes!("../../tests/fixtures/merc-recruit-pc-1080p.png")).unwrap();
                image::imageops::overlay(&mut screen, &panel, 700, 400);
                screen
            },
            monitor_id: 1,
            origin: (0, 0),
            client: [0, 0, 1920, 1080],
            timing: GrabTiming::default(),
        }
    }

    /// The live sheet moved 40 px right of the seeded origin, past the banked
    /// probe's fine window, so only the sweep finds its Entrance.
    fn shifted_capture() -> Capture {
        let live = capture(1);
        let mut screen = image::DynamicImage::new_rgb8(1920, 1080);
        image::imageops::overlay(&mut screen, &live.image, 40, 0);
        Capture { image: screen, ..live }
    }

    /// One press through `dispatch_press` with only the temple reader, no
    /// Client.txt reason; returns the outcome and the press's log lines.
    fn press(world: &World, frame: fn() -> Capture) -> (crate::read_hotkey::DispatchOutcome, Vec<String>) {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let mut armed = crate::read_hotkey::prepare_press(Press { cursor: None }, Default::default(), &[&TEMPLE_READER], world);
        let sink = Arc::clone(&lines);
        armed.log = Some(Box::new(move |line| sink.lock().unwrap().push(line)));
        let outcome = crate::read_hotkey::dispatch_press(armed, world, &mut |_| {}, &mut || Ok(frame()), &mut |_| true);
        let lines = lines.lock().unwrap().clone();
        (outcome, lines)
    }

    fn calibrations(lines: &[String]) -> usize {
        lines.iter().filter(|l| l.starts_with("Temple: calibration")).count()
    }

    mod keep_happy {
        use super::*;
        use crate::read_hotkey::{DispatchOutcome, ReaderId};

        #[test]
        fn an_anchored_live_sheet_is_selected_with_no_client_reason_and_no_calibration() {
            let (world, rx) = world(ArmState::default());
            *world.slot.lock().unwrap() = Some(slice_1080(Some([960, 713])));

            let (outcome, lines) = press(&world, || capture(1));

            assert_eq!(outcome, DispatchOutcome::Selected(ReaderId::Temple));
            assert_eq!(calibrations(&lines), 0, "{lines:?}");
            let read = rx.try_recv().unwrap();
            assert_eq!(read.reason, None);
            assert!(!read.probe.unwrap().located());
        }

        #[test]
        fn an_anchored_non_temple_frame_is_no_match_with_one_miss_line() {
            let (world, rx) = world(ArmState::default());
            *world.slot.lock().unwrap() = Some(slice_1080(Some([960, 713])));

            let (outcome, lines) = press(&world, merc_capture);

            assert_eq!(outcome, DispatchOutcome::NoMatch);
            assert_eq!(calibrations(&lines), 0, "{lines:?}");
            assert_eq!(lines.iter().filter(|l| l.starts_with("Read key: Temple probe — no match")).count(), 1, "{lines:?}");
            assert!(rx.try_recv().is_err());
        }

        #[test]
        fn a_sheet_the_banked_probe_misses_is_found_by_one_sweep_and_then_read_from_its_bank() {
            let (world, rx) = world(ArmState::default());
            *world.slot.lock().unwrap() = Some(crate::ssot::screen_from_geometry(1920, 1080, 1, (0, 0), [0, 0, 1920, 1080], 0));

            let (first, lines) = press(&world, shifted_capture);
            assert_eq!(first, DispatchOutcome::Selected(ReaderId::Temple));
            assert!(lines[0].starts_with("Read key: Temple probe — no match"), "{lines:?}");
            assert_eq!(lines[1], "Read key: no mechanics found — running a recalibration sweep now");
            assert!(lines[2].starts_with("Temple: calibration — Entrance found"), "{lines:?}");
            assert!(rx.try_recv().unwrap().probe.unwrap().located());

            let (second, lines) = press(&world, shifted_capture);
            assert_eq!(second, DispatchOutcome::Selected(ReaderId::Temple));
            assert_eq!(calibrations(&lines), 0, "{lines:?}");
        }
    }

    mod keep_regression {
        use super::*;
        use crate::read_hotkey::DispatchOutcome;

        /// Escaped bug (audit A2): while no plate was ever found, every press
        /// re-ran the whole-frame sweep.
        #[test]
        fn a_second_press_on_an_unanchored_screen_without_the_sheet_runs_no_sweep() {
            let (world, _rx) = world(ArmState::default());
            *world.slot.lock().unwrap() = Some(slice_1080(None));

            let (first, lines) = press(&world, merc_capture);
            assert_eq!(first, DispatchOutcome::NoMatch);
            assert_eq!(calibrations(&lines), 1, "{lines:?}");

            let (second, lines) = press(&world, merc_capture);
            assert_eq!(second, DispatchOutcome::NoMatch);
            assert_eq!(calibrations(&lines), 0, "{lines:?}");
            assert_eq!(lines.iter().filter(|l| l.starts_with("Read key: Temple probe — no match")).count(), 1, "{lines:?}");
        }

        /// Escaped bug (audit A4): a stopped calibration logged "Entrance found".
        #[test]
        fn a_calibration_stopped_before_it_runs_logs_cancelled_and_banks_nothing() {
            let (world, _rx) = world(ArmState::default());
            *world.slot.lock().unwrap() = Some(slice_1080(None));
            let frame = Arc::new(capture(1));
            let mut pending = TEMPLE_READER.prepare(&world, &Press { cursor: None }).unwrap();
            *world.sender.lock().unwrap() = None;

            let line = pending.calibrate(&frame, &world).unwrap();

            assert!(line.starts_with("Temple: calibration — cancelled"), "{line}");
            assert!(world.slot.lock().unwrap().unwrap().anchors.and_then(|a| a.temple_entrance).is_none());
            assert_eq!(pending.probe(&frame, &world), Ok(false));
        }
    }

    mod keep_critical {
        use super::*;
        use crate::read_hotkey::{DispatchOutcome, ReaderId};

        /// Silent consequence: a found plate confirming the seed, then a failed
        /// read, would leave the screen uncalibrated and every later press
        /// would re-sweep. The inbox is never read (the OCR never succeeds).
        #[test]
        fn a_seed_confirming_calibration_with_no_read_still_calibrates_the_screen() {
            let (world, _rx) = world(ArmState::default());

            assert_eq!(calibrations(&press(&world, shifted_capture).1), 1);
            let (second, lines) = press(&world, shifted_capture);

            assert_eq!(second, DispatchOutcome::Selected(ReaderId::Temple));
            assert_eq!(calibrations(&lines), 0, "{lines:?}");
        }

        /// A stale slice of another resolution is reconciled to the pressed
        /// frame, the Entrance is banked before any worker read, and the next
        /// press does not calibrate.
        #[test]
        fn a_geometry_mismatched_slice_is_reconciled_and_calibrated_before_any_read() {
            let (world, _rx) = world(ArmState::default());
            *world.slot.lock().unwrap() = Some(crate::ssot::screen_from_geometry(2560, 1440, 1, (0, 0), [0, 0, 2560, 1440], 0));

            assert_eq!(calibrations(&press(&world, shifted_capture).1), 1);
            let live = world.slot.lock().unwrap().unwrap();
            assert_eq!((live.width, live.height), (1920, 1080));
            assert!(live.anchors.and_then(|a| a.temple_entrance).is_some());

            assert_eq!(calibrations(&press(&world, shifted_capture).1), 0);
        }

        /// Silent consequence: a module turned off after calibration would
        /// still select Temple and suppress every later reader's probe.
        #[test]
        fn calibration_evidence_is_not_selected_after_the_module_stops() {
            let (world, _rx) = world(ArmState::default());
            let frame = Arc::new(capture(1));
            let mut pending = TEMPLE_READER.prepare(&world, &Press { cursor: None }).unwrap();
            assert!(pending.calibrate(&frame, &world).unwrap().starts_with("Temple: calibration — Entrance found"));

            *world.sender.lock().unwrap() = None;

            assert_eq!(pending.probe(&frame, &world), Ok(false));
        }

        /// Silent consequence: a budget that never re-opens leaves a moved or
        /// recalibrated screen with no way to find the sheet.
        #[test]
        fn a_recalibrate_or_a_new_capture_size_reopens_one_sweep_and_a_same_size_move_does_not() {
            let (world, _rx) = world(ArmState::default());
            assert_eq!(calibrations(&press(&world, merc_capture).1), 1);
            assert_eq!(calibrations(&press(&world, merc_capture).1), 0);

            *world.rearm.lock().unwrap() += 1;
            assert_eq!(calibrations(&press(&world, merc_capture).1), 1);
            assert_eq!(calibrations(&press(&world, merc_capture).1), 0);

            fn small() -> Capture { Capture { image: image::DynamicImage::new_rgb8(1280, 720), client: [0, 0, 1280, 720], ..merc_capture() } }
            fn moved() -> Capture { Capture { client: [50, 30, 1280, 720], ..small() } }
            assert_eq!(calibrations(&press(&world, small).1), 1);
            assert_eq!(calibrations(&press(&world, small).1), 0);

            assert_eq!(calibrations(&press(&world, moved).1), 0);
        }

        /// Silent consequence: a Recalibrate pressed during a sweep would be
        /// spent by that older sweep, leaving the recalibrated screen none.
        #[test]
        fn a_recalibrate_during_a_sweep_keeps_the_new_generation_sweep() {
            let (world, _rx) = world(ArmState::default());
            *world.rearm_during_sweep.lock().unwrap() = Some(());

            assert_eq!(calibrations(&press(&world, merc_capture).1), 1);

            assert_eq!(calibrations(&press(&world, merc_capture).1), 1);
            assert_eq!(calibrations(&press(&world, merc_capture).1), 0);
        }

        /// Silent consequence: an Entrance found by a sweep that started
        /// before Recalibrate would mark the recalibrated screen calibrated and
        /// skip its fresh sweep.
        #[test]
        fn a_sweep_superseded_by_recalibrate_banks_and_selects_nothing() {
            let (world, _rx) = world(ArmState::default());
            *world.rearm_during_sweep.lock().unwrap() = Some(());

            let (outcome, lines) = press(&world, shifted_capture);
            assert_eq!(outcome, DispatchOutcome::NoMatch);
            assert_eq!(lines.iter().filter(|l| l.starts_with("Temple: calibration — superseded by Recalibrate")).count(), 1, "{lines:?}");
            assert!(world.slot.lock().unwrap().unwrap().anchors.and_then(|a| a.temple_entrance).is_none());

            assert_eq!(calibrations(&press(&world, shifted_capture).1), 1);
        }

        /// Silent consequence: a setup change that a banked probe answers
        /// would leave the old setup's spent sweep, so returning to it never
        /// sweeps again.
        #[test]
        fn a_press_on_another_setup_reopens_the_old_setup_sweep_without_sweeping() {
            fn small() -> Capture { Capture { image: image::DynamicImage::new_rgb8(1280, 720), client: [0, 0, 1280, 720], ..merc_capture() } }
            let (world, _rx) = world(ArmState::default());
            assert_eq!(calibrations(&press(&world, small).1), 1);

            *world.slot.lock().unwrap() = Some(slice_1080(Some([960, 713])));
            let (outcome, lines) = press(&world, || capture(1));
            assert_eq!(outcome, DispatchOutcome::Selected(ReaderId::Temple));
            assert_eq!(calibrations(&lines), 0, "{lines:?}");

            assert_eq!(calibrations(&press(&world, small).1), 1);
        }

        /// Silent consequence: old-frame coordinates banked into a replacement
        /// slice would misplace every later temple overlay.
        #[test]
        fn an_owner_replaced_during_the_sweep_receives_no_entrance() {
            let (world, _rx) = world(ArmState::default());
            *world.replaced_by.lock().unwrap() = Some(crate::ssot::screen_from_geometry(2560, 1440, 1, (0, 0), [0, 0, 2560, 1440], 0));

            let (outcome, lines) = press(&world, shifted_capture);

            assert_eq!(outcome, DispatchOutcome::Selected(ReaderId::Temple));
            assert_eq!(lines.iter().filter(|l| l.starts_with("Temple: calibration — Entrance found")).count(), 1, "{lines:?}");
            let live = world.slot.lock().unwrap().unwrap();
            assert_eq!(live.width, 2560);
            assert!(live.anchors.and_then(|a| a.temple_entrance).is_none());
        }
    }
}
