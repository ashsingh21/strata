//! Project management: New/Open/Save/Save As/Rename, plus Ctrl+S. Owns
//! which file (if any) the current project lives at - everything else
//! (the arrangement, instrument patches) belongs to `TimelineState` and
//! `SynthModel`; this model only reads them to save, and on New/Open
//! replaces them wholesale via `TimelineEvent::LoadArrangement` and
//! `SynthEvent::LoadPatches` rather than owning them itself.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use vizia::prelude::*;

use shared::arrangement::{empty_arrangement, Arrangement, TrackId};
use shared::project::{load, save, Project};
use shared::synth::SynthState;

use crate::synth::state::SynthEvent;
use crate::timeline::state::TimelineEvent;

/// Where New/Open/Save As start browsing from, if the current project
/// doesn't already live somewhere else - a plain sibling of `assets/`,
/// created on first use.
pub fn default_projects_dir() -> PathBuf {
    let dir = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../projects"));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Runs `zenity --file-selection` as a plain child process for an Open
/// dialog. Deliberately not a Rust-native dialog crate (`rfd` was tried
/// first): every backend it offers either deadlocks Vizia's own event
/// loop when called inline, or - moved to a background thread to avoid
/// that - silently fails, because the underlying toolkit (GTK, or the
/// portal's own GTK-based implementation) expects to own its one true
/// thread and doesn't tolerate being reached from an ad-hoc spawned one.
/// A separate process sidesteps all of that: it's `zenity`'s main thread,
/// not this app's.
fn zenity_pick_file(dir: &Path) -> Option<PathBuf> {
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

/// The Save As counterpart - see `zenity_pick_file`.
fn zenity_save_file(dir: &Path, suggested_name: &str) -> Option<PathBuf> {
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

/// The file this app has always saved to before project management
/// existed - still the default a fresh checkout opens, so upgrading
/// doesn't lose anyone's place.
fn legacy_project_path() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../project.json"))
}

/// A path's display name: the file stem, capitalized ("my-song.json" ->
/// "My-song"). Falls back to "Untitled" for a not-yet-saved project.
fn name_from_path(path: Option<&Path>) -> String {
    let Some(stem) = path.and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()) else {
        return "Untitled".to_string();
    };
    let mut chars = stem.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => "Untitled".to_string(),
    }
}

pub struct ProjectModel {
    arrangement: Signal<Arrangement>,
    patches: Signal<BTreeMap<TrackId, SynthState>>,
    decode_request_tx: std::sync::mpsc::Sender<Arc<str>>,
    /// `None` for a New project that's never been saved anywhere yet -
    /// Ctrl+S then behaves like Save As instead of failing.
    pub current_path: Signal<Option<PathBuf>>,
    pub display_name: Signal<String>,
    /// The project as last saved (or loaded), serialized - the header
    /// compares the live project against it to show "Saved" or "Edited".
    pub saved: Signal<String>,
}

pub enum ProjectEvent {
    Save,
    SaveAsDialog,
    OpenDialog,
    New,
    /// A new file stem, typed into the header's title field - moves the
    /// project's file on disk if it's been saved before, otherwise just
    /// updates the name a future Save As will suggest.
    Rename(String),
    /// A dialog's own result, reported back from the background thread
    /// that ran it (see `spawn_dialog`) - `None` if the user cancelled.
    OpenPicked(Option<PathBuf>),
    SaveAsPicked(Option<PathBuf>),
}

/// Runs a (`zenity`) dialog on a background thread and reports whatever
/// it returns back as `event`, rather than calling it inline and blocking
/// Vizia's own event loop on a child process for however long the person
/// takes to pick a file.
fn spawn_dialog(
    cx: &mut EventContext,
    dialog: impl FnOnce() -> Option<PathBuf> + Send + 'static,
    event: impl FnOnce(Option<PathBuf>) -> ProjectEvent + Send + 'static,
) {
    cx.spawn(move |proxy| {
        let picked = dialog();
        let _ = proxy.emit(event(picked));
    });
}

/// The project as it would be saved: the arrangement plus the patch of
/// every track that still has an instrument (a deleted track's patch is
/// kept in memory for undo, but not written out).
pub fn project(arrangement: &Arrangement, patches: &BTreeMap<TrackId, SynthState>) -> Project {
    let instruments = arrangement
        .tracks
        .iter()
        .filter(|t| t.instrument.is_some())
        .filter_map(|t| {
            let mut patch = patches.get(&t.id)?.clone();
            // Held keys are play state, not an edit.
            patch.held_notes.clear();
            Some((t.id, patch))
        })
        .collect();
    Project { arrangement: arrangement.clone(), instruments, synth: None }
}

/// The project's saved form, for comparing against the last save.
pub fn snapshot(arrangement: &Arrangement, patches: &BTreeMap<TrackId, SynthState>) -> String {
    serde_json::to_string(&project(arrangement, patches)).unwrap_or_default()
}

