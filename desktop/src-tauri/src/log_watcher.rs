use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

pub struct LogWatcher {
    path: PathBuf,
}

impl LogWatcher {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Spawns a background task that tails the log file and sends new lines.
    /// Returns a receiver for new log lines.
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

const MAX_WAIT_ATTEMPTS: u32 = 150; // ~5 minutes at 2s intervals

fn watch_file(path: &Path, tx: mpsc::Sender<String>) -> anyhow::Result<()> {
    // Wait for file to exist, with timeout
    let mut attempts = 0;
    while !path.exists() {
        attempts += 1;
        if attempts > MAX_WAIT_ATTEMPTS {
            anyhow::bail!("Log file {:?} not found after {} attempts", path, MAX_WAIT_ATTEMPTS);
        }
        if tx.is_closed() {
            return Ok(());
        }
        log::info!("Waiting for {:?} to exist... (attempt {}/{})", path, attempts, MAX_WAIT_ATTEMPTS);
        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    let mut file = File::open(path)?;
    // Seek to end — only read new lines
    file.seek(SeekFrom::End(0))?;
    let mut pos = file.metadata()?.len();

    let (notify_tx, notify_rx) = std::sync::mpsc::channel();
    let watch_dir = path.parent().unwrap_or(Path::new("."));
    let mut watcher = RecommendedWatcher::new(notify_tx, Config::default())?;
    watcher.watch(watch_dir, RecursiveMode::NonRecursive)?;

    log::info!("Watching {:?} for changes", path);

    for event in notify_rx {
        match event {
            Ok(event) if matches!(event.kind, EventKind::Modify(_)) => {
                let new_len = match std::fs::metadata(path) {
                    Ok(m) => m.len(),
                    Err(e) => {
                        log::warn!("Log watcher: failed to read metadata for {:?}: {}", path, e);
                        continue;
                    }
                };

                if new_len < pos {
                    // File was truncated, reset
                    pos = 0;
                }

                if new_len > pos {
                    file.seek(SeekFrom::Start(pos))?;
                    let reader = BufReader::new(&file);
                    for line in reader.lines() {
                        match line {
                            Ok(line) if !line.is_empty() => {
                                if tx.blocking_send(line).is_err() {
                                    return Ok(());
                                }
                            }
                            Ok(_) => {} // empty line
                            Err(e) => {
                                log::warn!("Log watcher: failed to read line: {}", e);
                            }
                        }
                    }
                    pos = new_len;
                }
            }
            Ok(event) if matches!(event.kind, EventKind::Remove(_)) => {
                log::warn!("Log file was removed: {:?}", path);
            }
            Err(e) => {
                log::error!("Log watcher: notify error: {}", e);
            }
            _ => {}
        }
    }

    Ok(())
}
