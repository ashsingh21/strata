//! "Show me": each action step done for you, as the walk-through tests
//! do it - so a stuck learner sees (and hears) the step, then can undo it
//! and try it themselves. Every entry is checked against its step by
//! `course::tests::show_me_does_every_step`.

use shared::arrangement::{
    Clip, ClipContent, Command, CompressorState, Effect, EqState, Instrument, MidiNote, Ticks, Track, TrackKind, DEFAULT_TRACK_HEIGHT,
    EQ_LOW_CUT, PPQ,
};
use shared::drums::{CLAP, CLOSED_HAT, KICK, OPEN_HAT, SNARE};
use shared::lessons::*;
use shared::synth::{FilterType, LfoTarget, SynthParam, VoiceMode, Waveform};

use super::course::MARKER_BARS;
use super::Snapshot;

pub type Show = Box<dyn Fn(&mut Snapshot)>;

/// Runs `show` on `s`, false if it couldn't (the learner's project has
/// wandered from what the step expects - a deleted track, say). Never
/// takes the app down with it.
pub fn run(show: &dyn Fn(&mut Snapshot), s: &mut Snapshot) -> bool {
    let mut attempt = s.clone();
    let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| show(&mut attempt))).is_ok();
    if ok {
        *s = attempt;
    }
    ok
}

fn b(f: impl Fn(&mut Snapshot) + 'static) -> Show {
    Box::new(f)
}

const fn bars(n: i64) -> Ticks {
    n * BAR
}

/// The "and" of every beat.
const OFFBEATS: [Ticks; 4] = [PPQ / 2, PPQ + PPQ / 2, 2 * PPQ + PPQ / 2, 3 * PPQ + PPQ / 2];
/// The "e" and "a" of every beat, in ticks.
const SIXTEENTHS_E_AND_A: [Ticks; 8] = [
    PPQ / 4, 3 * PPQ / 4, PPQ + PPQ / 4, PPQ + 3 * PPQ / 4, 2 * PPQ + PPQ / 4, 2 * PPQ + 3 * PPQ / 4, 3 * PPQ + PPQ / 4, 3 * PPQ + 3 * PPQ / 4,
];

