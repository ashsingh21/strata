//! Project management: New/Open/Save/Save As/Rename, plus Ctrl+S, and the
//! unsaved-changes prompt before Close/New/Open. Owns
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
/// doesn't already live somewhere else - "projects" in the user's data
/// folder (see `paths`), created on first use.
pub fn default_projects_dir() -> PathBuf {
    let dir = crate::paths::data_dir().join("projects");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Where lesson projects save themselves: "My tracks", in the projects
/// folder. Created on first use.
pub fn my_tracks_dir() -> PathBuf {
    let dir = default_projects_dir().join("My tracks");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// The saved tracks, newest first.
pub fn my_tracks() -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(my_tracks_dir()) else { return vec![] };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .map(|p| (std::fs::metadata(&p).and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH), p))
        .collect();
    files.sort_by(|a, b| b.0.cmp(&a.0));
    files.into_iter().map(|(_, p)| p).collect()
}

/// A file name in My tracks for a new try at `title` that no earlier
/// try already has ("House track 1 - the groove 2.json").
fn new_track_path(title: &str) -> PathBuf {
    let stem: String = title.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { ' ' }).collect();
    let stem = stem.split_whitespace().collect::<Vec<_>>().join(" ");
    let dir = my_tracks_dir();
    (1..)
        .map(|n| dir.join(if n == 1 { format!("{stem}.json") } else { format!("{stem} {n}.json") }))
        .find(|p| !p.exists())
        .expect("some free name")
}

/// The learner's own finished previous part of the house track, with its
/// tracks named as the next part's steps expect - or `None` to start from
/// the stock version.
fn carried_over(lesson: &str) -> Option<Project> {
    let previous = shared::lessons::previous_part(lesson)?;
    let mut project = load(&crate::settings::load_lesson_track(previous)?).ok()?;
    project.migrate();
    let arr = &mut project.arrangement;
    let drums = arr.tracks.iter().position(|t| t.instrument == Some(shared::arrangement::Instrument::Drums));
    let carve: Vec<usize> =
        arr.tracks.iter().enumerate().filter(|(_, t)| t.instrument == Some(shared::arrangement::Instrument::Carve)).map(|(i, _)| i).collect();
    let names = shared::lessons::part_track_names(lesson).iter().copied();
    for (index, name) in drums.into_iter().map(|i| (i, "Drums")).chain(carve.into_iter().zip(names)) {
        arr.tracks[index].name = name.into();
    }
    Some(project)
}

/// Sets the piano roll's key and scale (a `theory::SCALE_PRESETS` name).
fn set_key(cx: &mut EventContext, root: u8, scale: &str) {
    cx.emit(crate::interval_input::state::IntervalInputEvent::SetKey(root));
    if let Some(preset) = shared::theory::SCALE_PRESETS.iter().find(|p| p.name == scale) {
        cx.emit(crate::interval_input::state::IntervalInputEvent::SetScaleMask(preset.mask));
    }
}

/// What to do with unsaved changes before a destructive action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscardChoice {
    Save,
    DontSave,
    Cancel,
}

/// The file this app has always saved to before project management
/// existed - still the default a fresh checkout opens, so upgrading
/// doesn't lose anyone's place.
fn legacy_project_path() -> PathBuf {
    crate::paths::data_dir().join("project.json")
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
    /// The status bar's export line: progress, then the result. Empty when
    /// no export has run.
    pub export_status: Signal<String>,
    exporting: bool,
    /// An action waiting on the unsaved-changes dialog, or on a Save As
    /// the dialog's "Save" kicked off - runs once that save succeeds.
    pending: Option<GuardedAction>,
    /// A dialog is already up - further close/New/Open requests are
    /// swallowed rather than stacking a second dialog.
    asking: bool,
    /// Set just before re-emitting `WindowClose` once the user has
    /// decided, so this model lets it through to the window.
    allow_close: bool,
    /// The files in My tracks, newest first (the sidebar lists them).
    pub my_tracks: Signal<Vec<PathBuf>>,
}

