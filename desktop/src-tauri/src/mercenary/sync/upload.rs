use super::*;

// ---------------------------------------------------------------------------
// Upload
// ---------------------------------------------------------------------------

/// Offer samples to the pool.
///
/// Returns immediately: the POST happens on a task, never on the capture tick.
/// The first Windows smoke measured a 4 s tick with OCR alone, and a
/// synchronous upload there would stall the read the user is waiting on.
pub fn enqueue(app: &AppHandle, samples: Vec<PendingSample>) {
    if samples.is_empty() {
        return;
    }
    let start = with_state(app, |s| {
        s.queue.extend(samples);
        s.status.queued_uploads = s.queue.len();
        if s.uploading {
            return false;
        }
        s.uploading = true;
        true
    });
    crate::ssot::emit_ssot(app);
    if !start {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move { drain_queue(app).await });
}

/// Send everything queued, one batch at a time, then stop.
///
/// One drain task at a time (`uploading`), so the queue has a single reader and
/// batches keep their order.
pub(super) async fn drain_queue(app: AppHandle) {
    loop {
        let batch = with_state(&app, |s| {
            let take = s.queue.len().min(MAX_TEMPLATES_PER_BATCH);
            let batch: Vec<PendingSample> = s.queue.drain(..take).collect();
            if batch.is_empty() {
                s.uploading = false;
            }
            s.status.queued_uploads = s.queue.len();
            batch
        });
        if batch.is_empty() {
            crate::ssot::emit_ssot(&app);
            return;
        }
        // Only an acknowledgement that SETTLES the offer closes it out. An
        // outcome the pool invites a retry on leaves the samples owed, and the
        // whole batch is offered again at the next module start — the server
        // dedupes, so re-offering the settled part of it costs one request —
        // per module start, for as long as a key in the batch sits at the
        // cap (a steady state until someone retires a sample), so the
        // recurring "N at the cap" log line is expected, not a fault.
        if let Some(ack) = send_batch(&app, &batch).await {
            // A refusal is per SAMPLE, and it is the one outcome the player can
            // act on: the log names the incumbent so they know which family to
            // forget on the device that learned it.
            for conflict in &ack.conflicts {
                crate::app_log(
                    &app,
                    format!(
                        "Merc: pool refused {} (tier {}) — same art already pooled as {}",
                        conflict.family, conflict.tier, conflict.incumbent_family,
                    ),
                );
            }
            let settled = settlement(&ack, &batch);
            if settled.unnamed > 0 {
                // The pool refused more than it could place (a `-1`, an index
                // this batch does not carry, or a count with no detail behind
                // it). Its own logs call that a server bug; this side answers
                // it by settling nothing.
                crate::app_log(
                    &app,
                    format!(
                        "Merc: {} refusal(s) named no template this batch carries — the whole batch stays owed and is re-offered next start; report it if it repeats",
                        settled.unnamed,
                    ),
                );
            }
            if should_mark_published(&ack) {
                // The SETTLE SET, never the whole batch: a refused sample stays
                // `uploaded: false` on disk so `enqueue_backfill` offers it
                // again at the next module start, which is the retry the
                // recovery path needs — the moment somebody tombstones the
                // incumbent, the art stores. Already EMPTY when a refusal went
                // unnamed, so that case marks nothing.
                mark_uploaded(&app, &settled.settle);
            }
        }
        crate::ssot::emit_ssot(&app);
    }
}

/// Read an acknowledgement out of a 2xx body. `None` when the bytes are not one.
///
/// Pure, and separate from the request, because the `None` is load-bearing: a
/// 2xx whose body this build cannot read is NOT an acknowledgement. Defaulting
/// it would produce an all-zero [`UploadAck`], which [`should_mark_published`]
/// reads as settled — so a proxy's HTML error page, a truncated response or an
/// empty body would mark the whole batch `uploaded: true` on disk and retire
/// art the pool never took, permanently and silently.
///
/// Every field of [`UploadAck`] already defaults on its own, so a server that
/// merely grew or dropped a counter still parses here. What reaches this `None`
/// is a body that is not the ack shape at all.
pub(super) fn ack_from_body(bytes: &[u8]) -> Option<UploadAck> {
    serde_json::from_slice(bytes).ok()
}

/// Whether an acknowledged batch may be recorded as published.
///
/// Two outcomes are NOT settled and must leave the samples owed:
///
/// - `capped` — the pool refused only because the key already holds three live
///   samples. A tombstone can free a slot at any time, and the server's own
///   comment calls this the outcome that "invites a retry once a slot frees".
/// - `rejected_unknown_family` — the SERVER's support vocabulary is older than
///   this build's. A deploy fixes it, and marking the offer published would
///   retire art the pool wants as soon as it ships.
///
/// The other two ARE settled: `duplicate` means the pool already holds this art,
/// and `rejected` means this build sent bytes the server cannot decode. Sending
/// the identical payload again changes neither.
pub(super) fn should_mark_published(ack: &UploadAck) -> bool {
    ack.capped == 0 && ack.rejected_unknown_family == 0
}

