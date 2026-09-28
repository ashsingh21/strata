//! "House demo": a finished two-minute house track (64 bars at 128 BPM),
//! opened from the sidebar as a new project. It exists to show what
//! Strata does in one place: a Drum Kit track playing section patterns,
//! four Carve patches (bass, stabs, pad, pluck lead), insert and
//! master-bus effects, section markers, and automation lanes of every
//! kind - track gain, an effect parameter and synth parameters.
//!
//! Every automation value is computed through the same `norm` functions
//! the knobs use, so a lane's line always lands on the value it names.

use crate::arrangement::{
    gain_db_to_fader_pos, Arrangement, AutomationLane, AutomationTarget, Breakpoint, Clip, ClipColor, ClipContent,
    CompressorState, Effect, EffectGraph, EffectNodeId, EffectParam, EqState, Instrument, Marker, MidiNote, TempoMap,
    Ticks, TimeSignature, Track, TrackId, TrackKind, DEFAULT_TRACK_HEIGHT, PPQ,
};
use crate::drums::{CLAP, CLOSED_HAT, KICK, OPEN_HAT};
use crate::project::Project;
use crate::synth::{
    deep_rave_bass, seed_synth, soft_pad, Envelope, Filter, FilterType, Fx, Lfo, LfoTarget, Oscillator, SynthParam,
    SynthState, Unison, VoiceMode, Waveform,
};

pub const NAME: &str = "House Demo";
const BPM: f64 = 128.0;
/// 64 bars of 4/4 at 128 BPM is exactly two minutes.
pub const BARS: i64 = 64;

const BAR: Ticks = PPQ * 4;
const SIXTEENTH: Ticks = PPQ / 4;

/// Song sections, in bars: (name, first bar).
const SECTIONS: [(&str, i64); 6] =
    [("Intro", 0), ("Build", 8), ("Groove", 16), ("Breakdown", 32), ("Drop", 40), ("Outro", 56)];

/// The four-bar progression everything follows: Am7 - Fmaj7 - Cmaj7 - G6.
/// (bass root, stab voicing, pad voicing)
const CHORDS: [(u8, [u8; 4], [u8; 4]); 4] = [
    (33, [57, 60, 64, 67], [45, 52, 55, 60]),
    (29, [57, 60, 64, 65], [41, 48, 52, 57]),
    (36, [55, 59, 60, 64], [48, 52, 55, 59]),
    (31, [55, 59, 62, 64], [43, 50, 55, 59]),
];

/// The lead hook over the progression, per bar: (16th, pitch, length in 16ths).
const HOOK: [&[(i64, u8, i64)]; 4] = [
    &[(0, 81, 2), (3, 79, 1), (4, 76, 2), (6, 74, 2), (8, 76, 3), (11, 72, 1), (12, 74, 2), (14, 76, 2)],
    &[(0, 72, 3), (3, 74, 1), (4, 76, 2), (6, 72, 2), (8, 69, 4), (14, 72, 2)],
    &[(0, 76, 2), (2, 79, 2), (4, 81, 2), (6, 79, 2), (8, 76, 3), (11, 74, 1), (12, 72, 2), (14, 74, 2)],
    &[(0, 74, 4), (4, 72, 2), (6, 69, 2), (8, 67, 6), (14, 69, 2)],
];

struct Builder {
    arr: Arrangement,
    instruments: Vec<(TrackId, SynthState)>,
}

impl Builder {
    fn track(&mut self, name: &str, color: ClipColor, kind: TrackKind, gain_db: f32) -> TrackId {
        let id = self.arr.alloc_id();
        self.arr.tracks.push(Track {
            id,
            name: name.into(),
            color,
            kind,
            mute: false,
            solo: false,
            arm: false,
            gain_db,
            height: DEFAULT_TRACK_HEIGHT,
            instrument: Instrument::default_for(kind),
            effects: vec![],
            effect_slots: vec![],
            fx: EffectGraph::new(),
        });
        id
    }

    fn synth_track(&mut self, name: &str, color: ClipColor, gain_db: f32, patch: SynthState) -> TrackId {
        let id = self.track(name, color, TrackKind::Midi, gain_db);
        self.instruments.push((id, patch));
        id
    }

    fn patch(&self, track: TrackId) -> &SynthState {
        &self.instruments.iter().find(|(id, _)| *id == track).expect("synth track").1
    }

