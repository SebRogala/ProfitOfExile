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
    /// How long a no-match result offers its recalibration: a read press
    /// within it may sweep (POE-459). The dismissal timeout's choices minus
    /// Off, since Off would never offer a sweep.
    pub recalibrate_seconds: u32,
}

/// The choices `recalibrate_seconds` takes.
pub const RECALIBRATE_SECONDS: [u32; 4] = [5, 10, 20, 30];
impl Default for ReadHotkeySettings {
    fn default() -> Self {
        Self {
            shortcut: Shortcut {
                key: 65,
                shift: false,
                ctrl: true,
                alt: false,
            },
            delay_ms: 0,
            recalibrate_seconds: 5,
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
        return Err("Read screen shortcut must differ from the boss loot and read exchange shortcuts".into());
    }
    Ok(())
}

/// What stands in for a refused read hotkey: the default, or the first of
/// Ctrl+Q / Ctrl+Z when the boss-loot keys already hold it. Two boss-loot
/// keys can block at most two of the three.
pub fn fallback_apart_from(loot: &crate::loot_capture::LootSettings) -> ReadHotkeySettings {
    let default = ReadHotkeySettings::default();
    [65, 81, 90]
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
        if !RECALIBRATE_SECONDS.contains(&self.recalibrate_seconds) {
            return Err("Invalid recalibration window".into());
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
    let exchange = state.exchange_read.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if settings.shortcut == exchange.shortcut {
        return Err("Read screen shortcut must differ from the boss loot and read exchange shortcuts".into());
    }
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

#[derive(Default)]
struct DismissPoll {
    shortcuts: Vec<(Shortcut, ReadPoll)>,
}

impl DismissPoll {
    fn poll(&mut self, sample: &Sample, on_dismiss: &mut dyn FnMut(&Shortcut) -> u64) -> Vec<u64> {
        self.shortcuts.retain(|(shortcut, _)| sample.dismiss.contains(shortcut));
        for shortcut in &sample.dismiss {
            if !self.shortcuts.iter().any(|(known, _)| known == shortcut) {
                self.shortcuts.push((shortcut.clone(), ReadPoll::default()));
            }
        }
        let mut dismissed = Vec::new();
        for (shortcut, poll) in &mut self.shortcuts {
            if poll.poll(&sample.down, sample.foreground, shortcut, &mut || None).is_some() {
                dismissed.push(on_dismiss(shortcut));
            }
        }
        dismissed
    }
}

#[cfg(any(windows, test))]
pub fn polled_keys(read: &Shortcut, exchange: &Shortcut, dismiss: &[Shortcut]) -> std::collections::BTreeSet<u16> {
    [16, 17, 18, read.key, exchange.key].into_iter().chain(dismiss.iter().map(|shortcut| shortcut.key)).collect()
}

/// A reader identity used for first-match ordering and dispatch results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReaderId { Temple, Font, Gem, Mercenary, Exchange }

impl ReaderId {
    fn mechanic(self) -> Option<crate::read_prompt::Mechanic> {
        use crate::read_prompt::Mechanic;
        match self {
            Self::Temple => Some(Mechanic::Incursion),
            Self::Font | Self::Gem => Some(Mechanic::DivineFont),
            Self::Mercenary => Some(Mechanic::Mercenary),
            Self::Exchange => None,
        }
    }
}

pub trait ScreenReader<C> {
    fn id(&self) -> ReaderId;
    fn recent(&self, _ctx: &C) -> Option<crate::read_prompt::Mechanic> { None }
    fn prepare(&self, ctx: &C, press: &Press) -> Option<Box<dyn PendingRead<C>>>;
}
/// Prepared context; probing retains evidence without publishing module state.
pub trait PendingRead<C>: Send {
    /// Whether this read's region still needs the recalibration sweep, asked
    /// only after every probe on the press missed.
    fn needs_calibration(&mut self, _frame: &std::sync::Arc<crate::capture::Capture>, _ctx: &C) -> bool { false }
    /// Runs the recalibration sweep on the pressed frame, only after every
    /// probe missed and `needs_calibration` said so; returns one app-log line.
    fn calibrate(&mut self, _frame: &std::sync::Arc<crate::capture::Capture>, _ctx: &C) -> Option<String> { None }
    fn probe(&mut self, frame: &std::sync::Arc<crate::capture::Capture>, ctx: &C) -> Result<bool, String>;
    fn consume(self: Box<Self>, frame: Result<&std::sync::Arc<crate::capture::Capture>, &str>, ctx: &C);
}

/// Every reader the read key serves, in dispatch order.
pub const READERS: &[&dyn ScreenReader<tauri::AppHandle>] = &[
    &crate::temple::press::TEMPLE_READER,
    &crate::lab::font_reader::LabFontReader,
    &crate::lab::gem_reader::LabGemReader,
    &crate::mercenary::press::MERC_READER,
];

/// A press with its eligible candidates prepared, waiting for its grab.
pub struct PreparedPress<C> {
    pub delay: std::time::Duration,
    pub pendings: Vec<(ReaderId, Box<dyn PendingRead<C>>)>,
    pub dismissed: Vec<u64>,
    pub finish: Option<FinishedRead>,
    /// Where the press writes its calibration, miss and timing lines; `None` writes nothing.
    pub log: Option<PressLog>,
    /// `Some` for a press made while a no-match result offered recalibration:
    /// only such a press sweeps when its own read finds nothing, and this is
    /// raised once before the sweep runs. `None`: the press never sweeps.
    pub sweep: Option<SweepNotice>,
}

pub type FinishedRead = Box<dyn FnOnce(DispatchOutcome) + Send>;
pub type PressLog = Box<dyn FnMut(String) + Send>;
pub type SweepNotice = Box<dyn FnOnce() + Send>;

/// What a Read exchange press brings to the shared queue: its one reader and
/// its own result hook.
pub struct ExchangePress<C> {
    pub pending: Box<dyn PendingRead<C>>,
    pub finish: FinishedRead,
}

/// What accepting a press gives it: its result hook and its sweep permission.
#[derive(Default)]
pub struct Accepted {
    pub finish: Option<FinishedRead>,
    pub sweep: Option<SweepNotice>,
}

/// An accepted press: `finish` always, `notice` only when `stamp` was taken
/// inside a recalibration offer.
pub fn accepted(stamp: crate::read_prompt::PressStamp, finish: FinishedRead, notice: SweepNotice) -> Accepted {
    Accepted { finish: Some(finish), sweep: stamp.confirming().then_some(notice) }
}

pub fn prepare_press<C>(
    press: Press,
    delay: std::time::Duration,
    readers: &[&dyn ScreenReader<C>],
    ctx: &C,
) -> PreparedPress<C> {
    let recent = readers.iter().find_map(|r| r.recent(ctx));
    let mut pendings = Vec::new();
    for reader in readers {
        let id = reader.id();
        if pendings.iter().any(|(existing, _)| *existing == id) { continue; }
        if let Some(pending) = reader.prepare(ctx, &press) { pendings.push((id, pending)); }
    }
    pendings.sort_by_key(|(id, _)| if *id == ReaderId::Font { 0 } else if recent.is_some() && id.mechanic() == recent { 1 } else { 2 });
    PreparedPress { delay, pendings, dismissed: Vec::new(), finish: None, log: None, sweep: None }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchOutcome { Selected(ReaderId), NoMatch, Failed(String), Suppressed }

/// Await all same-tick dismissals, then delay, capture once and select the first match.
pub fn dispatch_press<C>(
    mut armed: PreparedPress<C>,
    ctx: &C,
    sleep: &mut dyn FnMut(std::time::Duration),
    grab: &mut dyn FnMut() -> Result<crate::capture::Capture, crate::capture::CaptureError>,
    await_hidden: &mut dyn FnMut(&[u64]) -> bool,
) -> DispatchOutcome {
    if !armed.dismissed.is_empty() && !await_hidden(&armed.dismissed) {
        return DispatchOutcome::Suppressed;
    }
    sleep(armed.delay);
    let exchange = matches!(armed.pendings.as_slice(), [(ReaderId::Exchange, _)]);
    let prefix = if exchange { "Exchange read:" } else { "Read key:" };
    let mut log = armed.log.take();
    let mut write = |line: String| if let Some(log) = log.as_mut() { log(line) };
    let ms = |since: std::time::Instant| since.elapsed().as_millis();
    let started = std::time::Instant::now();
    let frame = match grab() {
        // Nothing grabbed and already logged by capture; the press is a no-op.
        Err(crate::capture::CaptureError::NotAccepted) => {
            if exchange {
                write(format!("{prefix} privacy refused in {} ms (grab {} ms, calibration 0 ms, probes 0 ms)", ms(started), ms(started)));
            }
            return DispatchOutcome::Suppressed;
        }
        Err(crate::capture::CaptureError::Failed(e)) => {
            log::warn!("{prefix} screen grab failed: {e}");
            write(format!("{prefix} failed in {} ms (grab {} ms, calibration 0 ms, probes 0 ms)", ms(started), ms(started)));
            return DispatchOutcome::Failed(e);
        }
        Ok(capture) => std::sync::Arc::new(capture),
    };
    let grab_ms = ms(started);
    let probes_started = std::time::Instant::now();
    let mut outcome = DispatchOutcome::NoMatch;
    let mut missed = Vec::new();
    for (id, mut pending) in armed.pendings {
        let probe_started = std::time::Instant::now();
        match pending.probe(&frame, ctx) {
            Ok(true) => {
                pending.consume(Ok(&frame), ctx);
                outcome = DispatchOutcome::Selected(id);
                break;
            }
            Ok(false) => {
                write(format!("{prefix} {id:?} probe — no match, {} ms", ms(probe_started)));
                missed.push((id, pending));
            }
            Err(error) => {
                log::warn!("{prefix} probe failed: {error}");
                outcome = DispatchOutcome::Failed(error);
                break;
            }
        }
    }
    let probes_ms = ms(probes_started);
    let calibration_started = std::time::Instant::now();
    // Only a press confirming a no-match offer sweeps, and on this same frame:
    // nothing here grabs again.
    let notice = armed.sweep.take();
    if let (true, Some(notice)) = (outcome == DispatchOutcome::NoMatch, notice) {
        let mut sweep: Vec<_> = missed.into_iter().filter_map(|(id, mut p)| p.needs_calibration(&frame, ctx).then_some((id, p))).collect();
        if !sweep.is_empty() {
            write(format!("{prefix} no mechanics found — running a recalibration sweep now"));
            notice();
        }
        for (id, mut pending) in sweep.drain(..) {
            if let Some(line) = pending.calibrate(&frame, ctx) { write(line); }
            match pending.probe(&frame, ctx) {
                Ok(true) => {
                    pending.consume(Ok(&frame), ctx);
                    outcome = DispatchOutcome::Selected(id);
                    break;
                }
                Ok(false) => {}
                Err(error) => {
                    log::warn!("{prefix} probe failed: {error}");
                    outcome = DispatchOutcome::Failed(error);
                    break;
                }
            }
        }
    }
    let calibration_ms = ms(calibration_started);
    let result = match &outcome {
        DispatchOutcome::Selected(id) => format!("{id:?} selected"),
        DispatchOutcome::Failed(_) => "failed".to_string(),
        _ => "nothing detected".to_string(),
    };
    write(format!(
        "{prefix} {result} in {} ms (grab {grab_ms} ms, calibration {calibration_ms} ms, probes {probes_ms} ms)",
        ms(started),
    ));
    outcome
}

/// Prepare, delay and capture in one call; the threads split it at the queue.
#[cfg(test)]
pub fn dispatch<C>(
    press: Press,
    readers: &[&dyn ScreenReader<C>],
    delay: std::time::Duration,
    ctx: &C,
    sleep: &mut dyn FnMut(std::time::Duration),
    grab: &mut dyn FnMut() -> Result<crate::capture::Capture, crate::capture::CaptureError>,
) {
    dispatch_press(prepare_press(press, delay, readers, ctx), ctx, sleep, grab, &mut |_| panic!("read-only press must not wait"));
}

/// One key-sampling tick's inputs; `None` from the sampler ends the loop.
pub struct Sample {
    pub down: std::collections::BTreeSet<u16>,
    pub foreground: bool,
    pub(crate) foreground_kind: Option<crate::FocusState>,
    pub editable_focus: bool,
    pub settings: ReadHotkeySettings,
    pub exchange: crate::exchange_read::ExchangeReadSettings,
    pub dismiss: Vec<Shortcut>,
}

pub(crate) fn read_eligible(foreground: crate::FocusState, editable_focus: bool) -> bool {
    matches!(foreground, crate::FocusState::Game)
        || (matches!(foreground, crate::FocusState::OwnWindow) && !editable_focus)
}

/// The key-sampling side: prepares eligible readers at each press and queues it, so
/// delay, grab and reading never pause sampling.
pub fn run_sampler<C>(
    ctx: &C,
    readers: &[&dyn ScreenReader<C>],
    sample: &mut dyn FnMut() -> Option<Sample>,
    cursor: &mut dyn FnMut() -> Option<(i32, i32)>,
    queue: &std::sync::mpsc::Sender<PreparedPress<C>>,
    on_accept: &mut dyn FnMut() -> Accepted,
    on_dismiss: &mut dyn FnMut(&Shortcut) -> u64,
    on_exchange: &mut dyn FnMut(&Press) -> ExchangePress<C>,
) {
    let mut poll = ReadPoll::default();
    let mut exchange_poll = ReadPoll::default();
    let mut dismiss_poll = DismissPoll::default();
    while let Some(s) = sample() {
        let dismissed = dismiss_poll.poll(&s, on_dismiss);
        let eligible = s.foreground_kind.map_or(s.foreground, |kind| read_eligible(kind, s.editable_focus));
        if let Some(press) = poll.poll(&s.down, eligible, &s.settings.shortcut, cursor) {
            let accepted = on_accept();
            let delay = std::time::Duration::from_millis(u64::from(s.settings.delay_ms));
            let mut armed = prepare_press(press, delay, readers, ctx);
            armed.dismissed = dismissed;
            armed.finish = accepted.finish;
            armed.sweep = accepted.sweep;
            if queue.send(armed).is_err() {
                return;
            }
        }
        if let Some(press) = exchange_poll.poll(&s.down, s.foreground, &s.exchange.shortcut, cursor) {
            let exchange = on_exchange(&press);
            let armed = PreparedPress {
                delay: std::time::Duration::from_millis(u64::from(s.settings.delay_ms)),
                pendings: vec![(ReaderId::Exchange, exchange.pending)],
                dismissed: Vec::new(),
                finish: Some(exchange.finish),
                log: None,
                sweep: None,
            };
            if queue.send(armed).is_err() { return; }
        }
    }
}

/// The dispatch side: each queued press in order gets its own delay and grab.
pub fn run_dispatcher<C>(
    ctx: &C,
    queue: impl IntoIterator<Item = PreparedPress<C>>,
    sleep: &mut dyn FnMut(std::time::Duration),
    grab: &mut dyn FnMut() -> Result<crate::capture::Capture, crate::capture::CaptureError>,
    await_hidden: &mut dyn FnMut(&[u64]) -> bool,
) {
    for mut armed in queue {
        let finish = armed.finish.take();
        let outcome = dispatch_press(armed, ctx, sleep, grab, await_hidden);
        if let Some(finish) = finish { finish(outcome); }
    }
}

/// Starts the app-life key-sampling and dispatch threads. The sampler stops
/// when `cancel` turns true; the dispatcher stops once the sampler is gone.
pub fn spawn_read_hotkey(app: tauri::AppHandle, cancel: std::sync::Arc<std::sync::atomic::AtomicBool>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let dispatch_app = app.clone();
    std::thread::spawn(move || {
        let log_app = dispatch_app.clone();
        let queue = rx.into_iter().map(move |mut armed: PreparedPress<tauri::AppHandle>| {
            let app = log_app.clone();
            armed.log = Some(Box::new(move |line| crate::app_log(&app, line)));
            armed
        });
        run_dispatcher(
            &dispatch_app,
            queue,
            &mut std::thread::sleep,
            &mut || crate::capture::capture_screen(&dispatch_app),
            &mut |gen| crate::overlay_dismiss::await_hidden(&dispatch_app, gen),
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
        run_sampler(&app, READERS, &mut sample, &mut platform::cursor, &tx, &mut || {
            let stamp = crate::read_prompt::accept_press(&app);
            let (result_app, notice_app) = (app.clone(), app.clone());
            accepted(
                stamp,
                Box::new(move |outcome| crate::read_prompt::finish_read(&result_app, stamp, outcome)),
                Box::new(move || crate::read_prompt::begin_sweep_for(&notice_app, stamp)),
            )
        }, &mut |shortcut| crate::overlay_dismiss::publish(&app, shortcut), &mut |_| {
            use tauri::Emitter;
            let pressed_at_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default().as_millis() as u64;
            let standing = *app.state::<crate::AppState>().screen.lock().unwrap_or_else(|e| e.into_inner());
            let (pending, handle) = crate::exchange_read::reader::prepare(standing, Box::new(crate::ocr::recognize_lines));
            let result_app = app.clone();
            ExchangePress {
                pending: Box::new(pending),
                finish: Box::new(move |outcome| crate::exchange_read::finish_logged(outcome, &handle, pressed_at_ms, &mut |event| {
                    if let Err(error) = result_app.emit("exchange-read", event) {
                        log::warn!("Exchange read: event emission failed: {error}");
                    }
                }, &mut |line| crate::app_log(&result_app, line))),
            }
        });
    });
}

#[cfg(windows)]
mod platform {
    use super::*;
    pub fn sample(app: &tauri::AppHandle) -> Option<Sample> {
        let state = app.state::<crate::AppState>();
        let settings = state.read_hotkey.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let dismiss = state.overlay_dismiss.lock().unwrap_or_else(|e| e.into_inner()).shortcuts();
        let exchange = state.exchange_read.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let down = polled_keys(&settings.shortcut, &exchange.shortcut, &dismiss)
            .into_iter()
            .filter(|key| {
                (unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(*key as i32) }
                    as u16
                    & 0x8000)
                    != 0
            })
            .collect();
        let foreground = state.game_in_foreground.load(std::sync::atomic::Ordering::SeqCst);
        let (kind, editable_focus) = crate::focus::read_focus();
        Some(Sample { down, foreground, foreground_kind: Some(kind), editable_focus, settings, exchange, dismiss })
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
        Some(Sample { down: Default::default(), foreground: false, foreground_kind: None, editable_focus: false, settings: ReadHotkeySettings::default(), exchange: Default::default(), dismiss: Vec::new() })
    }
    pub fn cursor() -> Option<(i32, i32)> {
        None
    }
}

#[cfg(test)]
mod dispatch_tests {
    use super::*;
    use crate::capture::{Capture, CaptureError, GrabTiming};
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::Duration;

    /// The test readers' world: the context `arm` fixes, and what reads saw.
    #[derive(Default)]
    struct World {
        context: Mutex<String>,
        armed_order: Mutex<Vec<&'static str>>,
        consumed: Mutex<Vec<(&'static str, String, Result<u32, String>)>>,
        recent: Mutex<Option<crate::read_prompt::Mechanic>>,
        misses: Mutex<Vec<&'static str>>,
        probes: Mutex<Vec<(&'static str, u32)>>,
        probe_error: Mutex<bool>,
        trace: Mutex<Vec<String>>,
        uncalibrated: Mutex<Vec<&'static str>>,
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
        fn id(&self) -> ReaderId {
            match self.name { "b" => ReaderId::Mercenary, "g" => ReaderId::Gem, "f" => ReaderId::Font, _ => ReaderId::Temple }
        }
        fn recent(&self, ctx: &Arc<World>) -> Option<crate::read_prompt::Mechanic> { *ctx.recent.lock().unwrap() }
        fn prepare(&self, ctx: &Arc<World>, _press: &Press) -> Option<Box<dyn PendingRead<Arc<World>>>> {
            if !self.armed {
                return None;
            }
            ctx.armed_order.lock().unwrap().push(self.name);
            let context = ctx.context.lock().unwrap().clone();
            Some(Box::new(TestPending { name: self.name, context }))
        }
    }
    impl PendingRead<Arc<World>> for TestPending {
        fn needs_calibration(&mut self, _frame: &Arc<Capture>, ctx: &Arc<World>) -> bool {
            ctx.uncalibrated.lock().unwrap().contains(&self.name)
        }
        fn calibrate(&mut self, _frame: &Arc<Capture>, ctx: &Arc<World>) -> Option<String> {
            ctx.trace.lock().unwrap().push("calibrate".into());
            ctx.uncalibrated.lock().unwrap().retain(|n| *n != self.name);
            ctx.misses.lock().unwrap().retain(|n| *n != self.name);
            Some(format!("calibrated {}", self.name))
        }
        fn probe(&mut self, frame: &Arc<Capture>, ctx: &Arc<World>) -> Result<bool, String> {
            ctx.trace.lock().unwrap().push("probe".into());
            ctx.probes.lock().unwrap().push((self.name, frame.monitor_id));
            if *ctx.probe_error.lock().unwrap() { return Err("recognition failed".into()); }
            Ok(!ctx.misses.lock().unwrap().contains(&self.name))
        }
        fn consume(self: Box<Self>, frame: Result<&Arc<Capture>, &str>, ctx: &Arc<World>) {
            ctx.trace.lock().unwrap().push("consume".into());
            let frame = frame.map(|c| c.monitor_id).map_err(str::to_string);
            ctx.consumed.lock().unwrap().push((self.name, self.context, frame));
        }
    }
    const A: TestReader = TestReader { name: "a", armed: true };
    const B: TestReader = TestReader { name: "b", armed: true };
    const IDLE: TestReader = TestReader { name: "idle", armed: false };
    const GEM: TestReader = TestReader { name: "g", armed: true };
    const FONT: TestReader = TestReader { name: "f", armed: true };

    /// The production acceptance over `state`'s prompt: the result hook and,
    /// inside a recalibration offer, the sweep.
    fn accept_in(state: &Arc<crate::AppState>) -> Accepted {
        let stamp = crate::read_prompt::accept_press_in(state);
        let (result_state, notice_state) = (state.clone(), state.clone());
        accepted(
            stamp,
            Box::new(move |outcome| { crate::read_prompt::finish_read_in(&result_state, stamp, outcome); }),
            Box::new(move || { crate::read_prompt::begin_sweep_in(&notice_state); }),
        )
    }

    /// One read-key press through the sampler and the dispatcher; its grabs.
    fn press_once(world: &Arc<World>, state: &Arc<crate::AppState>) -> usize {
        let (tx, rx) = mpsc::channel();
        let mut samples = vec![sample(PRESSED, true)].into_iter();
        run_sampler(world, &[&A], &mut || samples.next(), &mut || None, &tx, &mut || accept_in(state), &mut |_| panic!("no dismissal"), &mut no_exchange);
        drop(tx);
        let mut grabs = 0;
        run_dispatcher(world, rx, &mut |_| {}, &mut || { grabs += 1; Ok(capture(1)) }, &mut |_| panic!("no dismissal to await"));
        grabs
    }

    fn calibrated(world: &Arc<World>) -> bool {
        world.trace.lock().unwrap().iter().any(|t| t == "calibrate")
    }

    fn result_pipeline(selected: bool, handshake: bool, capture_error: Option<CaptureError>) -> (usize, crate::read_prompt::ReadPromptSnapshot) {
        let world = Arc::new(World::default());
        if !selected { world.misses.lock().unwrap().push("a"); }
        let state = Arc::new(crate::settings::tests::test_app_state());
        state.modules_enabled.lock().unwrap().insert(crate::mercenary::MODULE_ID.into(), true);
        crate::read_prompt::apply_in(&state, crate::read_prompt::Change::Arm(crate::read_prompt::Mechanic::Mercenary));
        let (tx, rx) = mpsc::channel();
        let mut shared = sample(PRESSED, true);
        shared.dismiss = vec![shared.settings.shortcut.clone()];
        let mut samples = vec![shared].into_iter();
        run_sampler(&world, &[&A], &mut || samples.next(), &mut || None, &tx, &mut || accept_in(&state), &mut |_| 1, &mut no_exchange);
        drop(tx);
        let mut grabs = 0;
        let mut error = capture_error;
        run_dispatcher(&world, rx, &mut |_| {}, &mut || {
            grabs += 1;
            match error.take() { Some(error) => Err(error), None => Ok(capture(1)) }
        }, &mut |gens| { assert_eq!(gens, &[1]); handshake });
        let snapshot = state.read_prompt.lock().unwrap().snapshot();
        (grabs, snapshot)
    }

    #[test]
    fn dispatched_all_miss_frame_publishes_no_match_after_one_capture() {
        let (grabs, snapshot) = result_pipeline(false, true, None);
        assert_eq!(grabs, 1);
        assert_eq!(snapshot, crate::read_prompt::ReadPromptSnapshot { mechanic: None, no_match: true, reading: false, seq: 3, recalibrating: false, gem_uncertain: [false; 3], recalibrate_offer: true });
    }

    #[test]
    fn failed_hide_handshake_preserves_prompt_without_capture() {
        let (grabs, snapshot) = result_pipeline(false, false, None);
        assert_eq!(grabs, 0);
        assert_eq!(snapshot, crate::read_prompt::ReadPromptSnapshot { mechanic: Some(crate::read_prompt::Mechanic::Mercenary), no_match: false, reading: false, seq: 2, recalibrating: false, gem_uncertain: [false; 3], recalibrate_offer: false });
    }

    #[test]
    fn dispatched_selected_frame_does_not_publish_no_match() {
        let (grabs, snapshot) = result_pipeline(true, true, None);
        assert_eq!(grabs, 1);
        assert_eq!(snapshot, crate::read_prompt::ReadPromptSnapshot { mechanic: Some(crate::read_prompt::Mechanic::Mercenary), no_match: false, reading: false, seq: 3, recalibrating: false, gem_uncertain: [false; 3], recalibrate_offer: false });
    }

    #[test]
    fn dispatched_capture_failure_preserves_prompt() {
        let (grabs, snapshot) = result_pipeline(false, true, Some(CaptureError::Failed("no monitor".into())));
        assert_eq!(grabs, 1);
        assert_eq!(snapshot, crate::read_prompt::ReadPromptSnapshot { mechanic: Some(crate::read_prompt::Mechanic::Mercenary), no_match: false, reading: false, seq: 2, recalibrating: false, gem_uncertain: [false; 3], recalibrate_offer: false });
    }

    fn route(world: &Arc<World>, readers: &[&dyn ScreenReader<Arc<World>>]) -> DispatchOutcome {
        let prepared = prepare_press(press(), Duration::ZERO, readers, world);
        dispatch_press(prepared, world, &mut |_| {}, &mut || Ok(capture(42)), &mut |_| panic!("no dismissal to await"))
    }

    #[test]
    fn latest_client_mechanic_wins_before_registry_order() {
        let world = Arc::new(World::default());
        *world.recent.lock().unwrap() = Some(crate::read_prompt::Mechanic::Mercenary);
        assert_eq!(route(&world, &[&A, &GEM, &B]), DispatchOutcome::Selected(ReaderId::Mercenary));
        assert_eq!(*world.probes.lock().unwrap(), vec![("b", 42)]);
        assert_eq!(*world.consumed.lock().unwrap(), vec![("b", String::new(), Ok(42))]);
    }
    #[test]
    fn armed_font_wins_before_client_recency() {
        let world = Arc::new(World::default());
        *world.recent.lock().unwrap() = Some(crate::read_prompt::Mechanic::Mercenary);
        assert_eq!(route(&world, &[&A, &FONT, &GEM, &B]), DispatchOutcome::Selected(ReaderId::Font));
        assert_eq!(*world.probes.lock().unwrap(), vec![("f", 42)]);
    }
    #[test]
    fn font_miss_falls_through_to_recent_mechanic() {
        let world = Arc::new(World::default());
        *world.recent.lock().unwrap() = Some(crate::read_prompt::Mechanic::Mercenary);
        *world.misses.lock().unwrap() = vec!["f"];
        assert_eq!(route(&world, &[&A, &FONT, &GEM, &B]), DispatchOutcome::Selected(ReaderId::Mercenary));
        assert_eq!(*world.probes.lock().unwrap(), vec![("f", 42), ("b", 42)]);
        assert_eq!(*world.consumed.lock().unwrap(), vec![("b", String::new(), Ok(42))]);
    }
    #[test]
    fn all_misses_return_no_match_after_one_capture() {
        let world = Arc::new(World::default());
        *world.misses.lock().unwrap() = vec!["a", "g", "b"];
        let prepared = prepare_press(press(), Duration::ZERO, &[&A, &GEM, &B], &world);
        let mut captures = 0;
        let result = dispatch_press(prepared, &world, &mut |_| {}, &mut || { captures += 1; Ok(capture(7)) }, &mut |_| panic!("no dismissal to await"));
        assert_eq!((captures, result), (1, DispatchOutcome::NoMatch));
        assert_eq!(*world.probes.lock().unwrap(), vec![("a", 7), ("g", 7), ("b", 7)]);
        assert!(world.consumed.lock().unwrap().is_empty());
    }
    #[test]
    fn no_candidates_return_no_match_after_one_capture() {
        let world = Arc::new(World::default());
        let prepared = prepare_press(press(), Duration::ZERO, &[], &world);
        let mut captures = 0;
        let result = dispatch_press(prepared, &world, &mut |_| {}, &mut || { captures += 1; Ok(capture(7)) }, &mut |_| panic!("no dismissal to await"));
        assert_eq!((captures, result), (1, DispatchOutcome::NoMatch));
    }
    #[test]
    fn probe_error_stops_routing_without_no_match_or_consumption() {
        let world = Arc::new(World::default());
        *world.probe_error.lock().unwrap() = true;
        assert_eq!(route(&world, &[&A, &B]), DispatchOutcome::Failed("recognition failed".into()));
        assert_eq!(*world.probes.lock().unwrap(), vec![("a", 42)]);
        assert!(world.consumed.lock().unwrap().is_empty());
    }

    #[test]
    fn unavailable_latest_mechanic_is_skipped() {
        let world = Arc::new(World::default());
        *world.recent.lock().unwrap() = Some(crate::read_prompt::Mechanic::Incursion);
        assert_eq!(route(&world, &[&IDLE, &GEM, &B]), DispatchOutcome::Selected(ReaderId::Gem));
        assert_eq!(*world.probes.lock().unwrap(), vec![("g",42)]);
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
    fn press() -> Press {
        Press { cursor: Some((5, 6)) }
    }
    fn sample(keys: &[u16], foreground: bool) -> Sample {
        Sample {
            down: keys.iter().copied().collect(),
            foreground,
            foreground_kind: None,
            editable_focus: false,
            settings: ReadHotkeySettings::default(),
            exchange: crate::exchange_read::ExchangeReadSettings::default(),
            dismiss: crate::overlay_dismiss::DismissSettings::default().shortcuts(),
        }
    }
    /// Ctrl+A, the default read key.
    const PRESSED: &[u16] = &[17, 65];
    /// Alt + Right click, the default Read exchange binding.
    const EXCHANGE: &[u16] = &[18, 2];

    fn no_exchange(_: &Press) -> ExchangePress<Arc<World>> {
        panic!("no exchange press expected")
    }

    mod keep_regression {
        use super::*;

        #[test]
        fn keep_regression_read_key_press_while_own_overlay_focused_reads_and_triggers_prompt() {
            let world = Arc::new(World::default());
            let state = Arc::new(crate::settings::tests::test_app_state());
            let (tx, rx) = mpsc::channel();
            let mut own = sample(PRESSED, false);
            own.foreground_kind = Some(crate::FocusState::OwnWindow);
            own.settings.delay_ms = 300;
            let mut samples = vec![own].into_iter();
            run_sampler(&world, &[&A], &mut || samples.next(), &mut || None, &tx,
                &mut || accept_in(&state), &mut |_| panic!("own-window read must not widen dismiss"), &mut no_exchange);
            drop(tx);
            let queued: Vec<_> = rx.into_iter().collect();
            assert_eq!(queued.len(), 1, "OwnWindow Ctrl+A must queue a read before dispatch");
            let pending = state.read_prompt.lock().unwrap().snapshot();
            assert!(pending.reading, "accepted press must present progress before delay/capture");
            assert_eq!(pending.seq, 1, "acceptance has its own presentation identity");
            assert!(world.probes.lock().unwrap().is_empty(), "completion is held in the queue");
            assert!(world.consumed.lock().unwrap().is_empty(), "triggered progress precedes read results");
            let mut grabs = 0;
            run_dispatcher(&world, queued, &mut |delay| assert_eq!(delay, Duration::from_millis(300)),
                &mut || { grabs += 1; Ok(capture(9)) }, &mut |_| panic!("no dismissal"));
            assert_eq!(grabs, 1);
            assert_eq!(*world.consumed.lock().unwrap(), vec![("a", String::new(), Ok(9))]);
            let done = state.read_prompt.lock().unwrap().snapshot();
            assert!(!done.reading);
            assert!(!done.no_match);
        }
    }

    fn assert_ignored_read(kind: crate::FocusState, editable: bool) {
        let world = Arc::new(World::default());
        let state = Arc::new(crate::settings::tests::test_app_state());
        state.read_prompt.lock().unwrap().arm(crate::read_prompt::Mechanic::Mercenary);
        let before = state.read_prompt.lock().unwrap().snapshot();
        let (tx, rx) = mpsc::channel();
        let mut input = sample(PRESSED, false);
        input.foreground_kind = Some(kind);
        input.editable_focus = editable;
        let mut samples = vec![input].into_iter();
        run_sampler(&world, &[&A], &mut || samples.next(), &mut || None, &tx,
            &mut || accept_in(&state), &mut |_| panic!("no dismissal"), &mut no_exchange);
        drop(tx);
        let queued: Vec<_> = rx.into_iter().collect();
        assert!(queued.is_empty(), "silent unwanted OCR read was queued");
        run_dispatcher(&world, queued, &mut |_| panic!("no delay"),
            &mut || panic!("silent unwanted capture"), &mut |_| panic!("no dismissal"));
        assert!(world.armed_order.lock().unwrap().is_empty());
        assert!(world.consumed.lock().unwrap().is_empty());
        assert_eq!(state.read_prompt.lock().unwrap().snapshot(), before);
    }

    mod focus_keep_critical {
        use super::*;

        #[test]
        fn keep_critical_read_key_press_in_own_text_input_does_not_read() {
            assert_ignored_read(crate::FocusState::OwnWindow, true);
        }

        #[test]
        fn keep_critical_read_key_press_while_other_app_focused_does_not_read() {
            assert_ignored_read(crate::FocusState::Other, false);
        }

        #[test]
        fn keep_critical_foreground_decision_prevents_silent_unwanted_capture() {
            use crate::FocusState::*;
            for (kind, editable, expected) in [(Game, false, true), (Game, true, true),
                (OwnWindow, false, true), (OwnWindow, true, false), (Other, false, false), (Other, true, false)] {
                assert_eq!(read_eligible(kind, editable), expected, "{kind:?}, editable={editable}");
            }
        }
    }

    /// What the sampler and dispatcher did with a run of samples when the
    /// exchange reader is "x" and the read key serves only reader A.
    #[derive(Debug, PartialEq)]
    struct ExchangeRun {
        grabs: u32,
        /// Exchange presses prepared by the sampler, counted before any dispatch.
        prepared_at_press: usize,
        /// What each exchange press's own result hook received.
        exchange_outcomes: Vec<DispatchOutcome>,
        /// Read-prompt acceptances, which only read-key presses take.
        read_accepts: usize,
    }

    fn run_with_exchange(world: &Arc<World>, samples: Vec<Sample>) -> ExchangeRun {
        let (tx, rx) = mpsc::channel();
        let mut samples = samples.into_iter();
        let outcomes = Arc::new(Mutex::new(Vec::new()));
        let mut prepared = 0;
        let mut accepts = 0;
        run_sampler(world, &[&A], &mut || samples.next(), &mut || None, &tx, &mut || {
            accepts += 1;
            Accepted::default()
        }, &mut |_| panic!("no dismissal"), &mut |_press| {
            prepared += 1;
            let sink = outcomes.clone();
            ExchangePress {
                pending: Box::new(TestPending { name: "x", context: String::new() }),
                finish: Box::new(move |outcome| sink.lock().unwrap().push(outcome)),
            }
        });
        drop(tx);
        let prepared_at_press = prepared;
        let mut grabs = 0;
        run_dispatcher(world, rx, &mut |_| {}, &mut || {
            grabs += 1;
            Ok(capture(grabs))
        }, &mut |_| panic!("an exchange press carries no dismissal"));
        let exchange_outcomes = outcomes.lock().unwrap().clone();
        ExchangeRun { grabs, prepared_at_press, exchange_outcomes, read_accepts: accepts }
    }

    fn exchange_sample(keys: &[u16], foreground: bool, binding: &Shortcut) -> Sample {
        Sample { exchange: crate::exchange_read::ExchangeReadSettings { shortcut: binding.clone() }, ..sample(keys, foreground) }
    }

    mod keep_critical {
        use super::*;

        fn alt_right() -> Shortcut {
            crate::exchange_read::ExchangeReadSettings::default().shortcut
        }

        // keep:critical — irreversible/silent: a Read exchange press that grabs
        // more than once breaks the one-capture-per-press rule, and one routed
        // through the read-key readers fills a module from a frame the player
        // pressed to read the exchange.
        #[test]
        fn one_read_exchange_press_grabs_once_and_reads_only_through_the_exchange_reader() {
            let shift_middle = Shortcut { key: 4, shift: true, ctrl: false, alt: false };
            for (binding, keys) in [(alt_right(), EXCHANGE), (shift_middle, &[16, 4][..])] {
                let world = Arc::new(World::default());

                let run = run_with_exchange(&world, vec![exchange_sample(keys, true, &binding), exchange_sample(&[], true, &binding)]);

                assert_eq!(
                    run,
                    ExchangeRun { grabs: 1, prepared_at_press: 1, exchange_outcomes: vec![DispatchOutcome::Selected(ReaderId::Exchange)], read_accepts: 0 },
                    "{binding:?}",
                );
                assert_eq!(*world.probes.lock().unwrap(), vec![("x", 1)], "{binding:?}");
                assert_eq!(*world.consumed.lock().unwrap(), vec![("x", String::new(), Ok(1))], "{binding:?}");
                assert!(world.armed_order.lock().unwrap().is_empty(), "{binding:?}: a read-key reader was armed");
            }
        }

        // keep:critical — irreversible: a held binding taken as repeated presses
        // grabs a frame per sampler tick.
        #[test]
        fn holding_the_read_exchange_binding_is_one_press() {
            let world = Arc::new(World::default());
            let held = || exchange_sample(EXCHANGE, true, &alt_right());

            let run = run_with_exchange(&world, vec![held(), held(), held()]);

            assert_eq!((run.grabs, run.prepared_at_press, run.exchange_outcomes.len()), (1, 1, 1));
        }

        // keep:critical — irreversible: a grab on an inexact chord or with the
        // game in the background captures a screen the player never pressed
        // the exchange binding on.
        #[test]
        fn an_inexact_chord_or_a_background_game_makes_no_exchange_press() {
            let cases: [(&[u16], bool); 6] = [
                (&[16, 18, 2], true),
                (&[17, 18, 2], true),
                (&[2], true),
                (&[18], true),
                (&[18, 4], true),
                (EXCHANGE, false),
            ];
            for (keys, foreground) in cases {
                let world = Arc::new(World::default());

                let run = run_with_exchange(&world, vec![exchange_sample(keys, foreground, &alt_right())]);

                assert_eq!(
                    run,
                    ExchangeRun { grabs: 0, prepared_at_press: 0, exchange_outcomes: Vec::new(), read_accepts: 0 },
                    "keys {keys:?}, foreground {foreground}",
                );
            }
        }

        // keep:critical — silent (AC-4): a read-key press that reaches the
        // exchange reader fills the Companion from a frame read for another module.
        #[test]
        fn a_read_key_press_never_reaches_the_exchange_reader() {
            let world = Arc::new(World::default());

            let run = run_with_exchange(&world, vec![exchange_sample(PRESSED, true, &alt_right())]);

            assert_eq!(
                run,
                ExchangeRun { grabs: 1, prepared_at_press: 0, exchange_outcomes: Vec::new(), read_accepts: 1 },
            );
            assert_eq!(*world.probes.lock().unwrap(), vec![("a", 1)]);
        }

        // keep:critical — silent (AC-4): a read-key press after an exchange press
        // that shares or replaces the exchange press's result reports a second
        // or a wrong exchange outcome.
        #[test]
        fn a_read_key_press_after_an_exchange_press_leaves_the_exchange_outcome_alone() {
            let world = Arc::new(World::default());
            let binding = alt_right();

            let run = run_with_exchange(&world, vec![
                exchange_sample(EXCHANGE, true, &binding),
                exchange_sample(&[], true, &binding),
                exchange_sample(PRESSED, true, &binding),
            ]);

            assert_eq!(
                run,
                ExchangeRun { grabs: 2, prepared_at_press: 1, exchange_outcomes: vec![DispatchOutcome::Selected(ReaderId::Exchange)], read_accepts: 1 },
            );
            assert_eq!(
                *world.consumed.lock().unwrap(),
                vec![("x", String::new(), Ok(1)), ("a", String::new(), Ok(2))],
            );
        }
    }

    #[test]
    fn a_shared_press_grabs_only_after_every_host_acknowledges_its_dismissal() {
        let world = Arc::new(World::default());
        let ledger = crate::overlay_dismiss::DismissLedger::default();
        ledger.register("temple".into());
        let events = &world.trace;
        let mut shared = sample(PRESSED, true);
        shared.dismiss = vec![shared.settings.shortcut.clone()];
        let (tx, rx) = mpsc::channel();
        let mut samples = vec![shared].into_iter();
        run_sampler(&world, &[&A], &mut || samples.next(), &mut || {
            assert_eq!(ledger.pending().len(), 1, "dismiss before evaluating the read edge");
            None
        }, &tx, &mut Accepted::default, &mut |shortcut| {
            assert!(world.armed_order.lock().unwrap().is_empty(), "dismiss before arming readers");
            let gen = ledger.publish(shortcut).gen;
            events.lock().unwrap().push(format!("dismiss({gen})"));
            gen
        }, &mut no_exchange);
        drop(tx);
        run_dispatcher(&world, rx, &mut |_| events.lock().unwrap().push("sleep".into()), &mut || {
            assert!(world.probes.lock().unwrap().is_empty(), "capture before probing");
            events.lock().unwrap().push("grab".into());
            Ok(capture(1))
        }, &mut |generations| {
            assert_eq!(generations, &[1]);
            assert!(world.probes.lock().unwrap().is_empty(), "hide acknowledgement before probing");
            let gen = generations[0];
            events.lock().unwrap().push(format!("await_hidden({gen}) starts"));
            assert_eq!(ledger.ready(gen, &mut |_| true), None);
            ledger.ack("temple", gen);
            events.lock().unwrap().push(format!("ack(temple, {gen})"));
            let ready = ledger.ready(gen, &mut |_| true) == Some(true);
            events.lock().unwrap().push(format!("await_hidden returns {ready}"));
            ready
        });
        assert_eq!(*events.lock().unwrap(), vec!["dismiss(1)", "await_hidden(1) starts", "ack(temple, 1)", "await_hidden returns true", "sleep", "grab", "probe", "consume"]);
        assert_eq!(world.consumed.lock().unwrap().len(), 1);
        assert_eq!(*world.probes.lock().unwrap(), vec![("a", 1)]);
    }

    #[test]
    fn a_shared_press_without_an_acknowledgement_in_time_takes_no_grab_and_reads_nothing() {
        let world = Arc::new(World::default());
        let mut armed = prepare_press(press(), Duration::ZERO, &[&A], &world);
        armed.dismissed = vec![1];
        let outcome = dispatch_press(armed, &world, &mut |_| panic!("must not sleep"), &mut || panic!("must not grab"), &mut |generations| {
            assert_eq!(generations, &[1]);
            false
        });
        assert_eq!(outcome, DispatchOutcome::Suppressed);
        assert!(world.probes.lock().unwrap().is_empty());
        assert!(world.consumed.lock().unwrap().is_empty());
    }

    #[test]
    fn empty_candidates_still_require_the_hide_barrier_before_capture() {
        let world = Arc::new(World::default());
        let mut press = prepare_press(press(), Duration::ZERO, &[], &world);
        press.dismissed = vec![1,2];
        let outcome = dispatch_press(press, &world, &mut |_| panic!("must not delay"),
            &mut || panic!("must not capture"), &mut |generations| { assert_eq!(generations, &[1,2]); false });
        assert_eq!(outcome, DispatchOutcome::Suppressed);
        assert!(world.probes.lock().unwrap().is_empty());
        assert!(world.consumed.lock().unwrap().is_empty());
    }

    #[test]
    fn a_later_generation_ack_does_not_release_an_unapplied_earlier_dismissal() {
        use std::cell::Cell;
        for acknowledge_earlier in [false, true] {
            let world = Arc::new(World::default());
            let ledger = crate::overlay_dismiss::DismissLedger::default();
            ledger.register("temple".into());
            let mut f = sample(&[70], true);
            f.settings.shortcut = Shortcut { key: 70, shift: false, ctrl: false, alt: false };
            f.dismiss = vec![f.settings.shortcut.clone(), crate::overlay_dismiss::DismissSettings::default().shortcut];
            let esc = Sample { down: [27].into_iter().collect(), foreground: true, foreground_kind: None, editable_focus: false, settings: f.settings.clone(), exchange: f.exchange.clone(), dismiss: f.dismiss.clone() };
            let (tx, rx) = mpsc::channel();
            let mut samples = vec![f, esc].into_iter();
            run_sampler(&world, &[&A], &mut || samples.next(), &mut || None, &tx, &mut Accepted::default, &mut |s| ledger.publish(s).gen, &mut no_exchange);
            drop(tx);
            ledger.ack("temple", 2);
            let elapsed = Cell::new(Duration::ZERO);
            let mut grabs = 0;
            run_dispatcher(&world, rx, &mut |_| {}, &mut || {
                assert_eq!(ledger.ready(1, &mut |_| true), Some(true));
                grabs += 1;
                Ok(capture(1))
            }, &mut |gen| ledger.await_with(gen, &mut |_| true, &mut || elapsed.get(), &mut |remaining| {
                if acknowledge_earlier { ledger.ack("temple", 1); }
                elapsed.set(if acknowledge_earlier { Duration::from_millis(20) } else { remaining });
            }));
            assert_eq!(grabs, usize::from(acknowledge_earlier));
            assert_eq!(world.probes.lock().unwrap().len(), usize::from(acknowledge_earlier));
            assert_eq!(world.consumed.lock().unwrap().len(), usize::from(acknowledge_earlier));
        }
    }

    #[test]
    fn same_tick_dismissals_with_only_the_later_ack_take_no_grab() {
        same_tick_dismissals(false);
    }

    #[test]
    fn same_tick_dismissals_grab_once_when_the_earlier_ack_arrives_before_the_deadline() {
        same_tick_dismissals(true);
    }

    fn same_tick_dismissals(acknowledge_earlier: bool) {
        use std::cell::Cell;
        let world = Arc::new(World::default());
        let ledger = crate::overlay_dismiss::DismissLedger::default();
        ledger.register("temple".into());
        let mut shared = sample(&[70, 71], true);
        shared.settings.shortcut = Shortcut { key: 70, shift: false, ctrl: false, alt: false };
        shared.dismiss = vec![shared.settings.shortcut.clone(), Shortcut { key: 71, ..shared.settings.shortcut.clone() }];
        let (tx, rx) = mpsc::channel();
        let mut samples = vec![shared].into_iter();
        run_sampler(&world, &[&A], &mut || samples.next(), &mut || None, &tx, &mut Accepted::default, &mut |s| ledger.publish(s).gen, &mut no_exchange);
        drop(tx);
        ledger.ack("temple", 2);
        let elapsed = Cell::new(Duration::ZERO);
        let mut grabs = 0;
        run_dispatcher(&world, rx, &mut |_| {}, &mut || {
            assert_eq!(ledger.ready(1, &mut |_| true), Some(true), "F dismissal must be applied before capture");
            grabs += 1;
            Ok(capture(1))
        }, &mut |gen| {
            assert_eq!(gen, &[1, 2], "all same-tick dismissals must be acknowledged");
            ledger.await_with(gen, &mut |_| true, &mut || elapsed.get(), &mut |remaining| {
                if acknowledge_earlier { ledger.ack("temple", 1); }
                elapsed.set(if acknowledge_earlier { Duration::from_millis(20) } else { remaining });
            })
        });
        assert_eq!(grabs, usize::from(acknowledge_earlier));
        assert_eq!(world.probes.lock().unwrap().len(), usize::from(acknowledge_earlier));
        assert_eq!(world.consumed.lock().unwrap().len(), usize::from(acknowledge_earlier));
        assert_eq!(elapsed.get(), Duration::from_millis(if acknowledge_earlier { 20 } else { 500 }));
    }

    #[test]
    fn dismissal_keys_are_observed_once_per_foreground_press() {
        let world = Arc::new(World::default());
        let mut samples = vec![sample(&[27], false), sample(&[27], true), sample(&[], true), sample(&[27], true), sample(&[27], true)].into_iter();
        let (tx, rx) = mpsc::channel();
        let mut dismissed = Vec::new();
        run_sampler(&world, &[&A], &mut || samples.next(), &mut || None, &tx, &mut Accepted::default, &mut |s| {
            dismissed.push(s.key);
            dismissed.len() as u64
        }, &mut no_exchange);
        assert_eq!(dismissed, vec![27]);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn polled_keys_include_escape_and_every_distinct_override() {
        let mut settings = crate::overlay_dismiss::DismissSettings::default();
        for (id, key) in [("temple.offers", 70), ("lab.fontalerts", 71), ("mercenary.verdict", 70)] {
            settings.overlays.insert(id.into(), crate::overlay_dismiss::OverlayDismiss {
                shortcut: crate::overlay_dismiss::Inheritance::Override(Shortcut { key, ..settings.shortcut.clone() }),
                ..Default::default()
            });
        }
        let dismiss = settings.shortcuts();
        assert_eq!(dismiss.len(), 3);
        let exchange = crate::exchange_read::ExchangeReadSettings::default().shortcut;
        assert_eq!(polled_keys(&ReadHotkeySettings::default().shortcut, &exchange, &dismiss), [2,16,17,18,27,65,70,71].into_iter().collect());
    }

    /// Runs the sampler over `samples` and the dispatcher to the end; returns grab count.
    fn run_pipeline(world: &Arc<World>, readers: &[&dyn ScreenReader<Arc<World>>], samples: Vec<Sample>) -> usize {
        let (tx, rx) = mpsc::channel();
        let mut samples = samples.into_iter();
        run_sampler(world, readers, &mut || samples.next(), &mut || None, &tx, &mut Accepted::default, &mut |_| panic!("no dismissal"), &mut no_exchange);
        drop(tx);
        let mut grabs = 0;
        run_dispatcher(world, rx, &mut |_| {}, &mut || {
            grabs += 1;
            Ok(capture(grabs))
        }, &mut |_| panic!("no dismissal to await"));
        grabs as usize
    }

    mod keep_happy {
        use super::*;

        fn logged_press(world: &Arc<World>, readers: &[&dyn ScreenReader<Arc<World>>]) -> (DispatchOutcome, Vec<String>) {
            let lines = Arc::new(Mutex::new(Vec::new()));
            let sink = lines.clone();
            let mut prepared = prepare_press(press(), Duration::ZERO, readers, world);
            prepared.log = Some(Box::new(move |line| sink.lock().unwrap().push(line)));
            let notice_world = world.clone();
            prepared.sweep = Some(Box::new(move || notice_world.trace.lock().unwrap().push("notice".into())));
            let outcome = dispatch_press(prepared, world, &mut |_| {}, &mut || {
                world.trace.lock().unwrap().push("grab".into());
                Ok(capture(1))
            }, &mut |_| panic!("no dismissal to await"));
            let lines = lines.lock().unwrap().clone();
            (outcome, lines)
        }

        #[test]
        fn banked_probes_run_before_any_calibration_and_a_hit_skips_it() {
            let world = Arc::new(World::default());
            *world.uncalibrated.lock().unwrap() = vec!["a", "b"];
            *world.misses.lock().unwrap() = vec!["a"];
            let (outcome, lines) = logged_press(&world, &[&A, &B]);
            assert_eq!(outcome, DispatchOutcome::Selected(ReaderId::Mercenary));
            assert_eq!(*world.trace.lock().unwrap(), vec!["grab", "probe", "probe", "consume"]);
            assert!(!lines.iter().any(|l| l.starts_with("calibrated")), "{lines:?}");
        }

        #[test]
        fn an_all_miss_press_raises_the_sweep_notice_once_then_selects_the_calibrated_reader() {
            let world = Arc::new(World::default());
            *world.uncalibrated.lock().unwrap() = vec!["a"];
            *world.misses.lock().unwrap() = vec!["a", "b"];
            let (outcome, lines) = logged_press(&world, &[&A, &B]);
            assert_eq!(outcome, DispatchOutcome::Selected(ReaderId::Temple));
            assert_eq!(*world.trace.lock().unwrap(), vec!["grab", "probe", "probe", "notice", "calibrate", "probe", "consume"]);
            assert_eq!(lines.len(), 5, "{lines:?}");
            assert!(lines[0].starts_with("Read key: Temple probe — no match, "), "{lines:?}");
            assert!(lines[1].starts_with("Read key: Mercenary probe — no match, "), "{lines:?}");
            assert_eq!(lines[2], "Read key: no mechanics found — running a recalibration sweep now");
            assert_eq!(lines[3], "calibrated a");
            assert!(lines[4].starts_with("Read key: Temple selected in "), "{lines:?}");
        }

        #[test]
        fn a_no_match_press_offers_recalibration_without_sweeping() {
            let world = Arc::new(World::default());
            *world.uncalibrated.lock().unwrap() = vec!["a"];
            *world.misses.lock().unwrap() = vec!["a"];
            let state = Arc::new(crate::settings::tests::test_app_state());

            let grabs = press_once(&world, &state);

            assert_eq!(grabs, 1);
            assert!(!calibrated(&world), "{:?}", world.trace.lock().unwrap());
            let snapshot = state.read_prompt.lock().unwrap().snapshot();
            assert!(snapshot.no_match && snapshot.recalibrate_offer, "{snapshot:?}");
        }

        #[test]
        fn a_press_inside_the_offer_that_finds_nothing_sweeps_on_its_one_grab() {
            let world = Arc::new(World::default());
            *world.uncalibrated.lock().unwrap() = vec!["a"];
            *world.misses.lock().unwrap() = vec!["a"];
            let state = Arc::new(crate::settings::tests::test_app_state());
            press_once(&world, &state);
            world.trace.lock().unwrap().clear();

            let grabs = press_once(&world, &state);

            assert_eq!(grabs, 1);
            assert_eq!(*world.trace.lock().unwrap(), vec!["probe", "calibrate", "probe", "consume"]);
        }

        #[test]
        fn a_press_inside_the_offer_reads_a_mechanic_now_present_without_sweeping() {
            let world = Arc::new(World::default());
            *world.uncalibrated.lock().unwrap() = vec!["a"];
            *world.misses.lock().unwrap() = vec!["a"];
            let state = Arc::new(crate::settings::tests::test_app_state());
            press_once(&world, &state);
            world.misses.lock().unwrap().clear();
            world.trace.lock().unwrap().clear();

            press_once(&world, &state);

            assert_eq!(*world.trace.lock().unwrap(), vec!["probe", "consume"]);
            assert!(!state.read_prompt.lock().unwrap().snapshot().no_match);
        }

        #[test]
        fn an_expired_offer_clears_the_prompt_and_the_next_press_does_not_sweep() {
            let world = Arc::new(World::default());
            *world.uncalibrated.lock().unwrap() = vec!["a"];
            *world.misses.lock().unwrap() = vec!["a"];
            let state = Arc::new(crate::settings::tests::test_app_state());
            press_once(&world, &state);
            let shown = crate::read_prompt::no_match_id_in(&state).expect("the no-match result is shown");

            crate::read_prompt::expire_no_match_in(&state, shown);

            let snapshot = state.read_prompt.lock().unwrap().snapshot();
            assert!(!snapshot.no_match && !snapshot.recalibrate_offer, "{snapshot:?}");
            press_once(&world, &state);
            assert!(!calibrated(&world), "{:?}", world.trace.lock().unwrap());
        }

        #[test]
        fn each_probe_miss_logs_one_line_and_the_press_logs_its_timing() {
            let world = Arc::new(World::default());
            *world.misses.lock().unwrap() = vec!["a", "b"];
            let (outcome, lines) = logged_press(&world, &[&A, &B]);
            assert!(!world.trace.lock().unwrap().iter().any(|t| t == "notice"));
            assert_eq!(outcome, DispatchOutcome::NoMatch);
            assert_eq!(lines.len(), 3, "{lines:?}");
            assert!(lines[0].starts_with("Read key: Temple probe — no match, "), "{lines:?}");
            assert!(lines[1].starts_with("Read key: Mercenary probe — no match, "), "{lines:?}");
            assert!(lines[2].starts_with("Read key: nothing detected in "), "{lines:?}");
        }
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
        let grabs = run_pipeline(&world, &[&A], vec![sample(&[], true), sample(&[17], true), sample(&[65], true)]);
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
    fn two_matching_readers_select_only_the_first_after_one_capture() {
        let world = Arc::new(World::default());
        let mut grabs = 0;
        dispatch(press(), &[&A, &B], Duration::ZERO, &world, &mut |_| {}, &mut || {
            grabs += 1;
            Ok(capture(9))
        });
        assert_eq!(grabs, 1);
        let consumed = world.consumed.lock().unwrap().clone();
        assert_eq!(consumed, vec![("a", String::new(), Ok(9))]);
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
    fn grab_error_consumes_no_reader_after_one_attempt() {
        let world = Arc::new(World::default());
        let mut attempts = 0;
        dispatch(press(), &[&A, &B], Duration::ZERO, &world, &mut |_| {}, &mut || {
            attempts += 1;
            Err(CaptureError::Failed("no monitor".to_string()))
        });
        assert_eq!(attempts, 1);
        let consumed = world.consumed.lock().unwrap().clone();
        assert!(consumed.is_empty());
    }
    /// The privacy refusal is no frame for any reader (and no warn, which only
    /// the `Failed` arm emits): no armed read is consumed, so none reports.
    #[test]
    fn refused_grab_reaches_no_armed_read() {
        let world = Arc::new(World::default());
        let mut attempts = 0;
        dispatch(press(), &[&A, &B], Duration::ZERO, &world, &mut |_| {}, &mut || {
            attempts += 1;
            Err(CaptureError::NotAccepted)
        });
        assert_eq!(attempts, 1);
        assert!(world.consumed.lock().unwrap().is_empty());
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
            }, &mut |_| panic!("no dismissal to await"));
            grabs
        });
        let mut samples = vec![sample(PRESSED, true), sample(&[], true), sample(PRESSED, true)].into_iter();
        run_sampler(&world, &[&A], &mut || samples.next(), &mut || None, &tx, &mut Accepted::default, &mut |_| panic!("no dismissal"), &mut no_exchange);
        // The first grab is still blocked, yet both presses were armed.
        assert_eq!(world.armed_order.lock().unwrap().len(), 2);
        drop(tx);
        release_grab.send(()).unwrap();
        release_grab.send(()).unwrap();
        assert_eq!(dispatcher.join().unwrap(), 2);
    }
    #[test]
    fn presses_capture_once_each_with_no_reader_registered() {
        let world = Arc::new(World::default());
        let grabs = run_pipeline(&world, &[], vec![sample(PRESSED, true), sample(&[], true), sample(PRESSED, true)]);
        assert_eq!(grabs, 2);
    }
    #[test]
    fn no_press_grabs_no_frame_with_no_reader_registered() {
        let world = Arc::new(World::default());
        let grabs = run_pipeline(&world, &[], vec![sample(&[], true), sample(&[65], true)]);
        assert_eq!(grabs, 0);
    }
    #[test]
    fn presses_capture_once_each_when_every_reader_declines() {
        let world = Arc::new(World::default());
        let grabs = run_pipeline(&world, &[&IDLE, &IDLE], vec![sample(PRESSED, true), sample(&[], true), sample(PRESSED, true)]);
        assert_eq!(grabs, 2);
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
        run_sampler(&world, &[&A], &mut || samples.next(), &mut || None, &tx, &mut Accepted::default, &mut |_| panic!("no dismissal"), &mut no_exchange);
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
            &mut |_| panic!("no dismissal to await"),
        );
        assert_eq!(*events.lock().unwrap(), vec!["sleep 300ms", "grab"]);
    }
    #[test]
    fn queued_press_reads_with_the_context_of_its_press() {
        let world = Arc::new(World::default());
        *world.context.lock().unwrap() = "x".into();
        let (tx, rx) = mpsc::channel();
        let mut samples = vec![sample(PRESSED, true)].into_iter();
        run_sampler(&world, &[&A], &mut || samples.next(), &mut || None, &tx, &mut Accepted::default, &mut |_| panic!("no dismissal"), &mut no_exchange);
        drop(tx);
        *world.context.lock().unwrap() = "y".into();
        let mut grabs = 0;
        run_dispatcher(&world, rx, &mut |_| {}, &mut || {
            grabs += 1;
            Ok(capture(3))
        }, &mut |_| panic!("no dismissal to await"));
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
            recalibrate_seconds: 5,
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
        let refused = ReadHotkeySettings { shortcut: capture, delay_ms: 100, ..ReadHotkeySettings::default() };
        assert!(store_settings(&state, refused).is_err());
        assert_eq!(read_of(&state), ReadHotkeySettings::default());
    }
    #[test]
    fn storing_a_read_hotkey_on_the_boss_loot_preview_key_is_refused_and_keeps_the_old_one() {
        let state = test_app_state();
        let preview = loot_of(&state).preview_shortcut;
        let refused = ReadHotkeySettings { shortcut: preview, delay_ms: 100, ..ReadHotkeySettings::default() };
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
        assert_eq!(fallback.shortcut.key, 81);
        assert_eq!(apart_from_boss_loot(&fallback.shortcut, &loot), Ok(()));
    }
    #[test]
    fn fallback_skips_a_default_held_by_boss_loot_preview() {
        let loot = LootSettings { preview_shortcut: ReadHotkeySettings::default().shortcut, ..Default::default() };
        assert_eq!(fallback_apart_from(&loot).shortcut.key, 81);
    }
    #[test]
    fn fallback_skips_two_candidates_held_by_both_boss_loot_keys() {
        let default = ReadHotkeySettings::default().shortcut;
        let loot = LootSettings {
            capture_shortcut: default.clone(),
            preview_shortcut: Shortcut { key: 81, ..default },
            ..Default::default()
        };
        assert_eq!(fallback_apart_from(&loot).shortcut.key, 90);
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

    mod keep_critical {
        use super::*;
        use crate::exchange_read::ExchangeReadSettings;

        const READ_CLASH: &str = "Read screen shortcut must differ from the boss loot and read exchange shortcuts";
        const LOOT_CLASH: &str = "Boss loot shortcuts must differ from the read exchange shortcut";

        fn alt(key: u16) -> Shortcut {
            Shortcut { key, shift: false, ctrl: false, alt: true }
        }
        fn state_with_exchange(shortcut: Shortcut) -> crate::AppState {
            let state = test_app_state();
            *state.exchange_read.lock().unwrap() = ExchangeReadSettings { shortcut };
            state
        }

        // keep:critical — silent: a read key on a boss-loot or the stored read
        // exchange key makes one press start two reads, so one of them reads a
        // frame the player pressed for the other.
        #[test]
        fn a_read_screen_shortcut_on_a_boss_loot_or_the_read_exchange_key_is_refused_and_the_stored_one_kept() {
            let loot = LootSettings::default();
            let cases = [
                (alt(2), loot.capture_shortcut.clone()),
                (alt(2), loot.preview_shortcut.clone()),
                (alt(2), alt(2)),
                (alt(5), alt(5)),
            ];
            for (exchange, candidate) in cases {
                let state = state_with_exchange(exchange.clone());
                let refused = ReadHotkeySettings { shortcut: candidate.clone(), delay_ms: 100, ..ReadHotkeySettings::default() };
                assert_eq!(store_settings(&state, refused), Err(READ_CLASH.to_string()), "exchange {exchange:?}, read {candidate:?}");
                assert_eq!(read_of(&state), ReadHotkeySettings::default(), "exchange {exchange:?}, read {candidate:?}");
            }
        }

        // keep:critical — silent: a boss-loot key on the stored read exchange key
        // makes one press both read the exchange and capture boss loot.
        #[test]
        fn a_boss_loot_shortcut_on_the_read_exchange_key_is_refused_and_the_stored_ones_kept() {
            let on_capture = |key: Shortcut| LootSettings { capture_shortcut: key, auto_save_seconds: 30, ..Default::default() };
            let on_preview = |key: Shortcut| LootSettings { preview_shortcut: key, auto_save_seconds: 30, ..Default::default() };
            let cases = [
                (alt(2), on_capture(alt(2))),
                (alt(2), on_preview(alt(2))),
                (alt(5), on_capture(alt(5))),
            ];
            for (exchange, refused) in cases {
                let state = state_with_exchange(exchange.clone());
                let label = format!("exchange {exchange:?}, loot {:?}/{:?}", refused.capture_shortcut, refused.preview_shortcut);
                assert_eq!(crate::loot_capture::store_settings(&state, refused), Err(LOOT_CLASH.to_string()), "{label}");
                assert_eq!(loot_of(&state), LootSettings::default(), "{label}");
            }
        }
    }
}