/// One entry per action step of `lesson`, in order (none for an unknown
/// lesson).
pub fn steps(lesson: &str) -> Vec<Show> {
    match lesson {
        FIRST_BEAT => {
            vec![
                b(move |s| add_track(s, "Drums", Some(Instrument::Drums))),
                b(draw_clip),
                b(move |s| add_notes(s, KICK, &[0, PPQ, 2 * PPQ, 3 * PPQ])),
                b(move |s| s.playing = true),
                b(move |s| add_notes(s, CLAP, &[PPQ, 3 * PPQ])),
                b(move |s| add_notes(s, OPEN_HAT, &OFFBEATS)),
                b(stretch),
            ]
        }
        BASSLINE => {
            vec![
                b(move |s| add_midi_track(s, "MIDI 1")),
                b(move |s| s.synth = shared::synth::deep_rave_bass()),
                b(move |s| s.synth.held_notes.push(48)),
                b(draw_clip),
                b(move |s| add_notes(s, BASS_NOTE, &OFFBEATS)),
                b(move |s| s.playing = true),
                b(stretch),
            ]
        }
        CHORDS => {
            vec![
                b(move |s| add_midi_track(s, "MIDI 1")),
                b(move |s| s.synth = shared::synth::PRESETS.iter().find(|p| p.0 == "Soft Pad").unwrap().1()),
                b(draw_clip),
                b(move |s| {
                    let mut clip = s.arrangement.clip(last_clip(s)).unwrap().clone();
                    clip.length = 2 * BAR;
                    clip.content = ClipContent::Midi { notes: vec![], loop_len: Some(2 * BAR), link: None };
                    Command::ReplaceClip { clip: Box::new(clip) }.apply(&mut s.arrangement);
                }),
                b(move |s| {
                    for p in [57, 60, 64] {
                        add_notes(s, p, &[0, 2 * PPQ]);
                    }
                }),
                b(move |s| {
                    for p in [60, 64, 67] {
                        add_notes(s, p, &[4 * PPQ, 6 * PPQ]);
                    }
                }),
                b(move |s| s.playing = true),
            ]
        }
        CARVE_WAVES => {
            vec![
                b(play),
                b(move |s| s.synth.osc1.waveform = Waveform::Square),
                b(move |s| s.synth.osc1.waveform = Waveform::Triangle),
                b(move |s| s.synth.osc1.waveform = Waveform::Sine),
                b(move |s| s.synth.osc1.waveform = Waveform::Saw),
            ]
        }
        CARVE_MIX => {
            vec![
                b(play),
                b(move |s| s.synth.mix.osc2_db = -8.0),
                b(move |s| s.synth.osc2.knob_a_cents = 10.0),
                b(move |s| s.synth.osc2.octave = 1),
                b(move |s| s.synth.osc2.waveform = Waveform::Square),
                b(move |s| s.synth.mix.sub_db = -8.0),
                b(move |s| s.synth.mix.noise_db = -30.0),
                b(move |s| s.synth.mix.osc1_db = -12.0),
            ]
        }
        CARVE_FILTER => {
            vec![
                b(play),
                b(move |s| s.synth.filter.cutoff_hz = 300.0),
                b(move |s| s.synth.filter.resonance = 0.8),
                b(move |s| s.synth.filter.cutoff_hz = 4000.0),
                b(move |s| s.synth.filter.filter_type = FilterType::Hp),
                b(move |s| s.synth.filter.filter_type = FilterType::Lp24),
            ]
        }
        CARVE_ENVELOPES => {
            vec![
                b(play),
                b(move |s| s.synth.amp_env.sustain = 0.0),
                b(move |s| s.synth.amp_env.decay_ms = 120.0),
                b(move |s| s.synth.filter.cutoff_hz = 1000.0),
                b(move |s| s.synth.filter.env_amount_oct = 3.5),
                b(move |s| s.synth.filter_env.decay_ms = 120.0),
                b(move |s| s.synth.amp_env.attack_ms = 400.0),
            ]
        }
        CARVE_MOVEMENT => {
            vec![
                b(play),
                b(move |s| s.synth.filter.cutoff_hz = 700.0),
                b(move |s| s.synth.lfo1.target = LfoTarget::Cutoff),
                b(move |s| s.synth.lfo1.depth = 0.7),
                b(move |s| knob(s, SynthParam::Lfo1Rate, hz_norm(0.5))),
                b(move |s| knob(s, SynthParam::Lfo1Rate, hz_norm(6.0))),
                b(move |s| s.synth.unison.voices = 3),
                b(move |s| s.synth.fx.reverb_mix = 0.4),
            ]
        }
        RECIPE_BASS => {
            vec![
                b(play),
                b(move |s| s.synth.osc1.octave = -1),
                b(move |s| s.synth.mix.sub_db = -6.0),
                b(move |s| s.synth.filter.cutoff_hz = 250.0),
                b(move |s| {
                    s.synth.filter.env_amount_oct = 2.5;
                    s.synth.filter_env.decay_ms = 200.0;
                }),
                b(move |s| s.synth.filter.drive_db = 9.0),
                b(move |s| {
                    s.synth.voice_mode = VoiceMode::Mono;
                    s.synth.output.glide_ms = 50.0;
                }),
            ]
        }
        RECIPE_PAD => {
            vec![
                b(play),
                b(move |s| {
                    s.synth.mix.osc2_db = -6.0;
                    s.synth.osc2.knob_a_cents = 9.0;
                }),
                b(move |s| {
                    s.synth.filter.cutoff_hz = 900.0;
                    s.synth.filter.resonance = 0.1;
                }),
                b(move |s| {
                    s.synth.amp_env.attack_ms = 500.0;
                    s.synth.amp_env.release_ms = 1200.0;
                }),
                b(move |s| {
                    s.synth.filter.env_amount_oct = 1.5;
                    s.synth.filter_env.attack_ms = 800.0;
                }),
                b(move |s| {
                    s.synth.unison.voices = 3;
                    s.synth.unison.detune_cents = 18.0;
                }),
                b(move |s| {
                    s.synth.fx.chorus_mix = 0.35;
                    s.synth.fx.reverb_mix = 0.35;
                    s.synth.fx.reverb_size = 0.8;
                }),
            ]
        }
        RECIPE_TANPURA => {
            vec![
                b(play),
                b(move |s| {
                    s.synth.amp_env.attack_ms = 2.0;
                    s.synth.amp_env.decay_ms = 1900.0;
                    s.synth.amp_env.sustain = 0.4;
                    s.synth.amp_env.release_ms = 1800.0;
                }),
                b(move |s| {
                    s.synth.filter.cutoff_hz = 700.0;
                    s.synth.filter.resonance = 0.4;
                }),
                b(move |s| {
                    s.synth.filter.env_amount_oct = 2.0;
                    s.synth.filter_env.attack_ms = 500.0;
                    s.synth.filter_env.decay_ms = 1500.0;
                }),
                b(move |s| s.synth.filter.drive_db = 8.0),
                b(move |s| {
                    s.synth.lfo1.target = LfoTarget::Cutoff;
                    knob(s, SynthParam::Lfo1Rate, hz_norm(0.3));
                    s.synth.lfo1.depth = 0.3;
                }),
                b(move |s| {
                    s.synth.fx.chorus_mix = 0.3;
                    s.synth.fx.reverb_mix = 0.45;
                    s.synth.fx.reverb_size = 0.85;
                }),
            ]
        }
        RECIPE_REED => {
            vec![
                b(play),
                b(move |s| {
                    s.synth.osc1.waveform = Waveform::Square;
                    s.synth.mix.osc2_db = -9.0;
                }),
                b(move |s| {
                    s.synth.filter.filter_type = FilterType::Bp;
                    s.synth.filter.cutoff_hz = 1700.0;
                    s.synth.filter.resonance = 0.4;
                }),
                b(move |s| s.synth.mix.noise_db = -32.0),
                b(move |s| s.synth.amp_env.attack_ms = 35.0),
                b(move |s| {
                    s.synth.voice_mode = VoiceMode::Mono;
                    s.synth.output.glide_ms = 70.0;
                }),
                b(move |s| {
                    knob(s, SynthParam::Lfo2Rate, hz_norm(5.5));
                    s.synth.lfo2.depth = 0.15;
                }),
                b(move |s| s.synth.fx.reverb_mix = 0.3),
            ]
        }
        RECIPE_FLUTE => {
            vec![
                b(play),
                b(move |s| s.synth.osc1.waveform = Waveform::Triangle),
                b(move |s| s.synth.mix.noise_db = -26.0),
                b(move |s| s.synth.filter.cutoff_hz = 2500.0),
                b(move |s| s.synth.amp_env.attack_ms = 80.0),
                b(move |s| {
                    knob(s, SynthParam::Lfo2Rate, hz_norm(5.0));
                    s.synth.lfo2.depth = 0.2;
                }),
                b(move |s| s.synth.fx.reverb_mix = 0.3),
            ]
        }
        RECIPE_HARP => {
            vec![
                b(play),
                b(move |s| {
                    s.synth.osc2.waveform = Waveform::Square;
                    s.synth.osc2.octave = 1;
                    s.synth.mix.osc2_db = -10.0;
                }),
                b(move |s| {
                    s.synth.amp_env.attack_ms = 1.0;
                    s.synth.amp_env.sustain = 0.0;
                    s.synth.amp_env.decay_ms = 1000.0;
                }),
                b(move |s| s.synth.amp_env.release_ms = 1000.0),
                b(move |s| {
                    s.synth.filter.cutoff_hz = 1000.0;
                    s.synth.filter.env_amount_oct = 3.0;
                    s.synth.filter_env.decay_ms = 300.0;
                }),
                b(move |s| s.synth.fx.chorus_mix = 0.3),
                b(move |s| {
                    s.synth.fx.reverb_mix = 0.4;
                    s.synth.fx.reverb_size = 0.8;
                }),
            ]
        }
        RECIPE_LEAD => {
            vec![
                b(play),
                b(move |s| {
                    s.synth.osc1.waveform = Waveform::Square;
                    s.synth.mix.osc2_db = -8.0;
                    s.synth.osc2.knob_a_cents = 7.0;
                }),
                b(move |s| {
                    s.synth.filter.cutoff_hz = 3000.0;
                    s.synth.filter.resonance = 0.3;
                }),
                b(move |s| {
                    s.synth.voice_mode = VoiceMode::Mono;
                    s.synth.output.glide_ms = 60.0;
                }),
                b(move |s| {
                    knob(s, SynthParam::Lfo2Rate, hz_norm(5.0));
                    s.synth.lfo2.depth = 0.15;
                }),
                b(move |s| {
                    s.synth.fx.chorus_mix = 0.2;
                    s.synth.fx.reverb_mix = 0.25;
                }),
            ]
        }
        ARRANGE_HOUSE => {
            vec![
                b(play),
                b(move |s| s.playhead = bars(9)),
                b(move |s| s.playhead = bars(33)),
                b(move |s| set_track(s, "Drums", |t| t.mute = true)),
                b(move |s| set_track(s, "Drums", |t| t.mute = false)),
                b(move |s| s.playhead = bars(41)),
                b(move |s| {
                    let id = s.arrangement.alloc_id();
                    s.arrangement.markers.push(shared::arrangement::Marker { id, position: bars(24), name: "Mine".into() });
                }),
            ]
        }
        ARRANGE_BHAIRAV => {
            vec![
                b(play),
                b(move |s| set_track(s, "Tanpura", |t| t.solo = true)),
                b(move |s| set_track(s, "Tanpura", |t| t.solo = false)),
                b(move |s| s.playhead = bars(9)),
                b(move |s| s.playhead = bars(33)),
                b(move |s| set_track(s, "Tanpura", |t| t.mute = true)),
                b(move |s| set_track(s, "Tanpura", |t| t.mute = false)),
                b(move |s| s.playhead = bars(41)),
            ]
        }
        PROJECT_GROOVE => {
            vec![
                b(move |s| add_track(s, "Drums", Some(Instrument::Drums))),
                b(move |s| draw_clip_at(s, 0)),
                b(move |s| add_notes(s, KICK, &[0, PPQ, 2 * PPQ, 3 * PPQ])),
                b(move |s| add_notes(s, CLAP, &[PPQ, 3 * PPQ])),
                b(move |s| add_notes(s, CLOSED_HAT, &SIXTEENTHS_E_AND_A)),
                b(move |s| add_notes(s, OPEN_HAT, &OFFBEATS)),
                b(play),
                b(move |s| add_notes(s, SNARE, &[BAR - PPQ / 4])),
                b(move |s| stretch_to(s, 16)),
            ]
        }
        PROJECT_BASS => {
            let roots = PROJECT_BASS_ROOTS;
            let offbeats = |bar: i64| OFFBEATS.map(|o| bar * BAR + o);
            vec![
                b(move |s| add_midi_track(s, "MIDI 1")),
                b(move |s| s.synth = shared::synth::recipes::deep_bass()),
                b(move |s| draw_clip_at(s, 4)),
                b(move |s| pattern_bars(s, 4)),
                b(move |s| add_notes(s, roots[0], &offbeats(0))),
                b(move |s| add_notes(s, roots[1], &offbeats(1))),
                b(move |s| add_notes(s, roots[2], &offbeats(2))),
                b(move |s| add_notes(s, roots[3], &offbeats(3))),
                b(move |s| stretch_to(s, 16)),
                b(play),
                // An E (the chord's fifth) in bar 1.
                b(move |s| add_notes(s, 64, &[3 * PPQ / 2])),
            ]
        }
        PROJECT_CHORDS => {
            let chord = |s: &mut Snapshot, bar: usize| {
                for p in PROJECT_CHORDS_NOTES[bar] {
                add_notes(s, p, &[bar as i64 * BAR]);
                }
            };
            vec![
                b(move |s| add_midi_track(s, "MIDI 1")),
                b(move |s| s.synth = shared::synth::soft_pad()),
                b(move |s| draw_clip_at(s, 8)),
                b(move |s| pattern_bars(s, 4)),
                b(move |s| chord(s, 0)),
                b(move |s| chord(s, 1)),
                b(move |s| chord(s, 2)),
                b(move |s| chord(s, 3)),
                b(move |s| stretch_to(s, 16)),
                b(play),
                b(move |s| {
                    for p in PROJECT_CHORDS_NOTES[0] {
                        add_notes(s, p, &[PPQ + PPQ / 2]);
                    }
                }),
            ]
        }
        PROJECT_ARRANGE => {
            vec![
                b(move |s| {
                    for bar in MARKER_BARS {
                        let id = s.arrangement.alloc_id();
                        s.arrangement.markers.push(shared::arrangement::Marker { id, position: bar * BAR, name: "M".into() });
                    }
                }),
                b(move |s| {
                    for name in ["Drums", "Bass", "Chords"] {
                        let t = s.arrangement.tracks.iter().find(|t| t.name == name).unwrap().id;
                        let clip = s.arrangement.clips.iter().find(|c| c.track == t).unwrap().clone();
                        let longer = clip.extended_as_loop(32 * BAR - clip.start).unwrap();
                        Command::ReplaceClip { clip: Box::new(longer) }.apply(&mut s.arrangement);
                    }
                }),
                b(move |s| split_all_at(s, 16)),
                b(move |s| split_all_at(s, 24)),
                b(move |s| delete_piece(s, "Drums", 16)),
                b(move |s| delete_piece(s, "Bass", 16)),
                b(move |s| {
                    s.playhead = bars(12);
                    s.playing = true;
                }),
            ]
        }
        PROJECT_FINISH => {
            vec![
                b(move |s| {
                    let track = s.selected_track.unwrap();
                    let id = s.arrangement.alloc_id();
                    let lane = shared::arrangement::AutomationLane {
                        id,
                        track,
                        parameter_name: "Carve \u{b7} Cutoff".into(),
                        display_value: String::new(),
                        breakpoints: vec![shared::arrangement::Breakpoint { tick: 0, value: 0.5 }],
                        target: Some(shared::arrangement::AutomationTarget::Synth(SynthParam::Cutoff)),
                    };
                    s.arrangement.automation.push(lane);
                }),
                b(move |s| {
                    let lane = s.arrangement.automation.last_mut().unwrap();
                    lane.breakpoints = vec![
                        shared::arrangement::Breakpoint { tick: 16 * BAR, value: 0.2 },
                        shared::arrangement::Breakpoint { tick: 24 * BAR, value: 0.8 },
                    ];
                }),
                b(move |s| select_gain(s, "Chords", -10.0)),
                b(move |s| {
                    s.playhead = 0;
                    s.playing = true;
                }),
            ]
        }
        RECIPE_KEYS => vec![
            b(play),
            b(|s| s.synth.osc1.waveform = Waveform::Sine),
            b(|s| {
                s.synth.osc2.waveform = Waveform::Triangle;
                s.synth.osc2.octave = 1;
                s.synth.mix.osc2_db = -14.0;
            }),
            b(|s| {
                s.synth.amp_env.decay_ms = 1500.0;
                s.synth.amp_env.sustain = 0.15;
                s.synth.amp_env.release_ms = 1500.0;
            }),
            b(|s| s.synth.filter.cutoff_hz = 1500.0),
            b(|s| {
                knob(s, SynthParam::Lfo2Rate, hz_norm(0.6));
                s.synth.lfo2.depth = 0.08;
            }),
            b(|s| {
                s.synth.fx.chorus_mix = 0.3;
                s.synth.fx.reverb_mix = 0.3;
            }),
        ],
        LOFI_BEAT => vec![
            b(|s| add_track(s, "Drums", Some(Instrument::Drums))),
            b(|s| draw_clip_at(s, 0)),
            b(|s| add_notes(s, KICK, &[0, 2 * PPQ + PPQ / 2])),
            b(|s| add_notes(s, SNARE, &[PPQ, 3 * PPQ])),
            b(|s| add_notes(s, CLOSED_HAT, &[0, PPQ / 2, PPQ, 3 * PPQ / 2, 2 * PPQ, 5 * PPQ / 2, 3 * PPQ, 7 * PPQ / 2])),
            b(play),
            b(|s| add_notes(s, SNARE, &[BAR - PPQ / 4])),
            b(|s| stretch_to(s, 16)),
        ],
        LOFI_KEYS => {
            let chord = |s: &mut Snapshot, bar: usize| {
                for p in LOFI_CHORDS[bar] {
                    add_notes(s, p, &[bar as i64 * BAR]);
                }
            };
            vec![
                b(|s| add_midi_track(s, "MIDI 1")),
                b(|s| s.synth = shared::synth::recipes::lofi_keys()),
                b(|s| draw_clip_at(s, 0)),
                b(|s| pattern_bars(s, 4)),
                b(move |s| chord(s, 0)),
                b(move |s| chord(s, 1)),
                b(move |s| chord(s, 2)),
                b(move |s| chord(s, 3)),
                b(|s| stretch_to(s, 16)),
                b(play),
                // A 9th: B on the A minor chord.
                b(|s| add_notes(s, 71, &[3 * BAR])),
            ]
        }
        LOFI_BASS => {
            let root = |s: &mut Snapshot, bar: usize| {
                let at = bar as i64 * BAR;
                add_notes(s, LOFI_BASS_ROOTS[bar], &[at, at + 2 * PPQ + PPQ / 2]);
            };
            vec![
                b(|s| add_midi_track(s, "MIDI 2")),
                b(|s| s.synth = shared::synth::recipes::deep_bass()),
                b(|s| {
                    s.synth.filter.drive_db = 0.0;
                    s.synth.amp_env.release_ms = 700.0;
                }),
                b(|s| draw_clip_at(s, 0)),
                b(|s| pattern_bars(s, 4)),
                b(move |s| root(s, 0)),
                b(move |s| root(s, 1)),
                b(move |s| root(s, 2)),
                b(move |s| root(s, 3)),
                b(|s| stretch_to(s, 16)),
                b(play),
                // A C on beat 4 of bar 3, walking down to the A.
                b(|s| add_notes(s, 60, &[2 * BAR + 3 * PPQ])),
            ]
        }
        LOFI_FINISH => vec![
            b(|s| split_all_at(s, 4)),
            b(|s| delete_piece(s, "Drums", 0)),
            b(|s| delete_piece(s, "Bass", 0)),
            b(|s| {
                let track = s.arrangement.tracks.iter().find(|t| t.name == "Keys").unwrap().id;
                s.selected_track = Some(track);
                let id = s.arrangement.alloc_id();
                s.arrangement.automation.push(shared::arrangement::AutomationLane {
                    id,
                    track,
                    parameter_name: "Carve \u{b7} Cutoff".into(),
                    display_value: String::new(),
                    breakpoints: vec![shared::arrangement::Breakpoint { tick: 0, value: 0.5 }],
                    target: Some(shared::arrangement::AutomationTarget::Synth(SynthParam::Cutoff)),
                });
            }),
            b(|s| {
                let lane = s.arrangement.automation.last_mut().unwrap();
                lane.breakpoints = vec![
                    shared::arrangement::Breakpoint { tick: 0, value: 0.3 },
                    shared::arrangement::Breakpoint { tick: 4 * BAR, value: lofi_keys_open() },
                ];
            }),
            b(|s| select_gain(s, "Drums", -8.0)),
            b(|s| {
                s.playhead = 0;
                s.playing = true;
            }),
        ],
        BOLLY_MELODY => {
            let phrase = |s: &mut Snapshot, bars: std::ops::Range<i64>| {
                for &(at, pitch, _) in BOLLY_PHRASE.iter().filter(|(at, ..)| bars.contains(&(at / 16))) {
                    add_notes(s, pitch, &[at * SIXTEENTH]);
                }
            };
            vec![
                b(|s| add_midi_track(s, "MIDI 3")),
                b(|s| s.synth = shared::synth::recipes::indian_harp()),
                b(|s| draw_clip_at(s, 4)),
                b(|s| pattern_bars(s, 4)),
                b(move |s| phrase(s, 0..2)),
                b(move |s| phrase(s, 2..4)),
                b(|s| stretch_to(s, 16)),
                b(play),
                // B just before the A on beat 4 of bar 1.
                b(|s| add_notes(s, 71, &[11 * SIXTEENTH])),
            ]
        }
        TRAP_DRUMS => vec![
            b(|s| add_track(s, "Drums", Some(Instrument::Drums))),
            b(|s| draw_clip_at(s, 0)),
            b(|s| add_notes(s, KICK, &TRAP_KICKS.map(|at| at * SIXTEENTH))),
            b(|s| add_notes(s, SNARE, &[TRAP_SNARE * SIXTEENTH])),
            b(|s| {
                s.snap = shared::arrangement::SnapGrid::Eighth;
                add_notes(s, CLOSED_HAT, &[0, PPQ / 2, PPQ, 3 * PPQ / 2, 2 * PPQ, 5 * PPQ / 2]);
            }),
            b(|s| {
                s.snap = shared::arrangement::SnapGrid::ThirtySecond;
                add_notes(s, CLOSED_HAT, &(0..8).map(|i| 3 * PPQ + i * PPQ / 8).collect::<Vec<_>>());
            }),
            b(play),
            b(|s| stretch_to(s, 16)),
        ],
        TRAP_808 => vec![
            b(|s| add_midi_track(s, "MIDI 1")),
            b(|s| s.synth = shared::synth::recipes::eight_oh_eight()),
            b(|s| draw_clip_at(s, 0)),
            b(|s| add_notes(s, 57, &TRAP_KICKS.map(|at| at * SIXTEENTH))),
            b(play),
            b(|s| {
                let clip = last_clip(s);
                s.snap = shared::arrangement::SnapGrid::Quarter;
                Command::AddMidiNote { clip, note: MidiNote { start: 2 * PPQ, length: PPQ, pitch: 57, velocity: 100 } }.apply(&mut s.arrangement);
                s.snap = shared::arrangement::SnapGrid::Sixteenth;
                add_notes(s, 60, &[2 * PPQ + PPQ / 2]);
            }),
            b(|s| stretch_to(s, 16)),
        ],
        TRAP_MELODY => {
            let bar = |s: &mut Snapshot, bar: i64| {
                for &(at, pitch, _) in TRAP_PHRASE.iter().filter(|(at, ..)| at / 16 == bar) {
                    add_notes(s, pitch, &[at * SIXTEENTH]);
                }
            };
            vec![
                b(|s| add_midi_track(s, "MIDI 2")),
                b(|s| s.synth = shared::synth::recipes::flute()),
                b(|s| draw_clip_at(s, 0)),
                b(|s| pattern_bars(s, 2)),
                b(move |s| bar(s, 0)),
                b(move |s| bar(s, 1)),
                b(|s| stretch_to(s, 16)),
                b(play),
                // A C one square before bar 2's B: a kan from above.
                b(|s| add_notes(s, 72, &[19 * SIXTEENTH])),
            ]
        }
        TRAP_ARRANGE => vec![
            b(|s| split_all_at(s, 4)),
            b(|s| delete_piece(s, "Drums", 0)),
            b(|s| delete_piece(s, "808", 0)),
            b(|s| split_all_at(s, TRAP_GAP_BAR)),
            b(|s| split_all_at(s, TRAP_GAP_BAR + 1)),
            b(|s| delete_piece(s, "Drums", TRAP_GAP_BAR)),
            b(play),
        ],
        BOLLY_DRONE => vec![
            b(|s| add_midi_track(s, "MIDI 4")),
            b(|s| s.synth = shared::synth::recipes::tanpura()),
            b(|s| draw_clip_at(s, 0)),
            b(|s| {
                for &(at, pitch, _) in &BOLLY_TANPURA {
                    add_notes(s, pitch, &[at * SIXTEENTH]);
                }
            }),
            b(|s| stretch_to(s, 16)),
            b(|s| {
                let id = s.selected_track.unwrap();
                s.arrangement.track_mut(id).unwrap().gain_db = -10.0;
            }),
            b(|s| {
                s.playhead = 0;
                s.playing = true;
            }),
        ],
        MATCH_WAVE => vec![b(|s| s.synth = super::sound_match::target(MATCH_WAVE).unwrap())],
        MATCH_CUTOFF => vec![b(|s| s.synth = super::sound_match::target(MATCH_CUTOFF).unwrap())],
        MATCH_RESONANCE => vec![b(|s| s.synth = super::sound_match::target(MATCH_RESONANCE).unwrap())],
        MATCH_SUB => vec![b(|s| s.synth = super::sound_match::target(MATCH_SUB).unwrap())],
        MATCH_PLUCK => vec![b(|s| s.synth = super::sound_match::target(MATCH_PLUCK).unwrap())],
        MATCH_SWELL => vec![b(|s| s.synth = super::sound_match::target(MATCH_SWELL).unwrap())],
        MATCH_MYSTERY => vec![b(|s| s.synth = super::sound_match::target(MATCH_MYSTERY).unwrap())],
        THEORY_OCTAVES => vec![
            b(play),
            b(open_theory_clip),
            b(|s| {
                for (at, p, _) in OCTAVE_TUNE {
                    move_note(s, at * SIXTEENTH, p, p + 12);
                }
            }),
            b(|s| add_notes(s, 60, &[0])),
        ],
        THEORY_SCALES => vec![
            b(open_theory_clip),
            b(|s| add_notes(s, 60, &[0])),
            b(|s| {
                add_notes(s, 62, &[PPQ]);
                add_notes(s, 64, &[2 * PPQ]);
            }),
            b(|s| add_notes(s, 65, &[3 * PPQ])),
            b(|s| {
                add_notes(s, 67, &[4 * PPQ]);
                add_notes(s, 69, &[5 * PPQ]);
                add_notes(s, 71, &[6 * PPQ]);
            }),
            b(|s| add_notes(s, 72, &[7 * PPQ])),
            b(play),
        ],
        THEORY_KEYS => vec![
            b(play),
            b(open_theory_clip),
            b(|s| move_note(s, 7 * PPQ, 60, 57)),
            b(|s| set_key(s, 9, "Natural minor")),
        ],
        THEORY_MAJOR_MINOR => vec![
            b(play),
            b(|s| set_key(s, 0, "Natural minor")),
            b(open_theory_clip),
            b(|s| {
                for at in [2, 6, 10] {
                    move_note(s, at * SIXTEENTH, 64, 63);
                }
            }),
        ],
        THEORY_INTERVALS => vec![b(play)],
        THEORY_TRIADS => {
            let triad = |s: &mut Snapshot, pitches: [u8; 3], beat: i64| {
                for p in pitches {
                    add_notes(s, p, &[beat * PPQ]);
                }
            };
            vec![
                b(open_theory_clip),
                b(move |s| triad(s, [60, 64, 67], 0)),
                b(move |s| triad(s, [62, 65, 69], 1)),
                b(move |s| {
                    triad(s, [64, 67, 71], 2);
                    triad(s, [65, 69, 72], 3);
                }),
                b(move |s| {
                    triad(s, [67, 71, 74], 4);
                    triad(s, [69, 72, 76], 5);
                    triad(s, [71, 74, 77], 6);
                }),
                b(move |s| triad(s, [60, 64, 67], 7)),
                b(play),
            ]
        }
        THEORY_PROGRESSIONS => {
            let chord = |s: &mut Snapshot, i: usize| {
                for p in PROGRESSION[i] {
                    add_notes(s, p, &[i as i64 * BAR]);
                }
            };
            vec![
                b(open_theory_clip),
                b(move |s| chord(s, 0)),
                b(move |s| chord(s, 1)),
                b(move |s| chord(s, 2)),
                b(move |s| chord(s, 3)),
                b(play),
            ]
        }
        THEORY_MELODY => vec![
            b(play),
            b(|s| s.open_clip = Some(clip_on_track(s, "Melody"))),
            // E over C, D over G.
            b(|s| {
                add_notes_on(s, "Melody", 64, &[0]);
                add_notes_on(s, "Melody", 62, &[BAR]);
            }),
            // C over A minor, A over F.
            b(|s| {
                add_notes_on(s, "Melody", 60, &[2 * BAR]);
                add_notes_on(s, "Melody", 69, &[3 * BAR]);
            }),
            // Stepping towards each next note.
            b(|s| {
                for (bar, p) in [(0, 62), (1, 60), (2, 67), (3, 65)] {
                    add_notes_on(s, "Melody", p, &[bar * BAR + 2 * PPQ]);
                }
            }),
        ],
        THEORY_SEVENTHS => {
            let chord = |s: &mut Snapshot, pitches: [u8; 4], beat: i64| {
                for p in pitches {
                    add_notes(s, p, &[beat * PPQ]);
                }
            };
            vec![
                b(open_theory_clip),
                b(move |s| chord(s, [50, 53, 57, 60], 0)),
                b(move |s| chord(s, [55, 59, 62, 65], 2)),
                b(move |s| chord(s, [48, 52, 55, 59], 4)),
                b(play),
            ]
        }
        THEORY_RAAG => vec![
            b(play),
            b(open_theory_clip),
            b(|s| {
                for (beat, p) in [60, 62, 64, 65, 67, 69, 71, 72].into_iter().enumerate() {
                    add_notes(s, p, &[beat as i64 * PPQ]);
                }
            }),
            b(|s| set_key(s, 0, "Raga Bhairav")),
            b(|s| {
                move_note(s, PPQ, 62, 61);
                move_note(s, 5 * PPQ, 69, 68);
            }),
        ],
        MELODY_STEPS => vec![
            b(play),
            b(|s| s.open_clip = Some(clip_on_track(s, "Melody"))),
            b(|s| {
                for (beat, p) in [64, 62, 60, 62].into_iter().enumerate() {
                    add_notes_on(s, "Melody", p, &[beat as i64 * PPQ]);
                }
            }),
            b(|s| {
                add_notes_on(s, "Melody", 67, &[BAR]);
                add_notes_on(s, "Melody", 65, &[BAR + PPQ]);
                add_notes_on(s, "Melody", 64, &[BAR + 2 * PPQ]);
            }),
        ],
        MELODY_CALL => vec![
            b(play),
            b(|s| s.open_clip = Some(clip_on_track(s, "Melody"))),
            b(|s| {
                for (beat, p) in [60, 64, 67].into_iter().enumerate() {
                    add_notes_on(s, "Melody", p, &[beat as i64 * PPQ]);
                }
            }),
            b(|s| {
                for (beat, p) in [64, 62, 60].into_iter().enumerate() {
                    add_notes_on(s, "Melody", p, &[BAR + beat as i64 * PPQ]);
                }
            }),
        ],
        MELODY_MOTIF => {
            let bar = |pitches: [u8; 3], at: i64| {
                move |s: &mut Snapshot| {
                    for (beat, p) in pitches.into_iter().enumerate() {
                        add_notes_on(s, "Melody", p, &[at * BAR + beat as i64 * PPQ]);
                    }
                }
            };
            vec![
                b(play),
                b(|s| s.open_clip = Some(clip_on_track(s, "Melody"))),
                b(bar([60, 62, 64], 0)),
                b(bar([62, 64, 65], 1)),
                b(bar([64, 65, 67], 2)),
                b(bar([60, 62, 60], 3)),
            ]
        }
        ROLL_DYNAMICS => {
            let soften = |at: &'static [i64], velocity: u8| {
                move |s: &mut Snapshot| {
                    for &a in at {
                        set_velocity(s, "Drums", shared::drums::CLOSED_HAT, a * SIXTEENTH, velocity);
                    }
                }
            };
            vec![b(play), b(|s| s.open_clip = Some(clip_on_track(s, "Drums"))), b(soften(&ROLL_GHOSTS, 40)), b(soften(&[2, 6, 10, 14], 75))]
        }
        ROLL_PAINT => vec![
            b(play),
            b(|s| s.open_clip = Some(clip_on_track(s, "Drums"))),
            b(|s| add_notes_on(s, "Drums", CLOSED_HAT, &(0..16).map(|i| i * SIXTEENTH).collect::<Vec<_>>())),
            b(|s| {
                let clip = clip_on_track(s, "Drums");
                let erase = (12..16).map(|i| Command::RemoveMidiNote { clip, start: i * SIXTEENTH, pitch: CLOSED_HAT }).collect();
                Command::Batch(erase).apply(&mut s.arrangement);
            }),
            b(|s| {
                for at in [0, PPQ, 2 * PPQ] {
                    set_velocity(s, "Drums", CLOSED_HAT, at, 127);
                }
            }),
            b(|s| {
                add_note_on(s, "Drums", SNARE, 15 * SIXTEENTH, SIXTEENTH);
                set_velocity(s, "Drums", SNARE, 15 * SIXTEENTH, 45);
            }),
            b(humanize_drums),
        ],
        ROLL_SWING => vec![
            b(play),
            b(|s| s.open_clip = Some(clip_on_track(s, "Drums"))),
            b(|s| set_swing(s, 1.0)),
            b(|s| set_swing(s, 0.5)),
        ],
        ROLL_ROLLS => vec![
            b(play),
            b(|s| s.open_clip = Some(clip_on_track(s, "Drums"))),
            b(|s| s.snap = shared::arrangement::SnapGrid::ThirtySecond),
            b(|s| paint_hats(s, 3 * PPQ, BAR, PPQ / 8)),
            b(|s| s.snap = shared::arrangement::SnapGrid::SixteenthTriplet),
            b(|s| paint_hats(s, PPQ, 2 * PPQ, PPQ / 6)),
            b(|s| {
                s.snap = shared::arrangement::SnapGrid::ThirtySecond;
                for at in [3 * PPQ, 3 * PPQ + PPQ / 8] {
                    set_velocity(s, "Drums", CLOSED_HAT, at, 45);
                }
            }),
        ],
        ROLL_LENGTH => vec![
            b(play),
            b(|s| s.open_clip = Some(clip_on_track(s, "Keys"))),
            b(|s| {
                add_note_on(s, "Keys", 57, 0, PPQ);
                add_note_on(s, "Keys", 57, 2 * PPQ, PPQ);
            }),
            b(|s| add_note_on(s, "Keys", 60, 3 * PPQ, SIXTEENTH)),
            b(|s| add_note_on(s, "Keys", 64, PPQ + 40, SIXTEENTH)),
        ],
        CARVE_SYNC_FM => vec![
            b(play),
            b(|s| {
                s.synth.mix.osc2_db = -6.0;
                s.synth.mix.osc1_db = -60.0;
            }),
            b(|s| s.synth.osc2.sync = true),
            b(|s| s.synth.osc2.octave = 2),
            b(|s| {
                s.synth.osc2.sync = false;
                s.synth.osc2.octave = 0;
            }),
            b(|s| s.synth.osc2.knob_c = 0.3),
            b(|s| s.synth.osc2.waveform = Waveform::Sine),
            b(|s| s.synth.filter.cutoff_hz = 500.0),
            b(|s| s.synth.filter.key_track = 1.0),
        ],
        MIX_LEVELS => vec![b(play), b(|s| select_gain(s, "Drums", -6.0)), b(|s| select_gain(s, "Chords", -12.0))],
        MIX_EQ => vec![
            b(play),
            b(|s| s.analyzer_open = true),
            b(|s| set_track(s, "Chords", |t| t.solo = true)),
            b(|s| add_effect(s, "Chords", Effect::Eq(EqState::default()))),
            b(|s| {
                edit_effect(s, "Chords", |e| {
                    if let Effect::Eq(eq) = e {
                        eq.bands[EQ_LOW_CUT].on = true;
                        eq.bands[EQ_LOW_CUT].freq_hz = 250.0;
                    }
                })
            }),
            b(|s| set_track(s, "Chords", |t| t.solo = false)),
        ],
        MIX_COMPRESS => {
            let comp = |f: fn(&mut CompressorState)| {
                move |s: &mut Snapshot| {
                    edit_effect(s, "Drums", |e| {
                        if let Effect::Compressor(c) = e {
                            f(c)
                        }
                    })
                }
            };
            vec![
                b(play),
                b(|s| add_effect(s, "Drums", Effect::Compressor(CompressorState::default()))),
                b(comp(|c| c.threshold_db = -25.0)),
                b(comp(|c| c.ratio = 6.0)),
                b(comp(|c| c.attack_ms = 20.0)),
                b(comp(|c| c.makeup_db = 4.0)),
                b(comp(|c| {
                    c.ratio = 20.0;
                    c.threshold_db = -40.0;
                })),
            ]
        }
        MIX_FINISH => vec![b(play), b(|s| s.exported = true)],
        _ => vec![],
    }
}

