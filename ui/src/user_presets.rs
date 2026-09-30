//! Carve presets you save yourself: one JSON file per preset in the data
//! folder's `Presets`. The file name is the preset's name.

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use shared::synth::{SynthState, PRESETS};

const MAX_NAME: usize = 32;

pub fn dir() -> PathBuf {
    crate::paths::data_dir().join("Presets")
}

/// What a typed name becomes: no characters a file name can't hold,
/// single spaces, a length that fits the dropdown.
pub fn clean_name(raw: &str) -> String {
    let kept: String = raw.chars().filter(|c| !c.is_control() && !"/\\:*?\"<>|".contains(*c)).collect();
    let spaced = kept.split_whitespace().collect::<Vec<_>>().join(" ");
    spaced.chars().take(MAX_NAME).collect::<String>().trim().to_string()
}

/// A built-in's name (case aside) can't be taken: two presets that read
/// the same in the list would be indistinguishable.
pub fn is_builtin(name: &str) -> bool {
    PRESETS.iter().any(|(n, _)| n.eq_ignore_ascii_case(name))
}

/// `SynthState::name` is a `&'static str`; a saved preset's name is
/// made static here, once per distinct name, so the patch can carry it.
pub fn intern(name: &str) -> &'static str {
    static NAMES: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let mut names = NAMES.get_or_init(Default::default).lock().unwrap();
    if let Some(&known) = names.get(name) {
        return known;
    }
    let leaked: &'static str = Box::leak(name.to_string().into_boxed_str());
    names.insert(leaked);
    leaked
}

fn file(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.json"))
}

/// The saved presets, alphabetical.
pub fn list_in(dir: &Path) -> Vec<&'static str> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension()? != "json" {
                return None;
            }
            path.file_stem()?.to_str().map(str::to_string)
        })
        .filter(|name| !name.is_empty())
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names.iter().map(|n| intern(n)).collect()
}

/// Saves `state` as `name` (cleaned), replacing a preset of that name -
/// or of the same name in other letter case, which would be the same file
/// on macOS and Windows. Returns the name it was saved under.
pub fn save_in(dir: &Path, name: &str, state: &SynthState) -> io::Result<&'static str> {
    let name = clean_name(name);
    if name.is_empty() || is_builtin(&name) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "that name can't be used"));
    }
    let name = list_in(dir).into_iter().find(|n| n.eq_ignore_ascii_case(&name)).unwrap_or_else(|| intern(&name));
    std::fs::create_dir_all(dir)?;
    let mut patch = state.clone();
    patch.held_notes.clear();
    let text = serde_json::to_string_pretty(&patch).map_err(io::Error::other)?;
    let path = file(dir, name);
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, &path)?;
    Ok(name)
}

pub fn load_in(dir: &Path, name: &str) -> Option<SynthState> {
    let text = std::fs::read_to_string(file(dir, name)).ok()?;
    let mut state: SynthState = serde_json::from_str(&text).ok()?;
    state.name = intern(name);
    state.held_notes.clear();
    Some(state)
}

pub fn delete_in(dir: &Path, name: &str) {
    let _ = std::fs::remove_file(file(dir, name));
}

pub fn list() -> Vec<&'static str> {
    list_in(&dir())
}

pub fn save(name: &str, state: &SynthState) -> io::Result<&'static str> {
    save_in(&dir(), name, state)
}

pub fn load(name: &str) -> Option<SynthState> {
    load_in(&dir(), name)
}

pub fn delete(name: &str) {
    delete_in(&dir(), name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("shor-presets-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_saved_patch_comes_back_the_same_with_its_name() {
        let dir = scratch("roundtrip");
        let mut patch = shared::synth::seed_synth();
        patch.filter.cutoff_hz = 777.0;
        patch.held_notes = vec![60];
        let name = save_in(&dir, "  My  bass ", &patch).unwrap();
        assert_eq!(name, "My bass");
        assert_eq!(list_in(&dir), vec!["My bass"]);
        let back = load_in(&dir, "My bass").unwrap();
        assert_eq!(back.name, "My bass");
        assert_eq!(back.filter.cutoff_hz, 777.0);
        assert!(back.held_notes.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn saving_under_a_name_again_replaces_it_whatever_the_case() {
        let dir = scratch("replace");
        let mut patch = shared::synth::seed_synth();
        save_in(&dir, "Pluck", &patch).unwrap();
        patch.filter.cutoff_hz = 1234.0;
        assert_eq!(save_in(&dir, "pluck", &patch).unwrap(), "Pluck");
        assert_eq!(list_in(&dir), vec!["Pluck"]);
        assert_eq!(load_in(&dir, "Pluck").unwrap().filter.cutoff_hz, 1234.0);
        delete_in(&dir, "Pluck");
        assert!(list_in(&dir).is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn built_in_and_empty_names_are_refused() {
        let dir = scratch("refuse");
        let patch = shared::synth::seed_synth();
        assert!(save_in(&dir, "init", &patch).is_err());
        assert!(save_in(&dir, " /:* ", &patch).is_err());
        assert!(list_in(&dir).is_empty());
    }

    #[test]
    fn names_lose_what_a_file_name_cannot_hold() {
        assert_eq!(clean_name("a/b\\c:d*e?f\"g<h>i|j"), "abcdefghij");
        assert_eq!(clean_name(&"x".repeat(80)).len(), MAX_NAME);
    }
}
