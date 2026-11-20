use super::*;

// ---------------------------------------------------------------------------
// Tombstone
// ---------------------------------------------------------------------------

/// Record forgets as owed WITHOUT sending them (POE-211).
///
/// [`spawn_tombstone`] is the page's path: the user clicked ✕ and is waiting,
/// so the POST goes out now. The family re-key's path is the other half — it
/// runs at module start, a few lines before [`retry_pending_tombstones`] offers
/// everything the file holds, so sending from there would POST each key twice
/// and make the retry line misreport a fresh record as a forget "the pool has
/// not acknowledged".
///
/// Recording is owed WHETHER OR NOT the migration ran, because the load seam
/// installs the FOLDED store before it decides whether to save at all: a start
/// that defers the fold and a save that fails both leave it in memory, and any
/// later writer of it — the corpus merge, `mark_uploaded`, the off-tick
/// `SaveQueue`, a forget or a reset — persists the fold, spending the one-shot
/// marker (the old family leaving `index.json`) whichever gets there first. A
/// tombstone recorded only on the save's success arm would be lost with it.
/// The durable record is also what closes the crash window between the save and
/// the send: `pool-sync.json` outlives the process, a spawned POST does not.
///
/// `record_pending_in` dedupes against what the file already holds, so a start
/// that re-runs this over a key already recorded changes nothing.
pub(in crate::mercenary) fn record_tombstones(app: &AppHandle, keys: &[(String, u8)]) {
    let Some(dir) = icons_dir(app) else {
        return;
    };
    for (family, tier) in keys {
        if let Err(e) = record_pending_in(&dir, family, *tier) {
            crate::app_log(app, format!("Merc: could not record the forget — {e}"));
        }
    }
}

/// Make a local forget stick for everyone.
///
/// Recorded locally FIRST and only then sent: between the forget and the
/// server's acknowledgement the corpus still serves the disowned art, so the
/// key has to be suppressed on the local side or the very next pull puts it
/// back. The record is dropped when the POST lands.
///
/// Fire-and-forget with bounded retry, like the upload — a forget must not
/// block the page's button on a network round-trip.
pub fn spawn_tombstone(app: &AppHandle, family: String, tier: u8) {
    if let Some(dir) = icons_dir(app) {
        if let Err(e) = record_pending_in(&dir, &family, tier) {
            crate::app_log(app, format!("Merc: could not record the forget — {e}"));
        }
    }
    send_and_clear(app, family, tier);
}

/// Offer every forget the server has not acknowledged.
///
/// Called once per module start, next to [`enqueue_backfill`]. Without it the
/// three attempts a forget gets are the only ones it ever gets: a device that
/// was offline when the user clicked ✕ would suppress that key locally forever,
/// and — because a suppressed merge never stores the pull's ETag — would also
/// re-download the whole corpus and re-run its tombstone replace at every start.
pub fn retry_pending_tombstones(app: &AppHandle) {
    let Some(dir) = icons_dir(app) else {
        return;
    };
    let owed = pending_tombstones(&dir);
    if owed.is_empty() {
        return;
    }
    crate::app_log(
        app,
        format!("Merc: retrying {} forget(s) the pool has not acknowledged", owed.len()),
    );
    for (family, tier) in owed {
        send_and_clear(app, family, tier);
    }
}

/// POST one tombstone on a task and clear its local record if it lands.
///
/// Fire-and-forget with bounded retry: a forget must not block the page's button
/// on a network round-trip, and a retry budget that outlived the session would be
/// a task nobody is watching. The durable retry is the record on disk, which
/// [`retry_pending_tombstones`] offers again at the next module start.
pub(super) fn send_and_clear(app: &AppHandle, family: String, tier: u8) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if send_tombstone(&app, &family, tier).await {
            clear_pending_tombstone(&app, &family, tier);
            crate::app_log(
                &app,
                format!("Merc: pool tombstoned {family} (tier {tier})"),
            );
        } else {
            crate::app_log(&app, format!(
                "Merc: pool was not told about forgetting {family} (tier {tier}) — the key stays suppressed locally, and the next module start offers it again",
            ));
        }
    });
}

/// The forgets a module start owes the server: the keys recorded locally whose
/// tombstone the pool has not acknowledged.
pub fn pending_tombstones(dir: &Path) -> Vec<(String, u8)> {
    SyncFile::load(dir).owed()
}

/// Record a forget as owed. `true` when the file changed. App-free so the
/// record → offer → clear cycle is testable without a server.
pub(super) fn record_pending_in(dir: &Path, family: &str, tier: u8) -> Result<bool, String> {
    let _guard = SYNC_FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut file = SyncFile::load(dir);
    let key = WireKey {
        family: family.to_string(),
        tier,
    };
    if file.pending_tombstones.contains(&key) {
        return Ok(false);
    }
    file.pending_tombstones.push(key);
    file.save(dir)?;
    Ok(true)
}

/// Drop an acknowledged forget from the owed list, and drop the cached ETag with
/// it. `true` when the file changed.
///
/// The ETag goes because the suppression on this key has just lifted, and the
/// last pull skipped the key's served samples on account of it. Keeping the tag
/// would earn a 304 that can never deliver them.
pub(super) fn clear_pending_in(dir: &Path, family: &str, tier: u8) -> Result<bool, String> {
    let _guard = SYNC_FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut file = SyncFile::load(dir);
    let before = file.pending_tombstones.len();
    file.pending_tombstones
        .retain(|k| !(k.family == family && k.tier == tier));
    if file.pending_tombstones.len() == before {
        return Ok(false);
    }
    file.etag = None;
    file.save(dir)?;
    Ok(true)
}

pub(super) async fn send_tombstone(app: &AppHandle, family: &str, tier: u8) -> bool {
    let (server, http) = server_and_http(app);
    let url = format!("{server}/api/desktop/merc-templates/tombstone");
    let body = TombstoneBody {
        format_version: FORMAT_VERSION,
        family: family.to_string(),
        tier,
    };
    for attempt in 1..=MAX_ATTEMPTS {
        match http
            .post(&url)
            .timeout(REQUEST_TIMEOUT)
            .json(&body)
            .send()
            .await
        {
            Ok(res) if res.status().is_success() => {
                note_success(app);
                return true;
            }
            Ok(res) if res.status() == reqwest::StatusCode::TOO_MANY_REQUESTS => {
                let wait = retry_after_delay(
                    res.headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok()),
                );
                if wait > RETRY_CEILING || attempt == MAX_ATTEMPTS {
                    note_failure(app, "tombstone: rate limited".to_string());
                    return false;
                }
                tokio::time::sleep(wait).await;
            }
            Ok(res) if res.status().is_client_error() => {
                note_failure(app, format!("tombstone: {}", res.status()));
                return false;
            }
            Ok(res) => {
                if attempt == MAX_ATTEMPTS {
                    note_failure(app, format!("tombstone: server returned {}", res.status()));
                    return false;
                }
                tokio::time::sleep(RETRY_BASE * attempt).await;
            }
            Err(e) => {
                if attempt == MAX_ATTEMPTS {
                    note_failure(app, format!("tombstone: {e}"));
                    return false;
                }
                tokio::time::sleep(RETRY_BASE * attempt).await;
            }
        }
    }
    false
}

pub(super) fn clear_pending_tombstone(app: &AppHandle, family: &str, tier: u8) {
    let Some(dir) = icons_dir(app) else {
        return;
    };
    if let Err(e) = clear_pending_in(&dir, family, tier) {
        crate::app_log(app, format!("Merc: could not clear the forget record — {e}"));
    }
}
