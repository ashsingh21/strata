//! The left sidebar (design/sidebar/README.md): a 44px icon rail plus a
//! resizable panel - section title and count, search, type chips, "Fits
//! key", collections, the results, and a docked preview player. It
//! replaces the old plain browser list; everything that list held is a
//! result here (see `items`).
//!
//! Using an item: double-click it (or press its row's preview button to
//! audition it first), or drag it onto a track, a clip's lane, the
//! device panel or a collection. Lessons start on a single click.

pub mod icon;
pub mod items;
pub mod keys;
pub mod preview;
pub mod view;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use vizia::prelude::*;

use shared::arrangement::{Arrangement, ClipColor, TrackId};

use items::{Chip, Item, Kind};

/// The panel's width range and default (logical px), and the rail's.
pub const MIN_WIDTH: f32 = 200.0;
pub const MAX_WIDTH: f32 = 360.0;
pub const DEFAULT_WIDTH: f32 = 236.0;
pub const RAIL_WIDTH: f32 = 44.0;
/// How many recently used items History keeps.
const HISTORY_LEN: usize = 40;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Browse,
    Samples,
    Presets,
    Files,
    History,
    Learn,
}

impl Section {
    /// The rail's top group, in order.
    pub const RAIL: [Section; 6] = [Section::Browse, Section::Samples, Section::Presets, Section::Files, Section::History, Section::Learn];

    pub fn title(self) -> &'static str {
        match self {
            Section::Browse => "Browse",
            Section::Samples => "Samples",
            Section::Presets => "Presets",
            Section::Files => "Project files",
            Section::History => "History",
            Section::Learn => "Learn",
        }
    }

    pub fn icon(self) -> icon::IconKind {
        match self {
            Section::Browse => icon::IconKind::Browse,
            Section::Samples => icon::IconKind::Samples,
            Section::Presets => icon::IconKind::Presets,
            Section::Files => icon::IconKind::Files,
            Section::History => icon::IconKind::History,
            Section::Learn => icon::IconKind::Learn,
        }
    }

    fn key(self) -> &'static str {
        match self {
            Section::Browse => "browse",
            Section::Samples => "samples",
            Section::Presets => "presets",
            Section::Files => "files",
            Section::History => "history",
            Section::Learn => "learn",
        }
    }

    fn from_key(key: &str) -> Option<Section> {
        Section::RAIL.into_iter().find(|s| s.key() == key)
    }

    /// Whether the filters (chips, Fits key, collections) apply here.
    pub fn filters(self) -> bool {
        matches!(self, Section::Browse | Section::Samples | Section::Presets)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sort {
    Recent,
    Name,
    Type,
}

impl Sort {
    pub const ALL: [Sort; 3] = [Sort::Recent, Sort::Name, Sort::Type];

    pub fn label(self) -> &'static str {
        match self {
            Sort::Recent => "Recent",
            Sort::Name => "Name",
            Sort::Type => "Type",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Collection {
    pub name: String,
    pub color: ClipColor,
    pub items: Vec<String>,
}

/// Which collection the results are narrowed to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Coll {
    Favourites,
    User(usize),
}

/// Everything that decides the results, gathered for `results`.
pub struct Filter<'a> {
    pub section: Section,
    pub query: &'a str,
    pub chip: Chip,
    pub fits_key: bool,
    pub sort: Sort,
    pub collection: Option<Coll>,
    pub favourites: &'a [String],
    pub collections: &'a [Collection],
    pub history: &'a [String],
    /// Pitched samples' keys (samples not in it are unpitched or not yet
    /// analysed, and always pass "Fits key").
    pub keys: &'a HashMap<Arc<str>, keys::SampleKey>,
    /// The project's key (0 = C) and scale (a 12-bit mask from the key).
    pub project_key: (u8, u16),
}

/// The rows to show, in order.
pub fn results(all: &[Item], f: &Filter) -> Vec<Item> {
    let mut out: Vec<Item> = match f.section {
        Section::History => f.history.iter().filter_map(|id| all.iter().find(|i| &i.id == id).cloned()).collect(),
        Section::Browse => all.iter().filter(|i| matches!(i.kind, Kind::Instrument(_) | Kind::Effect(_) | Kind::Preset(_) | Kind::Sample(_) | Kind::Pattern(_))).cloned().collect(),
        Section::Samples => all.iter().filter(|i| matches!(i.kind, Kind::Sample(_) | Kind::Pattern(_))).cloned().collect(),
        Section::Presets => all.iter().filter(|i| matches!(i.kind, Kind::Preset(_))).cloned().collect(),
        Section::Files => all.iter().filter(|i| matches!(i.kind, Kind::ProjectAudio(_) | Kind::Track(_) | Kind::Song(_))).cloned().collect(),
        Section::Learn => all.iter().filter(|i| matches!(i.kind, Kind::Lesson(_))).cloned().collect(),
    };
    if f.section.filters() {
        if let Some(coll) = f.collection {
            let members: &[String] = match coll {
                Coll::Favourites => f.favourites,
                Coll::User(i) => f.collections.get(i).map(|c| c.items.as_slice()).unwrap_or(&[]),
            };
            out.retain(|i| members.contains(&i.id));
        }
        if f.section == Section::Browse && f.chip != Chip::All {
            out.retain(|i| i.chip() == f.chip);
        }
        if f.fits_key {
            out.retain(|i| i.source().and_then(|s| f.keys.get(s)).is_none_or(|k| k.fits(f.project_key.0, f.project_key.1)));
        }
    }
    out.retain(|i| i.matches(f.query));
    // History is already newest first; Learn keeps the course's order.
    if !matches!(f.section, Section::History | Section::Learn) {
        let recency = |i: &Item| f.history.iter().position(|h| h == &i.id).unwrap_or(usize::MAX);
        match f.sort {
            Sort::Recent => out.sort_by(|a, b| recency(a).cmp(&recency(b)).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))),
            Sort::Name => out.sort_by_key(|i| i.name.to_lowercase()),
            Sort::Type => out.sort_by_key(|i| (i.chip() as u8, i.name.to_lowercase())),
        }
    }
    out
}

