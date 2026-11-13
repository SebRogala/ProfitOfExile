//! The lab feature's Tauri surface (POE-342) and its two read-key screen
//! readers (POE-391). The `lab_*.rs` files stay at the crate root for now.

pub(crate) mod commands;
pub(crate) mod font_context;
pub(crate) mod font_reader;
pub(crate) mod gem_reader;
pub(crate) mod gem_vocab;
pub(crate) mod watch;

pub(crate) use watch::{catch_up, on_area_change, on_client_line, on_trial_line, LabWatchState};

/// A log line a lab reader writes at most once per armed context.
#[derive(Clone, Copy)]
pub(crate) enum LogOnce {
    FrameError = 1,
    OcrError = 2,
    ShortCrop = 4,
    Unscaled = 8,
    NoVocab = 16,
}

/// True the first time `what` is asked for under `token` on this thread.
/// Readers run on the read key's dispatcher thread, so a thread-local slot per
/// reader is enough.
pub(crate) fn once_per_context(
    slot: &'static std::thread::LocalKey<std::cell::Cell<(u64, u8)>>,
    token: u64,
    what: LogOnce,
) -> bool {
    slot.with(|cell| {
        let (seen_token, mut bits) = cell.get();
        if seen_token != token {
            bits = 0;
        }
        let first = bits & what as u8 == 0;
        cell.set((token, bits | what as u8));
        first
    })
}

thread_local! {
    static ENGINE_REPORTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// M11: the first lab read on a thread logs the OCR engine and pushes the
/// en-US fallback warning to the status.
pub(crate) fn report_engine_once(app: &tauri::AppHandle) {
    if !ENGINE_REPORTED.with(|c| c.replace(true)) {
        crate::report_ocr_engine(app);
    }
}
