//! The app-wide "Read screen" shortcut (POE-387): which key a press of grabs
//! one frame, and the fixed delay between that press and the grab.
use serde::{Deserialize, Serialize};
use tauri::Manager;

/// Longest press-to-grab delay a setting may hold, in milliseconds.
pub const MAX_DELAY_MS: u32 = 2000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shortcut {
    pub key: u16,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}
impl Shortcut {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if !(1..=254).contains(&self.key) || matches!(self.key, 16..=18 | 27 | 160..=165) {
            return Err("Choose a key or mouse button other than a modifier or Esc".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ReadHotkeySettings {
    pub shortcut: Shortcut,
    pub delay_ms: u32,
}
impl Default for ReadHotkeySettings {
    fn default() -> Self {
        Self {
            shortcut: Shortcut {
                key: 120,
                shift: true,
                ctrl: false,
                alt: false,
            },
            delay_ms: 0,
        }
    }
}

/// The read key reads the whole screen and the boss-loot keys read near the
/// mouse, so one key may not do both.
pub fn apart_from_boss_loot(
    read: &Shortcut,
    loot: &crate::loot_capture::LootSettings,
) -> Result<(), String> {
    if *read == loot.capture_shortcut || *read == loot.preview_shortcut {
        return Err("The read screen shortcut must differ from the boss loot shortcuts".into());
    }
    Ok(())
}

/// What stands in for a refused read hotkey: the default, or the first of
/// Shift+F10 / Shift+F11 when the boss-loot keys already hold it. Two boss-loot
/// keys can block at most two of the three.
pub fn fallback_apart_from(loot: &crate::loot_capture::LootSettings) -> ReadHotkeySettings {
    let default = ReadHotkeySettings::default();
    [120, 121, 122]
        .into_iter()
        .map(|key| ReadHotkeySettings {
            shortcut: Shortcut { key, ..default.shortcut.clone() },
            ..default.clone()
        })
        .find(|candidate| apart_from_boss_loot(&candidate.shortcut, loot).is_ok())
        .expect("two boss-loot keys cannot block three candidates")
}

impl ReadHotkeySettings {
    pub fn validate(&self) -> Result<(), String> {
        self.shortcut.validate()?;
        if self.delay_ms > MAX_DELAY_MS {
            return Err(format!("Delay must be at most {MAX_DELAY_MS} ms"));
        }
        Ok(())
    }
}

#[tauri::command]
pub fn read_hotkey_set_settings(
    app: tauri::AppHandle,
    settings: ReadHotkeySettings,
) -> Result<(), String> {
    store_settings(&app.state::<crate::AppState>(), settings)?;
    crate::persist_settings(&app);
    crate::ssot::emit_ssot(&app);
    Ok(())
}
/// The owner write behind `read_hotkey_set_settings`; refuses before touching state.
pub(crate) fn store_settings(state: &crate::AppState, settings: ReadHotkeySettings) -> Result<(), String> {
    settings.validate()?;
    let loot = state.loot.settings.lock().unwrap_or_else(|e| e.into_inner()).clone();
    apart_from_boss_loot(&settings.shortcut, &loot)?;
    *state.read_hotkey.lock().unwrap_or_else(|e| e.into_inner()) = settings;
    Ok(())
}

/// One press of the read key, with the cursor where it was at the press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Press {
    pub cursor: Option<(i32, i32)>,
}

/// Rising-edge detector for the read key.
#[derive(Default)]
pub struct ReadPoll {
    down: bool,
}
impl ReadPoll {
    /// A press only on the tick the shortcut goes down while the game is in
    /// the foreground; holding it is not a second press.
    pub fn poll(
        &mut self,
        down: &std::collections::BTreeSet<u16>,
        foreground: bool,
        shortcut: &Shortcut,
        cursor: &mut dyn FnMut() -> Option<(i32, i32)>,
    ) -> Option<Press> {
        let matched = down.contains(&shortcut.key)
            && down.contains(&16) == shortcut.shift
            && down.contains(&17) == shortcut.ctrl
            && down.contains(&18) == shortcut.alt;
        let press = (foreground && matched && !self.down).then(|| Press { cursor: cursor() });
        self.down = matched;
        press
    }
}

/// A screen reader the read key serves. `arm` runs at press time, before the
/// delay and the grab, and must not block; it fixes whatever context the read
/// needs and returns `None` when the reader has nothing to read now.
pub trait ScreenReader<C> {
    fn arm(&self, ctx: &C, press: &Press) -> Option<Box<dyn PendingRead<C>>>;
}
/// An armed read: receives the press's one grab, or the grab's error. It
/// reports nothing-found itself and never asks for another grab.
pub trait PendingRead<C>: Send {
    fn consume(self: Box<Self>, frame: Result<&std::sync::Arc<crate::capture::Capture>, &str>, ctx: &C);
}

/// Every reader the read key serves, in dispatch order.
pub const READERS: &[&dyn ScreenReader<tauri::AppHandle>] = &[];

/// A press with its readers already armed, waiting for its grab.
pub struct ArmedPress<C> {
    pub delay: std::time::Duration,
    pub pendings: Vec<Box<dyn PendingRead<C>>>,
}

pub fn arm_all<C>(
    press: Press,
    delay: std::time::Duration,
    readers: &[&dyn ScreenReader<C>],
    ctx: &C,
) -> ArmedPress<C> {
    let pendings = readers.iter().filter_map(|r| r.arm(ctx, &press)).collect();
    ArmedPress { delay, pendings }
}

/// The fixed delay, then exactly one grab, handed to every armed read; a press
/// no reader armed for takes no grab.
pub fn run_armed<C>(
    armed: ArmedPress<C>,
    ctx: &C,
    sleep: &mut dyn FnMut(std::time::Duration),
    grab: &mut dyn FnMut() -> Result<crate::capture::Capture, String>,
) {
    if armed.pendings.is_empty() {
        return;
    }
    sleep(armed.delay);
    let frame = grab().map(std::sync::Arc::new);
    if let Err(e) = &frame {
        log::warn!("read hotkey: screen grab failed: {e}");
    }
    for pending in armed.pendings {
        pending.consume(frame.as_ref().map_err(String::as_str), ctx);
    }
}

/// Arm, delay and grab in one call; the threads split it at the queue.
#[cfg(test)]
pub fn dispatch<C>(
    press: Press,
    readers: &[&dyn ScreenReader<C>],
    delay: std::time::Duration,
    ctx: &C,
    sleep: &mut dyn FnMut(std::time::Duration),
    grab: &mut dyn FnMut() -> Result<crate::capture::Capture, String>,
) {
    run_armed(arm_all(press, delay, readers, ctx), ctx, sleep, grab);
}

/// One key-sampling tick's inputs; `None` from the sampler ends the loop.
pub struct Sample {
    pub down: std::collections::BTreeSet<u16>,
    pub foreground: bool,
    pub settings: ReadHotkeySettings,
}

/// The key-sampling side: arms every reader at each press and queues it, so
/// delay, grab and reading never pause sampling.
pub fn run_sampler<C>(
    ctx: &C,
    readers: &[&dyn ScreenReader<C>],
    sample: &mut dyn FnMut() -> Option<Sample>,
    cursor: &mut dyn FnMut() -> Option<(i32, i32)>,
    queue: &std::sync::mpsc::Sender<ArmedPress<C>>,
) {
    let mut poll = ReadPoll::default();
    while let Some(s) = sample() {
        if let Some(press) = poll.poll(&s.down, s.foreground, &s.settings.shortcut, cursor) {
            let delay = std::time::Duration::from_millis(u64::from(s.settings.delay_ms));
            if queue.send(arm_all(press, delay, readers, ctx)).is_err() {
                return;
            }
        }
    }
}

/// The dispatch side: each queued press in order gets its own delay and grab.
pub fn run_dispatcher<C>(
    ctx: &C,
    queue: std::sync::mpsc::Receiver<ArmedPress<C>>,
    sleep: &mut dyn FnMut(std::time::Duration),
    grab: &mut dyn FnMut() -> Result<crate::capture::Capture, String>,
) {
    for armed in queue {
        run_armed(armed, ctx, sleep, grab);
    }
}

/// Starts the app-life key-sampling and dispatch threads. The sampler stops
/// when `cancel` turns true; the dispatcher stops once the sampler is gone.
pub fn spawn_read_hotkey(app: tauri::AppHandle, cancel: std::sync::Arc<std::sync::atomic::AtomicBool>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let dispatch_app = app.clone();
    std::thread::spawn(move || {
        run_dispatcher(
            &dispatch_app,
            rx,
            &mut std::thread::sleep,
            &mut || crate::capture::capture_screen(&dispatch_app),
        )
    });
    std::thread::spawn(move || {
        let mut sample = || {
            std::thread::sleep(std::time::Duration::from_millis(30));
            if cancel.load(std::sync::atomic::Ordering::SeqCst) {
                return None;
            }
            platform::sample(&app)
        };
        run_sampler(&app, READERS, &mut sample, &mut platform::cursor, &tx);
    });
}

#[cfg(windows)]
mod platform {
    use super::*;
    pub fn sample(app: &tauri::AppHandle) -> Option<Sample> {
        let state = app.state::<crate::AppState>();
        let settings = state.read_hotkey.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let mut keys = vec![settings.shortcut.key, 16, 17, 18];
        keys.dedup();
        let down = keys
            .into_iter()
            .filter(|key| {
                (unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(*key as i32) }
                    as u16
                    & 0x8000)
                    != 0
            })
            .collect();
        let foreground = state.game_in_foreground.load(std::sync::atomic::Ordering::SeqCst);
        Some(Sample { down, foreground, settings })
    }
    pub fn cursor() -> Option<(i32, i32)> {
        let mut p = windows::Win32::Foundation::POINT::default();
        unsafe { windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut p) }.ok()?;
        Some((p.x, p.y))
    }
}
#[cfg(not(windows))]
mod platform {
    use super::*;
    pub fn sample(_app: &tauri::AppHandle) -> Option<Sample> {
        Some(Sample { down: Default::default(), foreground: false, settings: ReadHotkeySettings::default() })
    }
    pub fn cursor() -> Option<(i32, i32)> {
        None
    }
}

