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
}

impl<C: PressContext> ScreenReader<C> for TempleReader {
    fn id(&self) -> crate::read_hotkey::ReaderId { crate::read_hotkey::ReaderId::Temple }
    fn recent(&self, ctx: &C) -> Option<crate::read_prompt::Mechanic> { ctx.recent() }
    fn prepare(&self, ctx: &C, _press: &Press) -> Option<Box<dyn PendingRead<C>>> {
        let tx = ctx.press_sender()?;
        let reason = press_reason(ctx.arm_state(), super::run::now_ms());
        Some(Box::new(TemplePending { tx, reason, prompt: ctx.prompt_arm(), screen: ctx.screen(), probe: None, epoch: ctx.epoch() }))
    }
}

impl<C: PressContext> PendingRead<C> for TemplePending {
    fn probe(&mut self, frame: &Arc<Capture>, ctx: &C) -> Result<bool, String> {
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

    /// A fake app: the worker inbox the module would install, and the arm.
    struct World {
        sender: Mutex<Option<Sender<PressedFrame>>>,
        arm: Mutex<ArmState>,
        prompt: Mutex<Option<u64>>,
    }
    impl PressContext for World {
        fn screen(&self) -> Option<crate::ssot::ScreenSlice> {
            Some(crate::ssot::ScreenSlice {
                width: 1920, height: 1080, ui_scale: 0.90,
                source: crate::ssot::ScreenScaleSource::Remembered, measured_at_ms: 0,
                verified_this_session: false, monitor_id: 1, origin: (0,0), client: [0,0,1920,1080], anchors: None,
            })
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
    }

    fn armed(reason: ArmReason) -> ArmState {
        ArmState { arm: TempleArm::Armed { until_ms: None, reason }, ..ArmState::default() }
    }

    /// A world with the worker running under `arm`, and the worker's inbox.
    fn world(arm: ArmState) -> (World, Receiver<PressedFrame>) {
        let (tx, rx) = mpsc::channel();
        (World { sender: Mutex::new(Some(tx)), arm: Mutex::new(arm), prompt: Mutex::new(None) }, rx)
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
        run_sampler(world, readers, &mut || samples.next(), &mut || None, &queue, &mut |_| 0);
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
}
