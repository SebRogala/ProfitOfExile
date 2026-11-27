use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc;

pub struct LogWatcher {
    path: PathBuf,
}

const MAX_WAIT_ATTEMPTS: u32 = 150; // ~5 minutes at 2s intervals

impl LogWatcher {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Spawns a background task that watches the log file for changes using
    /// filesystem events (notify crate) and sends new lines via channel.
    /// Falls back to polling if filesystem watching fails.
    pub async fn watch(&self) -> anyhow::Result<mpsc::Receiver<String>> {
        let (tx, rx) = mpsc::channel(256);
        let path = self.path.clone();

        tokio::task::spawn_blocking(move || {
            if let Err(e) = watch_file(&path, tx) {
                log::error!("Log watcher stopped: {}", e);
            }
        });

        Ok(rx)
    }
}

fn watch_file(path: &Path, tx: mpsc::Sender<String>) -> anyhow::Result<()> {
    // Wait for file to exist
    let mut attempts = 0;
    while !path.exists() {
        attempts += 1;
        if attempts > MAX_WAIT_ATTEMPTS {
            anyhow::bail!(
                "Log file {:?} not found after {} attempts",
                path,
                MAX_WAIT_ATTEMPTS
            );
        }
        if tx.is_closed() {
            return Ok(());
        }
        log::info!(
            "Waiting for {:?} to exist... (attempt {}/{})",
            path,
            attempts,
            MAX_WAIT_ATTEMPTS
        );
        std::thread::sleep(Duration::from_secs(2));
    }

    // Open file and seek to end — only read new lines
    let mut pos = std::fs::metadata(path)?.len();
    log::info!(
        "Watching {:?} for changes (notify), starting at pos {}, file size {}",
        path,
        pos,
        pos
    );

    // Log a diagnostic sample of the last few bytes to verify encoding
    if pos > 0 {
        if let Ok(mut f) = File::open(path) {
            let sample_start = if pos > 200 { pos - 200 } else { 0 };
            if f.seek(SeekFrom::Start(sample_start)).is_ok() {
                let mut buf = vec![0u8; (pos - sample_start) as usize];
                if std::io::Read::read(&mut f, &mut buf).is_ok() {
                    // Check for BOM or non-UTF8
                    if buf.starts_with(&[0xFF, 0xFE]) || buf.starts_with(&[0xFE, 0xFF]) {
                        log::warn!("Client.txt appears to be UTF-16 encoded — this may cause issues");
                    }
                    // Log last line as diagnostic
                    if let Ok(sample) = String::from_utf8(buf) {
                        if let Some(last_line) = sample.lines().last() {
                            log::info!("Last log line sample: {}", &last_line[..last_line.len().min(120)]);
                        }
                    } else {
                        log::warn!("Client.txt contains non-UTF8 bytes — encoding issue?");
                    }
                }
            }
        }
    }

    // Set up filesystem watcher
    let (notify_tx, notify_rx) = std::sync::mpsc::channel();
    let mut watcher = RecommendedWatcher::new(notify_tx, Config::default())?;

    // Watch the parent directory (some systems don't support watching individual files)
    let watch_dir = path.parent().unwrap_or(path);
    watcher.watch(watch_dir, RecursiveMode::NonRecursive)?;
    log::info!("Filesystem watcher active on {:?}", watch_dir);

    let file_name = path.file_name();

    loop {
        // Block until filesystem event or timeout (5s fallback to catch missed events)
        let event = notify_rx.recv_timeout(Duration::from_secs(5));

        if tx.is_closed() {
            return Ok(());
        }

        match event {
            Ok(Ok(event)) => {
                // Only process modify events for our specific file
                if !matches!(event.kind, EventKind::Modify(_)) {
                    continue;
                }
                // Check if the event is for our file
                if let Some(expected_name) = file_name {
                    let is_our_file = event.paths.iter().any(|p| {
                        p.file_name().map_or(false, |n| n == expected_name)
                    });
                    if !is_our_file {
                        continue;
                    }
                }
            }
            Ok(Err(e)) => {
                log::warn!("Filesystem watcher error: {}", e);
                continue;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Fallback: check file even without event (catches edge cases)
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                anyhow::bail!("Filesystem watcher disconnected");
            }
        }

        match read_new_lines(path, pos, &mut |line| tx.blocking_send(line).is_ok()) {
            Some(next) => pos = next,
            None => return Ok(()),
        }
    }
}

