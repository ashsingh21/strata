//! Whole-project save/load: one JSON file holding the arrangement and
//! each track's Carve patch. Nothing else - theme, window size, which tool/mode is
//! selected, Interval Input's key/scale, the recorder's input gain - is
//! considered project content; those reset to their defaults on restart
//! the same way they always have.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::arrangement::{Arrangement, TrackId};
use crate::synth::SynthState;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub arrangement: Arrangement,
    /// Each instrument track's Carve patch.
    #[serde(default)]
    pub instruments: Vec<(TrackId, SynthState)>,
    /// The single shared patch projects saved before per-track instruments
    /// had. Read only: `migrate` hands it to every MIDI track.
    #[serde(default, skip_serializing)]
    pub synth: Option<SynthState>,
}

impl Project {
    /// Brings an older project up to date: MIDI tracks saved before tracks
    /// had instruments get Carve, playing the old shared patch; tracks
    /// saved before per-effect enable bits existed get their old bare
    /// `Effect`s converted into `EffectSlot`s (enabled by default, so a
    /// project that already had a Compressor sounds the same on reload).
    pub fn migrate(&mut self) {
        let legacy = self.synth.take();
        for track in &mut self.arrangement.tracks {
            if track.kind == crate::arrangement::TrackKind::Midi && track.instrument.is_none() && legacy.is_some() {
                track.instrument = Some(crate::arrangement::Instrument::Carve);
            }
            let has_patch = self.instruments.iter().any(|(id, _)| *id == track.id);
            if track.instrument.is_some() && !has_patch {
                let patch = legacy.clone().unwrap_or_else(crate::synth::seed_synth);
                self.instruments.push((track.id, patch));
            }
            if track.effect_slots.is_empty() && !track.effects.is_empty() {
                track.effect_slots = track.effects.drain(..).map(crate::arrangement::EffectSlot::new).collect();
            }
            if track.fx.ordered().is_empty() && !track.effect_slots.is_empty() {
                track.fx = crate::arrangement::EffectGraph::from_flat(track.effect_slots.drain(..).collect());
            }
        }
    }
}

#[derive(Debug)]
pub enum ProjectError {
    Io(std::io::Error),
    Json(serde_json::Error),
}

impl std::fmt::Display for ProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Json(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ProjectError {}

impl From<std::io::Error> for ProjectError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for ProjectError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

pub fn save(project: &Project, path: &Path) -> Result<(), ProjectError> {
    let json = serde_json::to_string_pretty(project)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub fn load(path: &Path) -> Result<Project, ProjectError> {
    let json = std::fs::read_to_string(path)?;
    let mut project: Project = serde_json::from_str(&json)?;
    project.migrate();
    Ok(project)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrangement::{seed_arrangement, ClipContent};
    use crate::synth::seed_synth;

    #[test]
    fn round_trips_a_project_through_json() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("strata-project-test-{}.json", std::process::id()));

        let arrangement = seed_arrangement();
        let bass = arrangement.tracks.iter().find(|t| t.name == "Bass").unwrap().id;
        let mut patch = seed_synth();
        patch.filter.cutoff_hz = 333.0;
        let original = Project { arrangement, instruments: vec![(bass, patch)], synth: None };
        save(&original, &path).unwrap();
        let loaded = load(&path).unwrap();
        std::fs::remove_file(&path).ok();

        assert_eq!(loaded.arrangement.tracks.len(), original.arrangement.tracks.len());
        assert_eq!(loaded.arrangement.clips.len(), original.arrangement.clips.len());
        let bass_patch = &loaded.instruments.iter().find(|(id, _)| *id == bass).unwrap().1;
        assert_eq!(bass_patch.filter.cutoff_hz, 333.0);
        // Every instrument track ends up with a patch, even if none was saved.
        for track in loaded.arrangement.tracks.iter().filter(|t| t.instrument.is_some()) {
            assert!(loaded.instruments.iter().any(|(id, _)| *id == track.id), "{}", track.name);
        }

        // Peaks are never saved - they're rebuilt from the WAV file after
        // load, same as the first time a source is ever referenced.
        for clip in &loaded.arrangement.clips {
            if let ClipContent::Audio { peaks, .. } = &clip.content {
                assert!(peaks.is_none());
            }
        }
    }

    #[test]
    fn migrates_a_single_patch_project_to_per_track_instruments() {
        let mut arrangement = seed_arrangement();
        for track in &mut arrangement.tracks {
            track.instrument = None;
        }
        let mut old_patch = seed_synth();
        old_patch.filter.cutoff_hz = 444.0;
        let mut project = Project { arrangement, instruments: vec![], synth: Some(old_patch) };
        project.migrate();
        for track in &project.arrangement.tracks {
            let is_midi = track.kind == crate::arrangement::TrackKind::Midi;
            assert_eq!(track.instrument.is_some(), is_midi, "{}", track.name);
        }
        assert!(project.instruments.iter().all(|(_, p)| p.filter.cutoff_hz == 444.0));
        assert_eq!(project.instruments.len(), 2);
    }

    #[test]
    fn migrates_old_shape_effects_all_the_way_to_the_graph() {
        use crate::arrangement::{CompressorState, Effect};

        let mut arrangement = seed_arrangement();
        let track = &mut arrangement.tracks[0];
        track.effects = vec![Effect::Compressor(CompressorState { threshold_db: -12.0, ..CompressorState::default() })];
        track.effect_slots = vec![];
        track.fx = crate::arrangement::EffectGraph::new();
        let mut project = Project { arrangement, instruments: vec![], synth: None };

        project.migrate();

        let track = &project.arrangement.tracks[0];
        assert!(track.effects.is_empty(), "old-shape data should be drained, not left duplicated");
        assert!(track.effect_slots.is_empty(), "intermediate shape should also be drained, not left duplicated");
        let ordered = track.fx.ordered();
        assert_eq!(ordered.len(), 1);
        assert!(ordered[0].enabled, "an effect that was already on should stay on after migrating");
        match ordered[0].effect {
            Effect::Compressor(c) => assert_eq!(c.threshold_db, -12.0),
        }
    }
}
