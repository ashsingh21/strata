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

    fn midi_clip(&mut self, track: TrackId, name: &str, start_bar: i64, end_bar: i64, notes: Vec<MidiNote>) {
        let id = self.arr.alloc_id();
        self.arr.clips.push(Clip {
            id,
            track,
            start: start_bar * BAR,
            length: (end_bar - start_bar) * BAR,
            name: name.into(),
            content: ClipContent::Midi { notes },
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
    #[derive(Clone, Copy)]
    struct Parts {
        kick: bool,
        clap: bool,
        closed: bool,
        open: bool,
    }
    let pattern = move |parts: Parts| {
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
    };
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
        b.midi_clip(drums, name, start, end, repeat(end - start, pattern(parts)));
    }
    // A clap roll into the drop, getting louder.
    let roll = (0..16).map(|step| (step, CLAP, 1, (40 + step * 3) as u8)).collect::<Vec<_>>();
    b.midi_clip(drums, "Roll", 39, 40, repeat(1, move |_| roll.clone()));

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
    b.midi_clip(bass, "Bassline", 8, 32, repeat(24, bass_bar));
    b.midi_clip(bass, "Bassline", 40, 64, repeat(24, bass_bar));

    // Stabs: a syncopated chord hit pattern.
    let stab_bar = |bar: i64| {
        let chord = CHORDS[(bar % 4) as usize].1;
        [(2, 2), (6, 2), (10, 2), (13, 1)]
            .iter()
            .flat_map(|&(step, len)| chord.iter().map(move |&p| (step, p, len, if step == 2 { 110 } else { 92 })))
            .collect()
    };
    b.midi_clip(stabs, "Stabs", 16, 40, repeat(24, stab_bar));
    b.midi_clip(stabs, "Stabs", 40, 56, repeat(16, stab_bar));

    // Pad: one held chord per bar, the whole song.
    let pad_bar = |bar: i64| CHORDS[(bar % 4) as usize].2.iter().map(|&p| (0, p, 16, 84)).collect();
    b.midi_clip(pad, "Pad", 0, BARS, repeat(BARS, pad_bar));

    // Lead: the hook through the drop.
    let lead_bar = |bar: i64| HOOK[(bar % 4) as usize].iter().map(|&(step, p, len)| (step, p, len, 100)).collect();
    b.midi_clip(lead, "Hook", 40, 56, repeat(16, lead_bar));

    // -- Effects. ------------------------------------------------------
    add_effect(
        &mut b.arr,
        Some(bass),
        Effect::Compressor(CompressorState { threshold_db: -20.0, ratio: 4.0, attack_ms: 5.0, release_ms: 120.0, makeup_db: 3.0 }),
    );
    // The pad's EQ is a resonant peak that the breakdown sweeps upward.
    let pad_eq = add_effect(&mut b.arr, Some(pad), Effect::Eq(EqState { freq_hz: 400.0, gain_db: 8.0, q: 2.0 }));
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
        let e = Effect::Eq(EqState { freq_hz: hz, gain_db: 8.0, q: 2.0 });
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
}