/// Selecting `name` and pressing its "+ Compressor" / "+ EQ".
fn add_effect(s: &mut Snapshot, name: &str, effect: Effect) {
    let track = s.arrangement.tracks.iter().find(|t| t.name == name).unwrap().id;
    s.selected_track = Some(track);
    Command::AddEffectNode { track: Some(track), effect, position: None }.apply(&mut s.arrangement);
}

/// Turning a knob on `name`'s first effect of its kind (what the panel edits).
fn edit_effect(s: &mut Snapshot, name: &str, f: impl Fn(&mut Effect)) {
    let track = s.arrangement.tracks.iter().find(|t| t.name == name).unwrap().id;
    let fx = s.arrangement.fx_mut(Some(track)).unwrap();
    let node = fx.nodes.last_mut().unwrap();
    f(&mut node.effect);
}

/// Double-clicking the theory lessons' clip on the Keys track.
fn open_theory_clip(s: &mut Snapshot) {
    s.open_clip = Some(clip_on_track(s, "Keys"));
}

fn clip_on_track(s: &Snapshot, name: &str) -> shared::arrangement::ClipId {
    let track = s.arrangement.tracks.iter().find(|t| t.name == name).unwrap().id;
    s.arrangement.clips.iter().find(|c| c.track == track).unwrap().id
}

