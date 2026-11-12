//! Wall-clock time for code outside the feature loops.

/// Unix time in milliseconds, or 0 if the system clock reads before the epoch.
pub fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
