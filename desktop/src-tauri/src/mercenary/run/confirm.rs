use super::*;

/// What a hover-confirm established about one cell.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfirmedCell {
    pub family: String,
    pub tier: u8,
    pub ids: Vec<String>,
    pub name: Option<String>,
    pub score: f32,
}

/// Re-apply remembered confirmations to a freshly read capture.
///
/// A confirmed cell outranks whatever the template store said this tick: the
/// user told us what it is. The score comes from the tooltip read, not from the
/// icon correlation, so the page's tooltip does not claim an icon match that
/// never happened.
pub fn apply_confirmed(
    capture: &mut MercCapture,
    confirmed: &HashMap<(String, u8), ConfirmedCell>,
) {
    for row in &mut capture.rows {
        let key = row_key(&row.skill);
        for cell in &mut row.supports {
            let Some(c) = confirmed.get(&(key.clone(), cell.slot)) else {
                continue;
            };
            cell.family = Some(c.family.clone());
            cell.tier = Some(c.tier);
            cell.ids = c.ids.clone();
            cell.name = c.name.clone();
            cell.score = c.score;
            cell.state = ReadState::Confirmed;
            cell.candidates.clear();
        }
    }
}

/// How long an uncorroborated hover read waits for its second opinion.
///
/// Ten seconds is a hover the player has walked away from. The pending is not
/// refreshed by later reads — `at` is stamped at read 1 and never moved — so
/// this bounds the whole wait, not the gap between reads, and a cell whose
/// tooltip flickers cannot hold the slot indefinitely.
pub const PENDING_CONFIRM_TTL: Duration = Duration::from_secs(10);

/// How long a standing claim is allowed to explain a naming read on ANOTHER
/// cell as its own lagging tooltip.
///
/// Measured: the two "Ailment Damage" reads of 2026-08-26 landed at 23:23:21
/// and 23:23:22 — about one second apart, on adjacent slots, the second of them
/// the first cell's tooltip arriving over the second cell's crop. Two seconds is
/// that lag with margin. Past it the tooltip on screen is the cell the cursor is
/// actually in, so a read there is the player moving on rather than the game
/// catching up. See [`fold_pending`].
pub const TOOLTIP_LAG_BOUND: Duration = Duration::from_secs(2);

/// One tooltip read that NAMED a support, held until a second read agrees with
/// it (POE-207 L8).
///
/// The crop travels WITH the claim. A tooltip read is evidence about the cell
/// the cursor was in at read 1, and the cold crop cached for that cell at that
/// moment is the art the confirmation is about; looking the crop up again at
/// apply time would re-open the gap this guard exists to close, because a
/// re-detect can have replaced the cache in between.
///
/// `crop` is `None` when the cache held nothing for the cell — the capture that
/// cropped it has been replaced since. The confirmation still stands (it names
/// the cell); only the template is missing, which the apply path reports as
/// `Learned::NoCrop`.
#[derive(Debug, Clone)]
pub struct PendingConfirm {
    /// `(row key, slot)` — the same identity `Session::confirmed` and
    /// [`SigCache`] use.
    pub key: (String, u8),
    pub family: String,
    pub tier: u8,
    pub name: Option<String>,
    pub ids: Vec<String>,
    pub score: f32,
    /// The cold crop as it stood at read 1, and the [`ScaleSource`] it was cut
    /// at. See the type doc, and [`SigCache`] for why the registration travels
    /// with the crop instead of being read off the session at apply time.
    pub crop: Option<(CellSig, Option<image::RgbaImage>, ScaleSource)>,
    /// Stamped at read 1 and never refreshed. See [`PENDING_CONFIRM_TTL`].
    pub at: Instant,
}