/// The project key as the "Fits key" toggle names it: "A min pent".
pub fn key_label(root: u8, mask: u16) -> String {
    let scale = shared::theory::SCALE_PRESETS.iter().find(|p| p.mask == mask).map(|p| p.name).unwrap_or("custom");
    let short = match scale {
        "Major" => "maj",
        "Natural minor" => "min",
        "Major pentatonic" => "maj pent",
        "Minor pentatonic" => "min pent",
        other => other,
    };
    format!("{} {}", keys::NOTE_NAMES[root as usize % 12], short.to_lowercase())
}

pub enum BrowserEvent {
    /// Click on a rail icon: switch to it, or collapse if it's already
    /// showing.
    Rail(Section),
    SetChip(Chip),
    ToggleFitsKey,
    SetSort(Sort),
    SetQuery(String),
    /// Click on a collection: narrow to it, or back out if it's selected.
    SelectCollection(Coll),
    NewCollection,
    BeginRename(usize),
    Rename(usize, String),
    DeleteCollection(usize),
    AddToCollection(Coll, String),
    RemoveFromCollection(Coll, String),
    ToggleFavourite(String),
    Select(String),
    /// Double-click (or a lesson's single click): use the item.
    Activate(String),
    /// Something was dropped on a track (`None`: on empty timeline space,
    /// making a new track) at `tick`.
    DropOnTrack { item: String, track: Option<TrackId>, tick: shared::arrangement::Ticks },
    SetWidth(f32),
    /// A drag of the panel's edge ended: remember the width.
    CommitWidth,
    FocusSearch,
    /// Sample keys, analysed in the background (and, to cache, every
    /// file's size and key or none).
    KeysFound(HashMap<Arc<str>, keys::SampleKey>, HashMap<Arc<str>, (u64, Option<keys::SampleKey>)>),
    Tick,
}

