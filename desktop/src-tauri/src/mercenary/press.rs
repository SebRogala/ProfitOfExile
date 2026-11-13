//! The mercenary's reader on the app-wide read key (POE-390).
//!
//! [`MERC_READER`] is one entry of `crate::read_hotkey::READERS`. At a press it
//! arms whenever the merc worker runs with an open session — its inbox is
//! installed in `MercState.inbox` — with no Client.txt condition (ruling 1).
//! Armed, it hands the press's one frame and the cursor at the press to that
//! worker and does nothing else: the panel read and the hovered-tooltip read
//! run on the module thread (`super::run::worker_loop`), never on the
//! dispatcher. Policy: docs/OVERLAY-GUIDE.md § "Screen reading (GGG policy)".

use std::sync::mpsc::Sender;
use std::sync::Arc;

use tauri::{AppHandle, Manager};

use crate::capture::Capture;
use crate::read_hotkey::{PendingRead, Press, ScreenReader};

/// One pressed frame (or the grab's error), with the cursor at the press.
pub struct PressedMercFrame {
    pub frame: Result<Arc<Capture>, String>,
    pub cursor: Option<(i32, i32)>,
    /// The Mercenary prompt arm live when this press was accepted.
    pub prompt: Option<u64>,
}

/// What the merc worker receives.
pub enum MercInbox {
    /// A press of the read key, with its one frame.
    Press(PressedMercFrame),
    /// Client.txt says the player entered another area: the recruit window is
    /// gone with the area it was in (ruling 4).
    AreaChanged,
}

/// What the merc reader needs from the app at press time.
pub trait PressContext {
    /// The merc worker's inbox, `None` while the module is off, the session is
    /// not open, or capture is unavailable.
    fn inbox(&self) -> Option<Sender<MercInbox>>;
    fn prompt_arm(&self) -> Option<u64> { None }
}

impl PressContext for AppHandle {
    fn inbox(&self) -> Option<Sender<MercInbox>> {
        installed_inbox(self)
    }
    fn prompt_arm(&self) -> Option<u64> {
        crate::read_prompt::armed_id_in(&self.state::<crate::AppState>(), crate::read_prompt::Mechanic::Mercenary)
    }
}

/// The installed inbox, cloned out under one lock.
pub(crate) fn installed_inbox(app: &AppHandle) -> Option<Sender<MercInbox>> {
    let state = app.state::<crate::AppState>();
    let tx = state.merc.inbox.lock().unwrap_or_else(|e| e.into_inner()).clone();
    tx
}

/// Install (or, with `None`, clear) the worker's inbox.
pub(crate) fn install_inbox(app: &AppHandle, tx: Option<Sender<MercInbox>>) {
    let state = app.state::<crate::AppState>();
    *state.merc.inbox.lock().unwrap_or_else(|e| e.into_inner()) = tx;
}

/// The mercenary's entry in `crate::read_hotkey::READERS`.
pub struct MercReader;
pub static MERC_READER: MercReader = MercReader;

/// An armed merc read: where its frame goes, and the cursor at the press.
pub struct MercPending {
    tx: Sender<MercInbox>,
    cursor: Option<(i32, i32)>,
    prompt: Option<u64>,
}

impl<C: PressContext> ScreenReader<C> for MercReader {
    fn arm(&self, ctx: &C, press: &Press) -> Option<Box<dyn PendingRead<C>>> {
        let tx = ctx.inbox()?;
        Some(Box::new(MercPending { tx, cursor: press.cursor, prompt: ctx.prompt_arm() }))
    }
}