/// A velocity drag on `name`'s clip: the note at `start` of `pitch`.
fn set_velocity(s: &mut Snapshot, name: &str, pitch: u8, start: Ticks, velocity: u8) {
    let clip = clip_on_track(s, name);
    Command::SetNoteVelocity { clip, start, pitch, velocity }.apply(&mut s.arrangement);
}

/// A paint stroke along the Closed Hat row of the Drums clip: a `step`-long
/// hit on every empty `step` of `from..to`.
fn paint_hats(s: &mut Snapshot, from: Ticks, to: Ticks, step: Ticks) {
    let clip = clip_on_track(s, "Drums");
    let ClipContent::Midi { notes, .. } = &s.arrangement.clip(clip).unwrap().content else { panic!() };
    let empty: Vec<Ticks> = (from..to)
        .step_by(step as usize)
        .filter(|&at| !notes.iter().any(|n| n.pitch == CLOSED_HAT && n.start >= at && n.start < at + step))
        .collect();
    for at in empty {
        add_note_on(s, "Drums", CLOSED_HAT, at, step);
    }
}

/// The Swing knob on the Drums clip.
fn set_swing(s: &mut Snapshot, swing: f32) {
    let clip = clip_on_track(s, "Drums");
    s.arrangement.clips.iter_mut().find(|c| c.id == clip).unwrap().swing = swing;
}