    /// A MIDI clip from `start_bar` to `end_bar` looping `notes`, a pattern
    /// `pattern_bars` long - the way it'd be made by hand: write the
    /// pattern once, stretch the clip.
    fn midi_clip(&mut self, track: TrackId, name: &str, start_bar: i64, end_bar: i64, pattern_bars: i64, notes: Vec<MidiNote>) {
        let id = self.arr.alloc_id();
        let loop_len = (pattern_bars < end_bar - start_bar).then_some(pattern_bars * BAR);
        self.arr.clips.push(Clip {
            id,
            track,
            start: start_bar * BAR,
            length: (end_bar - start_bar) * BAR,
            name: name.into(),
            content: ClipContent::Midi { notes, loop_len, link: None },
            recording: false,
            gain_db: 0.0,
        });
    }

    fn lane(&mut self, track: TrackId, target: AutomationTarget, points: &[(i64, f32)]) {
        let id = self.arr.alloc_id();
        let label = self.arr.target_label(track, target).unwrap_or_default();
        self.arr.automation.push(AutomationLane {
            id,
            track,
            parameter_name: label,
            display_value: String::new(),
            breakpoints: points.iter().map(|&(bar, value)| Breakpoint { tick: bar * BAR, value }).collect(),
            target: Some(target),
        });
    }

    /// A synth-parameter lane, with points given in the parameter's own
    /// units (`set` writes one value into a patch).
    fn synth_lane(&mut self, track: TrackId, param: SynthParam, set: fn(&mut SynthState, f32), points: &[(i64, f32)]) {
        let base = self.patch(track).clone();
        let norm = |v: f32| {
            let mut s = base.clone();
            set(&mut s, v);
            param.norm(&s)
        };
        let points: Vec<(i64, f32)> = points.iter().map(|&(bar, v)| (bar, norm(v))).collect();
        self.lane(track, AutomationTarget::Synth(param), &points);
    }
}

/// Every MIDI note of `pattern` (16th, pitch, length in 16ths, velocity),
/// repeated for each bar in `bar_range`, relative to the clip's start.
fn repeat(bar_count: i64, per_bar: impl Fn(i64) -> Vec<(i64, u8, i64, u8)>) -> Vec<MidiNote> {
    (0..bar_count)
        .flat_map(|bar| {
            per_bar(bar).into_iter().map(move |(step, pitch, len, velocity)| MidiNote {
                start: bar * BAR + step * SIXTEENTH,
                length: len * SIXTEENTH,
                pitch,
                velocity,
            })
        })
        .collect()
}

fn house_bass() -> SynthState {
    let mut s = deep_rave_bass();
    s.name = "House Bass";
    s.filter.cutoff_hz = 380.0;
    s.filter.resonance = 0.3;
    s.filter.env_amount_oct = 2.6;
    s.filter_env = Envelope { attack_ms: 1.0, decay_ms: 160.0, sustain: 0.1, release_ms: 120.0 };
    s.amp_env = Envelope { attack_ms: 2.0, decay_ms: 220.0, sustain: 0.6, release_ms: 80.0 };
    s.output.glide_ms = 25.0;
    s
}

fn house_stab() -> SynthState {
    SynthState {
        name: "House Stab",
        voice_mode: VoiceMode::Poly,
        voices: 8,
        osc1: Oscillator { waveform: Waveform::Saw, octave: 0, knob_a_cents: 0.0, knob_b: 0.2, knob_c: 0.1, sync: false },
        osc2: Oscillator { waveform: Waveform::Square, octave: 0, knob_a_cents: 8.0, knob_b: 0.45, knob_c: 0.0, sync: false },
        mix: crate::synth::Mix { osc1_db: -4.0, osc2_db: -7.0, sub_db: -60.0, noise_db: -60.0 },
        filter: Filter {
            filter_type: FilterType::Lp24,
            cutoff_hz: 700.0,
            resonance: 0.35,
            drive_db: 2.0,
            env_amount_oct: 2.5,
            key_track: 0.4,
        },
        filter_env: Envelope { attack_ms: 1.0, decay_ms: 180.0, sustain: 0.0, release_ms: 150.0 },
        amp_env: Envelope { attack_ms: 1.0, decay_ms: 260.0, sustain: 0.0, release_ms: 160.0 },
        lfo1: Lfo { rate_label: "", rate_norm: 0.3, depth: 0.0, sync: false, target: LfoTarget::Cutoff, target_count: 1 },
        lfo2: Lfo { rate_label: "", rate_norm: 0.3, depth: 0.0, sync: false, target: LfoTarget::Pitch, target_count: 1 },
        output: crate::synth::Output { glide_ms: 1.0, volume_db: -6.0, meter_l: 0.0, meter_r: 0.0 },
        unison: Unison { voices: 2, detune_cents: 12.0, width: 0.6 },
        fx: Fx { chorus_depth: 0.4, chorus_mix: 0.25, reverb_size: 0.6, reverb_mix: 0.25 },
        held_notes: vec![],
    }
}

