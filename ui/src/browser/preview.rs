//! The docked preview player: audition a result before using it. A sample
//! plays as itself (optionally synced to the project tempo, by playing it
//! faster or slower); a Carve preset or instrument plays a short phrase in
//! the project's key; an effect plays that phrase through it; the Drum Kit
//! and beat templates play a bar or two of beat. Rendered off the UI
//! thread, then played by the shared preview player.

use std::sync::Arc;

use vizia::prelude::*;

use shared::arrangement::{
    empty_arrangement, Arrangement, Clip, ClipColor, ClipContent, EffectGraph, Instrument, MidiNote, TempoMap, Ticks,
    TimeSignature, Track, TrackKind, DEFAULT_TRACK_HEIGHT, PPQ,
};
use shared::synth::SynthState;

use super::items::{EffectKind, Item, Kind};
use crate::preview_player::SharedPlayer;

/// Points in the dock's waveform.
const PEAK_COLUMNS: usize = 240;

pub enum PreviewEvent {
    /// The row's round button: play this, or stop it if it's playing.
    Toggle(Item),
    Stop,
    ToggleLoop,
    ToggleSync,
    /// Preview volume, dB.
    SetVolume(f32),
    Ready { generation: u64, audio: Arc<Vec<f32>>, peaks: Arc<Vec<f32>>, info: String },
}

pub struct BrowserPreview {
    player: SharedPlayer,
    /// What the dock shows (the last thing previewed).
    pub item: Signal<Option<Item>>,
    pub playing: Signal<bool>,
    pub loading: Signal<bool>,
    pub progress: Signal<f32>,
    pub looping: Signal<bool>,
    pub sync: Signal<bool>,
    pub volume_db: Signal<f32>,
    pub peaks: Signal<Arc<Vec<f32>>>,
    /// "124 BPM · 4 bars".
    pub info: Signal<String>,
    token: Option<u64>,
    generation: u64,
    audio: Option<Arc<Vec<f32>>>,
    arrangement: Signal<Arrangement>,
    key: Signal<u8>,
    scale_mask: Signal<u16>,
}

impl BrowserPreview {
    pub fn new(player: SharedPlayer, arrangement: Signal<Arrangement>, key: Signal<u8>, scale_mask: Signal<u16>) -> Self {
        let volume_db = crate::settings::load_preview_volume().unwrap_or(-12.0);
        player.borrow().set_gain(db_to_gain(volume_db));
        Self {
            player,
            item: Signal::new(None),
            playing: Signal::new(false),
            loading: Signal::new(false),
            progress: Signal::new(0.0),
            looping: Signal::new(false),
            sync: Signal::new(true),
            volume_db: Signal::new(volume_db),
            peaks: Signal::new(Arc::new(Vec::new())),
            info: Signal::new(String::new()),
            token: None,
            generation: 0,
            audio: None,
            arrangement,
            key,
            scale_mask,
        }
    }

    /// Once a frame: progress, and whether it's still ours and playing.
    pub fn tick(&mut self, _cx: &mut EventContext) {
        let mut player = self.player.borrow_mut();
        player.tick();
        let ours = self.token.is_some() && player.current() == self.token;
        if self.playing.get() != ours {
            self.playing.set(ours);
        }
        if ours {
            if let Some(p) = player.progress() {
                self.progress.set(p);
            }
        } else if self.token.take().is_some() {
            self.progress.set(0.0);
        }
    }

    fn start(&mut self, cx: &mut EventContext, item: Item) {
        self.stop();
        self.generation += 1;
        let generation = self.generation;
        self.item.set(Some(item.clone()));
        self.loading.set(true);
        self.info.set(String::new());
        self.peaks.set(Arc::new(Vec::new()));
        let arr = self.arrangement.get();
        let bpm = arr.tempo_map.bpm_at(0);
        let (root, mask) = (self.key.get(), self.scale_mask.get());
        let sync = self.sync.get();
        let sample_rate = self.player.borrow().sample_rate();
        cx.spawn(move |proxy| {
            let Some((audio, info)) = render(&item, bpm, root, mask, sync, sample_rate) else { return };
            let peaks = Arc::new(peaks(&audio));
            let _ = proxy.emit(PreviewEvent::Ready { generation, audio: Arc::new(audio), peaks, info });
        });
    }