/// Fold a fresh naming read into the held one: `(new pending, apply now)`.
///
/// Why a confirmation costs TWO agreeing reads (measured, 2026-08-26 23:23:21
/// and 23:23:22): the game's tooltip lags a moving cursor, so a hover tick that
/// grabs the screen a frame after the cursor entered a new cell reads the
/// PREVIOUS cell's tooltip over the NEW cell's crop. Two "Ailment Damage"
/// confirms landed one second apart on adjacent slots that way, and the second
/// of them filed Faster Attacks art under Ailment Damage — 19 of the 21
/// poisoned templates in Sebastian's store on 2026-08-26 came from that
/// mechanism. A lagging tooltip names a DIFFERENT cell on each read of a
/// sweep, so requiring two reads that agree on the same `(row key, slot)`
/// costs a sweep everything and a deliberate hover one extra 400 ms tick.
///
/// The three outcomes:
///
/// - same key, same family AND tier — the claim is corroborated: apply the
///   HELD read (its crop, its score), and the slot is free again;
/// - same key, different family or tier — the two reads disagree about the
///   same cell, so neither is evidence: the fresh read replaces the held one
///   and starts its own wait;
/// - another key — split on the age of the CLAIM, at [`TOOLTIP_LAG_BOUND`]:
///   - a claim younger than the bound explains the read as its own lagging
///     tooltip, so the read is DISCARDED and the claim stands. A sweep is
///     exactly a run of reads on different keys inside the bound, and letting
///     each one take the slot would make the guard a one-tick delay instead of
///     a corroboration rule;
///   - a claim at least the bound old cannot: the tooltip has had longer than
///     the measured lag to catch up, so a read on another cell is the player
///     moving on. The fresh read REPLACES the claim (nothing applied) and the
///     new cell is confirmable on its own next read.
///
/// The residual the split accepts: a tooltip that lags for MORE than two
/// seconds can still be attributed to the cell the cursor moved to. What bounds
/// it is the guard's own rule one level down — that misattributed read is only
/// a claim, and it still needs a second read on the new cell to agree with it
/// before anything is learned.
///
/// An EXPIRED pending is treated as no pending at all — the fresh read takes
/// the slot. [`PENDING_CONFIRM_TTL`] is the outer bound on the whole wait; the
/// lag bound above only decides who wins a cross-cell read inside it. Both
/// checks are here rather than on a timer because nothing else wakes to run
/// them.
pub fn fold_pending(
    pending: Option<PendingConfirm>,
    read: PendingConfirm,
) -> (Option<PendingConfirm>, FoldOutcome) {
    let held = pending.filter(|p| read.at.duration_since(p.at) < PENDING_CONFIRM_TTL);
    match held {
        Some(p) if p.key == read.key && p.family == read.family && p.tier == read.tier => {
            (None, FoldOutcome::Applied(p))
        }
        Some(p) if p.key == read.key => (Some(read), FoldOutcome::Replaced),
        Some(p) if read.at.duration_since(p.at) < TOOLTIP_LAG_BOUND => {
            let age = read.at.duration_since(p.at);
            let standing = p.key.clone();
            (Some(p), FoldOutcome::Discarded { standing, age })
        }
        Some(_) => (Some(read), FoldOutcome::Replaced),
        None => (Some(read), FoldOutcome::Held),
    }
}

/// Which branch of [`fold_pending`] fired, so the caller can say what actually
/// happened rather than lumping three different outcomes under one line.
#[derive(Debug, Clone)]
pub enum FoldOutcome {
    /// A second read agreed with the standing claim: confirm the cell from
    /// THIS claim — the first read's family, tier, score and crop snapshot.
    Applied(PendingConfirm),
    /// There was no claim (or the old one had expired); this read is now the
    /// claim and the cell needs one more read.
    Held,
    /// A claim was displaced — either the same cell disagreeing with itself, or
    /// a read past [`TOOLTIP_LAG_BOUND`] on a cell the player moved to. The
    /// fresh read is the claim now; nothing was confirmed.
    Replaced,
    /// The read was thrown away: a claim on another cell is still young enough
    /// to explain it as its own lagging tooltip. Nothing changed — which is why
    /// the caller refunds the hover budget it charged for this tick.
    Discarded {
        /// The claim that won, for the log line.
        standing: (String, u8),
        /// How old that claim was when this read arrived.
        age: Duration,
    },
}

/// Drop a pending claim whose row is not in the capture any more.
///
/// The claim is about a cell, and a row key the fresh capture does not carry is
/// a cell that is no longer on screen — a rematch, a different mercenary, or a
/// skill line the reader now resolves to something else. Corroborating it
/// against a later read of a row that merely happens to key the same way is
/// exactly the cross-cell confusion the guard exists to stop, so it goes.
pub fn drop_pending_off_capture(
    pending: Option<PendingConfirm>,
    capture: &MercCapture,
) -> Option<PendingConfirm> {
    pending.filter(|p| capture.rows.iter().any(|r| row_key(&r.skill) == p.key.0))
}

/// One line of a hover-tooltip read, with how far it fell from the cursor.
#[derive(Debug, Clone, PartialEq)]
pub struct TooltipLine {
    pub text: String,
    /// Squared px distance from the cursor to the nearest point of the line's
    /// rect. Squared because only the ORDER is ever used, and integers do not
    /// need a total-order dance to sort.
    pub distance_sq: i64,
}