#[cfg(test)]
mod dispatch_tests {
    use super::*;
    use crate::capture::{Capture, GrabTiming};
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::Duration;

    /// The test readers' world: the context `arm` fixes, and what reads saw.
    #[derive(Default)]
    struct World {
        context: Mutex<String>,
        armed_order: Mutex<Vec<&'static str>>,
        consumed: Mutex<Vec<(&'static str, String, Result<u32, String>)>>,
    }
    struct TestReader {
        name: &'static str,
        armed: bool,
    }
    struct TestPending {
        name: &'static str,
        context: String,
    }
    impl ScreenReader<Arc<World>> for TestReader {
        fn arm(&self, ctx: &Arc<World>, _press: &Press) -> Option<Box<dyn PendingRead<Arc<World>>>> {
            if !self.armed {
                return None;
            }
            ctx.armed_order.lock().unwrap().push(self.name);
            let context = ctx.context.lock().unwrap().clone();
            Some(Box::new(TestPending { name: self.name, context }))
        }
    }
    impl PendingRead<Arc<World>> for TestPending {
        fn consume(self: Box<Self>, frame: Result<&Arc<Capture>, &str>, ctx: &Arc<World>) {
            let frame = frame.map(|c| c.monitor_id).map_err(str::to_string);
            ctx.consumed.lock().unwrap().push((self.name, self.context, frame));
        }
    }
    const A: TestReader = TestReader { name: "a", armed: true };
    const B: TestReader = TestReader { name: "b", armed: true };
    const IDLE: TestReader = TestReader { name: "idle", armed: false };

