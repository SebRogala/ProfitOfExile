//! Native capture identity, attribution, confirmation and submission owner.
//! The screen and polled shortcuts are read-only towards the game.
#![cfg_attr(not(windows), allow(dead_code))]
use crate::loot_api::*;
use crate::loot_profiles::{LineMatch, LootProfile, Selection};
use crate::read_hotkey::Shortcut;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::Manager;

pub const MODULE_ID: &str = "boss_loot";
pub fn now_ms() -> u64 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LootLayout {
    #[default]
    Grid,
    List,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NumberFormat {
    #[default]
    Auto,
    Space,
    Comma,
    None,
}
impl NumberFormat {
    pub fn migrate(typed: Option<Self>, legacy: Option<&String>) -> Self {
        typed.unwrap_or_else(|| match legacy.map(String::as_str) {
            Some("space") => Self::Space,
            Some("comma") => Self::Comma,
            Some("none") => Self::None,
            _ => Self::Auto,
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LootSettings {
    pub layout: LootLayout,
    pub auto_save_seconds: u32,
    #[serde(deserialize_with = "not_counted_without_group_items")]
    pub not_counted: Vec<String>,
    pub capture_shortcut: Shortcut,
    pub preview_shortcut: Shortcut,
}
/// Saved before 3.30, the Allflame Ember of Kulemak was a not-counted extra;
/// it is now a guaranteed-group item, so that one stale key is dropped on load
/// rather than failing validation and resetting every boss-loot setting.
fn not_counted_without_group_items<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<String>, D::Error> {
    let mut keys = Vec::<String>::deserialize(deserializer)?;
    keys.retain(|key| key != "Metadata/Items/MapFragments/BetrayalAllflamePack");
    Ok(keys)
}
impl Default for LootSettings {
    fn default() -> Self {
        Self {
            layout: LootLayout::Grid,
            auto_save_seconds: 10,
            not_counted: crate::loot_profiles::built_in_profiles()
                .iter()
                .flat_map(|p| &p.extra)
                .filter(|i| i.not_counted_default)
                .map(|i| i.id.clone())
                .collect(),
            capture_shortcut: Shortcut {
                key: 2,
                shift: true,
                ctrl: false,
                alt: false,
            },
            preview_shortcut: Shortcut {
                key: 119,
                shift: true,
                ctrl: false,
                alt: false,
            },
        }
    }
}
impl LootSettings {
    pub fn validate(&self) -> Result<(), String> {
        if ![0, 5, 10, 20, 30].contains(&self.auto_save_seconds) {
            return Err("Invalid auto-save timeout".into());
        }
        self.capture_shortcut.validate()?;
        self.preview_shortcut.validate()?;
        if self.capture_shortcut == self.preview_shortcut {
            return Err("Shortcuts must differ".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for key in &self.not_counted {
            if !crate::loot_profiles::built_in_profiles()
                .iter()
                .flat_map(|p| &p.extra)
                .any(|i| &i.id == key)
                || !seen.insert(key)
            {
                return Err(format!("Invalid not-counted item: {key}"));
            }
        }
        Ok(())
    }
}
pub fn counted(p: &LootProfile, s: &LootSettings) -> Vec<String> {
    p.groups
        .iter()
        .flat_map(|g| &g.items)
        .chain(&p.extra)
        .chain(&p.categories)
        .filter(|i| !s.not_counted.contains(&i.id))
        .map(|i| i.id.clone())
        .collect()
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attribution {
    pub area: String,
    pub level: u32,
    pub seed: Option<String>,
    pub profile: Option<ProfileRef>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Reading,
    Loading,
    Open,
    Saving,
    Saved,
    Unreachable,
    Rejected,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingCapture {
    pub id: u64,
    pub attribution: Attribution,
    pub league: String,
    pub profile: LootProfile,
    pub layout: LootLayout,
    pub rect: [i32; 4],
    pub incoming: Selection,
    pub items: RunItems,
    pub read_lines: Vec<String>,
    pub unknown_lines: Vec<String>,
    pub ambiguous: Vec<Vec<String>>,
    pub phase: Phase,
    pub deadline_ms: Option<u64>,
    pub close_at_ms: Option<u64>,
    pub target: Option<StoredRun>,
    pub force_new: bool,
    pub run: u64,
    pub next_run: u64,
    pub merge_blocked: bool,
    pub dropped_positive_keys: Vec<String>,
    pub error: Option<String>,
}
impl PendingCapture {
    fn effective(&self, settings: &LootSettings) -> Result<RunItems, String> {
        if self.merge_blocked {
            return Err("Merge target changed; save as new run or discard".into());
        }
        if !self.ambiguous.is_empty() {
            return Err("which one?".into());
        }
        if let Some(target) = &self.target {
            merge_items(&self.profile, target, &self.incoming)
        } else {
            let eligible = counted(&self.profile, settings);
            let quantities = self
                .incoming
                .iter()
                .filter(|(k, q)| **q > 0 && eligible.contains(k))
                .map(|(k, q)| (k.clone(), *q))
                .collect();
            validate_items(
                &self.profile,
                RunItems {
                    quantities,
                    counted: eligible,
                },
            )
        }
    }
    fn restart(&mut self, settings: &LootSettings, now: u64) {
        match self.effective(settings) {
            Ok(items) => {
                self.items = items;
                self.error = None;
                self.deadline_ms = (self.phase == Phase::Open && settings.auto_save_seconds > 0)
                    .then_some(now + u64::from(settings.auto_save_seconds) * 1000);
            }
            Err(error) => {
                self.error = Some(error);
                self.deadline_ms = None;
            }
        }
    }
}
fn validate_items(profile: &LootProfile, items: RunItems) -> Result<RunItems, String> {
    if items
        .quantities
        .keys()
        .any(|key| !items.counted.contains(key))
    {
        return Err("Selection outside counted set".into());
    }
    profile
        .validate_selection(&items.quantities)
        .map_err(|e| format!("{e:?}"))?;
    Ok(items)
}
pub fn merge_items(
    profile: &LootProfile,
    target: &StoredRun,
    incoming: &Selection,
) -> Result<RunItems, String> {
    let incoming: Selection = incoming
        .iter()
        .filter(|(key, q)| **q > 0 && target.items.counted.contains(key))
        .map(|(k, q)| (k.clone(), *q))
        .collect();
    let mut quantities = target.items.quantities.clone();
    for group in &profile.groups {
        if group.max == 1 && group.items.iter().any(|i| incoming.contains_key(&i.id)) {
            for item in &group.items {
                quantities.remove(&item.id);
            }
        }
    }
    for (key, quantity) in incoming {
        quantities
            .entry(key)
            .and_modify(|q| *q = (*q).max(quantity))
            .or_insert(quantity);
    }
    validate_items(
        profile,
        RunItems {
            quantities,
            counted: target.items.counted.clone(),
        },
    )
}
#[derive(Debug, Default)]
pub struct LootOwner {
    sequence: u64,
    poll_generation: u64,
    work_cancel: Option<tokio::sync::watch::Receiver<bool>>,
    pub area: Option<Attribution>,
    pub pending: Option<PendingCapture>,
    pub frame: Option<Frame>,
    pub preview: bool,
    pub rebind: Option<Rebind>,
}
pub struct LootState {
    pub(crate) boss: Mutex<LootOwner>,
    pub(crate) settings: Mutex<LootSettings>,
    pub(crate) number_format: Mutex<NumberFormat>,
}
impl LootState {
    /// The one construction source for `run()` and `test_app_state`.
    /// Deliberately not a `Default` impl, like `test_app_state`: a new field
    /// breaking this constructor is the intended signal to decide whether it
    /// also needs a settings touch point.
    pub(crate) fn new() -> Self {
        Self {
            boss: Mutex::new(LootOwner::default()),
            settings: Mutex::new(LootSettings::default()),
            number_format: Mutex::new(NumberFormat::default()),
        }
    }
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    pub rect: [i32; 4],
    pub phase: String,
    pub attribution: Option<Attribution>,
    pub until_ms: Option<u64>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Rebind {
    Capture,
    Preview,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LootSlice {
    pub enabled: bool,
    pub settings: LootSettings,
    pub number_format: NumberFormat,
    pub pending: Option<PendingCapture>,
    pub frame: Option<Frame>,
    pub rebind: Option<Rebind>,
    pub now_ms: u64,
}
impl Default for LootSlice {
    fn default() -> Self {
        Self {
            enabled: false,
            settings: LootSettings::default(),
            number_format: NumberFormat::Auto,
            pending: None,
            frame: None,
            rebind: None,
            now_ms: 0,
        }
    }
}
impl LootOwner {
    fn preview_tick(&mut self, foreground: bool, rect: Option<[i32; 4]>) {
        if !self.preview {
            return;
        }
        if !foreground {
            self.frame = None;
            return;
        }
        if let Some(rect) = rect {
            self.frame = Some(Frame {
                rect,
                phase: "preview".into(),
                attribution: self.area.clone(),
                until_ms: None,
            });
        }
    }
    pub fn catch_up(&mut self, tail: Option<&str>) {
        self.area = None;
        if let Some(area) = tail.and_then(|tail| {
            tail.lines()
                .rev()
                .find_map(crate::client_log::parse_area_change)
        }) {
            self.observe_area(&area);
        }
    }
    fn start_polling(&mut self) -> u64 {
        self.stop();
        self.poll_generation += 1;
        self.poll_generation
    }
    fn stop_polling(&mut self, generation: u64) {
        if generation == self.poll_generation {
            self.stop();
        }
    }
    pub fn observe_area(&mut self, area: &crate::client_log::AreaChange<'_>) {
        self.area = Some(Attribution {
            area: area.area.into(),
            level: area.level,
            seed: area.seed.map(|s| s.to_string()),
            profile: crate::loot_profiles::profile_for_area(Some(area)).map(ProfileRef::from),
        });
    }
    pub fn begin(
        &mut self,
        league: String,
        rect: [i32; 4],
        settings: &LootSettings,
        now: u64,
    ) -> Result<u64, String> {
        // A capture attempt ends preview even when attribution is refused.
        self.preview = false;
        let area = self.area.clone();
        let profile = area
            .as_ref()
            .and_then(|a| a.profile.as_ref())
            .and_then(|r| {
                crate::loot_profiles::built_in_profiles()
                    .iter()
                    .find(|p| ProfileRef::from(*p) == *r)
            });
        let Some(profile) = profile else {
            self.frame = Some(Frame {
                rect,
                phase: "refused".into(),
                attribution: area,
                until_ms: Some(now + 3000),
            });
            return Err("No boss profile for this zone".into());
        };
        if league.is_empty() {
            return Err("League not resolved".into());
        }
        self.sequence += 1;
        self.frame = Some(Frame {
            rect,
            phase: "reading".into(),
            attribution: area.clone(),
            until_ms: None,
        });
        self.pending = Some(PendingCapture {
            id: self.sequence,
            attribution: area.unwrap(),
            league,
            profile: profile.clone(),
            layout: settings.layout,
            rect,
            incoming: Selection::new(),
            items: RunItems {
                quantities: Selection::new(),
                counted: counted(profile, settings),
            },
            read_lines: Vec::new(),
            unknown_lines: Vec::new(),
            ambiguous: Vec::new(),
            phase: Phase::Reading,
            deadline_ms: None,
            close_at_ms: None,
            target: None,
            force_new: false,
            run: 0,
            next_run: 0,
            merge_blocked: false,
            dropped_positive_keys: Vec::new(),
            error: None,
        });
        Ok(self.sequence)
    }
    pub fn finish_ocr(&mut self, id: u64, lines: Vec<String>, _: &LootSettings, _: u64) {
        let Some(p) = self
            .pending
            .as_mut()
            .filter(|p| p.id == id && p.phase == Phase::Reading)
        else {
            return;
        };
        for line in &lines {
            match p.profile.match_line(line) {
                LineMatch::Item(key) => {
                    p.incoming.insert(key, 1);
                }
                LineMatch::Ambiguous(keys) => {
                    if !p.ambiguous.contains(&keys) {
                        p.ambiguous.push(keys);
                    }
                }
                LineMatch::Unknown => {
                    if !p.unknown_lines.contains(line) {
                        p.unknown_lines.push(line.clone());
                    }
                }
            }
        }
        p.read_lines = lines;
        p.phase = Phase::Loading;
        self.frame = None;
    }
    pub fn ready(
        &mut self,
        id: u64,
        latest: Option<StoredRun>,
        next: u64,
        settings: &LootSettings,
        now: u64,
    ) {
        let Some(p) = self.pending.as_mut().filter(|p| {
            p.id == id && matches!(p.phase, Phase::Loading | Phase::Open | Phase::Saving)
        }) else {
            return;
        };
        p.target = latest.filter(|r| {
            r.profile == ProfileRef::from(&p.profile)
                && r.league == p.league
                && r.seed.is_some()
                && r.seed == p.attribution.seed
        });
        p.run = p.target.as_ref().map(|r| r.run).unwrap_or(next);
        p.next_run = next;
        p.phase = Phase::Open;
        p.restart(settings, now);
    }
    fn editable(&mut self, id: u64) -> Result<&mut PendingCapture, String> {
        self.pending
            .as_mut()
            .filter(|p| p.id == id && p.phase == Phase::Open)
            .ok_or_else(|| "Capture is not editable".into())
    }
    pub fn touch(&mut self, id: u64, settings: &LootSettings, now: u64) -> Result<(), String> {
        self.editable(id)?.restart(settings, now);
        Ok(())
    }
    pub fn select(
        &mut self,
        id: u64,
        key: &str,
        quantity: u32,
        settings: &LootSettings,
        now: u64,
    ) -> Result<(), String> {
        let p = self.editable(id)?;
        let eligible = p
            .target
            .as_ref()
            .map(|r| r.items.counted.clone())
            .unwrap_or_else(|| counted(&p.profile, settings));
        if !eligible.contains(&key.to_string()) {
            return Err("Item is not counted".into());
        }
        if quantity == 0 {
            p.incoming.remove(key);
        } else {
            for group in &p.profile.groups {
                if group.max == 1 && group.items.iter().any(|i| i.id == key) {
                    for item in &group.items {
                        p.incoming.remove(&item.id);
                    }
                }
            }
            p.incoming.insert(key.into(), quantity);
            p.ambiguous.retain(|keys| !keys.iter().any(|k| k == key));
        }
        p.restart(settings, now);
        Ok(())
    }
    pub fn force_new(&mut self, id: u64, settings: &LootSettings, now: u64) -> Result<(), String> {
        let p = self.editable(id)?;
        p.force_new = true;
        p.merge_blocked = false;
        p.target = None;
        p.run = p.next_run;
        p.restart(settings, now);
        Ok(())
    }
    pub fn discard(&mut self, id: u64) -> Result<(), String> {
        if self
            .pending
            .as_ref()
            .is_some_and(|p| p.id == id && p.phase != Phase::Saving)
        {
            self.pending = None;
            self.frame = None;
            Ok(())
        } else {
            Err("Capture cannot be discarded".into())
        }
    }
    fn prepare(
        &mut self,
        settings: &LootSettings,
        now: u64,
        manual: bool,
    ) -> Option<(u64, SubmitRun)> {
        let p = self.pending.as_mut().filter(|p| p.phase == Phase::Open)?;
        if !manual && !p.deadline_ms.is_some_and(|deadline| now >= deadline) {
            return None;
        }
        let items = match p.effective(settings) {
            Ok(items) => items,
            Err(error) => {
                p.error = Some(error);
                p.deadline_ms = None;
                return None;
            }
        };
        let target = p.target.as_ref().map(|r| Target {
            id: r.id.clone(),
            expected_version: r.version,
        });
        let request = SubmitRun {
            profile: ProfileRef::from(&p.profile),
            league: p.league.clone(),
            seed: p.attribution.seed.clone(),
            mode: if target.is_some() {
                "merge"
            } else if p.force_new {
                "forceNew"
            } else {
                "new"
            }
            .into(),
            items: items.clone(),
            incoming_positive_keys: target.as_ref().map(|_| {
                p.incoming
                    .iter()
                    .filter(|(_, q)| **q > 0)
                    .map(|(k, _)| k.clone())
                    .collect()
            }),
            target,
        };
        p.items = items;
        p.phase = Phase::Saving;
        p.deadline_ms = None;
        Some((p.id, request))
    }
    fn complete(&mut self, id: u64, result: Result<SubmitOutcome, ApiError>, now: u64) {
        let Some(p) = self
            .pending
            .as_mut()
            .filter(|p| p.id == id && p.phase == Phase::Saving)
        else {
            return;
        };
        match result {
            Ok(outcome) => {
                p.run = outcome.run.run;
                p.items = outcome.run.items;
                p.phase = Phase::Saved;
                p.dropped_positive_keys = outcome.dropped_positive_keys;
                p.close_at_ms = Some(now + 2000);
            }
            Err(ApiError::Unreachable) => {
                p.phase = Phase::Unreachable;
                p.error = Some("Server unreachable. This run was not recorded.".into());
            }
            Err(error) => {
                p.phase = Phase::Rejected;
                p.error = Some(format!("{error:?}"));
            }
        }
    }
    pub fn retire(&mut self, now: u64) {
        if self
            .pending
            .as_ref()
            .is_some_and(|p| p.close_at_ms.is_some_and(|t| now >= t))
        {
            self.pending = None;
        }
        if self
            .frame
            .as_ref()
            .is_some_and(|f| f.until_ms.is_some_and(|t| now >= t))
        {
            self.frame = None;
        }
    }
    pub fn retire_and_due_id(&mut self, now: u64) -> Option<u64> {
        self.retire(now);
        self.pending
            .as_ref()
            .filter(|p| p.phase == Phase::Open && p.deadline_ms.is_some_and(|t| now >= t))
            .map(|p| p.id)
    }
    fn pause_conflict(&mut self, id: u64) {
        if let Some(p) = self.pending.as_mut().filter(|p| p.id == id) {
            p.merge_blocked = true;
            p.phase = Phase::Open;
            p.deadline_ms = None;
            p.error = Some("Merge target changed; save as new run or discard".into());
        }
    }
    pub fn stop(&mut self) {
        self.pending = None;
        self.frame = None;
        self.preview = false;
        self.rebind = None;
        self.work_cancel = None;
    }
}
#[cfg(test)]
pub async fn submit_pending(
    owner: &Mutex<LootOwner>,
    settings: &LootSettings,
    http: &impl LootTransport,
    now: u64,
    manual: bool,
) {
    let id = owner
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .pending
        .as_ref()
        .map(|p| p.id);
    if let Some(id) = id {
        submit_capture(owner, settings, http, now, manual, id).await;
    }
}
async fn submit_capture(
    owner: &Mutex<LootOwner>,
    settings: &LootSettings,
    http: &impl LootTransport,
    now: u64,
    manual: bool,
    id: u64,
) {
    let started = Instant::now();
    let request = {
        let mut owner = owner.lock().unwrap_or_else(|e| e.into_inner());
        if !owner.pending.as_ref().is_some_and(|p| p.id == id) {
            return;
        }
        owner.prepare(settings, now, manual)
    };
    let Some((id, request)) = request else { return };
    let result = http.submit(&request).await;
    if matches!(result, Err(ApiError::Cancelled)) {
        return;
    }
    if matches!(result, Err(ApiError::Conflict)) {
        let fresh = http.latest(&request.profile).await;
        let mut o = owner.lock().unwrap_or_else(|e| e.into_inner());
        if !o
            .pending
            .as_ref()
            .is_some_and(|p| p.id == id && p.phase == Phase::Saving)
        {
            return;
        }
        match fresh {
            Ok(Some(target)) if request.target.as_ref().is_none_or(|t| t.id == target.id) => {
                let next = o.pending.as_ref().unwrap().next_run;
                o.ready(
                    id,
                    Some(target),
                    next,
                    settings,
                    now + started.elapsed().as_millis() as u64,
                );
                // A different league/seed/profile cannot silently become an append.
                if o.pending.as_ref().is_some_and(|p| p.target.is_none()) {
                    o.pause_conflict(id);
                }
            }
            Ok(_) => o.pause_conflict(id),
            Err(ApiError::Cancelled) => {}
            Err(error) => o.complete(id, Err(error), now),
        }
    } else {
        owner.lock().unwrap_or_else(|e| e.into_inner()).complete(
            id,
            result,
            now + started.elapsed().as_millis() as u64,
        );
    }
}

pub fn capture_rect(cursor: (i32, i32), screen: (u32, u32)) -> [i32; 4] {
    let (sw, sh) = (screen.0 as i32, screen.1 as i32);
    let scale = screen.1 as f64 / 1080.0;
    let w = ((900.0 * scale).round() as i32).clamp(1, sw.max(1));
    let h = ((750.0 * scale).round() as i32).clamp(1, sh.max(1));
    [
        (cursor.0 - w / 2).clamp(0, (sw - w).max(0)),
        (cursor.1 - (h as f64 * 2.0 / 3.0).round() as i32).clamp(0, (sh - h).max(0)),
        w,
        h,
    ]
}
fn read_capture(
    active: impl Fn() -> bool,
    grab: impl FnOnce() -> Result<([i32; 4], image::DynamicImage), String>,
    mut recognize: impl FnMut(&image::DynamicImage) -> Result<Vec<String>, String>,
    preprocess: impl FnOnce(&image::DynamicImage) -> image::DynamicImage,
    scaled_error: impl FnOnce(String),
) -> Result<Option<([i32; 4], Vec<String>)>, String> {
    if !active() {
        return Ok(None);
    }
    let result = grab();
    if !active() {
        return Ok(None);
    }
    let (rect, crop) = result?;
    let raw = recognize(&crop);
    if !active() {
        return Ok(None);
    }
    let raw = raw?;
    let scaled = preprocess(&crop);
    if !active() {
        return Ok(None);
    }
    let upscaled = recognize(&scaled);
    if !active() {
        return Ok(None);
    }
    let upscaled = upscaled.unwrap_or_else(|error| {
        scaled_error(error);
        Vec::new()
    });
    Ok(Some((rect, raw.into_iter().chain(upscaled).collect())))
}
pub fn snapshot(state: &crate::AppState) -> LootSlice {
    let enabled = state
        .modules_enabled
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(MODULE_ID)
        == Some(&true);
    let settings = state
        .loot.settings
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let number_format = *state
        .loot.number_format
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let owner = state.loot.boss.lock().unwrap_or_else(|e| e.into_inner());
    LootSlice {
        enabled,
        settings,
        number_format,
        pending: if enabled { owner.pending.clone() } else { None },
        frame: if enabled { owner.frame.clone() } else { None },
        rebind: owner.rebind,
        now_ms: now_ms(),
    }
}
#[tauri::command]
pub fn loot_capture_view(state: tauri::State<crate::AppState>) -> LootSlice {
    snapshot(&state)
}
#[tauri::command]
pub fn boss_loot_profiles() -> Vec<LootProfile> {
    crate::loot_profiles::built_in_profiles().to_vec()
}
#[tauri::command]
pub fn boss_loot_set_settings(app: tauri::AppHandle, settings: LootSettings) -> Result<(), String> {
    settings.validate()?;
    let state = app.state::<crate::AppState>();
    let read = state.read_hotkey.lock().unwrap_or_else(|e| e.into_inner()).shortcut.clone();
    crate::read_hotkey::apart_from_boss_loot(&read, &settings)?;
    *state
        .loot.settings
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = settings.clone();
    {
        let mut owner = state.loot.boss.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(p) = owner.pending.as_mut().filter(|p| p.phase == Phase::Open) {
            p.restart(&settings, now_ms());
        }
    }
    crate::persist_settings(&app);
    crate::ssot::emit_ssot(&app);
    Ok(())
}
#[tauri::command]
pub fn set_number_format(app: tauri::AppHandle, format: NumberFormat) {
    *app.state::<crate::AppState>()
        .loot.number_format
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = format;
    crate::persist_settings(&app);
    crate::ssot::emit_ssot(&app);
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CaptureAction {
    Touch,
    Select { key: String, quantity: u32 },
    ForceNew,
    Discard,
}
#[tauri::command]
pub fn boss_loot_action(
    app: tauri::AppHandle,
    id: u64,
    action: CaptureAction,
) -> Result<(), String> {
    let state = app.state::<crate::AppState>();
    let settings = state
        .loot.settings
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    {
        let mut owner = state.loot.boss.lock().unwrap_or_else(|e| e.into_inner());
        match action {
            CaptureAction::Touch => owner.touch(id, &settings, now_ms())?,
            CaptureAction::Select { key, quantity } => {
                owner.select(id, &key, quantity, &settings, now_ms())?
            }
            CaptureAction::ForceNew => owner.force_new(id, &settings, now_ms())?,
            CaptureAction::Discard => owner.discard(id)?,
        }
    }
    crate::ssot::emit_ssot(&app);
    Ok(())
}
#[tauri::command]
pub async fn boss_loot_submit(app: tauri::AppHandle, id: u64) -> Result<(), String> {
    let state = app.state::<crate::AppState>();
    if state
        .modules_enabled
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(MODULE_ID)
        != Some(&true)
    {
        return Err("Module is off".into());
    }
    let cancel = {
        let owner = state.loot.boss.lock().unwrap_or_else(|e| e.into_inner());
        if !owner.pending.as_ref().is_some_and(|p| p.id == id) {
            return Err("Stale capture".into());
        }
        owner
            .work_cancel
            .clone()
            .ok_or_else(|| "Module is not running".to_string())?
    };
    let settings = state
        .loot.settings
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    submit_capture(
        &state.loot.boss,
        &settings,
        &LootApi::from_state(&state).with_cancel(cancel),
        now_ms(),
        true,
        id,
    )
    .await;
    crate::ssot::emit_ssot(&app);
    Ok(())
}
#[tauri::command]
pub fn boss_loot_rebind(app: tauri::AppHandle, binding: Option<Rebind>) {
    app.state::<crate::AppState>()
        .loot.boss
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .rebind = binding;
    crate::ssot::emit_ssot(&app);
}
fn keys_to_poll(rebinding: bool, settings: &LootSettings) -> Vec<u16> {
    if rebinding {
        return (1..=254).collect();
    }
    let mut keys = vec![
        settings.capture_shortcut.key,
        settings.preview_shortcut.key,
        16,
        17,
        18,
    ];
    keys.sort_unstable();
    keys.dedup();
    keys
}
#[derive(Default)]
struct ShortcutPoll {
    capture_down: bool,
    preview_down: bool,
    previous: std::collections::BTreeSet<u16>,
    previous_full: bool,
}
enum PollAction {
    Capture,
    Preview,
    Bind(Rebind, Shortcut),
    CancelBind,
}
impl ShortcutPoll {
    fn poll(
        &mut self,
        down: &std::collections::BTreeSet<u16>,
        foreground: bool,
        binding: Option<Rebind>,
        settings: &LootSettings,
    ) -> Option<PollAction> {
        let matches = |s: &Shortcut| {
            down.contains(&s.key)
                && down.contains(&16) == s.shift
                && down.contains(&17) == s.ctrl
                && down.contains(&18) == s.alt
        };
        let capture = matches(&settings.capture_shortcut);
        let preview = matches(&settings.preview_shortcut);
        let action = if let Some(binding) = binding {
            if down.contains(&27) {
                Some(PollAction::CancelBind)
            } else if !self.previous_full {
                None
            } else {
                down.difference(&self.previous)
                    .find(|k| !matches!(**k, 16..=18 | 160..=165))
                    .map(|key| {
                        PollAction::Bind(
                            binding,
                            Shortcut {
                                key: *key,
                                shift: down.contains(&16),
                                ctrl: down.contains(&17),
                                alt: down.contains(&18),
                            },
                        )
                    })
            }
        } else if foreground && capture && !self.capture_down {
            Some(PollAction::Capture)
        } else if foreground && preview && !self.preview_down {
            Some(PollAction::Preview)
        } else {
            None
        };
        self.capture_down = capture;
        self.preview_down = preview;
        self.previous = down.clone();
        self.previous_full = binding.is_some();
        action
    }
}
pub fn spawn_hotkey_poller(
    app: tauri::AppHandle,
    cancel: tokio::sync::watch::Receiver<bool>,
) -> crate::modules::ModuleJoin {
    crate::modules::ModuleJoin::Thread(std::thread::spawn(move || {
        let generation = {
            let state = app.state::<crate::AppState>();
            let mut owner = state.loot.boss.lock().unwrap_or_else(|e| e.into_inner());
            if work_cancelled(&cancel) {
                return;
            }
            let generation = owner.start_polling();
            owner.work_cancel = Some(cancel.clone());
            generation
        };
        #[cfg(windows)]
        let mut poll = ShortcutPoll::default();
        while !work_cancelled(&cancel) {
            std::thread::sleep(Duration::from_millis(30));
            if work_cancelled(&cancel) {
                break;
            }
            let state = app.state::<crate::AppState>();
            #[cfg(windows)]
            {
                let settings = state
                    .loot.settings
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone();
                let binding = state
                    .loot.boss
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .rebind;
                let down = keys_to_poll(binding.is_some(), &settings)
                    .into_iter()
                    .filter(|key| {
                        (unsafe {
                            windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(
                                *key as i32,
                            )
                        } as u16
                            & 0x8000)
                            != 0
                    })
                    .collect();
                let foreground = state
                    .game_in_foreground
                    .load(std::sync::atomic::Ordering::SeqCst);
                match poll.poll(&down, foreground, binding, &settings) {
                    Some(PollAction::Capture) => platform::capture_once(&app, &cancel),
                    Some(PollAction::Preview) => {
                        let mut owner = state.loot.boss.lock().unwrap_or_else(|e| e.into_inner());
                        owner.preview = !owner.preview;
                        if !owner.preview {
                            owner.frame = None;
                        }
                    }
                    Some(PollAction::CancelBind) => {
                        state
                            .loot.boss
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .rebind = None
                    }
                    Some(PollAction::Bind(binding, shortcut)) => {
                        let mut new = settings.clone();
                        match binding {
                            Rebind::Capture => new.capture_shortcut = shortcut,
                            Rebind::Preview => new.preview_shortcut = shortcut,
                        }
                        if let Err(error) = boss_loot_set_settings(app.clone(), new) {
                            crate::app_log(&app, error);
                        }
                        state
                            .loot.boss
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .rebind = None;
                    }
                    None => {}
                }
                if work_cancelled(&cancel) {
                    break;
                }
                platform::preview(&app, foreground);
            }
            let due = state
                .loot.boss
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .retire_and_due_id(now_ms());
            if let Some(id) = due {
                let app = app.clone();
                let cancel = cancel.clone();
                tauri::async_runtime::spawn(async move {
                    if work_cancelled(&cancel) {
                        return;
                    }
                    let state = app.state::<crate::AppState>();
                    if state
                        .modules_enabled
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .get(MODULE_ID)
                        != Some(&true)
                    {
                        return;
                    }
                    let settings = state
                        .loot.settings
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .clone();
                    submit_capture(
                        &state.loot.boss,
                        &settings,
                        &LootApi::from_state(&state).with_cancel(cancel),
                        now_ms(),
                        false,
                        id,
                    )
                    .await;
                    crate::ssot::emit_ssot(&app);
                });
            }
        }
        app.state::<crate::AppState>()
            .loot.boss
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .stop_polling(generation);
        crate::ssot::emit_ssot(&app);
    }))
}
#[cfg(windows)]
mod platform {
    use super::*;
    use std::cell::{Cell, RefCell};
    fn cursor() -> Option<(i32, i32)> {
        let mut p = windows::Win32::Foundation::POINT::default();
        unsafe { windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut p) }.ok()?;
        Some((p.x, p.y))
    }
    pub fn preview(app: &tauri::AppHandle, foreground: bool) {
        let state = app.state::<crate::AppState>();
        let game = *state.game_monitor.lock().unwrap_or_else(|e| e.into_inner());
        let mut owner = state.loot.boss.lock().unwrap_or_else(|e| e.into_inner());
        let rect = if owner.preview && foreground {
            cursor()
                .zip(game)
                .map(|(cur, g)| capture_rect((cur.0 - g.x, cur.1 - g.y), (g.width, g.height)))
        } else {
            None
        };
        owner.preview_tick(foreground, rect);
    }
    pub fn capture_once(app: &tauri::AppHandle, cancel: &tokio::sync::watch::Receiver<bool>) {
        if work_cancelled(cancel) {
            return;
        }
        let Some(cur) = cursor() else { return };
        let state = app.state::<crate::AppState>();
        let settings = state
            .loot.settings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let game = *state.game_monitor.lock().unwrap_or_else(|e| e.into_inner());
        let Some(g) = game else { return };
        let rect = capture_rect((cur.0 - g.x, cur.1 - g.y), (g.width, g.height));
        let league = crate::ssot::league_name(&state).unwrap_or_default();
        // Freeze the area, seed, profile, league and identity BEFORE grabbing/OCR.
        let begin = {
            let mut owner = state.loot.boss.lock().unwrap_or_else(|e| e.into_inner());
            if work_cancelled(cancel) {
                return;
            }
            owner.begin(league, rect, &settings, now_ms())
        };
        let id = match begin {
            Ok(id) => id,
            Err(error) => {
                crate::app_log(app, error);
                crate::ssot::emit_ssot(app);
                return;
            }
        };
        crate::ssot::emit_ssot(app);
        let grab = Cell::new(crate::capture::GrabTiming::default());
        let (crop_ms, preprocess_ms) = (Cell::new(0), Cell::new(0));
        let ocr_ms = RefCell::new(Vec::new());
        let sizes = Cell::new(((0, 0), (0, 0)));
        let started = Instant::now();
        let result = read_capture(
            || !work_cancelled(cancel),
            || {
                let shot = crate::capture::capture_screen(app)?;
                grab.set(shot.timing);
                let stage = Instant::now();
                let rect = capture_rect(
                    (cur.0 - shot.origin.0, cur.1 - shot.origin.1),
                    (shot.image.width(), shot.image.height()),
                );
                let crop = shot.image.crop_imm(
                    rect[0] as u32,
                    rect[1] as u32,
                    rect[2] as u32,
                    rect[3] as u32,
                );
                crop_ms.set(stage.elapsed().as_millis());
                sizes.set((
                    (shot.image.width(), shot.image.height()),
                    (crop.width(), crop.height()),
                ));
                Ok((rect, crop))
            },
            |img: &image::DynamicImage| {
                let stage = Instant::now();
                let lines = crate::ocr::recognize_text(img);
                ocr_ms
                    .borrow_mut()
                    .push(stage.elapsed().as_millis().to_string());
                lines
            },
            |img: &image::DynamicImage| {
                let stage = Instant::now();
                let scaled = crate::capture::preprocess_for_ocr_fast(img);
                preprocess_ms.set(stage.elapsed().as_millis());
                scaled
            },
            |error| crate::app_log(app, format!("loot: 2x OCR failed: {error}")),
        );
        let ((sw, sh), (cw, ch)) = sizes.get();
        let grab = grab.get();
        let outcome = match &result {
            Ok(Some((_, lines))) => format!("{} lines", lines.len()),
            Ok(None) => "cancelled".to_string(),
            Err(_) => "failed".to_string(),
        };
        crate::app_log(app, format!(
            "loot: capture timing — exclude {} ms, grab {} ms ({sw}x{sh}), restore {} ms, capture_screen {} ms, crop {} ms ({cw}x{ch}), OCR raw+2x {} ms, preprocess {} ms, total {} ms — {outcome}",
            grab.exclude_ms, grab.grab_ms, grab.restore_ms, grab.call_ms, crop_ms.get(), ocr_ms.borrow().join("+"), preprocess_ms.get(), started.elapsed().as_millis(),
        ));
        match result {
            Ok(None) => return,
            Ok(Some((rect, lines))) => {
                {
                    let mut owner = state.loot.boss.lock().unwrap_or_else(|e| e.into_inner());
                    if work_cancelled(cancel) {
                        return;
                    }
                    if let Some(p) = owner.pending.as_mut().filter(|p| p.id == id) {
                        p.rect = rect;
                    }
                    owner.finish_ocr(id, lines, &settings, now_ms());
                }
                let app = app.clone();
                let cancel = cancel.clone();
                tauri::async_runtime::spawn(async move {
                    if work_cancelled(&cancel) {
                        return;
                    }
                    let state = app.state::<crate::AppState>();
                    let api = LootApi::from_state(&state).with_cancel(cancel.clone());
                    let profile = {
                        state
                            .loot.boss
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .pending
                            .as_ref()
                            .filter(|p| p.id == id)
                            .map(|p| ProfileRef::from(&p.profile))
                    };
                    let Some(profile) = profile else { return };
                    let result = api.capture_context(&profile).await;
                    let settings = state
                        .loot.settings
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .clone();
                    {
                        let mut owner = state.loot.boss.lock().unwrap_or_else(|e| e.into_inner());
                        if work_cancelled(&cancel) {
                            return;
                        }
                        match result {
                            Ok((latest, next)) => {
                                owner.ready(id, latest, next, &settings, now_ms())
                            }
                            Err(error) => {
                                if let Some(p) = owner
                                    .pending
                                    .as_mut()
                                    .filter(|p| p.id == id && p.phase == Phase::Loading)
                                {
                                    p.phase = Phase::Unreachable;
                                    p.error = Some(format!("{error:?}"));
                                }
                            }
                        }
                    }
                    crate::ssot::emit_ssot(&app);
                });
            }
            Err(error) => {
                let mut owner = state.loot.boss.lock().unwrap_or_else(|e| e.into_inner());
                if work_cancelled(cancel) {
                    return;
                }
                if let Some(p) = owner.pending.as_mut().filter(|p| p.id == id) {
                    p.phase = Phase::Rejected;
                    p.error = Some(error);
                    owner.frame = None;
                }
            }
        }
        crate::ssot::emit_ssot(app);
    }
}

#[cfg(test)]
mod owner_tests {
    use super::*;
    use std::collections::VecDeque;

    const EYE: &str = "Metadata/Items/MapFragments/AbyssPinnacleKeyGhastly";
    const KULEMAK_EMBER: &str = "Metadata/Items/MapFragments/BetrayalAllflamePack";
    fn profile() -> LootProfile {
        crate::loot_profiles::built_in_profiles()[0].clone()
    }
    #[test]
    fn startup_catch_up_uses_latest_area_and_missing_history_refuses_capture() {
        let s = LootSettings::default();
        let mut owner = LootOwner::default();
        owner.catch_up(Some("Generating level 83 area \"MapWorldsStrand\" with seed 1\nGenerating level 83 area \"BetrayalMastermindFight\" with seed 42\n"));
        let id = owner.begin("test".into(), [0; 4], &s, 0).unwrap();
        assert_eq!(owner.pending.as_ref().unwrap().id, id);
        assert_eq!(
            owner.pending.as_ref().unwrap().attribution.seed.as_deref(),
            Some("42")
        );
        owner.catch_up(None);
        assert!(owner.begin("test".into(), [0; 4], &s, 1).is_err());
        assert_eq!(owner.frame.as_ref().unwrap().phase, "refused");
    }
    #[test]
    fn cancelled_poller_cannot_clear_a_new_generations_capture() {
        let s = LootSettings::default();
        let (owner, _) = pending(&s);
        let mut owner = owner.lock().unwrap();
        let old = owner.start_polling();
        let new = owner.start_polling();
        let id = owner.begin("test".into(), [0; 4], &s, 0).unwrap();
        owner.stop_polling(old);
        assert_eq!(owner.pending.as_ref().unwrap().id, id);
        owner.stop_polling(new);
        assert!(owner.pending.is_none());
    }
    fn selection(pairs: &[(&str, u32)]) -> Selection {
        pairs.iter().map(|(k, q)| (k.to_string(), *q)).collect()
    }
    fn run(version: u64) -> StoredRun {
        StoredRun {
            profile: ProfileRef::from(&profile()),
            id: "1".into(),
            league: "test".into(),
            run: 1,
            date: None,
            seed: Some("42".into()),
            version,
            items: RunItems {
                quantities: selection(&[("Cinderswallow Urn", 1), (KULEMAK_EMBER, 1)]),
                counted: counted(&profile(), &LootSettings::default()),
            },
        }
    }
    fn expired_frame() -> Frame {
        Frame {
            rect: [0; 4],
            phase: "saved".into(),
            attribution: None,
            until_ms: Some(0),
        }
    }
    #[test]
    fn ssot_read_leaves_an_expired_capture_and_frame_in_place() {
        let state = crate::settings::tests::test_app_state();
        state
            .modules_enabled
            .lock()
            .unwrap()
            .insert(MODULE_ID.into(), true);
        let (owner, id) = pending(&LootSettings::default());
        let mut owner = owner.into_inner().unwrap();
        owner.pending.as_mut().unwrap().phase = Phase::Saved;
        owner.pending.as_mut().unwrap().close_at_ms = Some(0);
        owner.frame = Some(expired_frame());
        *state.loot.boss.lock().unwrap() = owner;
        snapshot(&state);
        let owner = state.loot.boss.lock().unwrap();
        assert_eq!(owner.pending.as_ref().map(|p| p.id), Some(id));
        assert!(owner.frame.is_some());
    }
    #[test]
    fn tick_retires_a_saved_capture_past_its_close_time() {
        let (owner, _) = pending(&LootSettings::default());
        let mut owner = owner.into_inner().unwrap();
        owner.pending.as_mut().unwrap().phase = Phase::Saved;
        owner.pending.as_mut().unwrap().close_at_ms = Some(0);
        owner.retire_and_due_id(1);
        assert!(owner.pending.is_none());
    }
    #[test]
    fn tick_returns_the_open_capture_past_its_deadline() {
        let (owner, id) = pending(&LootSettings::default());
        let mut owner = owner.into_inner().unwrap();
        owner.pending.as_mut().unwrap().phase = Phase::Open;
        owner.pending.as_mut().unwrap().deadline_ms = Some(0);
        assert_eq!(owner.retire_and_due_id(now_ms()), Some(id));
    }
    #[test]
    fn tick_returns_no_capture_before_its_deadline() {
        let (owner, _) = pending(&LootSettings::default());
        let mut owner = owner.into_inner().unwrap();
        owner.pending.as_mut().unwrap().phase = Phase::Open;
        owner.pending.as_mut().unwrap().deadline_ms = Some(u64::MAX);
        assert_eq!(owner.retire_and_due_id(now_ms()), None);
    }
    fn pending(settings: &LootSettings) -> (Mutex<LootOwner>, u64) {
        pending_with_ocr(settings, &["Serpentine Staff", "Allflame Ember of Kulemak"])
    }
    fn pending_with_ocr(settings: &LootSettings, lines: &[&str]) -> (Mutex<LootOwner>, u64) {
        let mut owner = LootOwner::default();
        owner.observe_area(
            &crate::client_log::parse_area_change(
                r#"Generating level 83 area "BetrayalMastermindFight" with seed 42"#,
            )
            .unwrap(),
        );
        let id = owner
            .begin("test".into(), [0, 0, 900, 750], settings, 0)
            .unwrap();
        owner.finish_ocr(
            id,
            lines.iter().map(|line| (*line).into()).collect(),
            settings,
            0,
        );
        owner.ready(id, None, 1, settings, 0);
        (Mutex::new(owner), id)
    }
    #[test]
    fn saved_not_counted_ember_is_dropped_and_other_settings_kept() {
        let saved: LootSettings = serde_json::from_value(serde_json::json!({
            "layout": "list",
            "notCounted": ["Metadata/Items/MapFragments/BetrayalAllflamePack", "Communion Support"],
            "captureShortcut": {"key": 3, "shift": false, "ctrl": true, "alt": false},
        }))
        .unwrap();
        assert_eq!(saved.validate(), Ok(()));
        assert_eq!(saved.not_counted, ["Communion Support"]);
        assert_eq!(saved.layout, LootLayout::List);
        assert_eq!(
            saved.capture_shortcut,
            Shortcut { key: 3, shift: false, ctrl: true, alt: false }
        );
    }
    #[test]
    fn keepers_corruption_ocr_is_reported_as_unknown() {
        let s = LootSettings::default();
        let (owner, _) = pending_with_ocr(&s, &["Serpentine Staff", "Keeper's Corruption"]);
        let owner = owner.lock().unwrap();
        let capture = owner.pending.as_ref().unwrap();
        assert_eq!(capture.unknown_lines, ["Keeper's Corruption"]);
        assert_eq!(capture.incoming, selection(&[("Cane of Kulemak", 1)]));
    }
    #[derive(Default)]
    struct Http {
        requests: Mutex<Vec<SubmitRun>>,
        replies: Mutex<VecDeque<Result<SubmitOutcome, ApiError>>>,
        latest: Mutex<Option<StoredRun>>,
        reads: Mutex<u32>,
    }
    impl LootTransport for Http {
        async fn submit(&self, request: &SubmitRun) -> Result<SubmitOutcome, ApiError> {
            self.requests.lock().unwrap().push(request.clone());
            self.replies
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Err(ApiError::Unreachable))
        }
        async fn latest(&self, _: &ProfileRef) -> Result<Option<StoredRun>, ApiError> {
            *self.reads.lock().unwrap() += 1;
            Ok(self.latest.lock().unwrap().clone())
        }
    }
    #[tokio::test]
    async fn deselected_ocr_extras_are_omitted_from_submission() {
        let s = LootSettings::default();
        let (owner, id) = pending_with_ocr(
            &s,
            &[
                "Serpentine Staff",
                "Veiled Exalted Orb",
                "Allflame Ember of Kulemak",
            ],
        );
        {
            let mut o = owner.lock().unwrap();
            assert_eq!(
                o.pending.as_ref().unwrap().incoming,
                selection(&[
                    ("Cane of Kulemak", 1),
                    ("Metadata/Items/Currency/CurrencyRerollRareVeiled", 1),
                    ("Metadata/Items/MapFragments/BetrayalAllflamePack", 1),
                ])
            );
            let veiled = "Metadata/Items/Currency/CurrencyRerollRareVeiled";
            o.select(id, veiled, 0, &s, 2).unwrap();
            assert_eq!(
                o.pending.as_ref().unwrap().incoming,
                selection(&[("Cane of Kulemak", 1), (KULEMAK_EMBER, 1)])
            );
            assert_eq!(
                o.pending.as_ref().unwrap().read_lines,
                [
                    "Serpentine Staff",
                    "Veiled Exalted Orb",
                    "Allflame Ember of Kulemak"
                ]
            );
        }
        let http = Http::default();
        submit_pending(&owner, &s, &http, 5, true).await;
        let requests = http.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        let json = serde_json::to_value(&requests[0]).unwrap();
        assert_eq!(
            json["items"]["quantities"],
            serde_json::json!({"Cane of Kulemak": 1, "Metadata/Items/MapFragments/BetrayalAllflamePack": 1})
        );
    }
    #[tokio::test]
    async fn reselected_ocr_extras_are_included_in_submission() {
        let s = LootSettings::default();
        let (owner, id) = pending_with_ocr(
            &s,
            &[
                "Serpentine Staff",
                "Veiled Exalted Orb",
                "Allflame Ember of Kulemak",
            ],
        );
        {
            let mut o = owner.lock().unwrap();
            for key in [
                "Metadata/Items/Currency/CurrencyRerollRareVeiled",
                "Metadata/Items/MapFragments/BetrayalAllflamePack",
            ] {
                o.select(id, key, 0, &s, 1).unwrap();
                assert!(!o.pending.as_ref().unwrap().incoming.contains_key(key));
                o.select(id, key, 1, &s, 2).unwrap();
                assert_eq!(o.pending.as_ref().unwrap().incoming.get(key), Some(&1));
            }
        }
        let http = Http::default();
        submit_pending(&owner, &s, &http, 3, true).await;
        let requests = http.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        let json = serde_json::to_value(&requests[0]).unwrap();
        assert_eq!(
            json["items"]["quantities"],
            serde_json::json!({
                "Cane of Kulemak": 1,
                "Metadata/Items/Currency/CurrencyRerollRareVeiled": 1,
                "Metadata/Items/MapFragments/BetrayalAllflamePack": 1,
            })
        );
    }
    #[tokio::test]
    async fn cleared_ocr_guaranteed_pick_blocks_submission() {
        let s = LootSettings::default();
        let (owner, id) = pending(&s);
        {
            let mut o = owner.lock().unwrap();
            o.select(id, "Cane of Kulemak", 0, &s, 1).unwrap();
            let p = o.pending.as_ref().unwrap();
            assert_eq!(p.incoming, selection(&[(KULEMAK_EMBER, 1)]));
            assert_eq!(p.deadline_ms, None);
            assert_eq!(p.read_lines, ["Serpentine Staff", "Allflame Ember of Kulemak"]);
        }
        let http = Http::default();
        submit_pending(&owner, &s, &http, 2, true).await;
        assert!(http.requests.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn replacement_of_ocr_guaranteed_pick_submits_only_replacement() {
        let s = LootSettings::default();
        let (owner, id) = pending(&s);
        {
            let mut o = owner.lock().unwrap();
            o.select(id, "Cinderswallow Urn", 1, &s, 1).unwrap();
            assert_eq!(
                o.pending.as_ref().unwrap().incoming,
                selection(&[("Cinderswallow Urn", 1), (KULEMAK_EMBER, 1)])
            );
            assert_eq!(
                o.pending.as_ref().unwrap().read_lines,
                ["Serpentine Staff", "Allflame Ember of Kulemak"]
            );
        }
        let http = Http::default();
        submit_pending(&owner, &s, &http, 2, true).await;
        let requests = http.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        let json = serde_json::to_value(&requests[0]).unwrap();
        assert_eq!(
            json["items"]["quantities"],
            serde_json::json!({"Cinderswallow Urn": 1, "Metadata/Items/MapFragments/BetrayalAllflamePack": 1})
        );
    }
    #[tokio::test]
    async fn expiry_starts_once_and_failure_never_queues() {
        let s = LootSettings::default();
        let (owner, _) = pending(&s);
        let http = Http::default();
        submit_pending(&owner, &s, &http, 9_999, false).await;
        assert!(http.requests.lock().unwrap().is_empty());
        submit_pending(&owner, &s, &http, 10_000, false).await;
        submit_pending(&owner, &s, &http, 100_000, false).await;
        assert_eq!(http.requests.lock().unwrap().len(), 1);
        assert_eq!(
            owner.lock().unwrap().pending.as_ref().unwrap().phase,
            Phase::Unreachable
        );
    }
    #[tokio::test]
    async fn off_and_discard_send_nothing() {
        let s = LootSettings {
            auto_save_seconds: 0,
            ..Default::default()
        };
        let (owner, id) = pending(&s);
        let http = Http::default();
        submit_pending(&owner, &s, &http, 100_000, false).await;
        owner.lock().unwrap().discard(id).unwrap();
        submit_pending(&owner, &s, &http, 100_001, true).await;
        assert!(http.requests.lock().unwrap().is_empty());
    }
    #[test]
    fn actions_restart_deadline_and_invalid_or_ambiguous_selections_pause() {
        let s = LootSettings::default();
        let (owner, id) = pending(&s);
        let mut o = owner.lock().unwrap();
        o.touch(id, &s, 500).unwrap();
        assert_eq!(o.pending.as_ref().unwrap().deadline_ms, Some(10_500));
        o.select(id, "Cane of Kulemak", 0, &s, 600).unwrap();
        assert_eq!(o.pending.as_ref().unwrap().deadline_ms, None);
        o.select(id, "Cinderswallow Urn", 1, &s, 700).unwrap();
        assert_eq!(o.pending.as_ref().unwrap().deadline_ms, Some(10_700));
        o.pending
            .as_mut()
            .unwrap()
            .ambiguous
            .push(vec!["Cinderswallow Urn".into(), "Cane of Kulemak".into()]);
        o.touch(id, &s, 800).unwrap();
        assert_eq!(o.pending.as_ref().unwrap().deadline_ms, None);
    }
    #[test]
    fn attribution_is_frozen_before_ocr_and_old_responses_cannot_replace_new_capture() {
        let s = LootSettings::default();
        let (owner, old) = pending(&s);
        let mut o = owner.lock().unwrap();
        o.observe_area(
            &crate::client_log::parse_area_change(
                r#"Generating level 83 area "MapWorldsStrand" with seed 7"#,
            )
            .unwrap(),
        );
        assert_eq!(
            o.pending.as_ref().unwrap().attribution.seed.as_deref(),
            Some("42")
        );
        assert!(o.begin("test".into(), [0; 4], &s, 1).is_err());
        o.observe_area(
            &crate::client_log::parse_area_change(
                r#"Generating level 83 area "BetrayalMastermindFight" with seed 8"#,
            )
            .unwrap(),
        );
        let new = o.begin("test".into(), [0; 4], &s, 2).unwrap();
        o.complete(old, Err(ApiError::Unreachable), 3);
        o.finish_ocr(old, vec!["Silver Flask".into()], &s, 3);
        assert_eq!(o.pending.as_ref().unwrap().id, new);
        assert_eq!(o.pending.as_ref().unwrap().phase, Phase::Reading);
        assert_eq!(
            o.pending.as_ref().unwrap().attribution.seed.as_deref(),
            Some("8")
        );
    }
    #[tokio::test]
    async fn merge_replaces_pick_preserves_eligibility_and_serializes_original_positives() {
        let s = LootSettings::default();
        let (owner, id) = pending(&s);
        let mut target = run(3);
        target.items.counted.retain(|k| k != EYE);
        {
            let mut o = owner.lock().unwrap();
            o.pending.as_mut().unwrap().incoming.insert(EYE.into(), 1);
            o.ready(id, Some(target.clone()), 2, &s, 0);
        }
        let http = Http::default();
        let mut saved = target.clone();
        saved.items.quantities = selection(&[("Cane of Kulemak", 1)]);
        http.replies.lock().unwrap().push_back(Ok(SubmitOutcome {
            state: "saved".into(),
            run: saved,
            dropped_positive_keys: vec![EYE.into()],
        }));
        let changed = LootSettings {
            not_counted: vec!["Cane of Kulemak".into()],
            ..s
        };
        submit_pending(&owner, &changed, &http, 1, true).await;
        let requests = http.requests.lock().unwrap();
        let json = serde_json::to_value(&requests[0]).unwrap();
        assert_eq!(
            json["items"]["quantities"],
            serde_json::json!({"Cane of Kulemak": 1, "Metadata/Items/MapFragments/BetrayalAllflamePack": 1})
        );
        assert_eq!(
            json["incomingPositiveKeys"],
            serde_json::json!(["Cane of Kulemak", EYE, KULEMAK_EMBER])
        );
        assert_eq!(requests[0].items.counted, target.items.counted);
        assert_eq!(
            owner
                .lock()
                .unwrap()
                .pending
                .as_ref()
                .unwrap()
                .dropped_positive_keys,
            [EYE]
        );
    }
    #[test]
    fn repeated_picks_remain_single_and_quantities_use_max() {
        let mut p = profile();
        p.extra[0].quantity = true;
        let mut target = run(1);
        target.items.quantities.insert(EYE.into(), 4);
        let incoming = selection(&[("Cinderswallow Urn", 1), (EYE, 2)]);
        let merged = merge_items(&p, &target, &incoming).unwrap();
        assert_eq!(
            merged.quantities,
            selection(&[("Cinderswallow Urn", 1), (EYE, 4), (KULEMAK_EMBER, 1)])
        );
    }
    #[tokio::test]
    async fn conflict_rereads_version_and_counted_together_and_revalidates_without_resending() {
        let s = LootSettings::default();
        let (owner, id) = pending(&s);
        owner.lock().unwrap().ready(id, Some(run(1)), 2, &s, 0);
        let http = Http::default();
        http.replies
            .lock()
            .unwrap()
            .push_back(Err(ApiError::Conflict));
        let mut fresh = run(2);
        fresh.items.counted.retain(|k| k != "Cane of Kulemak");
        fresh.items.quantities.clear();
        *http.latest.lock().unwrap() = Some(fresh);
        submit_pending(&owner, &s, &http, 10_000, false).await;
        assert_eq!(*http.reads.lock().unwrap(), 1);
        submit_pending(&owner, &s, &http, 30_000, false).await;
        submit_pending(&owner, &s, &http, 30_001, true).await;
        assert_eq!(http.requests.lock().unwrap().len(), 1);
        let o = owner.lock().unwrap();
        let p = o.pending.as_ref().unwrap();
        assert_eq!(p.target.as_ref().unwrap().version, 2);
        assert!(!p
            .target
            .as_ref()
            .unwrap()
            .items
            .counted
            .contains(&"Cane of Kulemak".into()));
        assert_eq!(p.deadline_ms, None);
        assert_eq!(p.phase, Phase::Open);
    }
    #[test]
    fn wider_group_overflow_is_invalid_after_merge() {
        let mut p = profile();
        p.groups[0].max = 2;
        let mut target = run(1);
        target.items.quantities.insert("Spinehail".into(), 1);
        assert!(merge_items(&p, &target, &selection(&[("Cane of Kulemak", 1)])).is_err());
    }
    #[test]
    fn force_new_uses_current_counted_set_and_ineligible_previous_seed_does_not_merge() {
        let s = LootSettings::default();
        let (owner, id) = pending(&s);
        let mut o = owner.lock().unwrap();
        let mut target = run(1);
        target.seed = Some("9".into());
        o.ready(id, Some(target), 2, &s, 0);
        assert!(o.pending.as_ref().unwrap().target.is_none());
        o.ready(id, Some(run(1)), 2, &s, 0);
        o.force_new(id, &s, 1).unwrap();
        assert_eq!(o.pending.as_ref().unwrap().run, 2);
        let changed = LootSettings {
            not_counted: vec![EYE.into()],
            ..s
        };
        let request = o.prepare(&changed, 2, true).unwrap();
        assert_eq!(request.1.mode, "forceNew");
        assert!(!request.1.items.counted.contains(&EYE.into()));
    }
    #[tokio::test]
    async fn concurrent_expiry_and_late_http_response_cannot_overwrite_replacement() {
        struct Slow {
            started: tokio::sync::Notify,
            release: tokio::sync::Notify,
            writes: std::sync::atomic::AtomicU32,
        }
        impl LootTransport for Slow {
            async fn submit(&self, _: &SubmitRun) -> Result<SubmitOutcome, ApiError> {
                self.writes
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                self.started.notify_one();
                self.release.notified().await;
                Ok(SubmitOutcome {
                    state: "saved".into(),
                    run: run(1),
                    dropped_positive_keys: vec![],
                })
            }
            async fn latest(&self, _: &ProfileRef) -> Result<Option<StoredRun>, ApiError> {
                Ok(None)
            }
        }
        let s = LootSettings::default();
        let (owner, _) = pending(&s);
        let http = Slow {
            started: Default::default(),
            release: Default::default(),
            writes: Default::default(),
        };
        let first = submit_pending(&owner, &s, &http, 10_000, false);
        let replace = async {
            http.started.notified().await;
            submit_pending(&owner, &s, &http, 10_001, true).await;
            let new = owner
                .lock()
                .unwrap()
                .begin("test".into(), [0; 4], &s, 10_002)
                .unwrap();
            http.release.notify_one();
            new
        };
        let (_, new) = tokio::join!(first, replace);
        assert_eq!(http.writes.load(std::sync::atomic::Ordering::SeqCst), 1);
        let o = owner.lock().unwrap();
        assert_eq!(o.pending.as_ref().unwrap().id, new);
        assert_eq!(o.pending.as_ref().unwrap().phase, Phase::Reading);
    }
    #[tokio::test]
    async fn valid_conflict_restarts_confirmation_and_next_write_uses_fresh_eligibility() {
        let s = LootSettings::default();
        let (owner, id) = pending(&s);
        owner
            .lock()
            .unwrap()
            .pending
            .as_mut()
            .unwrap()
            .incoming
            .insert(EYE.into(), 1);
        owner.lock().unwrap().ready(id, Some(run(1)), 2, &s, 0);
        let http = Http::default();
        http.replies
            .lock()
            .unwrap()
            .push_back(Err(ApiError::Conflict));
        let mut fresh = run(7);
        fresh.items.counted.retain(|k| k != EYE);
        *http.latest.lock().unwrap() = Some(fresh.clone());
        submit_pending(&owner, &s, &http, 10_000, false).await;
        assert!(
            owner
                .lock()
                .unwrap()
                .pending
                .as_ref()
                .unwrap()
                .deadline_ms
                .unwrap()
                >= 20_000
        );
        submit_pending(&owner, &s, &http, 10_001, true).await;
        let requests = http.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[1].target.as_ref().unwrap().expected_version, 7);
        assert_eq!(requests[1].items.counted, fresh.items.counted);
        assert!(!requests[1].items.quantities.contains_key(EYE));
        assert!(requests[1]
            .incoming_positive_keys
            .as_ref()
            .unwrap()
            .contains(&EYE.into()));
    }
    #[tokio::test]
    async fn impossible_effective_merge_never_calls_submit() {
        let s = LootSettings::default();
        let (owner, id) = pending(&s);
        let mut target = run(1);
        target.items.quantities.insert("Spinehail".into(), 1);
        {
            let mut o = owner.lock().unwrap();
            o.pending.as_mut().unwrap().profile.groups[0].max = 2;
            o.ready(id, Some(target), 2, &s, 0);
        }
        let http = Http::default();
        submit_pending(&owner, &s, &http, 30_000, true).await;
        assert!(http.requests.lock().unwrap().is_empty());
        assert_eq!(
            owner.lock().unwrap().pending.as_ref().unwrap().deadline_ms,
            None
        );
    }
    #[tokio::test]
    async fn saved_response_closes_at_two_seconds_and_keeps_server_run_number() {
        let s = LootSettings::default();
        let (owner, _) = pending(&s);
        let http = Http::default();
        let mut saved = run(1);
        saved.run = 79;
        http.replies.lock().unwrap().push_back(Ok(SubmitOutcome {
            state: "saved".into(),
            run: saved,
            dropped_positive_keys: vec![],
        }));
        submit_pending(&owner, &s, &http, 0, true).await;
        let mut o = owner.lock().unwrap();
        assert_eq!(o.pending.as_ref().unwrap().run, 79);
        let close = o.pending.as_ref().unwrap().close_at_ms.unwrap();
        o.retire(close - 1);
        assert!(o.pending.is_some());
        o.retire(close);
        assert!(o.pending.is_none());
    }
    #[tokio::test]
    async fn conflicting_first_capture_revalidates_the_new_previous_run_before_merging() {
        let s = LootSettings::default();
        let (owner, _) = pending(&s);
        let http = Http::default();
        http.replies
            .lock()
            .unwrap()
            .push_back(Err(ApiError::Conflict));
        *http.latest.lock().unwrap() = Some(run(5));
        submit_pending(&owner, &s, &http, 10_000, false).await;
        assert_eq!(http.requests.lock().unwrap().len(), 1);
        let o = owner.lock().unwrap();
        assert_eq!(
            o.pending.as_ref().unwrap().target.as_ref().unwrap().version,
            5
        );
        assert_eq!(
            o.pending.as_ref().unwrap().items.quantities,
            selection(&[("Cane of Kulemak", 1), (KULEMAK_EMBER, 1)])
        );
        assert!(o.pending.as_ref().unwrap().deadline_ms.is_some());
    }
    #[tokio::test]
    async fn intervening_run_blocks_stale_merge_but_allows_explicit_force_new() {
        let s = LootSettings::default();
        let (owner, id) = pending(&s);
        owner.lock().unwrap().ready(id, Some(run(1)), 2, &s, 0);
        let http = Http::default();
        http.replies
            .lock()
            .unwrap()
            .push_back(Err(ApiError::Conflict));
        let mut intervening = run(2);
        intervening.id = "2".into();
        intervening.run = 2;
        *http.latest.lock().unwrap() = Some(intervening);
        submit_pending(&owner, &s, &http, 10_000, false).await;
        submit_pending(&owner, &s, &http, 30_000, true).await;
        assert_eq!(http.requests.lock().unwrap().len(), 1);
        assert_eq!(
            owner.lock().unwrap().pending.as_ref().unwrap().phase,
            Phase::Open
        );
        assert_eq!(
            owner.lock().unwrap().pending.as_ref().unwrap().deadline_ms,
            None
        );
        owner.lock().unwrap().force_new(id, &s, 30_001).unwrap();
        submit_pending(&owner, &s, &http, 30_002, true).await;
        let requests = http.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[1].mode, "forceNew");
        assert!(requests[1].target.is_none());
    }
}

#[cfg(test)]
mod shortcut_tests {
    use super::*;
    fn down(keys: &[u16]) -> std::collections::BTreeSet<u16> {
        keys.iter().copied().collect()
    }
    #[test]
    fn loot_shortcuts_require_foreground_and_a_new_press() {
        let s = LootSettings::default();
        let mut poll = ShortcutPoll::default();
        assert!(poll.poll(&down(&[16, 2]), false, None, &s).is_none());
        assert!(poll.poll(&down(&[16, 2]), true, None, &s).is_none());
        poll.poll(&down(&[]), true, None, &s);
        assert!(matches!(
            poll.poll(&down(&[16, 2]), true, None, &s),
            Some(PollAction::Capture)
        ));
        assert!(poll.poll(&down(&[16, 2]), true, None, &s).is_none());
    }
    #[test]
    fn loot_rebind_accepts_keyboard_and_mouse_with_modifiers() {
        let s = LootSettings::default();
        let mut poll = ShortcutPoll::default();
        poll.poll(&down(&[]), false, Some(Rebind::Capture), &s);
        match poll.poll(&down(&[17, 65]), false, Some(Rebind::Capture), &s) {
            Some(PollAction::Bind(Rebind::Capture, key)) => assert_eq!(
                key,
                Shortcut {
                    key: 65,
                    ctrl: true,
                    shift: false,
                    alt: false
                }
            ),
            _ => panic!("keyboard binding was not captured"),
        }
        poll.poll(&down(&[]), false, None, &s);
        poll.poll(&down(&[]), false, Some(Rebind::Preview), &s);
        match poll.poll(&down(&[18, 5]), false, Some(Rebind::Preview), &s) {
            Some(PollAction::Bind(Rebind::Preview, key)) => assert_eq!(
                key,
                Shortcut {
                    key: 5,
                    ctrl: false,
                    shift: false,
                    alt: true
                }
            ),
            _ => panic!("mouse binding was not captured"),
        }
    }
    #[test]
    fn loot_escape_cancels_rebinding_without_changing_the_binding() {
        let s = LootSettings::default();
        let mut poll = ShortcutPoll::default();
        assert!(matches!(
            poll.poll(&down(&[27]), false, Some(Rebind::Capture), &s),
            Some(PollAction::CancelBind)
        ));
        assert_eq!(s.capture_shortcut.key, 2);
    }
    #[test]
    fn loot_rebound_shortcut_activates_instead_of_default() {
        let s = LootSettings {
            capture_shortcut: Shortcut {
                key: 65,
                ctrl: true,
                shift: false,
                alt: false,
            },
            ..Default::default()
        };
        let mut poll = ShortcutPoll::default();
        assert!(poll.poll(&down(&[16, 2]), true, None, &s).is_none());
        assert!(matches!(
            poll.poll(&down(&[17, 65]), true, None, &s),
            Some(PollAction::Capture)
        ));
    }
    #[test]
    fn narrowed_poll_reads_only_the_bound_keys_and_modifiers() {
        let s = LootSettings {
            capture_shortcut: Shortcut {
                key: 65,
                shift: false,
                ctrl: false,
                alt: false,
            },
            preview_shortcut: Shortcut {
                key: 122,
                shift: false,
                ctrl: false,
                alt: false,
            },
            ..Default::default()
        };
        assert_eq!(keys_to_poll(false, &s), vec![16, 17, 18, 65, 122]);
    }
    #[test]
    fn rebinding_poll_reads_every_virtual_key() {
        assert_eq!(
            keys_to_poll(true, &LootSettings::default()),
            (1..=254).collect::<Vec<u16>>()
        );
    }
    #[test]
    fn key_held_when_rebinding_starts_does_not_bind() {
        let s = LootSettings::default();
        let mut poll = ShortcutPoll::default();
        poll.poll(&down(&[]), false, None, &s);
        assert!(poll
            .poll(&down(&[1]), false, Some(Rebind::Capture), &s)
            .is_none());
    }
    #[test]
    fn key_pressed_after_the_rebinding_baseline_binds() {
        let s = LootSettings::default();
        let mut poll = ShortcutPoll::default();
        poll.poll(&down(&[]), false, None, &s);
        poll.poll(&down(&[1]), false, Some(Rebind::Capture), &s);
        match poll.poll(&down(&[1, 65]), false, Some(Rebind::Capture), &s) {
            Some(PollAction::Bind(Rebind::Capture, key)) => assert_eq!(
                key,
                Shortcut {
                    key: 65,
                    shift: false,
                    ctrl: false,
                    alt: false
                }
            ),
            _ => panic!("fresh press after the baseline was not bound"),
        }
    }
    #[test]
    fn loot_geometry_preserves_poc_dimensions_and_clamping() {
        assert_eq!(capture_rect((960, 540), (1920, 1080)), [510, 40, 900, 750]);
        assert_eq!(
            capture_rect((1920, 1080), (3840, 2160)),
            [1020, 80, 1800, 1500]
        );
        assert_eq!(capture_rect((10, 1070), (1920, 1080)), [0, 330, 900, 750]);
    }
}

#[cfg(test)]
mod review_r1_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn refused_capture_survives_the_next_preview_tick_without_submission() {
        struct NoSubmit(AtomicUsize);
        impl LootTransport for NoSubmit {
            async fn submit(&self, _: &SubmitRun) -> Result<SubmitOutcome, ApiError> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Err(ApiError::Unreachable)
            }
            async fn latest(&self, _: &ProfileRef) -> Result<Option<StoredRun>, ApiError> {
                Ok(None)
            }
        }
        let settings = LootSettings::default();
        let mut owner = LootOwner::default();
        owner.preview = true;
        owner.catch_up(Some(
            "Generating level 83 area \"MapWorldsStrand\" with seed 7",
        ));
        assert!(owner
            .begin("test".into(), [10, 20, 900, 750], &settings, 100)
            .is_err());
        owner.preview_tick(true, Some([30, 40, 900, 750]));
        let frame = owner.frame.as_ref().unwrap();
        assert_eq!(frame.phase, "refused");
        assert_eq!(frame.rect, [10, 20, 900, 750]);
        assert_eq!(frame.until_ms, Some(3100));
        assert_eq!(frame.attribution.as_ref().unwrap().area, "MapWorldsStrand");
        let http = NoSubmit(AtomicUsize::new(0));
        submit_pending(&Mutex::new(owner), &settings, &http, 10_000, true).await;
        assert_eq!(http.0.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn normal_preview_tick_keeps_following_the_cursor() {
        let mut owner = LootOwner::default();
        owner.preview = true;
        owner.preview_tick(true, Some([10, 20, 900, 750]));
        owner.preview_tick(true, Some([30, 40, 900, 750]));
        assert_eq!(owner.frame.as_ref().unwrap().phase, "preview");
        assert_eq!(owner.frame.as_ref().unwrap().rect, [30, 40, 900, 750]);
        owner.preview_tick(false, Some([30, 40, 900, 750]));
        assert!(owner.frame.is_none());
    }

    #[test]
    fn cancellation_during_raw_ocr_starts_no_preprocessing_or_second_read() {
        let (cancel, rx) = tokio::sync::watch::channel(false);
        let (started, started_rx) = std::sync::mpsc::channel();
        let (release, release_rx) = std::sync::mpsc::channel();
        let reads = AtomicUsize::new(0);
        let scales = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            let read_count = &reads;
            let scale_count = &scales;
            let work = scope.spawn(move || {
                read_capture(
                    || !*rx.borrow(),
                    || Ok(([0, 0, 1, 1], image::DynamicImage::new_rgb8(1, 1))),
                    |_| {
                        let pass = read_count.fetch_add(1, Ordering::SeqCst);
                        if pass == 0 {
                            started.send(()).unwrap();
                            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                        }
                        Ok(vec!["Silver Flask".into()])
                    },
                    |crop| {
                        scale_count.fetch_add(1, Ordering::SeqCst);
                        crop.clone()
                    },
                    |_| panic!("OCR should not fail"),
                )
            });
            started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            cancel.send(true).unwrap();
            release.send(()).unwrap();
            assert!(work.join().unwrap().unwrap().is_none());
        });
        assert_eq!(
            scales.load(Ordering::SeqCst),
            0,
            "preprocessing started after cancellation"
        );
        assert_eq!(
            reads.load(Ordering::SeqCst),
            1,
            "2x OCR started after cancellation"
        );
    }

    #[test]
    fn active_capture_preserves_raw_and_scaled_ocr_union() {
        let mut pass = 0;
        let result = read_capture(
            || true,
            || Ok(([1, 2, 3, 4], image::DynamicImage::new_rgb8(1, 1))),
            |_| {
                pass += 1;
                Ok(vec![if pass == 1 {
                    "Silver Flask".into()
                } else {
                    "Serpentine Staff".into()
                }])
            },
            |crop| crop.clone(),
            |_| panic!("OCR should not fail"),
        )
        .unwrap()
        .unwrap();
        assert_eq!(result.0, [1, 2, 3, 4]);
        assert_eq!(result.1, ["Silver Flask", "Serpentine Staff"]);
    }
}
