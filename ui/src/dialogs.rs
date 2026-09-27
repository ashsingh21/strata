//! The app's dialogs - open, save, export, "save changes?", fatal error -
//! one implementation per platform behind the same functions. Every one
//! blocks, so callers run them off the UI thread (see
//! `project::spawn_dialog`), except `error`, used before the window exists.
//!
//! Linux uses `zenity` child processes (see `linux::pick_project` for why
//! not a native-dialog crate there); macOS and Windows use `rfd`, which
//! shows the system's own panels and is fine off the main thread in a
//! windowed app.

#[cfg(target_os = "linux")]
pub use linux::*;
#[cfg(not(target_os = "linux"))]
pub use native::*;

#[cfg(target_os = "linux")]
mod linux {
    use std::path::{Path, PathBuf};

    use crate::project::DiscardChoice;

    /// Runs `zenity --file-selection` as a plain child process for an Open
    /// dialog. Deliberately not a Rust-native dialog crate (`rfd` was tried
    /// first): every backend it offers either deadlocks Vizia's own event
    /// loop when called inline, or - moved to a background thread to avoid
    /// that - silently fails, because the underlying toolkit (GTK, or the
    /// portal's own GTK-based implementation) expects to own its one true
    /// thread and doesn't tolerate being reached from an ad-hoc spawned one.
    /// A separate process sidesteps all of that: it's `zenity`'s main thread,
    /// not this app's.
    pub fn pick_project(dir: &Path) -> Option<PathBuf> {
        let output = std::process::Command::new("zenity")
            .arg("--file-selection")
            .arg("--title=Open Project")
            .arg(format!("--filename={}/", dir.display()))
            .arg("--file-filter=*.json")
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (!path.is_empty()).then(|| PathBuf::from(path))
    }

    /// "Save changes to X?" - Save / Don't Save / Cancel. Dismissing the
    /// dialog (Esc, the window's X) counts as Cancel, the only safe default:
    /// "Don't Save" is deliberately the extra button, not zenity's cancel
    /// action, so nothing but an explicit click on it discards work. If
    /// zenity can't run at all this falls back to DontSave - i.e. the old
    /// behaviour - rather than making the window impossible to close.
    pub fn ask_save(name: &str, action: &str) -> DiscardChoice {
        let output = std::process::Command::new("zenity")
            .arg("--question")
            .arg("--title=Unsaved changes")
            .arg(format!("--text=Save changes to \u{201c}{name}\u{201d} before {action}?"))
            .arg("--ok-label=Save")
            .arg("--cancel-label=Cancel")
            .arg("--extra-button=Don't Save")
            .output();
        match output {
            Ok(out) if out.status.success() => DiscardChoice::Save,
            Ok(out) if String::from_utf8_lossy(&out.stdout).trim() == "Don't Save" => DiscardChoice::DontSave,
            Ok(_) => DiscardChoice::Cancel,
            Err(e) => {
                eprintln!("project: couldn't show the unsaved-changes dialog ({e}); continuing without saving");
                DiscardChoice::DontSave
            }
        }
    }

    /// The Save As counterpart - see `pick_project`.
    pub fn save_project(dir: &Path, suggested_name: &str) -> Option<PathBuf> {
        let output = std::process::Command::new("zenity")
            .arg("--file-selection")
            .arg("--save")
            .arg("--confirm-overwrite")
            .arg("--title=Save Project As")
            .arg(format!("--filename={}/{suggested_name}.json", dir.display()))
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            return None;
        }
        let path = if path.ends_with(".json") { path } else { format!("{path}.json") };
        Some(PathBuf::from(path))
    }

    /// The Export Audio dialog: a .wav path, `.wav` added if left off.
    pub fn export_wav(dir: &Path, suggested_name: &str) -> Option<PathBuf> {
        let output = std::process::Command::new("zenity")
            .arg("--file-selection")
            .arg("--save")
            .arg("--confirm-overwrite")
            .arg("--title=Export Audio")
            // Only .wav files listed: otherwise the dialog pre-selects the
            // first file in the folder (a project .json) over the suggested name.
            .arg("--file-filter=WAV audio | *.wav")
            .arg(format!("--filename={}/{suggested_name}.wav", dir.display()))
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            return None;
        }
        // OK with the name left empty hands back the folder: use the project's name.
        let path = PathBuf::from(path);
        if path.is_dir() {
            return Some(path.join(format!("{suggested_name}.wav")));
        }
        let path = path.to_string_lossy().into_owned();
        let path = if path.to_lowercase().ends_with(".wav") { path } else { format!("{path}.wav") };
        Some(PathBuf::from(path))
    }

    /// A fatal error, before the window exists.
    pub fn error(message: &str) {
        let _ = std::process::Command::new("zenity")
            .arg("--error")
            .arg("--title=Strata")
            .arg(format!("--text={message}"))
            .status();
    }
}

#[cfg(not(target_os = "linux"))]
mod native {
    use std::path::{Path, PathBuf};

    use rfd::{FileDialog, MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};

    use crate::project::DiscardChoice;

    pub fn pick_project(dir: &Path) -> Option<PathBuf> {
        FileDialog::new().set_title("Open Project").set_directory(dir).add_filter("Strata project", &["json"]).pick_file()
    }

    /// "Save changes to X?" - Save / Don't Save / Cancel; closing the
    /// dialog counts as Cancel, the only safe default.
    pub fn ask_save(name: &str, action: &str) -> DiscardChoice {
        let result = MessageDialog::new()
            .set_level(MessageLevel::Warning)
            .set_title("Unsaved changes")
            .set_description(format!("Save changes to \u{201c}{name}\u{201d} before {action}?"))
            .set_buttons(MessageButtons::YesNoCancelCustom("Save".into(), "Don't Save".into(), "Cancel".into()))
            .show();
        match result {
            MessageDialogResult::Yes => DiscardChoice::Save,
            MessageDialogResult::No => DiscardChoice::DontSave,
            MessageDialogResult::Custom(label) if label == "Save" => DiscardChoice::Save,
            MessageDialogResult::Custom(label) if label == "Don't Save" => DiscardChoice::DontSave,
            _ => DiscardChoice::Cancel,
        }
    }

    pub fn save_project(dir: &Path, suggested_name: &str) -> Option<PathBuf> {
        let path = FileDialog::new()
            .set_title("Save Project As")
            .set_directory(dir)
            .set_file_name(format!("{suggested_name}.json"))
            .add_filter("Strata project", &["json"])
            .save_file()?;
        Some(if path.extension().is_some_and(|e| e == "json") { path } else { path.with_extension("json") })
    }

    pub fn export_wav(dir: &Path, suggested_name: &str) -> Option<PathBuf> {
        let path = FileDialog::new()
            .set_title("Export Audio")
            .set_directory(dir)
            .set_file_name(format!("{suggested_name}.wav"))
            .add_filter("WAV audio", &["wav"])
            .save_file()?;
        Some(if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("wav")) { path } else { path.with_extension("wav") })
    }

    /// A fatal error, before the window exists.
    pub fn error(message: &str) {
        MessageDialog::new().set_level(MessageLevel::Error).set_title("Strata").set_description(message).set_buttons(MessageButtons::Ok).show();
    }
}
