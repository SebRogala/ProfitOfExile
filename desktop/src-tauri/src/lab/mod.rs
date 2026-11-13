//! The lab feature's Tauri surface (POE-342). The `lab_*.rs` files stay at
//! the crate root for now.

pub(crate) mod commands;
pub(crate) mod watch;

pub(crate) use watch::{catch_up, on_area_change, on_client_line, on_trial_line, LabWatchState};