impl ProjectModel {
    pub fn new(
        arrangement: Signal<Arrangement>,
        patches: Signal<BTreeMap<TrackId, SynthState>>,
        decode_request_tx: std::sync::mpsc::Sender<Arc<str>>,
        initial_path: Option<PathBuf>,
    ) -> Self {
        let saved = Signal::new(snapshot(&arrangement.get(), &patches.get()));
        let display_name = Signal::new(name_from_path(initial_path.as_deref()));
        Self {
            arrangement,
            patches,
            decode_request_tx,
            current_path: Signal::new(initial_path),
            display_name,
            saved,
        }
    }

    /// Loads `path`, replacing the live arrangement/patches, and kicks off
    /// the same waveform/decode work `main.rs` does for the project it
    /// opens at startup - a runtime Open is otherwise indistinguishable
    /// from a fresh launch pointed at a different file.
    fn open(&mut self, cx: &mut EventContext, path: PathBuf) {
        match load(&path) {
            Ok(project) => {
                cx.emit(TimelineEvent::LoadArrangement(project.arrangement.clone()));
                cx.emit(SynthEvent::LoadPatches(project.instruments.into_iter().collect()));
                let assets_dir = crate::timeline::assets_dir();
                for source in crate::timeline::peaks_loader::audio_sources(&project.arrangement) {
                    crate::timeline::peaks_loader::spawn_peak_loader_for_source(cx, &assets_dir, source.clone());
                    let _ = self.decode_request_tx.send(source);
                }
                self.current_path.set(Some(path.clone()));
                self.display_name.set(name_from_path(Some(&path)));
                self.saved.set(snapshot(&project.arrangement, &self.patches.get()));
            }
            Err(e) => eprintln!("project: failed to load {}: {e}", path.display()),
        }
    }

    fn save_to(&mut self, path: PathBuf) {
        let arrangement = self.arrangement.get();
        let patches = self.patches.get();
        match save(&project(&arrangement, &patches), &path) {
            Ok(()) => {
                self.current_path.set(Some(path.clone()));
                self.display_name.set(name_from_path(Some(&path)));
                self.saved.set(snapshot(&arrangement, &patches));
            }
            Err(e) => eprintln!("project: failed to save to {}: {e}", path.display()),
        }
    }
}

impl Model for ProjectModel {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|event, _| match event {
            ProjectEvent::Save => match self.current_path.get() {
                Some(path) => self.save_to(path),
                None => spawn_save_as_dialog(cx, self.display_name.get()),
            },
            ProjectEvent::SaveAsDialog => spawn_save_as_dialog(cx, self.display_name.get()),
            ProjectEvent::OpenDialog => {
                spawn_dialog(cx, || zenity_pick_file(&default_projects_dir()), ProjectEvent::OpenPicked)
            }
            ProjectEvent::OpenPicked(Some(path)) => self.open(cx, path.clone()),
            ProjectEvent::OpenPicked(None) => {}
            ProjectEvent::SaveAsPicked(Some(path)) => self.save_to(path.clone()),
            ProjectEvent::SaveAsPicked(None) => {}
            ProjectEvent::New => {
                cx.emit(TimelineEvent::LoadArrangement(empty_arrangement()));
                cx.emit(SynthEvent::LoadPatches(BTreeMap::new()));
                self.current_path.set(None);
                self.display_name.set(name_from_path(None));
                self.saved.set(snapshot(&empty_arrangement(), &BTreeMap::new()));
            }
            ProjectEvent::Rename(name) => {
                let name = name.trim();
                if name.is_empty() {
                    return;
                }
                match self.current_path.get() {
                    Some(old_path) => {
                        let new_path = old_path.with_file_name(format!("{name}.json"));
                        if new_path != old_path {
                            if let Err(e) = std::fs::rename(&old_path, &new_path) {
                                eprintln!("project: failed to rename {} to {}: {e}", old_path.display(), new_path.display());
                                return;
                            }
                        }
                        self.current_path.set(Some(new_path.clone()));
                        self.display_name.set(name_from_path(Some(&new_path)));
                    }
                    // Never saved yet: nothing on disk to rename, just
                    // change what a future Save As will suggest.
                    None => self.display_name.set(name.to_string()),
                }
            }
        });
    }
}

/// Shared by `Save` (when there's no current path yet) and `SaveAsDialog`.
fn spawn_save_as_dialog(cx: &mut EventContext, suggested: String) {
    spawn_dialog(cx, move || zenity_save_file(&default_projects_dir(), &suggested), ProjectEvent::SaveAsPicked);
}

/// The project this app should open at startup: the last-saved project's
/// own file if we somehow knew it (we don't persist that yet - a real
/// "recent projects" list is future work), otherwise the pre-project-
/// management default, if it exists, so upgrading doesn't lose anyone's
/// work; otherwise `None`, a fresh Untitled project.
pub fn startup_path() -> Option<PathBuf> {
    let legacy = legacy_project_path();
    legacy.exists().then_some(legacy)
}