fn pluck_lead() -> SynthState {
    let mut s = seed_synth();
    s.name = "Pluck Lead";
    s.voice_mode = VoiceMode::Poly;
    s.osc1 = Oscillator { waveform: Waveform::Square, octave: 0, knob_a_cents: 0.0, knob_b: 0.3, knob_c: 0.05, sync: false };
    s.osc2 = Oscillator { waveform: Waveform::Saw, octave: 1, knob_a_cents: 5.0, knob_b: 0.5, knob_c: 0.0, sync: false };
    s.mix = crate::synth::Mix { osc1_db: -4.0, osc2_db: -10.0, sub_db: -60.0, noise_db: -60.0 };
    s.filter = Filter {
        filter_type: FilterType::Lp12,
        cutoff_hz: 1400.0,
        resonance: 0.25,
        drive_db: 0.0,
        env_amount_oct: 2.2,
        key_track: 0.6,
    };
    s.filter_env = Envelope { attack_ms: 1.0, decay_ms: 220.0, sustain: 0.1, release_ms: 200.0 };
    s.amp_env = Envelope { attack_ms: 1.0, decay_ms: 350.0, sustain: 0.2, release_ms: 250.0 };
    s.lfo1.depth = 0.0;
    s.lfo2 = Lfo { rate_label: "", rate_norm: 0.35, depth: 0.1, sync: false, target: LfoTarget::Pitch, target_count: 1 };
    s.output.glide_ms = 1.0;
    s.output.volume_db = -6.0;
    s.unison = Unison { voices: 2, detune_cents: 8.0, width: 0.5 };
    s.fx = Fx { chorus_depth: 0.3, chorus_mix: 0.2, reverb_size: 0.7, reverb_mix: 0.3 };
    s
}

fn wide_pad() -> SynthState {
    let mut s = soft_pad();
    s.name = "Wide Pad";
    s
}

fn add_effect(arr: &mut Arrangement, track: Option<TrackId>, effect: Effect) -> EffectNodeId {
    arr.fx_mut(track).expect("fx chain").push_at_end(effect)
}

/// Which drum parts play in a section.
#[derive(Clone, Copy)]
struct Parts {
    kick: bool,
    clap: bool,
    closed: bool,
    open: bool,
}

/// A one-bar beat from `parts`: kick on every beat, clap on 2 and 4,
/// closed hats on the "e" and "a", open hat on every "and".
fn drum_bar(parts: Parts) -> impl Fn(i64) -> Vec<(i64, u8, i64, u8)> {
    move |_bar: i64| {
        let mut hits = Vec::new();
        for beat in 0..4 {
            let at = beat * 4;
            if parts.kick {
                hits.push((at, KICK, 1, 127));
            }
            if parts.clap && beat % 2 == 1 {
                hits.push((at, CLAP, 1, 86));
            }
            if parts.closed {
                hits.push((at + 1, CLOSED_HAT, 1, 55));
                hits.push((at + 3, CLOSED_HAT, 1, 40));
            }
            if parts.open {
                hits.push((at + 2, OPEN_HAT, 1, 51));
            }
        }
        hits
    }
}