/// Squared distance from `cursor` to the nearest point of `rect` — 0 inside it.
pub fn distance_sq(rect: [i32; 4], cursor: (i32, i32)) -> i64 {
    let [x, y, w, h] = rect;
    let dx = (x - cursor.0).max(cursor.0 - (x + w)).max(0) as i64;
    let dy = (y - cursor.1).max(cursor.1 - (y + h)).max(0) as i64;
    dx * dx + dy * dy
}

/// Map a hover crop's OCR lines back to screen space and score them by cursor
/// distance.
///
/// `upscale` is the factor `preprocess_for_ocr` applied to the crop, read off
/// the processed image rather than assumed:
/// every rect the OCR reports is in the PROCESSED image's pixel space, so
/// skipping the division would put every line at twice its real offset.
pub fn tooltip_lines(
    ocr: &[OcrLineBox],
    region: [i32; 4],
    upscale: (f32, f32),
    cursor: (i32, i32),
) -> Vec<TooltipLine> {
    let (sx, sy) = (upscale.0.max(f32::EPSILON), upscale.1.max(f32::EPSILON));
    ocr.iter()
        .map(|l| {
            let rect = [
                region[0] + (l.x as f32 / sx).round() as i32,
                region[1] + (l.y as f32 / sy).round() as i32,
                (l.w as f32 / sx).round().max(1.0) as i32,
                (l.h as f32 / sy).round().max(1.0) as i32,
            ];
            TooltipLine {
                text: l.text.clone(),
                distance_sq: distance_sq(rect, cursor),
            }
        })
        .collect()
}

/// A hover confirmation has to clear this Jaro-Winkler score, over and above
/// the name matcher's own `Matched` gate.
///
/// The tooltip's BODY names other supports' families: "Supported Skills deal
/// 15% more Melee Physical Damage" OCRs as a line reading "Physical Damage",
/// which scores 0.92 against the family `Physical Damage Reduction` — exactly
/// the matcher's `name_match` — and on 2026-09-06 that line, nearer the cursor
/// than the title, confirmed a Melee Physical Damage cell as Physical Damage
/// Reduction. A title reads its own name at 1.0 and a title with an OCR slip
/// still reads above 0.97; a body fragment that happens to prefix a family
/// name does not. A confirmation is the one read a hover cannot undo, so it
/// asks for more than a detect does.
pub(super) const TITLE_MATCH: f32 = 0.97;

/// Read a confirmation out of a hover tooltip (D5).
///
/// The BEST-scoring line wins, and the one nearest the cursor among equals.
/// Score first because the tooltip's own body is in the crop and names other
/// families in passing (see [`TITLE_MATCH`]); the title names the support
/// outright and outscores any fragment of the body. Distance second because
/// the hover region is ~600×620 scaled px and deliberately overlaps the panel
/// it was opened from, so it contains the skill-name column too — and the two
/// vocabularies overlap (`Frenzy` is both a merc skill and a support family):
/// a skill name three rows up would tie the title at 1.0, and the nearer line
/// is the tooltip that is actually open. Either rule alone was measured
/// wrong; first-match was the original bug, nearest-match the 2026-09-06 one.
///
/// `cell_tier` is the badge tier, used only when the tooltip title carried no
/// tier of its own. No tier at all → no confirmation: the family alone names up
/// to three different links. A `(family, tier)` the vocabulary has no entry
/// for — Physical Damage Reduction exists only Gilded, at tier 3 — is no
/// confirmation either: it names a support that does not exist, and the badge
/// tier saying otherwise is the badge disagreeing with the title.
pub fn confirm_from_tooltip(
    lines: &[TooltipLine],
    cell_tier: Option<u8>,
    vocab: &MercVocab,
    thresholds: &super::super::Thresholds,
) -> Option<ConfirmedCell> {
    let mut best: Option<(&TooltipLine, SupportTitleRead)> = None;
    for line in lines {
        let read = vocab.match_support_title(&line.text, thresholds);
        if read.state != ReadState::Matched || read.score < TITLE_MATCH {
            continue;
        }
        let better = match &best {
            None => true,
            Some((near, standing)) => {
                read.score > standing.score
                    || (read.score == standing.score && line.distance_sq < near.distance_sq)
            }
        };
        if better {
            best = Some((line, read));
        }
    }
    let (_, title) = best?;
    let family = title.family?;
    let tier = title.tier.or(cell_tier)?;
    // A title that named its own tier already resolved to ids; a bare family
    // name did not, so the badge tier has to do the resolving.
    let (ids, name) = if title.tier.is_some() {
        (title.ids, title.name)
    } else {
        let matches = vocab.resolve(&family, tier);
        let (ids, name, _, _) = classify_resolution(&matches);
        (ids, name)
    };
    if ids.is_empty() {
        return None;
    }
    Some(ConfirmedCell {
        family,
        tier,
        ids,
        name,
        score: title.score,
    })
}

