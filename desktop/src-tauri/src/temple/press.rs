//! The temple's reader on the app-wide read key (POE-389).
//!
//! [`TEMPLE_READER`] is one entry of `crate::read_hotkey::READERS`. At a press
//! it arms only under Client.txt temple context — Alva's START line or the
//! temple area ([`press_reason`]) — and only while the temple worker runs with
//! an OCR engine (its inbox is installed in `TempleState.press`). Armed, it
//! hands the press's one frame to that worker and does nothing else: the read
//! runs on the module thread (`super::run::worker_loop`), never on the
//! dispatcher. Policy: docs/OVERLAY-GUIDE.md § "Screen reading (GGG policy)".

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
    pub reason: ArmReason,
}

/// What the temple reader needs from the app at press time.
pub trait PressContext {
    /// The temple worker's inbox, `None` while the module is off or has no OCR
    /// engine.
    fn press_sender(&self) -> Option<Sender<PressedFrame>>;
    /// The Client.txt arm, copied out.
    fn arm_state(&self) -> ArmState;
}

impl PressContext for AppHandle {
    fn press_sender(&self) -> Option<Sender<PressedFrame>> {
        let state = self.state::<crate::AppState>();
        let tx = state.temple_state.press.lock().unwrap_or_else(|e| e.into_inner()).clone();
        tx
    }
    fn arm_state(&self) -> ArmState {
        super::trigger::arm_state(self)
    }
}

/// The reason a press reads the temple at `now_ms`, or `None` when Client.txt
/// puts no temple in scope. A Re-arm (`Manual`) arm is not temple context.
pub fn press_reason(arm: ArmState, now_ms: u64) -> Option<ArmReason> {
    arm.arm
        .reason()
        .filter(|_| arm.arm.is_armed(now_ms))
        .filter(|reason| matches!(reason, ArmReason::AlvaStart | ArmReason::TempleArea))
}

/// The temple's entry in `crate::read_hotkey::READERS`.
pub struct TempleReader;
pub static TEMPLE_READER: TempleReader = TempleReader;

/// An armed temple read: where its frame goes, and why it was armed.
pub struct TemplePending {
    tx: Sender<PressedFrame>,
    reason: ArmReason,
}

impl<C: PressContext> ScreenReader<C> for TempleReader {
    fn arm(&self, ctx: &C, _press: &Press) -> Option<Box<dyn PendingRead<C>>> {
        let tx = ctx.press_sender()?;
        let reason = press_reason(ctx.arm_state(), super::run::now_ms())?;
        Some(Box::new(TemplePending { tx, reason }))
    }
}

impl<C> PendingRead<C> for TemplePending {
    fn consume(self: Box<Self>, frame: Result<&Arc<Capture>, &str>, _ctx: &C) {
        let frame = frame.map(Arc::clone).map_err(str::to_string);
        if self.tx.send(PressedFrame { frame, reason: self.reason }).is_err() {
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
    }
    impl PressContext for World {
        fn press_sender(&self) -> Option<Sender<PressedFrame>> {
            self.sender.lock().unwrap().clone()
        }
        fn arm_state(&self) -> ArmState {
            *self.arm.lock().unwrap()
        }
    }

    fn armed(reason: ArmReason) -> ArmState {
        ArmState { arm: TempleArm::Armed { until_ms: None, reason }, ..ArmState::default() }
    }

    /// A world with the worker running under `arm`, and the worker's inbox.
    fn world(arm: ArmState) -> (World, Receiver<PressedFrame>) {
        let (tx, rx) = mpsc::channel();
        (World { sender: Mutex::new(Some(tx)), arm: Mutex::new(arm) }, rx)
    }

    fn capture(monitor_id: u32) -> Capture {
        Capture {
            image: image::DynamicImage::new_rgb8(1, 1),
            monitor_id,
            origin: (0, 0),
            client: [0, 0, 1, 1],
            timing: GrabTiming::default(),
        }
    }

    fn sample(keys: &[u16]) -> Sample {
        Sample { down: keys.iter().copied().collect(), foreground: true, settings: ReadHotkeySettings::default() }
    }
    /// Shift+F9, the default read key.
    const PRESSED: &[u16] = &[16, 120];

    /// The read key's sampler and dispatcher over `samples` with only the
    /// temple reader registered; returns the grab count.
    fn press_through_the_read_key(world: &World, samples: Vec<Sample>) -> u32 {
        let readers: &[&dyn ScreenReader<World>] = &[&TEMPLE_READER];
        let (queue, presses) = mpsc::channel();
        let mut samples = samples.into_iter();
        run_sampler(world, readers, &mut || samples.next(), &mut || None, &queue);
        drop(queue);
        let mut grabs = 0;
        run_dispatcher(world, presses, &mut |_| {}, &mut || {
            grabs += 1;
            Ok(capture(grabs))
        });
        grabs
    }

    /// Ends the world's inbox and drains it through the production receive
    /// path, recording each read's monitor id (or error) and reason.
    fn read_by_the_worker(world: World, rx: Receiver<PressedFrame>) -> Vec<(Result<u32, String>, ArmReason)> {
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

        let grabs = press_through_the_read_key(&world, vec![sample(&[]), sample(&[16]), sample(&[120])]);

        assert_eq!(grabs, 0);
        assert!(read_by_the_worker(world, rx).is_empty());
    }

    #[test]
    fn three_presses_under_a_temple_arm_grab_three_frames_and_read_each_once() {
        let (world, rx) = world(armed(ArmReason::AlvaStart));

        let grabs = press_through_the_read_key(&world, three_presses());

        assert_eq!(grabs, 3);
        let reasons = ArmReason::AlvaStart;
        assert_eq!(read_by_the_worker(world, rx), vec![(Ok(1), reasons), (Ok(2), reasons), (Ok(3), reasons)]);
    }

    #[test]
    fn a_press_with_the_module_off_grabs_nothing() {
        let (world, _rx) = world(armed(ArmReason::TempleArea));
        *world.sender.lock().unwrap() = None;

        assert_eq!(press_through_the_read_key(&world, vec![sample(PRESSED)]), 0);
    }

    #[test]
    fn a_press_with_no_client_txt_temple_arm_grabs_nothing_and_reads_nothing() {
        let (world, rx) = world(ArmState::default());

        let grabs = press_through_the_read_key(&world, vec![sample(PRESSED)]);

        assert_eq!(grabs, 0);
        assert!(read_by_the_worker(world, rx).is_empty());
    }

    /// Re-arm is not Client.txt temple context (Coordinator ruling, 2026-11-13).
    #[test]
    fn a_press_under_a_manual_arm_grabs_nothing() {
        let (world, _rx) = world(armed(ArmReason::Manual));

        assert_eq!(press_through_the_read_key(&world, vec![sample(PRESSED)]), 0);
    }

    #[test]
    fn a_press_under_the_temple_area_reads_one_frame_carrying_that_reason() {
        let (world, rx) = world(armed(ArmReason::TempleArea));

        let grabs = press_through_the_read_key(&world, vec![sample(PRESSED)]);

        assert_eq!(grabs, 1);
        assert_eq!(read_by_the_worker(world, rx), vec![(Ok(1), ArmReason::TempleArea)]);
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
        PressedFrame { frame: Ok(Arc::new(capture(monitor_id))), reason: ArmReason::AlvaStart }
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
        tx.send(PressedFrame { frame: Err("no monitor".to_string()), reason: ArmReason::AlvaStart }).unwrap();
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