pub fn house_demo() -> Project {
    let mut b = Builder {
        arr: Arrangement::new(TempoMap::constant(BPM, TimeSignature::FOUR_FOUR)),
        instruments: Vec::new(),
    };

    // -- Drums: one Drum Kit track, one MIDI clip per section. ---------
    // Each clip is a one-bar pattern repeated; which parts play changes
    // with the section. Velocities set each sound's level.
    let drums = b.track("Drums", ClipColor::Coral, TrackKind::Midi, -6.0);
    b.arr.track_mut(drums).expect("drums").instrument = Some(Instrument::Drums);
    let pattern = drum_bar;
    let (kick, open, clap, closed) = (true, true, true, true);
    let sections: [(&str, i64, i64, Parts); 6] = [
        ("Intro", 0, 4, Parts { kick, clap: false, closed: false, open: false }),
        ("Intro", 4, 8, Parts { kick, clap: false, closed: false, open }),
        ("Build", 8, 16, Parts { kick, clap, closed: false, open }),
        ("Groove", 16, 32, Parts { kick, clap, closed, open }),
        ("Drop", 40, 56, Parts { kick, clap, closed, open }),
        ("Outro", 56, 64, Parts { kick, clap: false, closed: false, open }),
    ];
    for (name, start, end, parts) in sections {
        b.midi_clip(drums, name, start, end, 1, repeat(1, pattern(parts)));
    }
    // A clap roll into the drop, getting louder.
    let roll = (0..16).map(|step| (step, CLAP, 1, (40 + step * 3) as u8)).collect::<Vec<_>>();
    b.midi_clip(drums, "Roll", 39, 40, 1, repeat(1, move |_| roll.clone()));

    // -- Carve tracks. -------------------------------------------------
    let bass = b.synth_track("Bass", ClipColor::Blue, -7.0, house_bass());
    let stabs = b.synth_track("Stabs", ClipColor::Violet, -12.0, house_stab());
    let pad = b.synth_track("Pad", ClipColor::Pink, -10.0, wide_pad());
    let lead = b.synth_track("Lead", ClipColor::Amber, -12.0, pluck_lead());

    // Bass: root on every off-beat, up an octave on the last one.
    let bass_bar = |bar: i64| {
        let root = CHORDS[(bar % 4) as usize].0;
        (0..4).map(|beat| (beat * 4 + 2, if beat == 3 { root + 12 } else { root }, 1, if beat == 0 { 118 } else { 96 })).collect()
    };
    b.midi_clip(bass, "Bassline", 8, 32, 4, repeat(4, bass_bar));
    b.midi_clip(bass, "Bassline", 40, 64, 4, repeat(4, bass_bar));

    // Stabs: a syncopated chord hit pattern.
    let stab_bar = |bar: i64| {
        let chord = CHORDS[(bar % 4) as usize].1;
        [(2, 2), (6, 2), (10, 2), (13, 1)]
            .iter()
            .flat_map(|&(step, len)| chord.iter().map(move |&p| (step, p, len, if step == 2 { 110 } else { 92 })))
            .collect()
    };
    b.midi_clip(stabs, "Stabs", 16, 40, 4, repeat(4, stab_bar));
    b.midi_clip(stabs, "Stabs", 40, 56, 4, repeat(4, stab_bar));

    // Pad: one held chord per bar, the whole song.
    let pad_bar = |bar: i64| CHORDS[(bar % 4) as usize].2.iter().map(|&p| (0, p, 16, 84)).collect();
    b.midi_clip(pad, "Pad", 0, BARS, 4, repeat(4, pad_bar));

    // Lead: the hook through the drop.
    let lead_bar = |bar: i64| HOOK[(bar % 4) as usize].iter().map(|&(step, p, len)| (step, p, len, 100)).collect();
    b.midi_clip(lead, "Hook", 40, 56, 4, repeat(4, lead_bar));

    // -- Effects. ------------------------------------------------------
    add_effect(
        &mut b.arr,
        Some(bass),
        Effect::Compressor(CompressorState { threshold_db: -20.0, ratio: 4.0, attack_ms: 5.0, release_ms: 120.0, makeup_db: 3.0 }),
    );
    // The pad's EQ is a resonant peak that the breakdown sweeps upward.
    let pad_eq = add_effect(&mut b.arr, Some(pad), Effect::Eq(EqState::bell(400.0, 8.0, 2.0)));
    // Glue on the master bus.
    add_effect(
        &mut b.arr,
        None,
        Effect::Compressor(CompressorState { threshold_db: -12.0, ratio: 2.0, attack_ms: 20.0, release_ms: 200.0, makeup_db: 0.0 }),
    );

    // -- Automation. ---------------------------------------------------
    // Pad volume: fades in over the intro, out over the outro.
    let fader = |db: f32| gain_db_to_fader_pos(db);
    b.lane(pad, AutomationTarget::TrackGain, &[(0, fader(-40.0)), (8, fader(-13.0)), (56, fader(-13.0)), (BARS, fader(-60.0))]);

    // Pad EQ: the breakdown's sweep - the peak rises from 400 Hz to 5 kHz.
    let eq_norm = |hz: f32| {
        let e = Effect::Eq(EqState::bell(hz, 8.0, 2.0));
        EffectParam::EqFreq.norm(&e).unwrap_or(0.0)
    };
    b.lane(
        pad,
        AutomationTarget::Effect { node: pad_eq, param: EffectParam::EqFreq },
        &[(0, eq_norm(400.0)), (32, eq_norm(400.0)), (40, eq_norm(5000.0)), (41, eq_norm(400.0))],
    );

    // Stabs cutoff: opens through the groove, closes for the breakdown and
    // rises back into the drop.
    b.synth_lane(
        stabs,
        SynthParam::Cutoff,
        |s, hz| s.filter.cutoff_hz = hz,
        &[(16, 350.0), (32, 2400.0), (33, 500.0), (40, 3000.0), (56, 3000.0)],
    );

    // Bass cutoff: closes down over the outro.
    b.synth_lane(bass, SynthParam::Cutoff, |s, hz| s.filter.cutoff_hz = hz, &[(8, 380.0), (56, 380.0), (BARS, 90.0)]);

    // Lead reverb: washes out at the end of the drop.
    b.synth_lane(lead, SynthParam::ReverbMix, |s, v| s.fx.reverb_mix = v, &[(40, 0.3), (54, 0.3), (56, 0.8)]);

    // -- Markers. ------------------------------------------------------
    for (name, bar) in SECTIONS {
        let id = b.arr.alloc_id();
        b.arr.markers.push(Marker { id, position: bar * BAR, name: name.into() });
    }

    Project { arrangement: b.arr, instruments: b.instruments, synth: None }
}

