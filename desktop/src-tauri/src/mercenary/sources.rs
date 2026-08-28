//! Who takes part in the verdict — the enabled-guide set (POE-199).
//!
//! The verdict engine is TypeScript and stays there; what lives here is the one
//! INPUT two windows have to agree on. The Mercenaries page and the verdict
//! overlay both evaluate the same capture against the same rulesets, so if they
//! read a different enabled-guide set they print a different headline for the
//! same mercenary — a page saying WORTH beside an overlay saying SKIP.
//!
//! It used to be an ADR-013 `ui_prefs` string (`mercSourcesOff`), which is
//! fetched once per webview and written back with no notification, so a second
//! window held whatever the map said when it booted. This module is the fix:
//! Rust owns the value, [`crate::ssot::compose_snapshot`] echoes it into the
//! `mercenary` slice, and every window reads it off the same 3-second poll.
//!
//! Stored as the OFF-list, like the preference it replaces: a guide added to
//! [`SOURCE_IDS`] later starts enabled for everyone rather than silently
//! switched off for every existing install.

use tauri::{AppHandle, Manager};

use crate::AppState;

/// Every guide the verdict engine knows, in the order the page shows them.
///
/// The TypeScript `SOURCE_IDS` (`$lib/mercenaries/rulesets.ts`) is the other
/// half of this list — it is what the rulesets are actually declared under, and
/// this side exists to VALIDATE, so an id that only one of the two knows is a
/// setting that can be stored and never applied. `merc-sources.test.ts` parses
/// this literal and fails if the two lists stop matching.
pub const SOURCE_IDS: &[&str] = &["guide-a", "guide-b", "guide-c", "guide-d"];

/// The `ui_prefs` key the off-list lived under before this module (ADR-013).
///
/// Read exactly once, by [`migrate_sources_off`], and only while the typed
/// setting is absent. The key itself is left in the map rather than deleted: it
/// costs nothing, and a user who rolls back to an older build finds their
/// choice where that build looks for it.
pub const LEGACY_PREF_KEY: &str = "mercSourcesOff";

/// Accept an off-list, or say which id is not a guide.
///
/// Rejects rather than drops, because this is the SETTER's gate: a caller
/// naming an unknown guide has a bug or a typo, and silently storing a
/// normalised list would leave the window that sent it showing a toggle nobody
/// honours. The tolerant read is [`sanitise_sources_off`], which is what a
/// settings FILE goes through — a file is not a caller and must never fail a
/// load.
///
/// The accepted list is returned normalised: deduplicated and in [`SOURCE_IDS`]
/// order, so the stored value does not depend on the order the user clicked in.
pub fn validate_sources_off(ids: &[String]) -> Result<Vec<String>, String> {
    if let Some(unknown) = ids.iter().find(|id| !SOURCE_IDS.contains(&id.as_str())) {
        return Err(format!(
            "{unknown:?} is not a guide — known guides: {}",
            SOURCE_IDS.join(", ")
        ));
    }
    Ok(normalise(ids.iter().map(String::as_str)))
}

/// The same list, read from a file that may say anything.
///
/// Unknown ids are DROPPED here, not rejected: the value outlives the code that
/// wrote it, so a guide renamed between builds must not be able to switch off a
/// guide that now has a different name — and must not take the whole settings
/// file down with it either. Returns the accepted list plus the ids it refused,
/// so `apply_to_state` can say out loud that the file and the running value
/// disagree.
pub fn sanitise_sources_off(ids: &[String]) -> (Vec<String>, Vec<String>) {
    let (known, unknown): (Vec<&String>, Vec<&String>) = ids
        .iter()
        .partition(|id| SOURCE_IDS.contains(&id.as_str()));
    (
        normalise(known.into_iter().map(String::as_str)),
        unknown.into_iter().cloned().collect(),
    )
}

/// Read the ADR-013 preference's comma-separated value.
///
/// Mirrors the TypeScript `parseSourcesOff` it takes over from, including the
/// trimming: the map is a hand-editable JSON file, and ` guide-a , guide-b `
/// meant the same thing there.
pub fn parse_legacy_sources_off(raw: &str) -> Vec<String> {
    normalise(
        raw.split(',')
            .map(str::trim)
            .filter(|part| SOURCE_IDS.contains(part)),
    )
}

/// Deduplicate and order by [`SOURCE_IDS`]. Every accepted list goes through
/// here, so one stored value has one spelling.
fn normalise<'a>(ids: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let seen: Vec<&str> = ids.into_iter().collect();
    SOURCE_IDS
        .iter()
        .filter(|id| seen.contains(id))
        .map(|id| id.to_string())
        .collect()
}