    fn capture(monitor_id: u32) -> Capture {
        Capture {
            image: image::DynamicImage::new_rgb8(1, 1),
            monitor_id,
            origin: (0, 0),
            client: [0, 0, 1, 1],
            timing: GrabTiming::default(),
        }
    }
    fn press() -> Press {
        Press { cursor: Some((5, 6)) }
    }
    fn sample(keys: &[u16], foreground: bool) -> Sample {
        Sample {
            down: keys.iter().copied().collect(),
            foreground,
            settings: ReadHotkeySettings::default(),
        }
    }
    /// Shift+F9, the default read key.
    const PRESSED: &[u16] = &[16, 120];

    /// Runs the sampler over `samples` and the dispatcher to the end; returns grab count.
    fn run_pipeline(world: &Arc<World>, readers: &[&dyn ScreenReader<Arc<World>>], samples: Vec<Sample>) -> usize {
        let (tx, rx) = mpsc::channel();
        let mut samples = samples.into_iter();
        run_sampler(world, readers, &mut || samples.next(), &mut || None, &tx);
        drop(tx);
        let mut grabs = 0;
        run_dispatcher(world, rx, &mut |_| {}, &mut || {
            grabs += 1;
            Ok(capture(grabs))
        });
        grabs as usize
    }

    #[test]
    fn three_presses_grab_three_frames() {
        let world = Arc::new(World::default());
        let up = || sample(&[], true);
        let down = || sample(PRESSED, true);
        let grabs = run_pipeline(&world, &[&A], vec![down(), up(), down(), up(), down()]);
        assert_eq!(grabs, 3);
    }
    #[test]
    fn no_press_grabs_no_frame() {
        let world = Arc::new(World::default());
        let grabs = run_pipeline(&world, &[&A], vec![sample(&[], true), sample(&[16], true), sample(&[120], true)]);
        assert_eq!(grabs, 0);
    }
    #[test]
    fn held_key_is_one_press() {
        let world = Arc::new(World::default());
        let grabs = run_pipeline(&world, &[&A], vec![sample(PRESSED, true), sample(PRESSED, true), sample(PRESSED, true)]);
        assert_eq!(grabs, 1);
    }
    #[test]
    fn press_with_the_game_in_the_background_grabs_nothing() {
        let world = Arc::new(World::default());
        let grabs = run_pipeline(&world, &[&A], vec![sample(PRESSED, false)]);
        assert_eq!(grabs, 0);
    }
    #[test]
    fn press_records_the_cursor_at_the_press() {
        let mut poll = ReadPoll::default();
        let got = poll.poll(&PRESSED.iter().copied().collect(), true, &ReadHotkeySettings::default().shortcut, &mut || Some((7, 8)));
        assert_eq!(got, Some(Press { cursor: Some((7, 8)) }));
    }
    #[test]
    fn two_armed_readers_share_one_grab_per_press() {
        let world = Arc::new(World::default());
        let mut grabs = 0;
        dispatch(press(), &[&A, &B], Duration::ZERO, &world, &mut |_| {}, &mut || {
            grabs += 1;
            Ok(capture(9))
        });
        assert_eq!(grabs, 1);
        let consumed = world.consumed.lock().unwrap().clone();
        assert_eq!(consumed, vec![("a", String::new(), Ok(9)), ("b", String::new(), Ok(9))]);
    }
    #[test]
    fn unarmed_reader_never_consumes() {
        let world = Arc::new(World::default());
        dispatch(press(), &[&IDLE, &A], Duration::ZERO, &world, &mut |_| {}, &mut || Ok(capture(1)));
        let names: Vec<_> = world.consumed.lock().unwrap().iter().map(|c| c.0).collect();
        assert_eq!(names, vec!["a"]);
    }
    #[test]
    fn readers_arm_before_the_delay_and_the_grab() {
        let world = Arc::new(World::default());
        let events = Mutex::new(Vec::new());
        let seen = |what: &str| format!("{what} after {} armed", world.armed_order.lock().unwrap().len());
        dispatch(
            press(),
            &[&A, &B],
            Duration::from_millis(250),
            &world,
            &mut |d| events.lock().unwrap().push(format!("{} {d:?}", seen("sleep"))),
            &mut || {
                events.lock().unwrap().push(seen("grab"));
                Ok(capture(1))
            },
        );
        assert_eq!(*events.lock().unwrap(), vec!["sleep after 2 armed 250ms", "grab after 2 armed"]);
    }
    #[test]
    fn grab_error_reaches_every_armed_read_after_one_attempt() {
        let world = Arc::new(World::default());
        let mut attempts = 0;
        dispatch(press(), &[&A, &B], Duration::ZERO, &world, &mut |_| {}, &mut || {
            attempts += 1;
            Err("no monitor".to_string())
        });
        assert_eq!(attempts, 1);
        let consumed = world.consumed.lock().unwrap().clone();
        let err = || Err("no monitor".to_string());
        assert_eq!(consumed, vec![("a", String::new(), err()), ("b", String::new(), err())]);
    }
    #[test]
    fn press_during_a_blocked_dispatch_is_sampled_and_gets_its_own_grab() {
        let world = Arc::new(World::default());
        let (tx, rx) = mpsc::channel();
        let (release_grab, grab_gate) = mpsc::channel::<()>();
        let dispatch_world = world.clone();
        let dispatcher = std::thread::spawn(move || {
            let mut grabs = 0;
            run_dispatcher(&dispatch_world, rx, &mut |_| {}, &mut || {
                grab_gate.recv_timeout(Duration::from_secs(5)).expect("a grab past the two presses");
                grabs += 1;
                Ok(capture(grabs))
            });
            grabs
        });
        let mut samples = vec![sample(PRESSED, true), sample(&[], true), sample(PRESSED, true)].into_iter();
        run_sampler(&world, &[&A], &mut || samples.next(), &mut || None, &tx);
        // The first grab is still blocked, yet both presses were armed.
        assert_eq!(world.armed_order.lock().unwrap().len(), 2);
        drop(tx);
        release_grab.send(()).unwrap();
        release_grab.send(()).unwrap();
        assert_eq!(dispatcher.join().unwrap(), 2);
    }
    #[test]
    fn presses_grab_no_frame_with_no_reader_registered() {
        let world = Arc::new(World::default());
        let grabs = run_pipeline(&world, &[], vec![sample(PRESSED, true), sample(&[], true), sample(PRESSED, true)]);
        assert_eq!(grabs, 0);
    }
    #[test]
    fn no_press_grabs_no_frame_with_no_reader_registered() {
        let world = Arc::new(World::default());
        let grabs = run_pipeline(&world, &[], vec![sample(&[], true), sample(&[120], true)]);
        assert_eq!(grabs, 0);
    }
    #[test]
    fn presses_grab_no_frame_when_every_reader_declines() {
        let world = Arc::new(World::default());
        let grabs = run_pipeline(&world, &[&IDLE, &IDLE], vec![sample(PRESSED, true), sample(&[], true), sample(PRESSED, true)]);
        assert_eq!(grabs, 0);
        assert!(world.consumed.lock().unwrap().is_empty());
    }
    #[test]
    fn queued_press_sleeps_its_press_time_delay_before_its_one_grab() {
        let world = Arc::new(World::default());
        let with_delay = |keys: &[u16], delay_ms| Sample {
            settings: ReadHotkeySettings { delay_ms, ..ReadHotkeySettings::default() },
            ..sample(keys, true)
        };
        let (tx, rx) = mpsc::channel();
        // The delay setting changes on the tick after the press, while it is queued.
        let mut samples = vec![with_delay(PRESSED, 300), with_delay(&[], 900)].into_iter();
        run_sampler(&world, &[&A], &mut || samples.next(), &mut || None, &tx);
        drop(tx);
        let events = Mutex::new(Vec::new());
        run_dispatcher(
            &world,
            rx,
            &mut |d| events.lock().unwrap().push(format!("sleep {d:?}")),
            &mut || {
                events.lock().unwrap().push("grab".to_string());
                Ok(capture(1))
            },
        );
        assert_eq!(*events.lock().unwrap(), vec!["sleep 300ms", "grab"]);
    }
    #[test]
    fn queued_press_reads_with_the_context_of_its_press() {
        let world = Arc::new(World::default());
        *world.context.lock().unwrap() = "x".into();
        let (tx, rx) = mpsc::channel();
        let mut samples = vec![sample(PRESSED, true)].into_iter();
        run_sampler(&world, &[&A], &mut || samples.next(), &mut || None, &tx);
        drop(tx);
        *world.context.lock().unwrap() = "y".into();
        let mut grabs = 0;
        run_dispatcher(&world, rx, &mut |_| {}, &mut || {
            grabs += 1;
            Ok(capture(3))
        });
        assert_eq!(grabs, 1);
        assert_eq!(*world.consumed.lock().unwrap(), vec![("a", "x".to_string(), Ok(3))]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: u16) -> ReadHotkeySettings {
        ReadHotkeySettings {
            shortcut: Shortcut { key, shift: false, ctrl: true, alt: false },
            delay_ms: 0,
        }
    }

    #[test]
    fn read_hotkey_accepts_the_longest_delay() {
        let settings = ReadHotkeySettings { delay_ms: MAX_DELAY_MS, ..key(65) };
        assert_eq!(settings.validate(), Ok(()));
    }
    #[test]
    fn read_hotkey_rejects_a_delay_past_the_longest() {
        let settings = ReadHotkeySettings { delay_ms: MAX_DELAY_MS + 1, ..key(65) };
        assert!(settings.validate().unwrap_err().contains("Delay"));
    }
    #[test]
    fn read_hotkey_rejects_a_modifier_or_escape_as_the_key() {
        for bad in [0, 16, 17, 18, 27, 160, 165, 255] {
            assert!(key(bad).validate().is_err(), "key {bad} must be rejected");
        }
    }
    #[test]
    fn read_hotkey_default_is_apart_from_the_default_boss_loot_keys() {
        let loot = crate::loot_capture::LootSettings::default();
        assert_eq!(apart_from_boss_loot(&ReadHotkeySettings::default().shortcut, &loot), Ok(()));
    }
    use crate::loot_capture::LootSettings;
    use crate::settings::tests::test_app_state;

    fn read_of(state: &crate::AppState) -> ReadHotkeySettings {
        state.read_hotkey.lock().unwrap().clone()
    }
    fn loot_of(state: &crate::AppState) -> LootSettings {
        state.loot.settings.lock().unwrap().clone()
    }
    #[test]
    fn storing_a_read_hotkey_on_the_boss_loot_capture_key_is_refused_and_keeps_the_old_one() {
        let state = test_app_state();
        let capture = loot_of(&state).capture_shortcut;
        let refused = ReadHotkeySettings { shortcut: capture, delay_ms: 100 };
        assert!(store_settings(&state, refused).is_err());
        assert_eq!(read_of(&state), ReadHotkeySettings::default());
    }
    #[test]
    fn storing_a_read_hotkey_on_the_boss_loot_preview_key_is_refused_and_keeps_the_old_one() {
        let state = test_app_state();
        let preview = loot_of(&state).preview_shortcut;
        let refused = ReadHotkeySettings { shortcut: preview, delay_ms: 100 };
        assert!(store_settings(&state, refused).is_err());
        assert_eq!(read_of(&state), ReadHotkeySettings::default());
    }
    #[test]
    fn storing_a_read_hotkey_apart_from_the_boss_loot_keys_is_kept() {
        let state = test_app_state();
        assert_eq!(store_settings(&state, key(65)), Ok(()));
        assert_eq!(read_of(&state), key(65));
    }
    #[test]
    fn storing_boss_loot_capture_on_the_read_key_is_refused_and_keeps_the_old_settings() {
        let state = test_app_state();
        let refused = LootSettings {
            capture_shortcut: read_of(&state).shortcut,
            auto_save_seconds: 30,
            ..Default::default()
        };
        assert!(crate::loot_capture::store_settings(&state, refused).is_err());
        assert_eq!(loot_of(&state), LootSettings::default());
    }
    #[test]
    fn storing_boss_loot_preview_on_the_read_key_is_refused_and_keeps_the_old_settings() {
        let state = test_app_state();
        let refused = LootSettings {
            preview_shortcut: read_of(&state).shortcut,
            auto_save_seconds: 30,
            ..Default::default()
        };
        assert!(crate::loot_capture::store_settings(&state, refused).is_err());
        assert_eq!(loot_of(&state), LootSettings::default());
    }
    #[test]
    fn storing_boss_loot_keys_apart_from_the_read_key_is_kept() {
        let state = test_app_state();
        let kept = LootSettings { auto_save_seconds: 30, ..Default::default() };
        assert_eq!(crate::loot_capture::store_settings(&state, kept.clone()), Ok(()));
        assert_eq!(loot_of(&state), kept);
    }
    #[test]
    fn fallback_skips_a_default_held_by_boss_loot_capture() {
        let loot = LootSettings { capture_shortcut: ReadHotkeySettings::default().shortcut, ..Default::default() };
        let fallback = fallback_apart_from(&loot);
        assert_eq!(fallback.shortcut.key, 121);
        assert_eq!(apart_from_boss_loot(&fallback.shortcut, &loot), Ok(()));
    }
    #[test]
    fn fallback_skips_a_default_held_by_boss_loot_preview() {
        let loot = LootSettings { preview_shortcut: ReadHotkeySettings::default().shortcut, ..Default::default() };
        assert_eq!(fallback_apart_from(&loot).shortcut.key, 121);
    }
    #[test]
    fn fallback_skips_two_candidates_held_by_both_boss_loot_keys() {
        let default = ReadHotkeySettings::default().shortcut;
        let loot = LootSettings {
            capture_shortcut: default.clone(),
            preview_shortcut: Shortcut { key: 121, ..default },
            ..Default::default()
        };
        assert_eq!(fallback_apart_from(&loot).shortcut.key, 122);
    }
    #[test]
    fn fallback_is_the_default_when_boss_loot_leaves_it_free() {
        assert_eq!(fallback_apart_from(&LootSettings::default()), ReadHotkeySettings::default());
    }
    #[test]
    fn read_hotkey_with_other_modifiers_than_a_boss_loot_key_is_accepted() {
        let loot = crate::loot_capture::LootSettings::default();
        let read = Shortcut { ctrl: true, ..loot.preview_shortcut.clone() };
        assert_eq!(apart_from_boss_loot(&read, &loot), Ok(()));
    }
    #[test]
    fn read_hotkey_accepts_a_mouse_button_as_the_key() {
        assert_eq!(key(4).validate(), Ok(()));
    }
}