/// Which positions of the batch the pool refused as another family's art.
///
/// The verdict above is per BATCH; this is what makes the settlement per
/// sample. Two kinds of entry are dropped: an index outside the batch, and
/// `-1` — the server saying its own bookkeeping lost the template's position.
///
/// Neither is a panic risk, and the checks are not here to prevent one: an
/// unchecked `c.index as usize` would turn `-1` into `usize::MAX`, which the
/// bounds check discards like any other out-of-range value. What they buy is
/// the COUNT. [`settlement`] compares what the pool said it refused against
/// what this could actually place, and a refusal it cannot place is what makes
/// it hold the whole batch owed instead of closing out a sample on a position
/// nobody could name.
///
/// DEDUPED for the same reason. Two refusals naming ONE position are two
/// templates the pool would not take and one this build can identify, so the
/// counts must disagree — returning the position twice would make them agree
/// and settle the sample the second refusal was really about.
pub(super) fn conflict_indexes(ack: &UploadAck, batch_len: usize) -> Vec<usize> {
    let placed: std::collections::BTreeSet<usize> = ack
        .conflicts
        .iter()
        .filter_map(|c| usize::try_from(c.index).ok())
        .filter(|i| *i < batch_len)
        .collect();
    placed.into_iter().collect()
}

/// What one acknowledgement closes out, per sample.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Settlement {
    /// The samples a settled verdict may mark published. EMPTY whenever
    /// `unnamed` is non-zero — see [`settlement`].
    pub(super) settle: Vec<PendingSample>,
    /// Refusals this build could not place in the batch.
    pub(super) unnamed: usize,
}

/// Decide, per sample, what an acknowledgement closes out (POE-210).
///
/// One function rather than a sequence the drain performs itself, because the
/// rule below is the whole point of the change and a drain that re-derived it
/// inline would be untestable: the only observable seam is what
/// `mark_uploaded` is handed, and that call takes an `AppHandle`.
///
/// The pool's `conflicting` count is the authority on HOW MANY templates it
/// refused; `conflicts[]` is the authority on WHICH. They disagree exactly when
/// the server could not name one — a missing entry, a `-1`, an index this batch
/// does not carry — so the claim is the larger of the two.
///
/// **An unplaceable refusal holds the WHOLE batch owed.** Nothing here can say
/// which sample the pool kept and which it refused, so none of them is closed
/// out: the batch takes the `capped` shape — dropped from this session's queue,
/// still `uploaded: false` on disk, offered again at the next module start. The
/// samples the pool really did store come back as `duplicate` that time and
/// settle then, so the cost is one re-offer per module start, and never a
/// sample retired on art the pool does not hold.
pub(super) fn settlement(ack: &UploadAck, batch: &[PendingSample]) -> Settlement {
    let refused = conflict_indexes(ack, batch.len());
    let claimed = (ack.conflicting as usize).max(ack.conflicts.len());
    let unnamed = claimed.saturating_sub(refused.len());
    if unnamed > 0 {
        return Settlement {
            settle: Vec::new(),
            unnamed,
        };
    }
    Settlement {
        settle: settle_set(batch, &refused),
        unnamed: 0,
    }
}

/// The batch minus the samples the pool refused — what a settled acknowledgement
/// may mark published.
///
/// D1: a refused sample is dropped from the in-session queue but NOT closed out
/// on disk, the same shape `capped` already has. It becomes storable the moment
/// the incumbent is tombstoned, and without the retry the recovery path the
/// refusal line tells the player about would not exist.
///
/// NOTE — the exclusion is by batch POSITION, while `mark_uploaded` matches by
/// signature BYTES. Two byte-identical entries in one batch would therefore
/// mis-settle: excluding position 0 would still mark position 1's identical
/// bytes. Unreachable today, because the server's own-key duplicate check runs
/// BEFORE its cross-family one, so the second copy comes back `duplicate` and
/// never `conflicting`. A reorder of the server's `Decide` would make it
/// reachable, and nothing here would notice.
pub(super) fn settle_set(batch: &[PendingSample], refused: &[usize]) -> Vec<PendingSample> {
    batch
        .iter()
        .enumerate()
        .filter(|(i, _)| !refused.contains(i))
        .map(|(_, sample)| sample.clone())
        .collect()
}