impl<C> PendingRead<C> for MercPending {
    fn consume(self: Box<Self>, frame: Result<&Arc<Capture>, &str>, _ctx: &C) {
        let frame = frame.map(Arc::clone).map_err(str::to_string);
        let pressed = PressedMercFrame { frame, cursor: self.cursor, prompt: self.prompt };
        if self.tx.send(MercInbox::Press(pressed)).is_err() {
            log::warn!("merc: read worker gone, pressed frame dropped");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::GrabTiming;
    use crate::mercenary::run::{worker_loop, WorkerEvent};
    use crate::read_hotkey::{run_dispatcher, run_sampler, ReadHotkeySettings, Sample};
    use std::sync::mpsc::{self, Receiver};
    use std::sync::Mutex;

    /// A fake app: the merc inbox the module would install.
    struct World {
        inbox: Mutex<Option<Sender<MercInbox>>>,
        prompt: Mutex<Option<u64>>,
    }
    impl PressContext for World {
        fn inbox(&self) -> Option<Sender<MercInbox>> {
            self.inbox.lock().unwrap().clone()
        }
        fn prompt_arm(&self) -> Option<u64> { *self.prompt.lock().unwrap() }
    }

    fn world() -> (World, Receiver<MercInbox>) {
        let (tx, rx) = mpsc::channel();
        (World { inbox: Mutex::new(Some(tx)), prompt: Mutex::new(None) }, rx)
    }

    #[test]
    fn a_pressed_frame_keeps_the_prompt_arm_from_acceptance() {
        let (world, rx) = world();
        *world.prompt.lock().unwrap() = Some(4);
        let armed = crate::read_hotkey::arm_all(Press { cursor: None }, Default::default(), &[&MERC_READER], &world);
        *world.prompt.lock().unwrap() = Some(5);
        crate::read_hotkey::run_armed(armed, &world, &mut |_| {}, &mut || Ok(capture(1)));
        let MercInbox::Press(pressed) = rx.try_recv().unwrap() else { panic!("expected press") };
        assert_eq!(pressed.prompt, Some(4));
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
    /// Ctrl+A, the default read key.
    const PRESSED: &[u16] = &[17, 65];

    /// The read key's sampler and dispatcher over `samples` with only the merc
    /// reader registered; the sampler reads the cursor once per press, taking
    /// the next of `cursors`. Returns the grab count.
    fn press_through_the_read_key(world: &World, samples: Vec<Sample>, cursors: &[(i32, i32)]) -> u32 {
        let readers: &[&dyn ScreenReader<World>] = &[&MERC_READER];
        let (queue, presses) = mpsc::channel();
        let mut samples = samples.into_iter();
        let mut cursors = cursors.iter().copied();
        run_sampler(world, readers, &mut || samples.next(), &mut || cursors.next(), &queue);
        drop(queue);
        let mut grabs = 0;
        run_dispatcher(world, presses, &mut |_| {}, &mut || {
            grabs += 1;
            Ok(capture(grabs))
        });
        grabs
    }

    /// What one inbox message is, flattened for comparison.
    #[derive(Debug, PartialEq)]
    enum Seen {
        Idle,
        Press(Result<u32, String>, Option<(i32, i32)>),
        AreaChanged,
    }

    fn seen(event: WorkerEvent) -> Seen {
        match event {
            WorkerEvent::Idle => Seen::Idle,
            WorkerEvent::Message(MercInbox::Press(p)) => Seen::Press(p.frame.map(|c| c.monitor_id), p.cursor),
            WorkerEvent::Message(MercInbox::AreaChanged) => Seen::AreaChanged,
        }
    }

    /// Ends the world's inbox and drains it through the production receive
    /// path, recording every message (idle timeouts excluded).
    fn read_by_the_worker(world: World, rx: Receiver<MercInbox>) -> Vec<Seen> {
        drop(world);
        drain(&rx, &|| false)
    }

    fn drain(rx: &Receiver<MercInbox>, stop: &dyn Fn() -> bool) -> Vec<Seen> {
        let mut out = Vec::new();
        worker_loop(rx, stop, &mut |event| match seen(event) {
            Seen::Idle => {}
            other => out.push(other),
        });
        out
    }

    // ------------------------------------------------ the read key's grabs --

    #[test]
    fn idle_samples_grab_nothing_and_send_nothing() {
        let (world, rx) = world();

        let grabs = press_through_the_read_key(&world, vec![sample(&[]), sample(&[17]), sample(&[65])], &[]);

        assert_eq!(grabs, 0);
        assert!(read_by_the_worker(world, rx).is_empty());
    }

    #[test]
    fn a_press_with_no_inbox_installed_grabs_nothing() {
        let (world, _rx) = world();
        *world.inbox.lock().unwrap() = None;

        assert_eq!(press_through_the_read_key(&world, vec![sample(PRESSED)], &[(1, 1)]), 0);
    }

    #[test]
    fn three_presses_grab_three_frames_each_sent_once_with_its_own_cursor() {
        let (world, rx) = world();

        let grabs = press_through_the_read_key(
            &world,
            vec![sample(PRESSED), sample(&[]), sample(PRESSED), sample(&[]), sample(PRESSED)],
            &[(1, 1), (2, 2), (3, 3)],
        );

        assert_eq!(grabs, 3);
        assert_eq!(
            read_by_the_worker(world, rx),
            vec![
                Seen::Press(Ok(1), Some((1, 1))),
                Seen::Press(Ok(2), Some((2, 2))),
                Seen::Press(Ok(3), Some((3, 3))),
            ]
        );
    }

    // ------------------------------------------- one frame for both readers --

    /// A world where both the temple worker and the merc worker are running.
    struct BothWorld {
        temple: Sender<crate::temple::press::PressedFrame>,
        merc: Sender<MercInbox>,
    }
    impl crate::temple::press::PressContext for BothWorld {
        fn press_sender(&self) -> Option<Sender<crate::temple::press::PressedFrame>> {
            Some(self.temple.clone())
        }
        fn arm_state(&self) -> crate::temple::trigger::ArmState {
            crate::temple::trigger::ArmState {
                arm: crate::temple::trigger::TempleArm::Armed {
                    until_ms: None,
                    reason: crate::temple::trigger::ArmReason::AlvaStart,
                },
                ..crate::temple::trigger::ArmState::default()
            }
        }
    }
    impl PressContext for BothWorld {
        fn inbox(&self) -> Option<Sender<MercInbox>> {
            Some(self.merc.clone())
        }
    }

    #[test]
    fn one_press_with_temple_and_merc_armed_grabs_once_and_hands_both_the_same_frame() {
        let (temple_tx, temple_rx) = mpsc::channel();
        let (merc_tx, merc_rx) = mpsc::channel();
        let world = BothWorld { temple: temple_tx, merc: merc_tx };
        let readers: &[&dyn ScreenReader<BothWorld>] =
            &[&crate::temple::press::TEMPLE_READER, &MERC_READER];
        let (queue, presses) = mpsc::channel();
        let mut samples = vec![sample(PRESSED)].into_iter();
        run_sampler(&world, readers, &mut || samples.next(), &mut || Some((5, 5)), &queue);
        drop(queue);
        let mut grabs = 0;
        run_dispatcher(&world, presses, &mut |_| {}, &mut || {
            grabs += 1;
            Ok(capture(grabs))
        });
        drop(world);

        assert_eq!(grabs, 1);
        let temple_frame = temple_rx.recv().unwrap().frame.unwrap();
        let Ok(MercInbox::Press(merc)) = merc_rx.recv() else {
            panic!("the merc worker got no press");
        };
        assert!(Arc::ptr_eq(&temple_frame, &merc.frame.unwrap()));
        assert!(temple_rx.recv().is_err() && merc_rx.recv().is_err(), "one message each");
    }

    // ------------------------------------------------- the worker's receive --

    fn press(monitor_id: u32) -> MercInbox {
        MercInbox::Press(PressedMercFrame { frame: Ok(Arc::new(capture(monitor_id))), cursor: None, prompt: None })
    }

    #[test]
    fn a_worker_sent_nothing_receives_nothing() {
        let (tx, rx) = mpsc::channel();
        drop(tx);

        assert!(drain(&rx, &|| false).is_empty());
    }

    #[test]
    fn a_worker_receives_each_sent_frame_once_in_order() {
        let (tx, rx) = mpsc::channel();
        for id in [7, 8, 9] {
            tx.send(press(id)).unwrap();
        }
        drop(tx);

        assert_eq!(
            drain(&rx, &|| false),
            vec![Seen::Press(Ok(7), None), Seen::Press(Ok(8), None), Seen::Press(Ok(9), None)]
        );
    }

    #[test]
    fn a_grab_error_is_received_once_as_the_error() {
        let (tx, rx) = mpsc::channel();
        tx.send(MercInbox::Press(PressedMercFrame { frame: Err("no monitor".to_string()), cursor: None, prompt: None }))
            .unwrap();
        drop(tx);

        assert_eq!(drain(&rx, &|| false), vec![Seen::Press(Err("no monitor".to_string()), None)]);
    }

    #[test]
    fn an_area_change_is_received_once() {
        let (tx, rx) = mpsc::channel();
        tx.send(MercInbox::AreaChanged).unwrap();
        drop(tx);

        assert_eq!(drain(&rx, &|| false), vec![Seen::AreaChanged]);
    }

    #[test]
    fn a_stopped_worker_returns_without_receiving_a_queued_frame() {
        let (tx, rx) = mpsc::channel();
        tx.send(press(1)).unwrap();

        assert!(drain(&rx, &|| true).is_empty());
    }

    /// The idle timeout is where the trade tick runs (D7).
    #[test]
    fn a_quiet_inbox_delivers_idle() {
        let (_tx, rx) = mpsc::channel::<MercInbox>();
        let idles = std::cell::Cell::new(0);
        let mut events = Vec::new();

        worker_loop(&rx, &|| idles.get() > 0, &mut |event| {
            let event = seen(event);
            if event == Seen::Idle {
                idles.set(idles.get() + 1);
            }
            events.push(event);
        });

        assert_eq!(events, vec![Seen::Idle]);
    }

    // ------------------------------------------------------ the source guard --

    /// The files under `src/mercenary/` whose production text calls
    /// `capture_screen(`. Production text = the file cut at its first
    /// `#[cfg(test)]` line, with `//` comment lines dropped.
    fn capture_callers<'a>(files: &[(&'a str, &str)]) -> Vec<&'a str> {
        let needle = ["capture_screen", "("].concat();
        files
            .iter()
            .filter(|(_, text)| {
                text.lines()
                    .take_while(|l| l.trim() != "#[cfg(test)]")
                    .filter(|l| !l.trim_start().starts_with("//"))
                    .any(|l| l.contains(&needle))
            })
            .map(|(name, _)| *name)
            .collect()
    }

    fn rust_files(dir: &std::path::Path, out: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                rust_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                out.push((name, std::fs::read_to_string(&path).unwrap()));
            }
        }
    }

    /// No merc capture without a press: the only screen grab left under
    /// `src/mercenary/` is the debug panel's one-shot button.
    #[test]
    fn only_the_debug_button_grabs_the_screen_in_merc_production_code() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/mercenary");
        let mut files = Vec::new();
        rust_files(&dir, &mut files);
        let borrowed: Vec<(&str, &str)> = files.iter().map(|(n, t)| (n.as_str(), t.as_str())).collect();

        assert_eq!(capture_callers(&borrowed), vec!["debug.rs"]);
    }

    #[test]
    fn the_source_guard_names_a_production_caller_and_skips_comments_and_tests() {
        let call = ["fn f() { crate::capture::", "capture_screen", "(app); }"].concat();
        let commented = ["//! ", "capture_screen", "(…)\n#[cfg(test)]\nmod t { ", "capture_screen", "( }"].concat();

        assert_eq!(capture_callers(&[("run.rs", call.as_str())]), vec!["run.rs"]);
        assert!(capture_callers(&[("run.rs", commented.as_str())]).is_empty());
    }
}