    fn stop(&mut self) {
        if let Some(token) = self.token.take() {
            self.player.borrow_mut().stop_if(token);
        }
        self.playing.set(false);
        self.progress.set(0.0);
    }

    fn play(&mut self) {
        if let Some(audio) = &self.audio {
            self.token = self.player.borrow_mut().play(audio.as_ref().clone(), self.looping.get());
            self.playing.set(self.token.is_some());
        }
    }

    pub fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|event, _| match event {
            PreviewEvent::Toggle(item) => {
                let same = self.item.get().is_some_and(|i| i.id == item.id);
                if same && (self.playing.get() || self.loading.get()) {
                    self.generation += 1;
                    self.loading.set(false);
                    self.stop();
                } else {
                    self.start(cx, item.clone());
                }
            }
            PreviewEvent::Stop => {
                self.generation += 1;
                self.loading.set(false);
                self.stop();
            }
            PreviewEvent::ToggleLoop => {
                self.looping.set(!self.looping.get());
                if self.playing.get() {
                    self.play();
                }
            }
            PreviewEvent::ToggleSync => {
                self.sync.set(!self.sync.get());
                // A sample plays at a new speed: render it again.
                if let Some(item) = self.item.get().filter(|i| i.source().is_some()) {
                    if self.playing.get() {
                        self.start(cx, item);
                    }
                }
            }
            PreviewEvent::SetVolume(db) => {
                let db = db.clamp(-48.0, 0.0);
                self.volume_db.set(db);
                self.player.borrow().set_gain(db_to_gain(db));
                crate::settings::save_preview_volume(db);
            }
            PreviewEvent::Ready { generation, audio, peaks, info } => {
                if *generation != self.generation {
                    return;
                }
                self.loading.set(false);
                self.audio = Some(audio.clone());
                self.peaks.set(peaks.clone());
                self.info.set(info.clone());
                self.play();
            }
        });
    }
}

fn db_to_gain(db: f32) -> f32 {
    if db <= -47.9 {
        0.0
    } else {
        10f32.powf(db / 20.0)
    }
}

/// The waveform: each column's loudest sample (mono).
fn peaks(stereo: &[f32]) -> Vec<f32> {
    let frames = stereo.len() / 2;
    if frames == 0 {
        return vec![];
    }
    let per = frames.div_ceil(PEAK_COLUMNS).max(1);
    (0..frames.div_ceil(per))
        .map(|c| {
            let (a, b) = (c * per, ((c + 1) * per).min(frames));
            (a..b).map(|f| stereo[2 * f].abs().max(stereo[2 * f + 1].abs())).fold(0.0, f32::max).min(1.0)
        })
        .collect()
}

/// The preview's audio and its "tempo · length" line.
fn render(item: &Item, bpm: f64, root: u8, mask: u16, sync: bool, sample_rate: u32) -> Option<(Vec<f32>, String)> {
    let bars = |seconds: f64, tempo: f64| seconds * tempo / 240.0;
    match &item.kind {
        Kind::Sample(source) | Kind::ProjectAudio(source) => {
            let path = crate::timeline::assets_dir().join(&**source);
            let (samples, spec) = crate::timeline::peaks_loader::decode_wav(&path)?;
            let speed = match (sync, item.bpm) {
                (true, Some(own)) => bpm / own as f64,
                _ => 1.0,
            };
            let audio = resample(&samples, spec.channels.max(1) as usize, spec.sample_rate, sample_rate, speed);
            let seconds = samples.len() as f64 / spec.channels.max(1) as f64 / spec.sample_rate as f64;
            let info = match item.bpm {
                Some(own) => format!("{own:.0} BPM \u{b7} {:.0} bars", bars(seconds, own as f64).max(1.0)),
                None => format!("{seconds:.1} s"),
            };
            Some((audio, info))
        }
        Kind::Preset(i) => Some(phrase((shared::synth::PRESETS[*i].1)(), None, bpm, root, mask, sample_rate)),
        Kind::Instrument(Instrument::Carve) => Some(phrase(shared::synth::seed_synth(), None, bpm, root, mask, sample_rate)),
        Kind::Effect(e) => {
            let effect = match e {
                EffectKind::Compressor => shared::arrangement::Effect::Compressor(Default::default()),
                EffectKind::Eq => shared::arrangement::Effect::Eq(Default::default()),
                EffectKind::Guitar(kind) => shared::arrangement::Effect::Guitar(shared::guitar::GuitarFx::new(*kind)),
            };
            Some(phrase(shared::synth::seed_synth(), Some(effect), bpm, root, mask, sample_rate))
        }
        Kind::Instrument(Instrument::Drums) => Some(beat(bpm, sample_rate)),
        Kind::Pattern(i) => pattern(*i, bpm, sample_rate),
        Kind::Song(_) | Kind::Track(_) | Kind::Lesson(_) => None,
    }
}