/// The finished demo songs the sidebar offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DemoSong {
    House,
    Bhairav,
}

impl DemoSong {
    pub const ALL: [DemoSong; 2] = [DemoSong::House, DemoSong::Bhairav];

    /// The project's name.
    pub fn name(self) -> &'static str {
        match self {
            DemoSong::House => NAME,
            DemoSong::Bhairav => BHAIRAV_NAME,
        }
    }

    /// How the sidebar lists it.
    pub fn label(self) -> &'static str {
        match self {
            DemoSong::House => "House demo \u{b7} 2 min",
            DemoSong::Bhairav => "Bhairav rave \u{b7} 2 min",
        }
    }

    pub fn project(self) -> Project {
        match self {
            DemoSong::House => house_demo(),
            DemoSong::Bhairav => bhairav_demo(),
        }
    }

    /// The key it's written in: its root (0 = C) and the name of its
    /// scale in `theory::SCALE_PRESETS`.
    pub fn key(self) -> (u8, &'static str) {
        match self {
            DemoSong::House => (9, "Natural minor"),
            DemoSong::Bhairav => (0, "Raga Bhairav"),
        }
    }
}

// -- Bhairav rave --------------------------------------------------------

pub const BHAIRAV_NAME: &str = "Bhairav Rave";

/// Raag Bhairav with Sa on C: Sa re Ga ma Pa dha Ni = C Db E F G Ab B.
/// (The same set as the "Raga Bhairav" scale preset.)
mod sargam {
    pub const SA: u8 = 0;
    pub const RE: u8 = 1; // komal (flat) re
    pub const GA: u8 = 4;
    pub const MA: u8 = 5;
    pub const PA: u8 = 7;
    pub const DHA: u8 = 8; // komal (flat) dha
    pub const NI: u8 = 11;
}

/// A tanpura-like drone: soft saws with a slow swell and a long tail,
/// detuned and chorused so the held Sa-Pa shimmers.
fn tanpura() -> SynthState {
    let mut s = soft_pad();
    s.name = "Tanpura drone";
    s.filter.cutoff_hz = 1400.0;
    s.filter.resonance = 0.25;
    s.amp_env = Envelope { attack_ms: 900.0, decay_ms: 1500.0, sustain: 0.9, release_ms: 1800.0 };
    s.unison = Unison { voices: 3, detune_cents: 10.0, width: 0.8 };
    s.fx = Fx { chorus_depth: 0.6, chorus_mix: 0.4, reverb_size: 0.85, reverb_mix: 0.4 };
    s.output.volume_db = -8.0;
    s
}