/// Actions that would throw away unsaved changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GuardedAction {
    Close,
    New,
    Open,
    /// Open a built-in demo song (`shared::demo`) as a new, unsaved project.
    Demo(shared::demo::DemoSong),
    /// Open lesson `n`'s starting project and begin it (`crate::lessons`).
    Lesson(usize),
    /// Open a known file (one of My tracks).
    OpenPath(PathBuf),
}

pub enum ProjectEvent {
    Save,
    SaveAsDialog,
    OpenDialog,
    New,
    OpenDemo(shared::demo::DemoSong),
    /// Start lesson `n` of `crate::lessons::course::LESSONS`.
    StartLesson(usize),
    /// Open one of My tracks.
    OpenTrack(PathBuf),
    /// A lesson is running: save it to its My tracks file if it's changed.
    AutoSave,
    /// Lesson `id` just finished: save, and remember the file as that
    /// part's result.
    LessonFinished(&'static str),
    /// A new file stem, typed into the header's title field - moves the
    /// project's file on disk if it's been saved before, otherwise just
    /// updates the name a future Save As will suggest.
    Rename(String),
    /// A dialog's own result, reported back from the background thread
    /// that ran it (see `spawn_dialog`) - `None` if the user cancelled.
    OpenPicked(Option<PathBuf>),
    SaveAsPicked(Option<PathBuf>),
    /// File > Export Audio: pick a .wav path, then render to it.
    ExportDialog,
    ExportPicked(Option<PathBuf>),
    ExportProgress(f32),
    /// The written file, or what went wrong.
    ExportDone(Result<PathBuf, String>),
    /// The unsaved-changes dialog's answer for a pending action.
    DiscardDecided(GuardedAction, DiscardChoice),
}

/// Runs a dialog (see `crate::dialogs`) on a background thread and reports whatever
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
        .filter(|t| t.instrument == Some(shared::arrangement::Instrument::Carve))
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
///
/// Id counters (`next_id`, in the arrangement and in every effect graph)
/// are left out: they only ever grow - an undone "add track" doesn't give
/// its id back - so including them made a project that matched its saved
/// file still read "Edited" after undo. Only the comparison drops them;
/// real saves keep them.
pub fn snapshot(arrangement: &Arrangement, patches: &BTreeMap<TrackId, SynthState>) -> String {
    fn strip_id_counters(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                map.remove("next_id");
                map.values_mut().for_each(strip_id_counters);
            }
            serde_json::Value::Array(items) => items.iter_mut().for_each(strip_id_counters),
            _ => {}
        }
    }
    let Ok(mut value) = serde_json::to_value(project(arrangement, patches)) else { return String::new() };
    strip_id_counters(&mut value);
    value.to_string()
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
            export_status: Signal::new(String::new()),
            exporting: false,
            pending: None,
            asking: false,
            allow_close: false,
            my_tracks: Signal::new(my_tracks()),
        }
    }

    /// Saves to the current file if it's a My tracks one and anything
    /// changed, without renaming the project in the header.
    fn auto_save(&mut self) {
        let Some(path) = self.current_path.get().filter(|p| p.starts_with(my_tracks_dir())) else { return };
        if !self.is_dirty() {
            return;
        }
        let arrangement = self.arrangement.get();
        let patches = self.patches.get();
        match save(&project(&arrangement, &patches), &path) {
            Ok(()) => {
                self.saved.set(snapshot(&arrangement, &patches));
                if !self.my_tracks.get().contains(&path) {
                    self.my_tracks.set(my_tracks());
                }
            }
            Err(e) => eprintln!("project: failed to save {}: {e}", path.display()),
        }
    }

    /// Loads `path`, replacing the live arrangement/patches, and kicks off
    /// the same waveform/decode work `main.rs` does for the project it
    /// opens at startup - a runtime Open is otherwise indistinguishable
    /// from a fresh launch pointed at a different file.
    fn open(&mut self, cx: &mut EventContext, path: PathBuf) {
        match load(&path) {
            Ok(project) => {
                self.replace_project(cx, project);
                self.current_path.set(Some(path.clone()));
                self.display_name.set(name_from_path(Some(&path)));
            }
            Err(e) => eprintln!("project: failed to load {}: {e}", path.display()),
        }
    }

    /// Swaps in `project` wholesale and marks it as saved. The caller sets
    /// the path/name.
    fn replace_project(&mut self, cx: &mut EventContext, project: Project) {
        let patches: BTreeMap<TrackId, SynthState> = project.instruments.into_iter().collect();
        // From `patches`, not `self.patches`: `LoadPatches` hasn't been
        // handled yet, so `self.patches` still holds the old project's.
        self.saved.set(snapshot(&project.arrangement, &patches));
        cx.emit(TimelineEvent::LoadArrangement(project.arrangement.clone()));
        cx.emit(SynthEvent::LoadPatches(patches));
        let assets_dir = crate::timeline::assets_dir();
        for source in crate::timeline::peaks_loader::audio_sources(&project.arrangement) {
            crate::timeline::peaks_loader::spawn_peak_loader_for_source(cx, &assets_dir, source.clone());
            let _ = self.decode_request_tx.send(source);
        }
    }

    /// Renders the project as it is now to `path` on a background thread,
    /// reporting progress to the status bar. The render reads a snapshot,
    /// so editing while it runs doesn't affect it.
    fn start_export(&mut self, cx: &mut EventContext, path: PathBuf) {
        const SAMPLE_RATE: u32 = 48_000;
        self.exporting = true;
        self.export_status.set("Exporting\u{2026}".to_string());
        let arrangement = self.arrangement.get();
        let patches = self.patches.get();
        cx.spawn(move |proxy| {
            let result = (|| {
                if engine::render::song_end(&arrangement) == 0 {
                    return Err("nothing to export: the project has no clips".to_string());
                }
                let sources = decode_sources(&arrangement, SAMPLE_RATE);
                let job = engine::render::RenderJob { arrangement, patches, sources, sample_rate: SAMPLE_RATE };
                let mut last = -1.0f32;
                let audio = engine::render::render(&job, |p| {
                    if p - last >= 0.02 {
                        last = p;
                        let _ = proxy.emit(ProjectEvent::ExportProgress(p));
                    }
                    true
                })
                .ok_or("cancelled")?;
                engine::render::write_wav(&path, &audio, SAMPLE_RATE).map_err(|e| e.to_string())?;
                Ok(path)
            })();
            let _ = proxy.emit(ProjectEvent::ExportDone(result));
        });
    }

    fn save_to(&mut self, path: PathBuf) -> bool {
        let arrangement = self.arrangement.get();
        let patches = self.patches.get();
        match save(&project(&arrangement, &patches), &path) {
            Ok(()) => {
                self.current_path.set(Some(path.clone()));
                self.display_name.set(name_from_path(Some(&path)));
                self.saved.set(snapshot(&arrangement, &patches));
                true
            }
            Err(e) => {
                eprintln!("project: failed to save to {}: {e}", path.display());
                false
            }
        }
    }

    fn is_dirty(&self) -> bool {
        snapshot(&self.arrangement.get(), &self.patches.get()) != self.saved.get()
    }

    /// Runs `action` now if there's nothing to lose, otherwise asks first.
    fn guard(&mut self, cx: &mut EventContext, action: GuardedAction) {
        if self.asking {
            return;
        }
        // A lesson's track saves itself: nothing to ask about.
        self.auto_save();
        if !self.is_dirty() {
            self.perform(cx, action);
            return;
        }
        self.asking = true;
        let name = self.display_name.get();
        let verb = match action {
            GuardedAction::Close => "closing",
            GuardedAction::New => "starting a new project",
            GuardedAction::Open => "opening another project",
            GuardedAction::Demo(_) => "opening the demo",
            GuardedAction::Lesson(_) => "starting a lesson",
            GuardedAction::OpenPath(_) => "opening another project",
        };
        cx.spawn(move |proxy| {
            let choice = crate::dialogs::ask_save(&name, verb);
            let _ = proxy.emit(ProjectEvent::DiscardDecided(action, choice));
        });
    }

    fn perform(&mut self, cx: &mut EventContext, action: GuardedAction) {
        match action {
            GuardedAction::Close => {
                self.allow_close = true;
                cx.emit_to(Entity::root(), WindowEvent::WindowClose);
            }
            GuardedAction::New => {
                cx.emit(TimelineEvent::LoadArrangement(empty_arrangement()));
                cx.emit(SynthEvent::LoadPatches(BTreeMap::new()));
                self.current_path.set(None);
                self.display_name.set(name_from_path(None));
                self.saved.set(snapshot(&empty_arrangement(), &BTreeMap::new()));
            }
            GuardedAction::Open => {
                spawn_dialog(cx, || crate::dialogs::pick_project(&default_projects_dir()), ProjectEvent::OpenPicked)
            }
            GuardedAction::Lesson(n) => {
                let Some(lesson) = crate::lessons::course::LESSONS.get(n) else { return };
                let project = carried_over(lesson.id).unwrap_or_else(|| shared::lessons::starting_project(lesson.id));
                self.replace_project(cx, project);
                // Saved to My tracks as it goes (the file appears on the
                // first change) - except the arrangement tours, which open
                // finished songs to listen to.
                let saves = lesson.group != crate::lessons::course::ARRANGEMENT;
                self.current_path.set(saves.then(|| new_track_path(lesson.title)));
                // Short: the header's name field is narrow (the bar shows the title).
                self.display_name.set(format!("Lesson {}", n + 1));
                // The key its steps name notes in, so they're the rows shown.
                if let Some((root, scale)) = shared::lessons::lesson_key(lesson.id) {
                    set_key(cx, root, scale);
                }
                cx.emit(crate::lessons::LessonEvent::Begin(n));
            }
            GuardedAction::OpenPath(path) => self.open(cx, path),
            GuardedAction::Demo(song) => {
                self.replace_project(cx, song.project());
                self.current_path.set(None);
                self.display_name.set(song.name().to_string());
                // Its key and scale, so the piano roll shows its notes.
                let (root, scale) = song.key();
                set_key(cx, root, scale);
            }
        }
    }
}