/// What the enabled-guide set should be on this launch.
///
/// The one-time migration lives here rather than in `settings.rs` so it is
/// testable without an `AppHandle`: while the typed setting has never been
/// written (`None`), the ADR-013 preference is read once and becomes the
/// starting value; from the first save onwards the typed field answers and the
/// old key is ignored. `None` with no preference either is the shipped default
/// — every guide ON, which is the empty off-list.
pub fn migrate_sources_off(
    stored: Option<&Vec<String>>,
    legacy_pref: Option<&String>,
) -> (Vec<String>, Vec<String>) {
    match stored {
        Some(ids) => sanitise_sources_off(ids),
        None => (
            legacy_pref
                .map(|raw| parse_legacy_sources_off(raw))
                .unwrap_or_default(),
            Vec::new(),
        ),
    }
}

/// Set which guides take no part in the verdict.
///
/// Written to the owner, persisted, and nudged out — the next `get_ssot` on
/// every window carries it, which is what makes the page and the overlay agree
/// (POE-199 L5). The rejection is returned AND logged, like every temple
/// setter: an `Err` alone leaves no trace a shipped build can read.
#[tauri::command]
pub fn merc_set_sources_off(sources_off: Vec<String>, app: AppHandle) -> Result<(), String> {
    let accepted = match validate_sources_off(&sources_off) {
        Ok(accepted) => accepted,
        Err(e) => {
            crate::app_log(&app, format!("Merc: guide set rejected — {e}"));
            return Err(e);
        }
    };
    {
        let state = app.state::<AppState>();
        *state
            .merc_sources_off
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = accepted.clone();
    }
    crate::persist_settings(&app);
    // No `publish` here: the set is composed onto the slice at read time
    // (`ssot::compose_snapshot`), so there is no slice field to write and no
    // second copy to keep in step. The nudge is what makes the page's own
    // re-fetch land the new value in the same frame.
    crate::ssot::emit_ssot(&app);
    crate::app_log(
        &app,
        if accepted.is_empty() {
            "Merc: every guide takes part in the verdict".to_string()
        } else {
            format!("Merc: guides switched off — {}", accepted.join(", "))
        },
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn a_caller_naming_an_unknown_guide_is_refused() {
        let err = validate_sources_off(&ids(&["guide-a", "guide-zzz"]))
            .expect_err("an unknown guide must not be storable");
        assert!(
            err.contains("guide-zzz"),
            "the rejection must name the id it refused, got {err:?}",
        );
    }

    #[test]
    fn an_accepted_list_is_stored_in_registry_order() {
        assert_eq!(
            validate_sources_off(&ids(&["guide-b", "guide-a"])).expect("both are real guides"),
            ids(&["guide-a", "guide-b"]),
        );
    }

    #[test]
    fn a_guide_named_twice_is_stored_once() {
        assert_eq!(
            validate_sources_off(&ids(&["guide-a", "guide-a"])).expect("a real guide"),
            ids(&["guide-a"]),
        );
    }

    #[test]
    fn an_empty_list_is_accepted_as_every_guide_on() {
        assert_eq!(validate_sources_off(&[]).expect("the default"), Vec::<String>::new());
    }

    /// A settings FILE is not a caller: one stale id must not fail the load,
    /// and must not switch off the guide that now carries that name either.
    #[test]
    fn a_stored_list_drops_the_id_that_is_no_longer_a_guide() {
        let (accepted, refused) = sanitise_sources_off(&ids(&["guide-b", "guide-zzz"]));
        assert_eq!(accepted, ids(&["guide-b"]));
        assert_eq!(refused, ids(&["guide-zzz"]), "the refusal must be reportable");
    }

    #[test]
    fn the_legacy_preference_is_read_as_the_off_list_it_was() {
        assert_eq!(
            parse_legacy_sources_off(" guide-b , guide-a "),
            ids(&["guide-a", "guide-b"]),
            "the pref was hand-editable, so its spacing carried over",
        );
    }

    #[test]
    fn a_legacy_preference_naming_a_dead_guide_drops_it() {
        assert_eq!(parse_legacy_sources_off("guide-a,guide-zzz"), ids(&["guide-a"]));
    }

    /// The migration's whole point: a user who switched guide A off before
    /// POE-199 still has it off afterwards, without touching anything.
    #[test]
    fn the_old_preference_seeds_the_setting_while_it_has_never_been_written() {
        let pref = "guide-a".to_string();
        let (accepted, _) = migrate_sources_off(None, Some(&pref));
        assert_eq!(accepted, ids(&["guide-a"]));
    }

    /// And the other half: once the typed setting exists it ANSWERS, so a
    /// stale pref cannot switch a guide back off after the user turned it on.
    #[test]
    fn the_typed_setting_outranks_the_old_preference_once_it_exists() {
        let pref = "guide-a".to_string();
        let stored = Vec::new();
        let (accepted, _) = migrate_sources_off(Some(&stored), Some(&pref));
        assert_eq!(
            accepted,
            Vec::<String>::new(),
            "an empty stored list means every guide on — the pref is ignored",
        );
    }

    #[test]
    fn a_fresh_install_starts_with_every_guide_on() {
        let (accepted, refused) = migrate_sources_off(None, None);
        assert!(accepted.is_empty());
        assert!(refused.is_empty());
    }
}
