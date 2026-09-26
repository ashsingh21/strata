//! Ctrl+S support: a tiny model that just holds read access to the two
//! signals a save needs (the arrangement and Carve's patch) and writes
//! them out via `shared::project` on `ProjectEvent::Save`. Kept separate
//! from `TimelineState`/`SynthModel` rather than bolted onto either -
//! saving isn't really either one's job, and `KeymapEntry` needs a plain
//! non-capturing `fn`, so the save trigger has to land on *some* Model's
//! own event handler rather than a closure that captures both signals.

use std::collections::BTreeMap;
use std::path::PathBuf;

use vizia::prelude::*;

use shared::arrangement::{Arrangement, TrackId};
use shared::project::{save, Project};
use shared::synth::SynthState;

pub fn project_path() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../project.json"))
}

pub struct ProjectModel {
    arrangement: Signal<Arrangement>,
    patches: Signal<BTreeMap<TrackId, SynthState>>,
    /// The project as last saved (or loaded), serialized - the header
    /// compares the live project against it to show "Saved" or "Edited".
    pub saved: Signal<String>,
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

/// The project's display name, from its file name ("project.json" ->
/// "Project").
pub fn project_name() -> String {
    let stem = project_path().file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let mut chars = stem.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => "Untitled".to_string(),
    }
}

pub enum ProjectEvent {
    Save,
}

impl ProjectModel {
    pub fn new(arrangement: Signal<Arrangement>, patches: Signal<BTreeMap<TrackId, SynthState>>) -> Self {
        let saved = Signal::new(snapshot(&arrangement.get(), &patches.get()));
        Self { arrangement, patches, saved }
    }
}

impl Model for ProjectModel {
    fn event(&mut self, _cx: &mut EventContext, event: &mut Event) {
        event.map(|event, _| match event {
            ProjectEvent::Save => {
                let arrangement = self.arrangement.get();
                let patches = self.patches.get();
                let path = project_path();
                match save(&project(&arrangement, &patches), &path) {
                    Ok(()) => {
                        eprintln!("project: saved to {}", path.display());
                        self.saved.set(snapshot(&arrangement, &patches));
                    }
                    Err(e) => eprintln!("project: failed to save to {}: {e}", path.display()),
                }
            }
        });
    }
}
