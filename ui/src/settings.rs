//! A tiny local settings file, separate from `project.json` (which is
//! per-project arrangement data): app-level preferences that persist
//! across projects and across launches. Currently just which input
//! device to record from - see `engine::input::available_input_devices`.
//! Read once at startup; there's no live-switching yet, so writing a new
//! choice here takes effect on the next launch, not immediately.

use std::path::PathBuf;

fn path() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../settings.json"))
}

/// The saved input device name, if any - `None` means "whatever the OS
/// calls default", same as before this setting existed.
pub fn load_input_device() -> Option<String> {
    let text = std::fs::read_to_string(path()).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value.get("input_device")?.as_str().map(str::to_string)
}

pub fn save_input_device(device: Option<&str>) {
    let value = serde_json::json!({ "input_device": device });
    if let Ok(text) = serde_json::to_string_pretty(&value) {
        let _ = std::fs::write(path(), text);
    }
}