/// Turn one [`TemplateStore::learn`] answer into the three things the confirm
/// path needs: what to tell the player, whether a disk save is owed, and
/// whether the pool may be offered the sample.
///
/// Pure, and lifted out of the locked block that produces the answer, so the
/// REFUSAL path has a seam. A sample the store refused must buy neither — and
/// the offer is the load-bearing half: publishing a mislabel is exactly how one
/// wrong confirmation would reach every other device, which is the half of
/// POE-207 AC3 the local hover guard cannot cover.
///
/// `confirmed_family` / `confirmed_tier` are the cell's, from the corroborated
/// read. The family and tier inside [`LearnOutcome::ConflictsWith`] are the
/// OTHER ones — whoever already holds the art.
///
/// **`outcome` is `None` when the store was never asked** (POE-215 D3): the
/// crop was cut at [`ScaleSource::Ocr`], which is the 6-12 px drift POE-214
/// removed, and learning from it would file art the matcher can never reach
/// again — and then publish it. The refusal is a REFUSAL and not a failure: the
/// cell is still confirmed for the overlay, the player is told why nothing was
/// stored, and hovering the same cell once the fit lands stores it.
///
/// Asking the store and then discarding the answer would not do: `learn`
/// mutates, so the sample would already be in memory and on the next save.
pub(super) fn learn_result(
    outcome: Option<LearnOutcome>,
    confirmed_family: &str,
    confirmed_tier: u8,
    bytes: Vec<u8>,
) -> (Learned, bool, Option<sync::PendingSample>) {
    let Some(outcome) = outcome else {
        return (Learned::Unregistered, false, None);
    };
    let stored = outcome.stored();
    let learned = match outcome {
        LearnOutcome::Stored => Learned::Saved,
        LearnOutcome::AlreadyKnown => Learned::AlreadyKnown,
        LearnOutcome::ConflictsWith {
            family,
            tier,
            origin,
            score,
        } => Learned::ConflictsWith {
            family,
            tier,
            origin,
            score,
        },
    };
    (
        learned,
        stored,
        // Only a sample the store actually took: art it already had was offered
        // to the pool when it was first learned, and art it REFUSED must never
        // be published.
        stored.then(|| sync::PendingSample {
            family: confirmed_family.to_string(),
            tier: confirmed_tier,
            bytes,
        }),
    )
}

/// What [`learn_confirmed`] did with one corroborated crop: the
/// [`learn_result`] triple plus the seeds the learn evicted, which the caller
/// blocklists once the store mutex is free.
#[derive(Debug)]
pub(super) struct LearnedConfirmation {
    pub(super) learned: Learned,
    pub(super) needs_save: bool,
    pub(super) offer: Option<sync::PendingSample>,
    pub(super) lost_seeds: Vec<String>,
}

