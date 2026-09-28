//! What the browser lists: everything the old sidebar held - instruments,
//! effects, Carve presets, the drum samples and loops, beat templates, the
//! demo songs, My tracks, the lessons - plus the audio the open project
//! uses, as one kind of row with a stable id (for favourites, collections
//! and history), a type, and the metadata a row shows.

use std::path::PathBuf;
use std::sync::Arc;

use shared::arrangement::Instrument;

use super::icon::IconKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectKind {
    Compressor,
    Eq,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Instrument(Instrument),
    Effect(EffectKind),
    /// An index into `shared::synth::PRESETS`.
    Preset(usize),
    /// A `.wav`, relative to the assets folder (`drums/kick.wav`).
    Sample(Arc<str>),
    /// An index into `timeline::beat_templates::TEMPLATES`.
    Pattern(usize),
    Song(shared::demo::DemoSong),
    /// One of My tracks.
    Track(PathBuf),
    /// An index into `lessons::course::LESSONS`.
    Lesson(usize),
    /// Audio the open project plays (a recording, an imported file).
    ProjectAudio(Arc<str>),
}

/// The type chips: they filter, they don't navigate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chip {
    All,
    Instruments,
    Effects,
    Samples,
    Presets,
}

impl Chip {
    pub const ALL: [Chip; 5] = [Chip::All, Chip::Instruments, Chip::Effects, Chip::Samples, Chip::Presets];

    pub fn label(self) -> &'static str {
        match self {
            Chip::All => "All",
            Chip::Instruments => "Instruments",
            Chip::Effects => "Effects",
            Chip::Samples => "Samples",
            Chip::Presets => "Presets",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// Stable across launches: what favourites and collections store.
    pub id: String,
    pub kind: Kind,
    pub name: String,
    /// The row's right-hand detail: BPM for a sample, the device for a
    /// preset, the category for an effect.
    pub meta: String,
    /// A sample's tempo, read from its name ("..._120_bpm").
    pub bpm: Option<f32>,
    /// Where it came from, for search ("Piano loops", "Recipes"...).
    pub group: String,
}

impl Item {
    pub fn icon(&self) -> IconKind {
        match self.kind {
            Kind::Instrument(_) => IconKind::Instrument,
            Kind::Effect(_) => IconKind::Effect,
            Kind::Preset(_) => IconKind::Preset,
            Kind::Sample(_) | Kind::ProjectAudio(_) => IconKind::Sample,
            Kind::Pattern(_) => IconKind::Pattern,
            Kind::Song(_) => IconKind::Song,
            Kind::Track(_) => IconKind::Track,
            Kind::Lesson(_) => IconKind::Lesson,
        }
    }

    pub fn chip(&self) -> Chip {
        match self.kind {
            Kind::Instrument(_) => Chip::Instruments,
            Kind::Effect(_) => Chip::Effects,
            Kind::Preset(_) => Chip::Presets,
            _ => Chip::Samples,
        }
    }

    /// Whether it can be auditioned from the row.
    pub fn previewable(&self) -> bool {
        !matches!(self.kind, Kind::Song(_) | Kind::Track(_) | Kind::Lesson(_))
    }

    /// Whether it can be dragged somewhere (projects and lessons open
    /// instead).
    pub fn draggable(&self) -> bool {
        !matches!(self.kind, Kind::Song(_) | Kind::Track(_) | Kind::Lesson(_))
    }

    /// A sample's audio file, if it's one.
    pub fn source(&self) -> Option<&Arc<str>> {
        match &self.kind {
            Kind::Sample(s) | Kind::ProjectAudio(s) => Some(s),
            _ => None,
        }
    }