pub struct BrowserModel {
    pub open: Signal<bool>,
    pub section: Signal<Section>,
    pub width: Signal<f32>,
    pub query: Signal<String>,
    pub chip: Signal<Chip>,
    pub fits_key: Signal<bool>,
    pub sort: Signal<Sort>,
    pub favourites: Signal<Vec<String>>,
    pub collections: Signal<Vec<Collection>>,
    pub collection: Signal<Option<Coll>>,
    pub renaming: Signal<Option<usize>>,
    pub history: Signal<Vec<String>>,
    pub selected: Signal<Option<String>>,
    pub keys: Signal<HashMap<Arc<str>, keys::SampleKey>>,
    /// Everything that can be listed (library, plus My tracks, project
    /// audio and lessons, kept current).
    pub all: Signal<Arc<Vec<Item>>>,
    library: Vec<Item>,
    arrangement: Signal<Arrangement>,
    selected_track: Signal<Option<TrackId>>,
    my_tracks: Signal<Vec<PathBuf>>,
    project_path: Signal<Option<PathBuf>>,
    last_path: Option<Option<PathBuf>>,
    last_sources: Vec<Arc<str>>,
    last_tracks: Vec<PathBuf>,
    pub preview: preview::BrowserPreview,
}

impl BrowserModel {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        open: Signal<bool>,
        arrangement: Signal<Arrangement>,
        selected_track: Signal<Option<TrackId>>,
        my_tracks: Signal<Vec<PathBuf>>,
        project_path: Signal<Option<PathBuf>>,
        preview: preview::BrowserPreview,
    ) -> Self {
        let library = items::library();
        let first_run = crate::settings::load_lessons_done().is_empty();
        let model = Self {
            open,
            section: Signal::new(if first_run { Section::Learn } else { Section::Browse }),
            width: Signal::new(DEFAULT_WIDTH),
            query: Signal::new(String::new()),
            chip: Signal::new(Chip::All),
            fits_key: Signal::new(false),
            sort: Signal::new(Sort::Recent),
            favourites: Signal::new(crate::settings::load_browser_list("browser_favourites")),
            collections: Signal::new(crate::settings::load_browser_collections()),
            collection: Signal::new(None),
            renaming: Signal::new(None),
            history: Signal::new(crate::settings::load_browser_list("browser_history")),
            selected: Signal::new(None),
            keys: Signal::new(HashMap::new()),
            all: Signal::new(Arc::new(Vec::new())),
            library,
            arrangement,
            selected_track,
            my_tracks,
            project_path,
            last_path: None,
            last_sources: Vec::new(),
            last_tracks: Vec::new(),
            preview,
        };
        model.rebuild_all(&[], &[]);
        model
    }

    fn rebuild_all(&self, sources: &[Arc<str>], tracks: &[PathBuf]) {
        let mut all = self.library.clone();
        all.extend(items::project_audio(sources));
        all.extend(items::my_tracks(tracks));
        for (i, lesson) in crate::lessons::course::LESSONS.iter().enumerate() {
            all.push(Item {
                id: format!("lesson:{}", lesson.id),
                kind: Kind::Lesson(i),
                name: lesson.title.into(),
                meta: lesson.group.into(),
                bpm: None,
                group: lesson.group.into(),
            });
        }
        self.all.set(Arc::new(all));
    }

    fn item(&self, id: &str) -> Option<Item> {
        self.all.get().iter().find(|i| i.id == id).cloned()
    }

    fn remember(&mut self, id: &str) {
        let mut history = self.history.get();
        history.retain(|h| h != id);
        history.insert(0, id.to_string());
        history.truncate(HISTORY_LEN);
        crate::settings::save_browser_list("browser_history", &history);
        self.history.set(history);
    }

    fn save_collections(&self) {
        crate::settings::save_browser_collections(&self.collections.get());
    }

    /// The panel's width and section for the open project.
    fn load_layout(&mut self) {
        let path = self.project_path.get();
        if let Some((width, section)) = crate::settings::load_browser_layout(path.as_deref()) {
            self.width.set(width.clamp(MIN_WIDTH, MAX_WIDTH));
            if let Some(section) = Section::from_key(&section) {
                self.section.set(section);
            }
        }
    }

    fn save_layout(&self) {
        crate::settings::save_browser_layout(self.project_path.get().as_deref(), self.width.get(), self.section.get().key());
    }

    /// Uses `item` the way a double-click does.
    fn activate(&mut self, cx: &mut EventContext, item: &Item) {
        use crate::project::ProjectEvent;
        use crate::synth::state::SynthEvent;
        use crate::timeline::state::TimelineEvent;
        match &item.kind {
            Kind::Instrument(i) => cx.emit(SynthEvent::AddInstrumentToSelected(*i)),
            Kind::Effect(e) => {
                if let Some(track) = self.selected_track.get() {
                    cx.emit(match e {
                        items::EffectKind::Compressor => TimelineEvent::AddCompressorEffect(track),
                        items::EffectKind::Eq => TimelineEvent::AddEqEffect(track),
                    });
                }
            }
            Kind::Preset(i) => cx.emit(SynthEvent::LoadPreset(shared::synth::PRESETS[*i].1)),
            Kind::Sample(s) | Kind::ProjectAudio(s) => cx.emit(TimelineEvent::AddDrumSample(s.clone())),
            Kind::Pattern(i) => cx.emit(TimelineEvent::AddDrumPattern(*i)),
            Kind::Song(song) => cx.emit(ProjectEvent::OpenDemo(*song)),
            Kind::Track(path) => cx.emit(ProjectEvent::OpenTrack(path.clone())),
            Kind::Lesson(i) => cx.emit(ProjectEvent::StartLesson(*i)),
        }
        if !matches!(item.kind, Kind::Lesson(_)) {
            self.remember(&item.id);
        }
    }

    /// Something dropped on a track's lane or header (or empty space).
    fn drop_on_track(&mut self, cx: &mut EventContext, item: &Item, track: Option<TrackId>, tick: shared::arrangement::Ticks) {
        use crate::synth::state::SynthEvent;
        use crate::timeline::state::TimelineEvent;
        let arr = self.arrangement.get();
        let target = track.and_then(|t| arr.track(t)).cloned();
        match &item.kind {
            Kind::Sample(s) | Kind::ProjectAudio(s) => cx.emit(TimelineEvent::AddSampleAt { source: s.clone(), track, start: tick }),
            Kind::Instrument(i) => match target {
                Some(t) if t.kind == shared::arrangement::TrackKind::Midi => {
                    cx.emit(TimelineEvent::SetInstrument { track: t.id, instrument: Some(*i) });
                    cx.emit(SynthEvent::SelectTrack(t.id));
                }
                _ => cx.emit(TimelineEvent::AddTrackWith(Some(*i))),
            },
            Kind::Preset(i) => {
                match target {
                    Some(t) if t.instrument == Some(shared::arrangement::Instrument::Carve) => cx.emit(SynthEvent::SelectTrack(t.id)),
                    _ => cx.emit(TimelineEvent::AddTrackWith(Some(shared::arrangement::Instrument::Carve))),
                }
                cx.emit(SynthEvent::LoadPreset(shared::synth::PRESETS[*i].1));
            }
            Kind::Effect(e) => {
                if let Some(t) = target.map(|t| t.id).or(self.selected_track.get()) {
                    cx.emit(match e {
                        items::EffectKind::Compressor => TimelineEvent::AddCompressorEffect(t),
                        items::EffectKind::Eq => TimelineEvent::AddEqEffect(t),
                    });
                }
            }
            Kind::Pattern(i) => cx.emit(TimelineEvent::AddDrumPattern(*i)),
            Kind::Song(_) | Kind::Track(_) | Kind::Lesson(_) => return,
        }
        self.remember(&item.id);
    }
}