/// A nasal, reedy lead in the spirit of a shehnai: square and saw through
/// a band-pass filter, mono with glide for the meend (slides between
/// notes), and vibrato.
fn reed_lead() -> SynthState {
    let mut s = seed_synth();
    s.name = "Reed lead";
    s.voice_mode = VoiceMode::Mono;
    s.osc1 = Oscillator { waveform: Waveform::Square, octave: 0, knob_a_cents: 0.0, knob_b: 0.3, knob_c: 0.05, sync: false };
    s.osc2 = Oscillator { waveform: Waveform::Saw, octave: 0, knob_a_cents: 6.0, knob_b: 0.5, knob_c: 0.0, sync: false };
    s.mix = crate::synth::Mix { osc1_db: -4.0, osc2_db: -9.0, sub_db: -60.0, noise_db: -34.0 };
    s.filter = Filter {
        filter_type: FilterType::Bp,
        cutoff_hz: 1700.0,
        resonance: 0.4,
        drive_db: 3.0,
        env_amount_oct: 0.8,
        key_track: 0.7,
    };
    s.filter_env = Envelope { attack_ms: 20.0, decay_ms: 300.0, sustain: 0.5, release_ms: 200.0 };
    s.amp_env = Envelope { attack_ms: 35.0, decay_ms: 300.0, sustain: 0.85, release_ms: 220.0 };
    s.lfo1.depth = 0.0;
    // ~5.5 Hz on pitch: the singer's vibrato.
    s.lfo2 = Lfo { rate_label: "", rate_norm: 0.785, depth: 0.16, sync: false, target: LfoTarget::Pitch, target_count: 1 };
    s.output.glide_ms = 70.0;
    // The band-pass filter that makes it nasal also takes a lot of level.
    s.output.volume_db = 0.0;
    s.unison = Unison::default();
    s.fx = Fx { chorus_depth: 0.3, chorus_mix: 0.1, reverb_size: 0.7, reverb_mix: 0.3 };
    s
}