    pub fn matches(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let q = query.to_lowercase();
        [&self.name, &self.meta, &self.group].iter().any(|s| s.to_lowercase().contains(&q))
    }
}

/// A tempo in a file name: "loop_120_bpm", "fairy_130bpm".
pub fn bpm_in_name(name: &str) -> Option<f32> {
    let lower = name.to_lowercase();
    let at = lower.find("bpm")?;
    let digits: String = lower[..at].trim_end_matches('_').chars().rev().take_while(|c| c.is_ascii_digit()).collect();
    let bpm: f32 = digits.chars().rev().collect::<String>().parse().ok()?;
    (40.0..=300.0).contains(&bpm).then_some(bpm)
}

/// The fixed part of the catalogue: built once (it reads the samples
/// folder).
pub fn library() -> Vec<Item> {
    let mut items = Vec::new();
    for instrument in [Instrument::Carve, Instrument::Drums] {
        let (id, meta) = match instrument {
            Instrument::Carve => ("inst:carve", "Synth"),
            Instrument::Drums => ("inst:drums", "Drums"),
        };
        items.push(Item {
            id: id.into(),
            kind: Kind::Instrument(instrument),
            name: instrument.name().into(),
            meta: meta.into(),
            bpm: None,
            group: "Instruments".into(),
        });
    }
    for (kind, id, name, meta) in [(EffectKind::Compressor, "fx:compressor", "Compressor", "Dynamics"), (EffectKind::Eq, "fx:eq", "EQ", "Tone")] {
        items.push(Item { id: id.into(), kind: Kind::Effect(kind), name: name.into(), meta: meta.into(), bpm: None, group: "Effects".into() });
    }
    for (i, (name, _)) in shared::synth::PRESETS.iter().enumerate() {
        items.push(Item {
            id: format!("preset:{name}"),
            kind: Kind::Preset(i),
            name: (*name).into(),
            meta: "Carve".into(),
            bpm: None,
            group: "Presets".into(),
        });
    }
    for category in crate::timeline::drum_sample_categories() {
        for file in &category.files {
            let bpm = bpm_in_name(file);
            items.push(Item {
                id: format!("sample:drums/{file}"),
                kind: Kind::Sample(format!("drums/{file}").into()),
                name: crate::timeline::sample_display_name(&category, file),
                meta: bpm.map(|b| format!("{b:.0}")).unwrap_or_else(|| "\u{2013}".into()),
                bpm,
                group: category.label.into(),
            });
        }
    }
    for (i, t) in crate::timeline::beat_templates::TEMPLATES.iter().enumerate() {
        items.push(Item {
            id: format!("pattern:{}", t.name),
            kind: Kind::Pattern(i),
            name: t.name.into(),
            meta: format!("{} bars", t.bars),
            bpm: None,
            group: "Beat templates".into(),
        });
    }
    for song in shared::demo::DemoSong::ALL {
        items.push(Item {
            id: format!("song:{}", song.name()),
            kind: Kind::Song(song),
            name: song.label().into(),
            meta: "Demo".into(),
            bpm: None,
            group: "Songs".into(),
        });
    }
    items
}

/// My tracks, as rows.
pub fn my_tracks(paths: &[PathBuf]) -> Vec<Item> {
    paths
        .iter()
        .map(|path| Item {
            id: format!("track:{}", path.display()),
            kind: Kind::Track(path.clone()),
            name: path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
            meta: "Mine".into(),
            bpm: None,
            group: "My tracks".into(),
        })
        .collect()
}

/// The audio the open project plays that isn't a library sample.
pub fn project_audio(sources: &[Arc<str>]) -> Vec<Item> {
    sources
        .iter()
        .filter(|s| !s.starts_with("drums/"))
        .map(|s| Item {
            id: format!("audio:{s}"),
            kind: Kind::ProjectAudio(s.clone()),
            name: std::path::Path::new(&**s).file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            meta: "In project".into(),
            bpm: bpm_in_name(s),
            group: "Project audio".into(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tempos_come_out_of_file_names() {
        assert_eq!(bpm_in_name("piano_octave_long_loop_120_bpm.wav"), Some(120.0));
        assert_eq!(bpm_in_name("loop_fairy_130bpm.wav"), Some(130.0));
        assert_eq!(bpm_in_name("kick.wav"), None);
    }

    #[test]
    fn the_library_keeps_everything_the_old_sidebar_had() {
        let lib = library();
        let has = |k: fn(&Kind) -> bool| lib.iter().any(|i| k(&i.kind));
        assert!(has(|k| matches!(k, Kind::Instrument(Instrument::Carve))));
        assert!(has(|k| matches!(k, Kind::Instrument(Instrument::Drums))));
        assert!(has(|k| matches!(k, Kind::Effect(EffectKind::Compressor))));
        assert!(has(|k| matches!(k, Kind::Effect(EffectKind::Eq))));
        assert!(has(|k| matches!(k, Kind::Sample(_))));
        assert!(has(|k| matches!(k, Kind::Pattern(_))));
        assert!(has(|k| matches!(k, Kind::Song(_))));
        assert_eq!(lib.iter().filter(|i| matches!(i.kind, Kind::Preset(_))).count(), shared::synth::PRESETS.len());
        // Ids are unique: favourites can't point at two things.
        let mut ids: Vec<&str> = lib.iter().map(|i| i.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), lib.len());
    }
}