/// The Humanize button on the Drums clip.
fn humanize_drums(s: &mut Snapshot) {
    let clip = clip_on_track(s, "Drums");
    let c = s.arrangement.clips.iter_mut().find(|c| c.id == clip).unwrap();
    let len = c.content_len();
    if let ClipContent::Midi { notes, .. } = &mut c.content {
        for n in notes.iter_mut() {
            *n = crate::timeline::state::humanized(n, len);
        }
    }
}

/// One click on `name`'s clip at `start`, with Snap making it `length` long.
fn add_note_on(s: &mut Snapshot, name: &str, pitch: u8, start: Ticks, length: Ticks) {
    let clip = clip_on_track(s, name);
    Command::AddMidiNote { clip, note: MidiNote { start, length, pitch, velocity: 100 } }.apply(&mut s.arrangement);
}

/// One 16th-long click on `name`'s clip at `start` (for tests).
#[cfg(test)]
pub(super) fn add_notes_on_track(s: &mut Snapshot, name: &str, pitch: u8, start: Ticks) {
    add_notes_on(s, name, pitch, &[start]);
}

/// Clicks on `name`'s clip, one note per start.
fn add_notes_on(s: &mut Snapshot, name: &str, pitch: u8, starts: &[Ticks]) {
    let clip = clip_on_track(s, name);
    for &start in starts {
        Command::AddMidiNote { clip, note: MidiNote { start, length: PPQ / 4, pitch, velocity: 100 } }.apply(&mut s.arrangement);
    }
}