/// A song in Raag Bhairav at 128 BPM, 64 bars: Deep Rave Bass rolling
/// round Sa and komal re, a tanpura drone on Sa and Pa, and a reed lead
/// singing Bhairav's signature phrase (Ga ma dha Pa, Ga ma re Sa).
pub fn bhairav_demo() -> Project {
    use sargam::*;
    let mut b = Builder {
        arr: Arrangement::new(TempoMap::constant(BPM, TimeSignature::FOUR_FOUR)),
        instruments: Vec::new(),
    };
    // Sa in each register.
    let bass_sa: u8 = 36; // C2 (Deep Rave Bass plays an octave down)
    let drone_sa: u8 = 48; // C3
    let lead_sa: u8 = 72; // C5

    // -- Drums --------------------------------------------------------
    let drums = b.track("Drums", ClipColor::Coral, TrackKind::Midi, -7.5);
    b.arr.track_mut(drums).expect("drums").instrument = Some(Instrument::Drums);
    let (kick, open, clap, closed) = (true, true, true, true);
    let sections: [(&str, i64, i64, Parts); 5] = [
        ("Intro", 0, 8, Parts { kick: false, clap: false, closed, open: false }),
        ("Build", 8, 16, Parts { kick, clap: false, closed, open: false }),
        ("Groove", 16, 32, Parts { kick, clap, closed, open }),
        ("Drop", 40, 56, Parts { kick, clap, closed, open }),
        ("Outro", 56, 64, Parts { kick, clap: false, closed: false, open }),
    ];
    for (name, start, end, parts) in sections {
        b.midi_clip(drums, name, start, end, 1, repeat(1, drum_bar(parts)));
    }
    let roll = (0..16).map(|step| (step, CLAP, 1, (40 + step * 3) as u8)).collect::<Vec<_>>();
    b.midi_clip(drums, "Roll", 39, 40, 1, repeat(1, move |_| roll.clone()));

    // -- Bass: Deep Rave Bass rolling round Sa and komal re -----------
    // Three 16ths in each beat's gaps (the kick keeps the downbeats), the
    // last one of a beat leaning on re or Ga and gliding home to Sa -
    // the Sa-re pull that gives Bhairav its gravity. Bar 2 answers with
    // dha and Pa.
    let bass = b.synth_track("Bass", ClipColor::Blue, -7.0, deep_rave_bass());
    let n = |degree: u8| bass_sa + degree;
    let bass_bar = move |bar: i64| {
        let last = if bar % 2 == 0 {
            [n(RE), n(GA), n(RE), n(MA)]
        } else {
            [n(RE), n(GA), n(DHA), n(PA)]
        };
        (0..4)
            .flat_map(|beat| {
                let at = beat * 4;
                [(at + 1, n(SA), 1, 118), (at + 2, n(SA), 1, 92), (at + 3, last[beat as usize], 1, 100)]
            })
            .collect()
    };
    b.midi_clip(bass, "Bassline", 8, 32, 2, repeat(2, bass_bar));
    b.midi_clip(bass, "Bassline", 40, 64, 2, repeat(2, bass_bar));

    // -- Drone: Sa, Pa and high Sa, held for the whole song -----------
    let drone = b.synth_track("Tanpura", ClipColor::Amber, -10.0, tanpura());
    let drone_notes = move |_bar: i64| vec![(0, drone_sa + SA, 64, 80), (0, drone_sa + PA, 64, 72), (0, drone_sa + 12, 64, 64)];
    b.midi_clip(drone, "Drone", 0, BARS, 4, repeat(1, drone_notes));

    // -- Lead: Bhairav's phrase ---------------------------------------
    // Bar 1: Ga ma dha Pa. Bar 2: Ga ma re Sa (the pakad, resolving home).
    // Bar 3: the scale rising, Sa to high Sa. Bar 4: coming down, Sa' Ni dha Pa.
    let lead = b.synth_track("Reed", ClipColor::Violet, -3.0, reed_lead());
    let l = |degree: u8| lead_sa + degree;
    let phrase: [&[(i64, u8, i64)]; 4] = [
        &[(0, l(GA), 3), (4, l(MA), 3), (8, l(DHA), 6), (14, l(PA), 2)],
        &[(0, l(GA), 3), (4, l(MA), 3), (8, l(RE), 5), (13, l(SA), 3)],
        &[(0, l(SA), 2), (2, l(RE), 2), (4, l(GA), 2), (6, l(MA), 2), (8, l(PA), 2), (10, l(DHA), 2), (12, l(NI), 2), (14, l(SA) + 12, 2)],
        &[(0, l(SA) + 12, 6), (6, l(NI), 2), (8, l(DHA), 4), (12, l(PA), 4)],
    ];
    let lead_bar = move |bar: i64| phrase[(bar % 4) as usize].iter().map(|&(at, p, len)| (at, p, len, 100)).collect();
    b.midi_clip(lead, "Pakad", 32, 40, 4, repeat(4, lead_bar));
    b.midi_clip(lead, "Pakad", 44, 56, 4, repeat(4, lead_bar));

    // -- Effects ------------------------------------------------------
    add_effect(
        &mut b.arr,
        None,
        Effect::Compressor(CompressorState { threshold_db: -12.0, ratio: 2.0, attack_ms: 20.0, release_ms: 200.0, makeup_db: 0.0 }),
    );

    // -- Automation ---------------------------------------------------
    // The drone swells in and fades out; the bass filter opens through
    // the build; the reed's reverb blooms at the end of the break.
    let fader = |db: f32| gain_db_to_fader_pos(db);
    b.lane(drone, AutomationTarget::TrackGain, &[(0, fader(-40.0)), (6, fader(-10.0)), (56, fader(-10.0)), (BARS, fader(-60.0))]);
    b.synth_lane(bass, SynthParam::Cutoff, |s, hz| s.filter.cutoff_hz = hz, &[(8, 120.0), (16, 260.0), (32, 260.0), (40, 320.0)]);
    b.synth_lane(lead, SynthParam::ReverbMix, |s, v| s.fx.reverb_mix = v, &[(32, 0.3), (38, 0.3), (40, 0.7), (44, 0.3)]);

    for (name, bar) in [("Intro", 0), ("Build", 8), ("Groove", 16), ("Break", 32), ("Drop", 40), ("Outro", 56)] {
        let id = b.arr.alloc_id();
        b.arr.markers.push(Marker { id, position: bar * BAR, name: name.into() });
    }

    Project { arrangement: b.arr, instruments: b.instruments, synth: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_minutes_long() {
        let p = house_demo();
        let end = p.arrangement.clips.iter().map(|c| c.end()).max().unwrap();
        assert!(end <= BARS * BAR + PPQ, "clips run past the end: {end}");
        let seconds = p.arrangement.tempo_map.ticks_to_seconds(BARS * BAR);
        assert!((seconds - 120.0).abs() < 1e-6, "{seconds}");
    }

    #[test]
    fn every_synth_track_has_a_patch_and_ids_are_unique() {
        let p = house_demo();
        for t in &p.arrangement.tracks {
            let is_carve = t.instrument == Some(Instrument::Carve);
            assert_eq!(is_carve, p.instruments.iter().any(|(id, _)| *id == t.id), "{}", t.name);
        }
        let arr = &p.arrangement;
        let mut ids: Vec<u32> = arr.tracks.iter().map(|t| t.id).collect();
        ids.extend(arr.clips.iter().map(|c| c.id));
        ids.extend(arr.automation.iter().map(|a| a.id));
        ids.extend(arr.markers.iter().map(|m| m.id));
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len());
    }

    #[test]
    fn every_lane_is_live_and_labelled() {
        let p = house_demo();
        let arr = &p.arrangement;
        assert_eq!(arr.automation.len(), 5);
        for lane in &arr.automation {
            let target = lane.target.expect("typed lane");
            assert!(arr.target_label(lane.track, target).is_some(), "{} targets nothing", lane.parameter_name);
            assert!(!lane.parameter_name.is_empty());
            assert!(lane.breakpoints.windows(2).all(|w| w[0].tick <= w[1].tick));
            assert!(lane.breakpoints.iter().all(|b| (0.0..=1.0).contains(&b.value)));
        }
    }

    #[test]
    fn stab_cutoff_lane_hits_its_values() {
        let p = house_demo();
        let arr = &p.arrangement;
        let stabs = arr.tracks.iter().find(|t| t.name == "Stabs").unwrap().id;
        let mut patch = p.instruments.iter().find(|(id, _)| *id == stabs).unwrap().1.clone();
        arr.apply_synth_automation(stabs, 32 * BAR, &mut patch);
        assert!((patch.filter.cutoff_hz - 2400.0).abs() < 1.0, "{}", patch.filter.cutoff_hz);
    }

    #[test]
    fn round_trips_through_save_format() {
        let p = house_demo();
        let json = serde_json::to_string(&p).unwrap();
        let back: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(back.arrangement.clips.len(), p.arrangement.clips.len());
        assert_eq!(back.instruments.len(), 4);
    }

    #[test]
    fn looped_clips_play_every_hit() {
        let p = house_demo();
        let arr = &p.arrangement;
        let drums = arr.tracks.iter().find(|t| t.name == "Drums").unwrap().id;
        let hits: usize = arr.clips.iter().filter(|c| c.track == drums).map(|c| c.played_notes().len()).sum();
        // kick 56 bars x4, clap 40 x2 + 16 roll, closed hats 32 x8, open hats 52 x4
        assert_eq!(hits, 56 * 4 + 40 * 2 + 16 + 32 * 8 + 52 * 4);
        let groove = arr.clips.iter().find(|c| c.name == "Groove").unwrap();
        assert!(matches!(groove.content, ClipContent::Midi { loop_len: Some(l), .. } if l == BAR));
    }

    #[test]
    fn bhairav_stays_in_raag_bhairav() {
        let p = bhairav_demo();
        let arr = &p.arrangement;
        let drums = arr.tracks.iter().find(|t| t.name == "Drums").unwrap().id;
        // Every pitched note is Sa re Ga ma Pa dha Ni (C Db E F G Ab B).
        for clip in arr.clips.iter().filter(|c| c.track != drums) {
            for note in clip.played_notes() {
                assert!([0, 1, 4, 5, 7, 8, 11].contains(&(note.pitch % 12)), "{} has {}", clip.name, note.pitch);
            }
        }
        let bass = arr.tracks.iter().find(|t| t.name == "Bass").unwrap().id;
        let patch = &p.instruments.iter().find(|(id, _)| *id == bass).unwrap().1;
        assert_eq!(patch.name, "Deep Rave Bass");
        let end = arr.clips.iter().map(|c| c.end()).max().unwrap();
        assert_eq!(end, BARS * BAR);
    }

    #[test]
    fn bhairav_ids_are_unique_and_lanes_live() {
        let p = bhairav_demo();
        let arr = &p.arrangement;
        let mut ids: Vec<u32> = arr.tracks.iter().map(|t| t.id).collect();
        ids.extend(arr.clips.iter().map(|c| c.id));
        ids.extend(arr.automation.iter().map(|a| a.id));
        ids.extend(arr.markers.iter().map(|m| m.id));
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len());
        for lane in &arr.automation {
            assert!(arr.target_label(lane.track, lane.target.unwrap()).is_some());
            assert!(lane.breakpoints.iter().all(|b| (0.0..=1.0).contains(&b.value)));
        }
    }

    #[test]
    fn every_demo_names_a_real_scale() {
        for song in DemoSong::ALL {
            let (_, scale) = song.key();
            assert!(crate::theory::SCALE_PRESETS.iter().any(|p| p.name == scale), "{scale}");
            assert!(!song.project().arrangement.tracks.is_empty());
        }
    }
}