impl Model for ProjectModel {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        // The window's close button. This model lives on the root (window)
        // entity, and Vizia visits an entity's models before its view, so
        // consuming the event here stops the Window view from closing.
        event.map(|window_event, meta| {
            if let WindowEvent::WindowClose = window_event {
                if !self.allow_close {
                    meta.consume();
                    self.guard(cx, GuardedAction::Close);
                }
            }
        });
        event.map(|event, _| match event {
            ProjectEvent::Save => match self.current_path.get() {
                Some(path) => {
                    self.save_to(path);
                }
                None => spawn_save_as_dialog(cx, self.display_name.get()),
            },
            ProjectEvent::SaveAsDialog => spawn_save_as_dialog(cx, self.display_name.get()),
            ProjectEvent::OpenDialog => self.guard(cx, GuardedAction::Open),
            ProjectEvent::OpenPicked(Some(path)) => self.open(cx, path.clone()),
            ProjectEvent::OpenPicked(None) => {}
            ProjectEvent::SaveAsPicked(Some(path)) => {
                let saved = self.save_to(path.clone());
                if let Some(action) = self.pending.take() {
                    if saved {
                        self.perform(cx, action);
                    }
                }
            }
            // Cancelling the Save As a "Save" answer opened cancels the
            // action it was saving for, too.
            ProjectEvent::SaveAsPicked(None) => self.pending = None,
            ProjectEvent::ExportDialog => {
                if !self.exporting {
                    let name = self.display_name.get();
                    spawn_dialog(cx, move || crate::dialogs::export_wav(&default_projects_dir(), &name), ProjectEvent::ExportPicked);
                }
            }
            ProjectEvent::ExportPicked(Some(path)) => self.start_export(cx, path.clone()),
            ProjectEvent::ExportPicked(None) => {}
            ProjectEvent::ExportProgress(p) => {
                self.export_status.set(format!("Exporting\u{2026} {:.0}%", p * 100.0));
            }
            ProjectEvent::ExportDone(result) => {
                self.exporting = false;
                match result {
                    Ok(path) => {
                        let file = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
                        self.export_status.set(format!("Exported {file}"));
                    }
                    Err(e) => {
                        eprintln!("export: {e}");
                        self.export_status.set("Export failed".to_string());
                    }
                }
            }
            ProjectEvent::New => self.guard(cx, GuardedAction::New),
            ProjectEvent::OpenDemo(song) => self.guard(cx, GuardedAction::Demo(*song)),
            ProjectEvent::StartLesson(n) => self.guard(cx, GuardedAction::Lesson(*n)),
            ProjectEvent::OpenTrack(path) => self.guard(cx, GuardedAction::OpenPath(path.clone())),
            ProjectEvent::AutoSave => self.auto_save(),
            ProjectEvent::LessonFinished(id) => {
                self.auto_save();
                if let Some(path) = self.current_path.get().filter(|p| p.exists()) {
                    crate::settings::save_lesson_track(id, &path);
                }
            }
            ProjectEvent::DiscardDecided(action, choice) => {
                self.asking = false;
                match choice {
                    DiscardChoice::Cancel => {}
                    DiscardChoice::DontSave => self.perform(cx, action.clone()),
                    DiscardChoice::Save => match self.current_path.get() {
                        Some(path) => {
                            if self.save_to(path) {
                                self.perform(cx, action.clone());
                            }
                        }
                        None => {
                            self.pending = Some(action.clone());
                            spawn_save_as_dialog(cx, self.display_name.get());
                        }
                    },
                }
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
    spawn_dialog(cx, move || crate::dialogs::save_project(&default_projects_dir(), &suggested), ProjectEvent::SaveAsPicked);
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

/// Every sample `arrangement` can play, decoded for the offline renderer at
/// `sample_rate` (the render's): its audio clips, and the drum kit if any
/// track uses it.
pub fn decode_sources(arrangement: &shared::arrangement::Arrangement, sample_rate: u32) -> Vec<shared::playback::DecodedSource> {
    let assets = crate::timeline::assets_dir();
    let mut names: Vec<Arc<str>> = crate::timeline::peaks_loader::audio_sources(arrangement).into_iter().collect();
    if arrangement.tracks.iter().any(|t| t.instrument == Some(shared::arrangement::Instrument::Drums)) {
        names.extend(shared::drums::DRUM_KIT.iter().map(|p| Arc::from(p.sample)));
    }
    names.sort();
    names.dedup();
    names
        .into_iter()
        .filter_map(|name| {
            let (samples, spec) = crate::timeline::peaks_loader::decode_wav(&assets.join(&*name))?;
            Some(
                shared::playback::DecodedSource {
                    source: name,
                    sample_rate: spec.sample_rate,
                    channels: spec.channels,
                    samples: Arc::from(samples),
                }
                .at_rate(sample_rate),
            )
        })
        .collect()
}

#[cfg(test)]
mod decode_tests {
    #[test]
    fn library_samples_are_decoded_at_the_render_rate() {
        // The piano loops (and kick, snare...) are 44.1 kHz files.
        let source: std::sync::Arc<str> = "drums/piano_octave_short_loop_120_bpm.wav".into();
        let path = crate::timeline::assets_dir().join(&*source);
        let (raw, spec) = crate::timeline::peaks_loader::decode_wav(&path).unwrap();
        assert_eq!(spec.sample_rate, 44_100);
        let seconds = raw.len() as f64 / spec.channels as f64 / 44_100.0;
        let mut arr = shared::arrangement::empty_arrangement();
        let track = arr.alloc_id();
        let id = arr.alloc_id();
        arr.clips.push(shared::arrangement::Clip {
            id,
            track,
            start: 0,
            length: 3840,
            name: "Loop".into(),
            content: shared::arrangement::ClipContent::Audio { source: source.clone(), peaks: None, source_offset_samples: 0 },
            recording: false,
            gain_db: 0.0,
        });
        let decoded = super::decode_sources(&arr, 48_000);
        let d = decoded.iter().find(|d| d.source == source).unwrap();
        assert_eq!(d.sample_rate, 48_000);
        let out_seconds = d.samples.len() as f64 / d.channels as f64 / 48_000.0;
        assert!((out_seconds - seconds).abs() < 0.001, "{seconds} s became {out_seconds} s");
    }
}