/// The note at `start` moved from pitch `from` to `to` (what picking it
/// and pressing an arrow key does).
fn move_note(s: &mut Snapshot, start: Ticks, from: u8, to: u8) {
    let clip = clip_on_track(s, "Keys");
    let ClipContent::Midi { notes, .. } = &s.arrangement.clip(clip).unwrap().content else { panic!() };
    let note = *notes.iter().find(|n| n.start == start && n.pitch == from).unwrap();
    Command::RemoveMidiNote { clip, start, pitch: from }.apply(&mut s.arrangement);
    Command::AddMidiNote { clip, note: MidiNote { pitch: to, ..note } }.apply(&mut s.arrangement);
}

/// Picking a key and scale in the Key menu.
fn set_key(s: &mut Snapshot, root: u8, scale: &str) {
    s.key = root;
    s.scale_mask = shared::theory::SCALE_PRESETS.iter().find(|p| p.name == scale).unwrap().mask;
}

pub(super) fn add_midi_track(s: &mut Snapshot, name: &str) {
    add_track(s, name, Instrument::default_for(TrackKind::Midi));
}

pub(super) fn add_track(s: &mut Snapshot, name: &str, instrument: Option<Instrument>) {
    let id = s.arrangement.alloc_id();
    let track = Track {
        id,
        name: name.into(),
        color: shared::arrangement::ClipColor::Violet,
        kind: TrackKind::Midi,
        mute: false,
        solo: false,
        arm: false,
        gain_db: 0.0,
        height: DEFAULT_TRACK_HEIGHT,
        instrument,
        effects: vec![],
        effect_slots: vec![],
        fx: Default::default(),
        drum_pads: Default::default(),
    };
    let index = s.arrangement.tracks.len();
    Command::InsertTrack { track: Box::new(track), index, clips: vec![], automation: vec![] }.apply(&mut s.arrangement);
    s.selected_track = Some(id);
}

