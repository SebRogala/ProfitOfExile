use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc;

pub struct LogWatcher {
    path: PathBuf,
}

impl LogWatcher {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Spawns a background task that tails the log file and sends new lines.
    pub async fn watch(&self) -> anyhow::Result<mpsc::Receiver<String>> {
        let (tx, rx) = mpsc::channel(256);
        let path = self.path.clone();

        tokio::task::spawn_blocking(move || {
            if let Err(e) = poll_file(&path, tx) {
                log::error!("Log watcher stopped: {}", e);
            }
        });

        Ok(rx)
    }
}

const MAX_WAIT_ATTEMPTS: u32 = 150; // ~5 minutes at 2s intervals
const POLL_INTERVAL_MS: u64 = 300;

fn poll_file(path: &Path, tx: mpsc::Sender<String>) -> anyhow::Result<()> {
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
        std::thread::sleep(Duration::from_secs(2));
    }

    let mut file = File::open(path)?;
    // Seek to end — only read new lines
    file.seek(SeekFrom::End(0))?;
    let mut pos = file.metadata()?.len();

    log::info!("Polling {:?} for changes ({}ms interval), starting at pos {}", path, POLL_INTERVAL_MS, pos);
    // Drop the initial file handle — we'll re-open on each read to avoid Windows caching
    drop(file);

    loop {
        std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));

        if tx.is_closed() {
            return Ok(());
        }

        let new_len = match std::fs::metadata(path) {
            Ok(m) => m.len(),
            Err(e) => {
                log::warn!("Log watcher: failed to read metadata for {:?}: {}", path, e);
                continue;
            }
        };

        if new_len < pos {
            log::info!("Log watcher: file truncated ({} -> {}), resetting", pos, new_len);
            pos = 0;
        }

        if new_len > pos {
            log::info!("Log watcher: file grew {} -> {}, reading new lines", pos, new_len);
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
                                log::info!("Log watcher: line: {}", &line[..line.len().min(80)]);
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
                    log::warn!("Log watcher: failed to re-open file: {}", e);
                    continue;
                }
            }
            pos = new_len;
        }
    }
}
