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
}

pub enum ProjectEvent {
    Save,
}

impl ProjectModel {
    pub fn new(arrangement: Signal<Arrangement>, synth: Signal<SynthState>) -> Self {
        Self { arrangement, synth }
    }
}

impl Model for ProjectModel {
    fn event(&mut self, _cx: &mut EventContext, event: &mut Event) {
        event.map(|event, _| match event {
            ProjectEvent::Save => {
                let project = Project { arrangement: self.arrangement.get(), synth: self.synth.get() };
                let path = project_path();
                match save(&project, &path) {
                    Ok(()) => eprintln!("project: saved to {}", path.display()),
                    Err(e) => eprintln!("project: failed to save to {}: {e}", path.display()),
                }
            }
        });
    }
}