/// Interleaved `channels` at `from` Hz to stereo at `to` Hz, played
/// `speed` times as fast (linear interpolation - fine for auditioning).
fn resample(samples: &[f32], channels: usize, from: u32, to: u32, speed: f64) -> Vec<f32> {
    let frames = samples.len() / channels;
    if frames == 0 {
        return vec![];
    }
    let step = from as f64 / to as f64 * speed;
    let out_frames = (frames as f64 / step) as usize;
    let at = |f: usize, c: usize| samples[f.min(frames - 1) * channels + c.min(channels - 1)];
    let mut out = Vec::with_capacity(out_frames * 2);
    for n in 0..out_frames {
        let pos = n as f64 * step;
        let (f, frac) = (pos as usize, (pos.fract()) as f32);
        for c in 0..2 {
            out.push(at(f, c) * (1.0 - frac) + at(f + 1, c) * frac);
        }
    }
    out
}

fn midi_track(arr: &mut Arrangement, instrument: Instrument) -> shared::arrangement::TrackId {
    let id = arr.alloc_id();
    arr.tracks.push(Track {
        id,
        name: "Preview".into(),
        color: ClipColor::Violet,
        kind: TrackKind::Midi,
        mute: false,
        solo: false,
        arm: false,
        gain_db: 0.0,
        height: DEFAULT_TRACK_HEIGHT,
        instrument: Some(instrument),
        effects: vec![],
        effect_slots: vec![],
        fx: EffectGraph::new(),
        drum_pads: Default::default(),
    });
    id
}

fn clip(arr: &mut Arrangement, track: shared::arrangement::TrackId, length: Ticks, content: ClipContent) {
    let id = arr.alloc_id();
    arr.clips.push(Clip { swing: 0.0, id, track, start: 0, length, name: "Preview".into(), content, recording: false, gain_db: 0.0 });
}

fn preview_arrangement(bpm: f64) -> Arrangement {
    let mut arr = empty_arrangement();
    arr.tempo_map = TempoMap::constant(bpm, TimeSignature::FOUR_FOUR);
    arr
}

/// A two-bar phrase in the project's key: up the scale's chord tones and
/// back, ending on a held root.
fn phrase(patch: SynthState, effect: Option<shared::arrangement::Effect>, bpm: f64, root: u8, mask: u16, sample_rate: u32) -> (Vec<f32>, String) {
    let degrees: Vec<u8> = (0..12u8).filter(|d| mask & (1 << d) != 0).collect();
    let degrees = if degrees.is_empty() { vec![0, 3, 7] } else { degrees };
    let pick = |i: usize| degrees[i.min(degrees.len() - 1)];
    let base = 57 + ((root as i32 - 9).rem_euclid(12)) as u8;
    let steps = [pick(0), pick(2), pick(4), 12, pick(4), pick(2), pick(1), pick(0)];
    let eighth = PPQ / 2;
    let mut notes: Vec<MidiNote> = steps
        .iter()
        .enumerate()
        .map(|(i, &d)| MidiNote { start: i as i64 * eighth, length: eighth, pitch: base + d, velocity: 100 })
        .collect();
    notes.last_mut().unwrap().length = 4 * eighth + PPQ * 2;
    let mut arr = preview_arrangement(bpm);
    let track = midi_track(&mut arr, Instrument::Carve);
    if let Some(effect) = effect {
        arr.tracks[0].fx.push_at_end(effect);
    }
    clip(&mut arr, track, 8 * PPQ, ClipContent::Midi { notes, loop_len: None, link: None });
    let mut patch = patch;
    patch.held_notes.clear();
    let job = engine::render::RenderJob {
        arrangement: arr,
        patches: std::collections::BTreeMap::from([(track, patch)]),
        sources: vec![],
        sample_rate,
    };
    let audio = engine::render::render_between(&job, 0, 8 * PPQ, 1.0);
    (audio, format!("{bpm:.0} BPM \u{b7} 2 bars"))
}

