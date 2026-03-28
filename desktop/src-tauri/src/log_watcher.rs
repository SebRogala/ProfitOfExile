use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
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
        "Watching {:?} for changes (notify), starting at pos {}",
        path,
        pos
    );

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

        // Read new lines if file grew
        let new_len = match std::fs::metadata(path) {
            Ok(m) => m.len(),
            Err(e) => {
                log::warn!("Log watcher: failed to read metadata: {}", e);
                continue;
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
                        continue;
                    }
                    let reader = BufReader::new(&f);
                    for line in reader.lines() {
                        match line {
                            Ok(line) if !line.is_empty() => {
                                if tx.blocking_send(line).is_err() {
                                    return Ok(());
                                }
                            }
                            Ok(_) => {}
                            Err(e) => {
                                log::warn!("Log watcher: failed to read line: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    log::warn!("Log watcher: failed to open file: {}", e);
                    continue;
                }
            }
            pos = new_len;
        }
    }
}