/// Reads the complete lines appended after `pos`, resetting to 0 when the file shrank.
/// Returns the new position, or None when `emit` reports the receiver is gone.
fn read_new_lines(path: &Path, pos: u64, emit: &mut dyn FnMut(String) -> bool) -> Option<u64> {
    let mut pos = pos;
    // Read new lines if file grew
    let new_len = match std::fs::metadata(path) {
        Ok(m) => m.len(),
        Err(e) => {
            log::warn!("Log watcher: failed to read metadata: {}", e);
            return Some(pos);
        }
    };

    if new_len < pos {
        log::info!("Log watcher: file truncated ({} -> {}), resetting", pos, new_len);
        pos = 0;
    }

    if new_len > pos {
        match File::open(path) {
            Ok(mut f) => {
                if let Err(e) = f.seek(SeekFrom::Start(pos)) {
                    log::warn!("Log watcher: seek failed: {}", e);
                    return Some(pos);
                }
                // Advance `pos` by what was actually consumed, and only over
                // NEWLINE-TERMINATED lines. The client keeps appending while we
                // drain, so the reader reaches an EOF past the `new_len` sampled
                // above; assigning `pos = new_len` rewound behind everything that
                // arrived mid-read and re-sent it on the next pass. A duplicated
                // "you have entered" line is a second RoomChanged for the room the
                // player is already standing in. Stopping at the last '\n' also
                // keeps a half-written trailing line unconsumed, so it is emitted
                // once, whole, when its newline lands.
                let mut reader = BufReader::new(&f);
                let mut consumed = pos;
                loop {
                    let mut buf = String::new();
                    match reader.read_line(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if !buf.ends_with('\n') {
                                break;
                            }
                            consumed += n as u64;
                            let line = buf.trim_end_matches(['\n', '\r']);
                            if !line.is_empty() && !emit(line.to_string()) {
                                return None;
                            }
                        }
                        Err(e) => {
                            log::warn!("Log watcher: failed to read line: {}", e);
                            break;
                        }
                    }
                }
                pos = consumed;
            }
            Err(e) => {
                log::warn!("Log watcher: failed to open file: {}", e);
                return Some(pos);
            }
        }
    }
    Some(pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    mod keep_regression {
        use super::*;

        fn read(path: &Path, pos: u64) -> (u64, Vec<String>) {
            let mut lines = Vec::new();
            let next = read_new_lines(path, pos, &mut |line| {
                lines.push(line);
                true
            })
            .expect("emit never reports a closed receiver");
            (next, lines)
        }

        fn append(path: &Path, text: &str) {
            use std::io::Write;
            std::fs::OpenOptions::new()
                .append(true)
                .open(path)
                .expect("open Client.txt for append")
                .write_all(text.as_bytes())
                .expect("append to Client.txt");
        }

        // keep:regression — POE-347: a truncated or regrown Client.txt re-emits
        // only new complete lines. After a truncation the reader restarts at 0
        // and sends the new line once; a half-written trailing line stays
        // unconsumed until its newline lands, then goes out once, whole. Fails
        // if the shrink is not detected (C is never read), if the old lines A
        // and B are re-sent, or if the half line is emitted early or twice.
        #[test]
        fn truncated_client_txt_emits_only_new_complete_lines() {
            let path = std::env::temp_dir()
                .join(format!("poe_test_log_watcher_truncate_{}", std::process::id()))
                .with_extension("txt");
            let a = "2026/11/19 21:04:11 1234567 cff94598 [INFO Client 4120] : You have entered Aspirants' Plaza.";
            let b = "2026/11/19 21:04:52 1275012 cff94598 [INFO Client 4120] : You have entered Aspirants' Trial.";
            let c = "2026/11/19 21:09:03 120 cff94598 [INFO Client 9] : You have entered Lioneye's Watch.";
            let d = "2026/11/19 21:09:40 157 cff94598 [INFO Client 9] : You have entered The Coast.";

            std::fs::write(&path, format!("{a}\n{b}\n")).expect("write Client.txt");
            let (pos, first) = read(&path, 0);
            assert_eq!(first, vec![a.to_string(), b.to_string()], "initial read");

            std::fs::write(&path, format!("{c}\n")).expect("truncate Client.txt");
            assert!((c.len() as u64 + 1) < pos, "C must shrink the file");
            let (pos, after_truncate) = read(&path, pos);
            assert_eq!(after_truncate, vec![c.to_string()], "read after truncation");

            let (d_head, d_tail) = d.split_at(30);
            append(&path, d_head);
            let (pos, half) = read(&path, pos);
            assert_eq!(half, Vec::<String>::new(), "read of a half-written line");

            append(&path, &format!("{d_tail}\n"));
            let (_, completed) = read(&path, pos);
            assert_eq!(completed, vec![d.to_string()], "read once the half line's newline lands");

            std::fs::remove_file(&path).expect("remove test Client.txt");
        }
    }
}
