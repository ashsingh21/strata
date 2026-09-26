//! Ctrl+S support: a tiny model that just holds read access to the two
//! signals a save needs (the arrangement and Carve's patch) and writes
//! them out via `shared::project` on `ProjectEvent::Save`. Kept separate
//! from `TimelineState`/`SynthModel` rather than bolted onto either -
//! saving isn't really either one's job, and `KeymapEntry` needs a plain
//! non-capturing `fn`, so the save trigger has to land on *some* Model's
//! own event handler rather than a closure that captures both signals.

use std::path::PathBuf;

use vizia::prelude::*;

use shared::arrangement::Arrangement;
use shared::project::{save, Project};
use shared::synth::SynthState;

pub fn project_path() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../project.json"))
}

pub struct ProjectModel {
    arrangement: Signal<Arrangement>,
    synth: Signal<SynthState>,
    /// The project as last saved (or loaded), serialized - the header
    /// compares the live project against it to show "Saved" or "Edited".
    pub saved: Signal<String>,
}

/// The project's saved form, for comparing against the last save. Held
/// keys are play state, not an edit, so they're left out.
pub fn snapshot(arrangement: &Arrangement, synth: &SynthState) -> String {
    let mut synth = synth.clone();
    synth.held_notes.clear();
    serde_json::to_string(&Project { arrangement: arrangement.clone(), synth }).unwrap_or_default()
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
    pub fn new(arrangement: Signal<Arrangement>, synth: Signal<SynthState>) -> Self {
        let saved = Signal::new(snapshot(&arrangement.get(), &synth.get()));
        Self { arrangement, synth, saved }
    }
}

impl Model for ProjectModel {
    fn event(&mut self, _cx: &mut EventContext, event: &mut Event) {
        event.map(|event, _| match event {
            ProjectEvent::Save => {
                let project = Project { arrangement: self.arrangement.get(), synth: self.synth.get() };
                let path = project_path();
                match save(&project, &path) {
                    Ok(()) => {
                        eprintln!("project: saved to {}", path.display());
                        self.saved.set(snapshot(&project.arrangement, &project.synth));
                    }
                    Err(e) => eprintln!("project: failed to save to {}: {e}", path.display()),
                }
            }
        });
    }
}