/// POST one batch, honouring the rate limit. The acknowledgement when the pool
/// answered, `None` when it never did.
///
/// A batch the pool never answered is DROPPED rather than requeued: its samples
/// are still `uploaded: false` on disk, so the next module start offers them
/// again. Requeuing inside the session would turn an outage into a task that
/// never ends.
pub(super) async fn send_batch(app: &AppHandle, batch: &[PendingSample]) -> Option<UploadAck> {
    let (server, http) = server_and_http(app);
    let url = format!("{server}/api/desktop/merc-templates");
    let bodies = build_batches(batch);
    let Some(body) = bodies.first() else {
        return None;
    };

    for attempt in 1..=MAX_ATTEMPTS {
        let response = http
            .post(&url)
            .timeout(REQUEST_TIMEOUT)
            .json(body)
            .send()
            .await;
        match response {
            Ok(res) if res.status().is_success() => {
                // An empty default here is a body that never fully arrived,
                // which `ack_from_body` refuses like any other non-ack — the
                // batch stays owed rather than being closed out on a status
                // line alone.
                let raw = res.bytes().await.unwrap_or_default();
                let Some(ack) = ack_from_body(&raw) else {
                    // Deliberately BEFORE `note_success`: a 2xx this build
                    // cannot read is a failed call, and calling it a success
                    // first would re-arm the once-per-session log gate and
                    // spend the line every batch.
                    note_failure(
                        app,
                        "upload: 2xx whose body is not an acknowledgement".to_string(),
                    );
                    return None;
                };
                note_success(app);
                if ack.rejected_unknown_family > 0 {
                    crate::app_log(app, format!(
                        "Merc: the pool refused {} template(s) as unknown families — its support vocabulary is older than this build's; they are not retried",
                        ack.rejected_unknown_family,
                    ));
                }
                crate::app_log(app, format!(
                    "Merc: offered {} template(s) to the pool — {} stored, {} already pooled, {} at the cap, {} retired, {} refused as another family's art, {} malformed",
                    body.templates.len(), ack.stored, ack.duplicate, ack.capped, ack.tombstoned, ack.conflicting, ack.rejected,
                ));
                return Some(ack);
            }
            Ok(res) if res.status() == reqwest::StatusCode::TOO_MANY_REQUESTS => {
                let wait = retry_after_delay(
                    res.headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok()),
                );
                if wait > RETRY_CEILING || attempt == MAX_ATTEMPTS {
                    note_failure(
                        app,
                        format!("upload: rate limited, retry in {}s", wait.as_secs()),
                    );
                    return None;
                }
                tokio::time::sleep(wait).await;
            }
            // A 4xx that is not the rate limit is this build's problem — a
            // format version the server refuses, a body it cannot read, or a 413
            // for a batch over the 128 KB cap. Retrying sends the identical bytes
            // and gets the identical answer, so the batch is dropped and left
            // owed on disk; a 413 in particular would come back at the SAME size,
            // because nothing here re-splits a rejected batch. The guard against
            // it is upstream, in [`MAX_TEMPLATES_PER_BATCH`] and the vocabulary
            // body-cap test that sizes it — not a retry loop.
            Ok(res) if res.status().is_client_error() => {
                let status = res.status();
                let text = res.text().await.unwrap_or_default();
                note_failure(app, format!("upload: {status} — {}", text.trim()));
                return None;
            }
            Ok(res) => {
                if attempt == MAX_ATTEMPTS {
                    note_failure(app, format!("upload: server returned {}", res.status()));
                    return None;
                }
                tokio::time::sleep(RETRY_BASE * attempt).await;
            }
            Err(e) => {
                if attempt == MAX_ATTEMPTS {
                    note_failure(app, format!("upload: {e}"));
                    return None;
                }
                tokio::time::sleep(RETRY_BASE * attempt).await;
            }
        }
    }
    None
}

/// Mark a placed batch in the store and persist it, so a restart does not
/// re-offer what the pool already has.
pub(super) fn mark_uploaded(app: &AppHandle, batch: &[PendingSample]) {
    // An empty settle set is an ordinary answer since POE-210 — a refusal the
    // pool could not place holds every sample owed — and taking the directory
    // and store locks to mark nothing is pure cost on the upload task.
    if batch.is_empty() {
        return;
    }
    let Some(dir) = icons_dir(app) else {
        return;
    };
    let save_err = {
        let state = app.state::<AppState>();
        // Inside the directory lock, like every other writer of it.
        super::super::icons::writing_icons_dir(&state.merc.icons_write, || {
            let mut store = state
                .merc.templates
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let mut touched = false;
            for sample in batch {
                touched |= store.mark_uploaded(&sample.family, sample.tier, &sample.bytes);
            }
            if touched {
                store.save(&dir).err()
            } else {
                None
            }
        })
    };
    if let Some(e) = save_err {
        crate::app_log(app, format!("Merc: upload flags not saved — {e}"));
    }
}

/// Offer every local sample the pool has not seen. Called once per module
/// start, which is what publishes a store learned while the pool did not exist
/// yet (or while the device was offline).
pub fn enqueue_backfill(app: &AppHandle) {
    let pending: Vec<PendingSample> = {
        let state = app.state::<AppState>();
        let store = state
            .merc.templates
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        store
            .pending_uploads()
            .into_iter()
            .map(|(family, tier, bytes)| PendingSample { family, tier, bytes })
            .collect()
    };
    if pending.is_empty() {
        return;
    }
    crate::app_log(
        app,
        format!("Merc: offering {} learned template(s) to the pool", pending.len()),
    );
    enqueue(app, pending);
}
