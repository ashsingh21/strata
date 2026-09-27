//! A tiny local settings file, separate from `project.json` (which is
//! per-project arrangement data): app-level preferences that persist
//! across projects and across launches - the recording input device, the
//! theme, and whether the sidebar is open. Each key is written on its own
//! (read-modify-write), so saving one never drops the others. The input
//! device is only read at startup (no live-switching yet), so a new choice
//! there takes effect on the next launch.

use std::path::PathBuf;

fn path() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../settings.json"))
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

/// `Some(true)` for Daylight, `Some(false)` for Studio, `None` if never set.
pub fn load_daylight() -> Option<bool> {
    load_all().get("daylight")?.as_bool()
}

pub fn save_daylight(daylight: bool) {
    save_key("daylight", serde_json::json!(daylight));
}

pub fn load_sidebar_open() -> Option<bool> {
    load_all().get("sidebar_open")?.as_bool()
}

pub fn save_sidebar_open(open: bool) {
    save_key("sidebar_open", serde_json::json!(open));
}