/// Two bars of the lessons' first beat, on the Drum Kit.
fn beat(bpm: f64, sample_rate: u32) -> (Vec<f32>, String) {
    let mut arr = preview_arrangement(bpm);
    let track = midi_track(&mut arr, Instrument::Drums);
    clip(&mut arr, track, 8 * PPQ, ClipContent::Midi { notes: shared::lessons::lesson_one_beat(), loop_len: Some(4 * PPQ), link: None });
    let sources = crate::project::decode_sources(&arr, sample_rate);
    let job = engine::render::RenderJob { arrangement: arr, patches: Default::default(), sources, sample_rate };
    (engine::render::render_between(&job, 0, 8 * PPQ, 0.5), format!("{bpm:.0} BPM \u{b7} 2 bars"))
}

/// A beat template's hits, as audio clips on one track.
fn pattern(index: usize, bpm: f64, sample_rate: u32) -> Option<(Vec<f32>, String)> {
    let template = crate::timeline::beat_templates::TEMPLATES.get(index)?;
    let mut arr = preview_arrangement(bpm);
    let track = arr.alloc_id();
    arr.tracks.push(Track {
        id: track,
        name: "Preview".into(),
        color: ClipColor::Coral,
        kind: TrackKind::Audio,
        mute: false,
        solo: false,
        arm: false,
        gain_db: 0.0,
        height: DEFAULT_TRACK_HEIGHT,
        instrument: None,
        effects: vec![],
        effect_slots: vec![],
        fx: EffectGraph::new(),
        drum_pads: Default::default(),
    });
    let assets = crate::timeline::assets_dir();
    for bar in 0..template.bars {
        for hit in template.hits {
            let seconds = crate::timeline::peaks_loader::wav_duration_seconds(&assets.join(hit.sample)).unwrap_or(0.25);
            let id = arr.alloc_id();
            let length = arr.tempo_map.seconds_to_ticks(seconds).max(1);
            arr.clips.push(Clip {
                id,
                track,
                start: bar * 4 * PPQ + hit.beat * PPQ,
                length,
                name: "Hit".into(),
                content: ClipContent::Audio { source: hit.sample.into(), peaks: None, source_offset_samples: 0 },
                recording: false,
                gain_db: 0.0,
                swing: 0.0,
            });
        }
    }
    let end = template.bars * 4 * PPQ;
    let sources = crate::project::decode_sources(&arr, sample_rate);
    let job = engine::render::RenderJob { arrangement: arr, patches: Default::default(), sources, sample_rate };
    Some((engine::render::render_between(&job, 0, end, 0.5), format!("{bpm:.0} BPM \u{b7} {} bars", template.bars)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampling_changes_length_by_the_speed() {
        let mono = vec![0.5f32; 48_000];
        // Twice as fast: half as long, as stereo.
        assert_eq!(resample(&mono, 1, 48_000, 48_000, 2.0).len(), 48_000);
        // 44.1 kHz to 48 kHz: a little longer.
        assert_eq!(resample(&mono, 1, 44_100, 48_000, 1.0).len() / 2, (48_000.0 * 48_000.0 / 44_100.0) as usize);
    }

    #[test]
    fn a_preset_preview_is_a_phrase_that_sounds() {
        let (audio, info) = phrase(shared::synth::soft_pad(), None, 120.0, 9, 0b0100_1010_1001, 48_000);
        assert!(audio.iter().any(|x| x.abs() > 0.01));
        assert_eq!(info, "120 BPM \u{b7} 2 bars");
    }
}