impl Model for BrowserModel {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|event, _| match event {
            BrowserEvent::Rail(section) => {
                if self.open.get() && self.section.get() == *section {
                    cx.emit(crate::app::AppEvent::ToggleSidebar);
                } else {
                    self.section.set(*section);
                    if !self.open.get() {
                        cx.emit(crate::app::AppEvent::ToggleSidebar);
                    }
                    self.save_layout();
                }
            }
            BrowserEvent::SetChip(chip) => self.chip.set(*chip),
            BrowserEvent::ToggleFitsKey => self.fits_key.set(!self.fits_key.get()),
            BrowserEvent::SetSort(sort) => self.sort.set(*sort),
            BrowserEvent::SetQuery(q) => self.query.set(q.clone()),
            BrowserEvent::SelectCollection(coll) => {
                self.collection.set(if self.collection.get() == Some(*coll) { None } else { Some(*coll) });
            }
            BrowserEvent::NewCollection => {
                const COLORS: [ClipColor; 6] =
                    [ClipColor::Teal, ClipColor::Coral, ClipColor::Violet, ClipColor::Amber, ClipColor::Blue, ClipColor::Pink];
                let n = self.collections.get().len();
                self.collections.update(|c| c.push(Collection { name: format!("Collection {}", n + 1), color: COLORS[n % COLORS.len()], items: vec![] }));
                self.renaming.set(Some(n));
                self.save_collections();
            }
            BrowserEvent::BeginRename(i) => self.renaming.set(Some(*i)),
            BrowserEvent::Rename(i, name) => {
                let name = name.trim();
                if !name.is_empty() {
                    self.collections.update(|c| {
                        if let Some(c) = c.get_mut(*i) {
                            c.name = name.to_string();
                        }
                    });
                    self.save_collections();
                }
                self.renaming.set(None);
            }
            BrowserEvent::DeleteCollection(i) => {
                if *i < self.collections.get().len() {
                    self.collections.update(|c| {
                        c.remove(*i);
                    });
                    self.collection.set(None);
                    self.save_collections();
                }
            }
            BrowserEvent::AddToCollection(coll, id) => match coll {
                Coll::Favourites => {
                    if !self.favourites.get().contains(id) {
                        self.favourites.update(|f| f.push(id.clone()));
                        crate::settings::save_browser_list("browser_favourites", &self.favourites.get());
                    }
                }
                Coll::User(i) => {
                    self.collections.update(|c| {
                        if let Some(c) = c.get_mut(*i) {
                            if !c.items.contains(id) {
                                c.items.push(id.clone());
                            }
                        }
                    });
                    self.save_collections();
                }
            },
            BrowserEvent::RemoveFromCollection(coll, id) => match coll {
                Coll::Favourites => {
                    self.favourites.update(|f| f.retain(|x| x != id));
                    crate::settings::save_browser_list("browser_favourites", &self.favourites.get());
                }
                Coll::User(i) => {
                    self.collections.update(|c| {
                        if let Some(c) = c.get_mut(*i) {
                            c.items.retain(|x| x != id);
                        }
                    });
                    self.save_collections();
                }
            },
            BrowserEvent::ToggleFavourite(id) => {
                let coll = Coll::Favourites;
                if self.favourites.get().contains(id) {
                    cx.emit(BrowserEvent::RemoveFromCollection(coll, id.clone()));
                } else {
                    cx.emit(BrowserEvent::AddToCollection(coll, id.clone()));
                }
            }
            BrowserEvent::Select(id) => self.selected.set(Some(id.clone())),
            BrowserEvent::Activate(id) => {
                if let Some(item) = self.item(id) {
                    self.selected.set(Some(id.clone()));
                    self.activate(cx, &item);
                }
            }
            BrowserEvent::DropOnTrack { item, track, tick } => {
                if let Some(item) = self.item(item) {
                    self.drop_on_track(cx, &item, *track, *tick);
                }
            }
            BrowserEvent::SetWidth(w) => self.width.set(w.clamp(MIN_WIDTH, MAX_WIDTH)),
            BrowserEvent::CommitWidth => self.save_layout(),
            BrowserEvent::FocusSearch => {
                if !self.open.get() {
                    cx.emit(crate::app::AppEvent::ToggleSidebar);
                }
                if let Some(search) = view::SEARCH.get() {
                    cx.emit_to(search, TextEvent::StartEdit);
                }
            }
            BrowserEvent::KeysFound(keys, cache) => {
                self.keys.set(keys.clone());
                crate::settings::save_sample_keys(cache);
            }
            BrowserEvent::Tick => {
                // The open project changed: its own panel layout.
                let path = self.project_path.get();
                if self.last_path.as_ref() != Some(&path) {
                    self.last_path = Some(path);
                    self.load_layout();
                }
                // Project audio and My tracks, when they change.
                let sources: Vec<Arc<str>> = {
                    let mut s: Vec<Arc<str>> = crate::timeline::peaks_loader::audio_sources(&self.arrangement.get()).into_iter().collect();
                    s.sort();
                    s
                };
                let tracks = self.my_tracks.get();
                if sources != self.last_sources || tracks != self.last_tracks {
                    self.rebuild_all(&sources, &tracks);
                    self.last_sources = sources;
                    self.last_tracks = tracks;
                }
                self.preview.tick(cx);
            }
        });
        self.preview.event(cx, event);
    }
}