/// What a double-click on an empty lane does: a one-bar empty clip on
/// the selected track.
pub(super) fn draw_clip(s: &mut Snapshot) {
    let id = s.arrangement.alloc_id();
    let clip = Clip {
        id,
        track: s.selected_track.unwrap(),
        start: 0,
        length: BAR,
        name: "Clip".into(),
        content: ClipContent::Midi { notes: vec![], loop_len: None, link: None },
        recording: false,
        gain_db: 0.0,
        swing: 0.0,
    };
    Command::InsertClip { clip: Box::new(clip) }.apply(&mut s.arrangement);
    // A double-click opens the new clip in the editor.
    s.open_clip = Some(id);
}

pub(super) fn add_notes(s: &mut Snapshot, pitch: u8, starts: &[Ticks]) {
    let clip = last_clip(s);
    for &start in starts {
        let note = MidiNote { start, length: PPQ / 4, pitch, velocity: 100 };
        Command::AddMidiNote { clip, note }.apply(&mut s.arrangement);
    }
}

/// Dragging the open clip's right edge out to bar 9.
pub(super) fn stretch(s: &mut Snapshot) {
    let clip = s.arrangement.clip(last_clip(s)).unwrap().extended_as_loop(8 * BAR).unwrap();
    Command::ReplaceClip { clip: Box::new(clip) }.apply(&mut s.arrangement);
}

