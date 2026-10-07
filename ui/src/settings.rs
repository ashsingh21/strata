//! A tiny local settings file, separate from `project.json` (which is
//! per-project arrangement data): app-level preferences that persist
//! across projects and across launches - the recording input device, the
//! theme, and whether the sidebar is open. Each key is written on its own
//! (read-modify-write), so saving one never drops the others. The input
//! device is only read at startup (no live-switching yet), so a new choice
//! there takes effect on the next launch.

use std::path::PathBuf;

fn path() -> PathBuf {
    crate::paths::data_dir().join("settings.json")
}

fn load_all() -> serde_json::Map<String, serde_json::Value> {
    std::fs::read_to_string(path())
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

fn save_key(key: &str, value: serde_json::Value) {
    let mut all = load_all();
    all.insert(key.to_string(), value);
    if let Ok(text) = serde_json::to_string_pretty(&serde_json::Value::Object(all)) {
        let _ = std::fs::write(path(), text);
    }
}

/// The saved input device name, if any - `None` means "whatever the OS
/// calls default", same as before this setting existed.
pub fn load_input_device() -> Option<String> {
    load_all().get("input_device")?.as_str().map(str::to_string)
}

pub fn save_input_device(device: Option<&str>) {
    save_key("input_device", serde_json::json!(device));
}

/// The colour theme's id, if one was chosen - or, from before there were
/// more than two, the old light/dark switch.
pub fn load_theme() -> Option<String> {
    let all = load_all();
    if let Some(id) = all.get("theme").and_then(|v| v.as_str()) {
        return Some(id.to_string());
    }
    all.get("daylight")?.as_bool().map(|d| if d { "daylight" } else { "studio" }.to_string())
}

/// Crash reports already mentioned at a launch (paths).
pub fn load_crashes_seen() -> Vec<String> {
    load_all()
        .get("crashes_seen")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

pub fn save_crashes_seen(paths: &[String]) {
    save_key("crashes_seen", serde_json::json!(paths));
}

pub fn save_theme(id: &str) {
    save_key("theme", serde_json::json!(id));
}

/// The UI zoom (1.0 = normal).
pub fn load_zoom() -> Option<f64> {
    load_all().get("zoom")?.as_f64()
}

pub fn save_zoom(zoom: f64) {
    save_key("zoom", serde_json::json!(zoom));
}

pub fn load_sidebar_open() -> Option<bool> {
    load_all().get("sidebar_open")?.as_bool()
}

pub fn save_sidebar_open(open: bool) {
    save_key("sidebar_open", serde_json::json!(open));
}

/// Ids of the lessons finished so far (see `crate::lessons`).
pub fn load_lessons_done() -> Vec<String> {
    load_all()
        .get("lessons_done")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

pub fn save_lessons_done(ids: &[String]) {
    save_key("lessons_done", serde_json::json!(ids));
}

/// The file holding each finished lesson's project (lesson id -> path),
/// so the next part of the house track can carry on from it.
pub fn load_lesson_track(id: &str) -> Option<std::path::PathBuf> {
    load_all().get("lesson_tracks")?.get(id)?.as_str().map(std::path::PathBuf::from)
}

pub fn save_lesson_track(id: &str, path: &std::path::Path) {
    let mut tracks = load_all().get("lesson_tracks").and_then(|v| v.as_object().cloned()).unwrap_or_default();
    tracks.insert(id.to_string(), serde_json::json!(path.to_string_lossy()));
    save_key("lesson_tracks", serde_json::Value::Object(tracks));
}

/// Ids of the tips already dismissed (see `crate::hints`).
pub fn load_hints_dismissed() -> Vec<String> {
    load_all()
        .get("hints_dismissed")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

pub fn save_hints_dismissed(ids: &[String]) {
    save_key("hints_dismissed", serde_json::json!(ids));
}

/// Whether the live spectrum analyzer was open.
pub fn load_analyzer_open() -> bool {
    load_all().get("analyzer_open").and_then(|v| v.as_bool()).unwrap_or(false)
}

pub fn save_analyzer_open(open: bool) {
    save_key("analyzer_open", serde_json::json!(open));
}

/// A browser list of item ids (favourites, history).
pub fn load_browser_list(key: &str) -> Vec<String> {
    load_all()
        .get(key)
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

pub fn save_browser_list(key: &str, ids: &[String]) {
    save_key(key, serde_json::json!(ids));
}

/// The browser's user collections.
pub fn load_browser_collections() -> Vec<crate::browser::Collection> {
    let Some(list) = load_all().get("browser_collections").and_then(|v| v.as_array()).cloned() else { return vec![] };
    list.iter()
        .filter_map(|c| {
            Some(crate::browser::Collection {
                name: c.get("name")?.as_str()?.to_string(),
                color: serde_json::from_value(c.get("color")?.clone()).ok()?,
                items: c.get("items")?.as_array()?.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
            })
        })
        .collect()
}

pub fn save_browser_collections(collections: &[crate::browser::Collection]) {
    let list: Vec<serde_json::Value> = collections
        .iter()
        .map(|c| serde_json::json!({ "name": c.name, "color": serde_json::to_value(c.color).unwrap_or_default(), "items": c.items }))
        .collect();
    save_key("browser_collections", serde_json::Value::Array(list));
}

/// The sidebar panel's width and section, per project (by its file; an
/// unsaved project shares one entry).
pub fn load_browser_layout(project: Option<&std::path::Path>) -> Option<(f32, String)> {
    let key = project.map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let entry = load_all().get("browser_layout")?.get(&key)?.clone();
    Some((entry.get("width")?.as_f64()? as f32, entry.get("section")?.as_str()?.to_string()))
}

pub fn save_browser_layout(project: Option<&std::path::Path>, width: f32, section: &str) {
    let key = project.map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let mut all = load_all().get("browser_layout").and_then(|v| v.as_object().cloned()).unwrap_or_default();
    all.insert(key, serde_json::json!({ "width": width, "section": section }));
    save_key("browser_layout", serde_json::Value::Object(all));
}

/// Analysed sample keys ("Fits key"), by source and file size: (size,
/// key or none for an unpitched sound).
pub fn load_sample_keys() -> std::collections::HashMap<String, (u64, Option<crate::browser::keys::SampleKey>)> {
    let Some(map) = load_all().get("sample_keys").and_then(|v| v.as_object().cloned()) else { return Default::default() };
    map.into_iter()
        .filter_map(|(source, v)| {
            let size = v.get("size")?.as_u64()?;
            let key = v.get("root").and_then(|r| r.as_u64()).map(|root| crate::browser::keys::SampleKey {
                root: root as u8,
                minor: v.get("minor").and_then(|m| m.as_bool()).unwrap_or(false),
                notes: v.get("notes").and_then(|n| n.as_u64()).unwrap_or(0) as u16,
            });
            Some((source, (size, key)))
        })
        .collect()
}

pub fn save_sample_keys(keys: &std::collections::HashMap<std::sync::Arc<str>, (u64, Option<crate::browser::keys::SampleKey>)>) {
    let map: serde_json::Map<String, serde_json::Value> = keys
        .iter()
        .map(|(source, (size, key))| {
            let v = match key {
                Some(k) => serde_json::json!({ "size": size, "root": k.root, "minor": k.minor, "notes": k.notes }),
                None => serde_json::json!({ "size": size }),
            };
            (source.to_string(), v)
        })
        .collect();
    save_key("sample_keys", serde_json::Value::Object(map));
}

/// The browser preview's volume, dB.
pub fn load_preview_volume() -> Option<f32> {
    load_all().get("preview_volume")?.as_f64().map(|v| v as f32)
}

pub fn save_preview_volume(db: f32) {
    save_key("preview_volume", serde_json::json!(db));
}

/// The lower panel's height (see `crate::splitter`), logical pixels.
pub fn load_lower_panel_height() -> Option<f32> {
    load_all().get("lower_panel_height")?.as_f64().map(|h| h as f32)
}

pub fn save_lower_panel_height(height: f32) {
    save_key("lower_panel_height", serde_json::json!(height));
}

/// The Learn goal being worked through.
pub fn load_learn_goal() -> Option<usize> {
    load_all().get("learn_goal").and_then(|v| v.as_u64()).map(|v| v as usize)
}

pub fn save_learn_goal(goal: usize) {
    save_key("learn_goal", serde_json::json!(goal));
}

/// Today's practice: the day (days since 1970) and the slots done on it.
pub fn load_practice_today() -> Option<(u64, Vec<usize>)> {
    let all = load_all();
    let v = all.get("practice_today")?;
    let day = v.get("day")?.as_u64()?;
    let slots = v.get("done")?.as_array()?.iter().filter_map(|s| s.as_u64().map(|s| s as usize)).collect();
    Some((day, slots))
}

pub fn save_practice_today(day: u64, slots: &[usize]) {
    save_key("practice_today", serde_json::json!({ "day": day, "done": slots }));
}

/// The days today's practice was finished, for the streak.
pub fn load_practice_days() -> Vec<u64> {
    load_all().get("practice_days").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|v| v.as_u64()).collect()).unwrap_or_default()
}

pub fn save_practice_days(days: &[u64]) {
    save_key("practice_days", serde_json::json!(days));
}

/// The chat's model name (see `chat::MODELS`).
pub fn load_ai_model() -> Option<String> {
    load_all().get("ai_model").and_then(|v| v.as_str()).map(str::to_string)
}

pub fn save_ai_model(model: &str) {
    save_key("ai_model", serde_json::json!(model));
}

/// The Anthropic workspace to bill, for an API key not scoped to one.
pub fn load_ai_workspace() -> Option<String> {
    load_all().get("ai_workspace").and_then(|v| v.as_str()).map(str::to_string).filter(|s| !s.is_empty())
}

pub fn save_ai_workspace(id: &str) {
    save_key("ai_workspace", serde_json::json!(id));
}