/// Analyses every library sample's key in the background ("Fits key"
/// uses them once they arrive).
pub fn start_key_analysis(cx: &mut Context) {
    // Keys already worked out, by file and size: only new or changed
    // files are analysed again.
    let cache = crate::settings::load_sample_keys();
    cx.spawn(move |proxy| {
        let assets = crate::timeline::assets_dir();
        let mut found = HashMap::new();
        let mut sizes = HashMap::new();
        for category in crate::timeline::drum_sample_categories() {
            for file in &category.files {
                let source: Arc<str> = format!("drums/{file}").into();
                let path = assets.join(&*source);
                let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                let key = match cache.get(&*source) {
                    Some((cached_size, key)) if *cached_size == size => *key,
                    _ => keys::analyse_file(&path),
                };
                sizes.insert(source.clone(), (size, key));
                if let Some(key) = key {
                    found.insert(source, key);
                }
            }
        }
        let _ = proxy.emit(BrowserEvent::KeysFound(found, sizes));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter<'a>(section: Section, keys: &'a HashMap<Arc<str>, keys::SampleKey>) -> Filter<'a> {
        Filter {
            section,
            query: "",
            chip: Chip::All,
            fits_key: false,
            sort: Sort::Name,
            collection: None,
            favourites: &[],
            collections: &[],
            history: &[],
            keys,
            project_key: (9, 0),
        }
    }

    #[test]
    fn sections_chips_search_and_collections_narrow_the_results() {
        let all = items::library();
        let keys = HashMap::new();
        let browse = results(&all, &filter(Section::Browse, &keys));
        assert!(browse.iter().any(|i| i.name == "Carve"));
        assert!(!browse.iter().any(|i| matches!(i.kind, Kind::Song(_))), "songs are project files");
        let presets = results(&all, &filter(Section::Presets, &keys));
        assert!(presets.iter().all(|i| matches!(i.kind, Kind::Preset(_))));
        let effects = results(&all, &Filter { chip: Chip::Effects, ..filter(Section::Browse, &keys) });
        assert_eq!(effects.len(), 2);
        let searched = results(&all, &Filter { query: "piano", ..filter(Section::Samples, &keys) });
        assert!(!searched.is_empty() && searched.iter().all(|i| i.matches("piano")));
        let favs = vec!["inst:carve".to_string()];
        let fav = results(&all, &Filter { collection: Some(Coll::Favourites), favourites: &favs, ..filter(Section::Browse, &keys) });
        assert_eq!(fav.len(), 1);
        // Recent: what was used last comes first.
        let history = vec!["fx:eq".to_string()];
        let recent = results(&all, &Filter { sort: Sort::Recent, history: &history, ..filter(Section::Browse, &keys) });
        assert_eq!(recent[0].id, "fx:eq");
    }

    #[test]
    fn fits_key_drops_only_samples_in_another_key() {
        let all = items::library();
        let sample = all.iter().find(|i| matches!(i.kind, Kind::Sample(_))).unwrap();
        let mut keys = HashMap::new();
        // F# major: F# G# A# B C# D# F - not in A minor pentatonic.
        keys.insert(sample.source().unwrap().clone(), keys::SampleKey { root: 6, minor: false, notes: 0b1010_1101_0101 << 0 });
        let minor_pent = shared::theory::SCALE_PRESETS.iter().find(|p| p.name == "Minor pentatonic").unwrap().mask;
        let f = Filter { fits_key: true, project_key: (9, minor_pent), ..filter(Section::Browse, &keys) };
        let shown = results(&all, &f);
        assert!(!shown.iter().any(|i| i.id == sample.id));
        // Everything without a key stays.
        assert!(shown.iter().any(|i| i.name == "Carve"));
        assert_eq!(key_label(9, minor_pent), "A min pent");
    }
}
