//! Where Shor's files live.
//!
//! - **Assets** (samples, read-only, shipped with the app): an `assets`
//!   folder next to the program (Windows, Linux), or in the app bundle's
//!   `Resources` (macOS).
//! - **Your data** (settings, projects, My tracks): the user's own folder -
//!   `%APPDATA%\Shor` on Windows, `~/Library/Application Support/Shor` on
//!   macOS, `$XDG_DATA_HOME/shor` (`~/.local/share/shor`) on Linux.
//!
//! Run from the source tree (`cargo run`, or the Windows build under Wine
//! on the machine that built it), both are the repo itself, as they always
//! were - so a developer's projects and settings stay where they are.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The repo this was built from; only meaningful where it exists.
const SOURCE_TREE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/..");

/// Running from the source tree: its `assets` folder is there.
/// `SHOR_PACKAGED=1` says no, to try out an installed copy's folders on
/// the machine that built it.
fn in_source_tree() -> bool {
    static DEV: OnceLock<bool> = OnceLock::new();
    *DEV.get_or_init(|| {
        std::env::var_os("SHOR_PACKAGED").is_none() && Path::new(SOURCE_TREE).join("assets").is_dir()
    })
}

/// The samples and other files that ship with the app.
pub fn assets_dir() -> PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        if in_source_tree() {
            return PathBuf::from(SOURCE_TREE).join("assets");
        }
        let exe_dir = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf)).unwrap_or_default();
        // A macOS bundle keeps them in Contents/Resources, beside
        // Contents/MacOS where the program is.
        [exe_dir.join("assets"), exe_dir.join("../Resources/assets")]
            .into_iter()
            .find(|dir| dir.is_dir())
            .unwrap_or_else(|| exe_dir.join("assets"))
    })
    .clone()
}

/// The user's own files: settings, projects and My tracks. Created on
/// first use.
pub fn data_dir() -> PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    let dir = DIR
        .get_or_init(|| {
            if in_source_tree() {
                return PathBuf::from(SOURCE_TREE);
            }
            user_data_dir().unwrap_or_else(|| std::env::temp_dir().join("Shor"))
        })
        .clone();
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Where recorded takes go: your data folder's Recordings (an installed
/// app's own folder isn't for writing), or `assets` in the source tree,
/// where they always went.
pub fn recordings_dir() -> PathBuf {
    let dir = if in_source_tree() { assets_dir() } else { data_dir().join("Recordings") };
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// The file behind an audio clip's `source` name: a recorded take if
/// there's one by that name, else a sample that ships in `assets`.
pub fn audio_file(assets: &Path, source: &str) -> PathBuf {
    let recorded = recordings_dir().join(source);
    if recorded.exists() {
        recorded
    } else {
        assets.join(source)
    }
}

fn user_data_dir() -> Option<PathBuf> {
    let env = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    if cfg!(target_os = "windows") {
        env("APPDATA").map(|d| d.join("Shor"))
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join("Library/Application Support/Shor"))
    } else {
        env("XDG_DATA_HOME").or_else(|| env("HOME").map(|h| h.join(".local/share"))).map(|d| d.join("shor"))
    }
}