/// The clip the learner just drew (the newest one).
pub(super) fn last_clip(s: &Snapshot) -> shared::arrangement::ClipId {
    s.arrangement.clips.last().unwrap().id
}

pub(super) fn play(s: &mut Snapshot) {
    s.playing = true;
}

/// Sets `param` so its knob reads `value` (in the knob's own units,
/// via the same table the knob uses).
pub(super) fn knob(s: &mut Snapshot, param: SynthParam, norm: f32) {
    param.apply_norm(&mut s.synth, norm);
}

pub(super) fn hz_norm(hz: f32) -> f32 {
    (hz / 0.05).ln() / (20.0f32 / 0.05).ln()
}

pub(super) fn set_track(s: &mut Snapshot, name: &str, f: fn(&mut Track)) {
    let id = s.arrangement.tracks.iter().find(|t| t.name == name).unwrap().id;
    f(s.arrangement.track_mut(id).unwrap());
}

/// A clip at `bar` on the selected track (what a double-click there does).
pub(super) fn draw_clip_at(s: &mut Snapshot, bar: i64) {
    let id = s.arrangement.alloc_id();
    let clip = Clip {
        id,
        track: s.selected_track.unwrap(),
        start: bar * BAR,
        length: BAR,
        name: "Clip".into(),
        content: ClipContent::Midi { notes: vec![], loop_len: None, link: None },
        recording: false,
        gain_db: 0.0,
        swing: 0.0,
    };
    Command::InsertClip { clip: Box::new(clip) }.apply(&mut s.arrangement);
    s.open_clip = Some(id);
}

/// Pattern + to `n` bars (the SetPatternBars handler).
pub(super) fn pattern_bars(s: &mut Snapshot, n: i64) {
    let mut clip = s.arrangement.clip(last_clip(s)).unwrap().clone();
    clip.length = clip.length.max(n * BAR);
    let ClipContent::Midi { notes, link, .. } = clip.content.clone() else { panic!() };
    clip.content = ClipContent::Midi { notes, loop_len: Some(n * BAR), link };
    Command::ReplaceClip { clip: Box::new(clip) }.apply(&mut s.arrangement);
}

/// Dragging the newest clip's right edge to 0-based bar `end`.
pub(super) fn stretch_to(s: &mut Snapshot, end: i64) {
    let old = s.arrangement.clip(last_clip(s)).unwrap().clone();
    let clip = old.extended_as_loop(end * BAR - old.start).unwrap_or(old);
    Command::ReplaceClip { clip: Box::new(clip) }.apply(&mut s.arrangement);
}

/// Ctrl+E at bar `bar`: every clip there splits.
pub(super) fn split_all_at(s: &mut Snapshot, bar: i64) {
    let at = bar * BAR;
    let crossing: Vec<_> = s.arrangement.clips.iter().filter(|c| c.start < at && c.end() > at).map(|c| c.id).collect();
    for clip in crossing {
        let new_id = s.arrangement.alloc_id();
        Command::SplitClip { clip, at, new_id }.apply(&mut s.arrangement);
    }
}

pub(super) fn delete_piece(s: &mut Snapshot, track: &str, bar: i64) {
    let t = s.arrangement.tracks.iter().find(|t| t.name == track).unwrap().id;
    let clip = s.arrangement.clips.iter().find(|c| c.track == t && c.start == bar * BAR).unwrap().id;
    Command::DeleteClip { clip }.apply(&mut s.arrangement);
}

pub(super) fn select_gain(s: &mut Snapshot, name: &str, db: f32) {
    let id = s.arrangement.tracks.iter().find(|t| t.name == name).unwrap().id;
    s.arrangement.track_mut(id).unwrap().gain_db = db;
}