/// The store half of a hover confirm: the registration gate, the learn, the
/// pool bytes and the seeds it evicted, in that order and in one lock
/// acquisition. `crop`, `family` and `tier` come from the CORROBORATED read
/// (`FoldOutcome::Applied`), never from the read that just arrived.
///
/// Returns with the store mutex released, so the caller's `block_seed`
/// (`seed::SEED_BLOCKLIST_LOCK`) never runs under it (POE-208).
pub(super) fn learn_confirmed(
    store: &std::sync::Mutex<TemplateStore>,
    crop: Option<(CellSig, Option<image::RgbaImage>, ScaleSource)>,
    family: &str,
    tier: u8,
    thresholds: &super::super::Thresholds,
) -> LearnedConfirmation {
    let (learned, needs_save, offer, lost_seeds) = match crop {
        // THE REGISTRATION GATE (POE-215 D3). A crop cut at the OCR guess is
        // 6-12 px off the art, and a template learned from one poisons every
        // later match on this device and — through the pool — on everybody
        // else's. The store is not asked at all, so no `Local` sample below
        // `icons::REGISTRATION` can be created; `learn_result` turns the
        // un-asked case into the line the player reads.
        Some((.., source)) if crop_registration(source) < super::super::icons::REGISTRATION => {
            let (learned, needs_save, offer) = learn_result(None, family, tier, Vec::new());
            (learned, needs_save, offer, Vec::new())
        }
        Some((sig, raw, source)) => {
            // The bytes the pool gets, taken before the signature is moved into
            // the store. Copied here rather than read back out of the store so
            // the payload is built from memory on the one path that has it
            // (POE-201 L4) — the store directory is never walked.
            let bytes = sig.bytes().to_vec();
            let registration = crop_registration(source);
            let (outcome, lost_seeds) = {
                let mut store = store.lock().unwrap_or_else(|e| e.into_inner());
                // Snapshotted under the SAME acquisition as the learn, like the
                // pull's merge and the forget command: `LearnOutcome` describes
                // the confirmation, so the only way to name the seed a colliding
                // confirm evicted is to diff the list across the mutation.
                let seeded = store.seeded_families();
                // The crop is the DETECT frame's, cached before the cursor ever
                // reached this cell (D5): a hovered cell may be drawn
                // highlighted, and a template learned from the highlight
                // matches nothing later.
                let outcome = store.learn(family, tier, sig, raw, registration, thresholds);
                let lost = store.seeds_lost_since(&seeded);
                (outcome, lost)
            };
            let (learned, needs_save, offer) = learn_result(Some(outcome), family, tier, bytes);
            (learned, needs_save, offer, lost_seeds)
        }
        // The confirmation still stands — it names the cell. Only the template
        // is missing, and saying so is the difference between "we already knew
        // this art" and "we never had the crop".
        None => (Learned::NoCrop, false, None, Vec::new()),
    };
    LearnedConfirmation {
        learned,
        needs_save,
        offer,
        lost_seeds,
    }
}

/// What a hover-confirm did to the template store.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Learned {
    /// A new `(family, tier)` sample was recorded and flushed to disk.
    Saved,
    /// The store already held this pair; a confirmed sample is never
    /// overwritten (`TemplateStore::learn`).
    AlreadyKnown,
    /// The identical art is already filed under a DIFFERENT family, so the
    /// template was refused (`TemplateStore::learn`). The cell is still
    /// confirmed — the player named it — and the line says which family holds
    /// the art, because that is the key a `forget` would have to target. It
    /// also says WHERE that sample came from: "learned here" is the player's
    /// own earlier mistake, "from the pool" is somebody else's, and the two
    /// read very differently to a player who is sure of what they just hovered.
    ConflictsWith {
        family: String,
        tier: u8,
        origin: Origin,
        score: f32,
    },
    /// No pre-hover crop was cached for the cell — the capture that produced it
    /// has been replaced since. The cell is still confirmed.
    NoCrop,
    /// The crop was cut at [`ScaleSource::Ocr`] — the pre-POE-214 guess, 6-12 px
    /// off the art — so the store was never asked and the pool was never
    /// offered anything (POE-215 D3). The cell is still confirmed; the fix is
    /// to hover it again once the frame fit lands, which the detect line's
    /// `via frame` / `via ocr` says.
    ///
    /// It OUTRANKS `AlreadyKnown`, and that is a consequence of not asking: a
    /// cell the store already holds, re-hovered in a session that never fits,
    /// reads "NOT learned — cut at the OCR guess" rather than "already known".
    /// Accepted. The alternative is asking the store to find out, and `learn`
    /// mutates — the unregistered sample would be in memory and on the next
    /// save before the answer came back.
    Unregistered,
}

impl Learned {
    pub(super) fn describe(&self) -> String {
        match self {
            Learned::Saved => "template saved".to_string(),
            Learned::AlreadyKnown => "template already known".to_string(),
            Learned::ConflictsWith {
                family,
                tier,
                origin,
                score,
            } => format!(
                "template NOT learned: identical art is already stored as {family} \
                 (tier {tier}, {}, ncc {score:.2})",
                origin.describe(),
            ),
            Learned::NoCrop => {
                "no pre-hover crop cached, template not learned".to_string()
            }
            Learned::Unregistered => {
                "template NOT learned — the cell was cut at the OCR guess (fit declined, \
                 nothing held)"
                    .to_string()
            }
        }
    }
}

/// Write a confirmation into a single cell. Shared with [`apply_confirmed`] so
/// a confirmed cell looks the same whether it was just confirmed or restored
/// onto a later capture.
pub(super) fn apply_one(cell: &mut MercSupportRead, c: &ConfirmedCell) {
    cell.family = Some(c.family.clone());
    cell.tier = Some(c.tier);
    cell.ids = c.ids.clone();
    cell.name = c.name.clone();
    cell.score = c.score;
    cell.state = ReadState::Confirmed;
    cell.candidates.clear();
}
