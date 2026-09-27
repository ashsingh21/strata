//! The course: every lesson's steps, their text, what each one checks and
//! which control glows. Text lives only here, so translating the course
//! is a change to this table.

use shared::arrangement::{Clip, ClipContent, Instrument, Ticks, PPQ};
use shared::drums::{CLAP, KICK, OPEN_HAT};
use shared::lessons::{
    BAR, BASSLINE, BASS_NOTE, CARVE_ENVELOPES, CARVE_FILTER, CARVE_MIX, CARVE_MOVEMENT, CARVE_WAVES, CHORDS, FIRST_BEAT,
    RECIPE_BASS, RECIPE_FLUTE, RECIPE_HARP, RECIPE_LEAD,
};
use shared::synth::{lfo_rate_hz, FilterType, LfoTarget, SynthParam, SynthState, VoiceMode, Waveform};

use super::{selected, tracks_with, Snapshot, Target};

pub enum Kind {
    /// Done when `check` passes; `target` says what glows meanwhile.
    Action { check: fn(&Snapshot) -> bool, target: fn(&Snapshot) -> Option<Target> },
    /// Read, then Continue.
    Info,
}

pub struct Step {
    pub text: &'static str,
    /// Shown if the step hasn't been done after a while.
    pub hint: &'static str,
    pub kind: Kind,
}

pub struct Lesson {
    /// Matches `shared::lessons` (starting project) and the saved "done" list.
    pub id: &'static str,
    pub title: &'static str,
    pub steps: &'static [Step],
}

const fn act(text: &'static str, hint: &'static str, check: fn(&Snapshot) -> bool, target: fn(&Snapshot) -> Option<Target>) -> Step {
    Step { text, hint, kind: Kind::Action { check, target } }
}

const fn info(text: &'static str) -> Step {
    Step { text, hint: "", kind: Kind::Info }
}

pub const LESSONS: &[Lesson] = &[
    Lesson {
        id: FIRST_BEAT,
        title: "Your first beat",
        steps: &[
            act(
                "Add a drum track: click \u{201c}+ Drums\u{201d} under the tracks.",
                "It's below the track list, on the left of the timeline.",
                |s| tracks_with(s, Instrument::Drums).next().is_some(),
                |_| Some(Target::AddDrumTrack),
            ),
            act(
                "Make a clip: double-click bar 1 of the Drums track.",
                "Two quick clicks on the empty lane, right of the track's name. A one-bar clip appears and opens below.",
                |s| clips_on(s, Instrument::Drums).next().is_some(),
                |s| tracks_with(s, Instrument::Drums).next().map(|t| Target::Lane(t.id)),
            ),
            act(
                "Kick on every beat: in the grid below, click the Kick row at 1, 2, 3 and 4.",
                "The numbers along the top are beats. Grid not showing? Double-click the clip to open it.",
                |s| drum_pattern_has(s, KICK, &[0, PPQ, 2 * PPQ, 3 * PPQ]),
                |s| row_or_clip(s, Instrument::Drums, KICK),
            ),
            act(
                "Press Space to hear it. (Space again stops.)",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            act(
                "Clap on beats 2 and 4: click the Clap row under 2 and 4.",
                "Beats 2 and 4 are the claps in almost every house and pop beat.",
                |s| drum_pattern_has(s, CLAP, &[PPQ, 3 * PPQ]),
                |s| row_or_clip(s, Instrument::Drums, CLAP),
            ),
            act(
                "Open hat between the beats: click the Open Hat row halfway between each beat.",
                "Halfway is two squares after each beat number (the \u{201c}and\u{201d}: 1-and, 2-and...).",
                |s| drum_pattern_has(s, OPEN_HAT, &[PPQ / 2, PPQ + PPQ / 2, 2 * PPQ + PPQ / 2, 3 * PPQ + PPQ / 2]),
                |s| row_or_clip(s, Instrument::Drums, OPEN_HAT),
            ),
            act(
                "Make it loop: in the timeline, drag the clip's right edge out to bar 9.",
                "Grab the very end of the clip. Stretching it repeats the bar you wrote.",
                |s| clips_on(s, Instrument::Drums).any(|c| loops(c, 4)),
                |s| tracks_with(s, Instrument::Drums).next().map(|t| Target::Lane(t.id)),
            ),
            info(
                "That's a house beat: kick on every beat, clap on 2 and 4, open hat in between. \
                 Everything else in a house track sits on top of this.",
            ),
        ],
    },
    Lesson {
        id: BASSLINE,
        title: "A bassline",
        steps: &[
            act(
                "Your beat is ready. For a bass, click \u{201c}+ MIDI track\u{201d}.",
                "It's below the track list, on the left of the timeline.",
                |s| tracks_with(s, Instrument::Carve).next().is_some(),
                |_| Some(Target::AddMidiTrack),
            ),
            act(
                "A MIDI track holds notes; its instrument turns them into sound. New ones play Carve, a synth. Pick a bass sound: click the preset name at the top of Carve (it says Warm Bass) and choose Deep Rave Bass.",
                "Or step through the presets with the \u{2039} \u{203a} arrows beside the name.",
                |s| selected(s).is_some_and(|t| t.instrument == Some(Instrument::Carve)) && s.synth.name == "Deep Rave Bass",
                |_| Some(Target::Preset("Deep Rave Bass")),
            ),
            act(
                "Try it: press Z, X or C on your computer keyboard.",
                "The bottom row of letters plays notes, like piano keys.",
                |s| !s.synth.held_notes.is_empty(),
                |_| None,
            ),
            act(
                "Make a clip: double-click bar 1 of the bass track.",
                "Two quick clicks on the empty lane. A one-bar clip appears and opens below.",
                |s| clips_on(s, Instrument::Carve).next().is_some(),
                |s| s.selected_track.map(Target::Lane),
            ),
            act(
                "Bass between the kicks: on the bottom row (A), click halfway between each beat.",
                "Halfway is two squares after each beat number - the same places as the open hat.",
                |s| carve_pattern_has(s, BASS_NOTE, &[PPQ / 2, PPQ + PPQ / 2, 2 * PPQ + PPQ / 2, 3 * PPQ + PPQ / 2]),
                |s| row_or_clip(s, Instrument::Carve, BASS_NOTE),
            ),
            act("Press Space to hear it with the drums.", "Or click the play button at the top.", |s| s.playing, |_| Some(Target::Play)),
            act(
                "Loop it: drag the bass clip's right edge out to bar 9.",
                "Grab the very end of the clip, like you did with the beat.",
                |s| clips_on(s, Instrument::Carve).any(|c| loops(c, 4)),
                |s| s.selected_track.map(Target::Lane),
            ),
            info(
                "Bass on the off-beats, between the kicks, is what makes house bounce. \
                 Try moving one: click it away, then click the C or E row in its place.",
            ),
        ],
    },
    Lesson {
        id: CHORDS,
        title: "Chords",
        steps: &[
            act(
                "Drums and bass are ready. Add one more MIDI track for chords.",
                "\u{201c}+ MIDI track\u{201d}, below the track list.",
                |s| tracks_with(s, Instrument::Carve).count() >= 2,
                |_| Some(Target::AddMidiTrack),
            ),
            act(
                "Pick a soft sound for chords: click the preset name at the top of Carve and choose Soft Pad.",
                "Or step through the presets with the \u{2039} \u{203a} arrows beside the name.",
                |s| selected(s).is_some_and(|t| t.instrument == Some(Instrument::Carve) && t.name != "Bass") && s.synth.name == "Soft Pad",
                |_| Some(Target::Preset("Soft Pad")),
            ),
            act(
                "Make a clip: double-click bar 1 of the new track.",
                "Two quick clicks on the empty lane. A one-bar clip appears and opens below.",
                |s| chord_clips(s).next().is_some(),
                |s| s.selected_track.map(Target::Lane),
            ),
            act(
                "Two chords need two bars: click + next to Pattern, above the grid.",
                "Pattern sets how long the part is before it repeats.",
                |s| chord_clips(s).any(|c| c.content_len() == 2 * BAR),
                |_| Some(Target::PatternPlus),
            ),
            act(
                "A minor: on beats 1 and 3 of bar 1, click A, C and E (stacked).",
                "Beats 1 and 3 are the 1.1 and 1.3 marks. A is the bottom row.",
                |s| chord_at(s, &[57, 60, 64], &[0, 2 * PPQ]),
                |s| if s.open_clip.is_none() { s.selected_track.map(Target::Lane) } else { first_missing(s, &[57, 60, 64], &[0, 2 * PPQ]).map(Target::PianoRollRow) },
            ),
            act(
                "C major: on beats 1 and 3 of bar 2, click C, E and G.",
                "Bar 2 starts at the 2 mark.",
                |s| chord_at(s, &[60, 64, 67], &[4 * PPQ, 6 * PPQ]),
                |s| if s.open_clip.is_none() { s.selected_track.map(Target::Lane) } else { first_missing(s, &[60, 64, 67], &[4 * PPQ, 6 * PPQ]).map(Target::PianoRollRow) },
            ),
            act("Press Space to hear all three parts.", "Or click the play button at the top.", |s| s.playing, |_| Some(Target::Play)),
            info(
                "Two chords, four hits. A minor and C major share two notes (C and E), \
                 which is why one flows so smoothly into the other.",
            ),
        ],
    },
    Lesson {
        id: CARVE_WAVES,
        title: "Carve: waves",
        steps: &[
            act(
                "Press Space. This is Oscillator 1 playing a saw wave: bright and buzzy, because it holds every harmonic.",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            act(
                "Click the square wave, the last shape above Oscillator 1. Hollow and woody: only the odd harmonics.",
                "The four little wave pictures next to \u{201c}Oscillator 1\u{201d}.",
                |s| carve(s).is_some_and(|p| p.osc1.waveform == Waveform::Square),
                |_| Some(Target::OscWave(1)),
            ),
            act(
                "Now the triangle, the second shape: softer - its harmonics are faint.",
                "The shape that looks like a mountain range.",
                |s| carve(s).is_some_and(|p| p.osc1.waveform == Waveform::Triangle),
                |_| Some(Target::OscWave(1)),
            ),
            act(
                "Now the sine, the first shape: pure, with no harmonics at all. Flutes and sub-basses live here.",
                "The smooth round wave.",
                |s| carve(s).is_some_and(|p| p.osc1.waveform == Waveform::Sine),
                |_| Some(Target::OscWave(1)),
            ),
            act(
                "Back to the saw. The more harmonics a wave has, the more a filter can shape - which is why most synth sounds start from a saw or square.",
                "The saw is the third shape: a ramp that drops.",
                |s| carve(s).is_some_and(|p| p.osc1.waveform == Waveform::Saw),
                |_| Some(Target::OscWave(1)),
            ),
            info("Sine is pure, triangle soft, square hollow, saw bright. Every sound in this course starts by picking one."),
        ],
    },
    Lesson {
        id: CARVE_MIX,
        title: "Carve: mixing oscillators",
        steps: &[
            act("Press Space: one saw, held.", "Or click the play button at the top.", |s| s.playing, |_| Some(Target::Play)),
            act(
                "Oscillator 2 is silent. In the Mixer, turn Osc 2 up past -12 dB.",
                "Drag the Osc 2 knob in the Mixer upward.",
                |s| carve(s).is_some_and(|p| p.mix.osc2_db > -12.0),
                |_| Some(Target::Knob(SynthParam::Osc2Level)),
            ),
            act(
                "Detune it: set Oscillator 2's Detune to about +10 cents. The two waves drift in and out of step - that beating makes it thick.",
                "Anywhere from +5 to +25 cents works.",
                |s| carve(s).is_some_and(|p| (5.0..=25.0).contains(&p.osc2.knob_a_cents)),
                |_| Some(Target::Knob(SynthParam::Osc2Detune)),
            ),
            act(
                "Set Oscillator 2's Octave to +1: it now plays an octave up, adding brightness on top.",
                "One step up on the Octave knob under Oscillator 2.",
                |s| carve(s).is_some_and(|p| p.osc2.octave == 1),
                |_| Some(Target::Knob(SynthParam::Osc2Octave)),
            ),
            act(
                "Add weight: turn Sub up past -12 dB - a sine one octave below the note.",
                "The Sub knob in the Mixer.",
                |s| carve(s).is_some_and(|p| p.mix.sub_db > -12.0),
                |_| Some(Target::Knob(SynthParam::SubLevel)),
            ),
            act(
                "A little air: Noise to about -30 dB. Not much - noise gets harsh fast.",
                "Between -40 and -15 dB.",
                |s| carve(s).is_some_and(|p| (-40.0..=-15.0).contains(&p.mix.noise_db)),
                |_| Some(Target::Knob(SynthParam::NoiseLevel)),
            ),
            info(
                "The Mixer blends four sources: two oscillators, a sub and noise. Most sounds use two or three, \
                 and their levels matter as much as their waves.",
            ),
        ],
    },
    Lesson {
        id: CARVE_FILTER,
        title: "Carve: the filter",
        steps: &[
            act("Press Space: a bright saw riff.", "Or click the play button at the top.", |s| s.playing, |_| Some(Target::Play)),
            act(
                "Turn Cutoff down below 400 Hz. The low-pass filter removes the highs: darker, muffled.",
                "Cutoff is the big knob in the Filter section.",
                |s| carve(s).is_some_and(|p| p.filter.cutoff_hz < 400.0),
                |_| Some(Target::Knob(SynthParam::Cutoff)),
            ),
            act(
                "Turn Resonance up past 70%. It boosts right at the cutoff - that whistling edge.",
                "Resonance is next to Cutoff.",
                |s| carve(s).is_some_and(|p| p.filter.resonance > 0.7),
                |_| Some(Target::Knob(SynthParam::Resonance)),
            ),
            act(
                "Now sweep Cutoff slowly back up past 3 kHz while it plays: the classic filter sweep of acid house.",
                "Drag it up gradually and listen.",
                |s| carve(s).is_some_and(|p| p.filter.cutoff_hz > 3000.0),
                |_| Some(Target::Knob(SynthParam::Cutoff)),
            ),
            act(
                "Try another filter type: click HP (high-pass). It keeps only the highs - thin and airy.",
                "The LP 24 / LP 12 / BP / HP switch at the top of the Filter.",
                |s| carve(s).is_some_and(|p| p.filter.filter_type == FilterType::Hp),
                |_| Some(Target::FilterType),
            ),
            act(
                "Back to LP 24, the warm low-pass most sounds use.",
                "The first option on the same switch.",
                |s| carve(s).is_some_and(|p| p.filter.filter_type == FilterType::Lp24),
                |_| Some(Target::FilterType),
            ),
            info(
                "Low-pass cuts highs (warm), high-pass cuts lows (thin), band-pass keeps a middle band. \
                 Cutoff sets where the filter cuts; resonance sets how sharp the edge is.",
            ),
        ],
    },
    Lesson {
        id: CARVE_ENVELOPES,
        title: "Carve: envelopes",
        steps: &[
            act("Press Space.", "Or click the play button at the top.", |s| s.playing, |_| Some(Target::Play)),
            act(
                "The Amp envelope shapes each note's loudness. Drag Amp Sustain to 0: notes now fade while they're held.",
                "Sustain is the third knob under Amp envelope.",
                |s| carve(s).is_some_and(|p| p.amp_env.sustain < 0.05),
                |_| Some(Target::Knob(SynthParam::AmpSustain)),
            ),
            act(
                "Shorten Amp Decay below 150 ms: a short, plucked note.",
                "Decay is the second knob under Amp envelope.",
                |s| carve(s).is_some_and(|p| p.amp_env.decay_ms < 150.0),
                |_| Some(Target::Knob(SynthParam::AmpDecay)),
            ),
            act(
                "Close the filter a little: Cutoff to about 1 kHz.",
                "Between 500 Hz and 2 kHz.",
                |s| carve(s).is_some_and(|p| (500.0..=2000.0).contains(&p.filter.cutoff_hz)),
                |_| Some(Target::Knob(SynthParam::Cutoff)),
            ),
            act(
                "Now the Filter envelope: turn Env amount above +3 oct. Each note opens the filter, then it closes - a \u{201c}blip\u{201d}.",
                "Env amount is in the Filter section.",
                |s| carve(s).is_some_and(|p| p.filter.env_amount_oct > 3.0),
                |_| Some(Target::Knob(SynthParam::EnvAmount)),
            ),
            act(
                "Filter Decay below 150 ms: a snappier blip.",
                "Decay under Filter envelope.",
                |s| carve(s).is_some_and(|p| p.filter_env.decay_ms < 150.0),
                |_| Some(Target::Knob(SynthParam::FilterDecay)),
            ),
            act(
                "Last: raise Amp Attack above 300 ms. Notes swell in instead of striking - that's how pads begin.",
                "Attack is the first knob under Amp envelope.",
                |s| carve(s).is_some_and(|p| p.amp_env.attack_ms > 300.0),
                |_| Some(Target::Knob(SynthParam::AmpAttack)),
            ),
            info(
                "Attack is how fast a note starts, Decay how fast it falls, Sustain the level while held, Release the tail \
                 after you let go. The Amp envelope moves loudness, the Filter envelope moves brightness.",
            ),
        ],
    },
    Lesson {
        id: CARVE_MOVEMENT,
        title: "Carve: movement",
        steps: &[
            act("Press Space: two held chords.", "Or click the play button at the top.", |s| s.playing, |_| Some(Target::Play)),
            act(
                "First darken it: Cutoff to about 700 Hz.",
                "Between 400 Hz and 1.2 kHz.",
                |s| carve(s).is_some_and(|p| (400.0..=1200.0).contains(&p.filter.cutoff_hz)),
                |_| Some(Target::Knob(SynthParam::Cutoff)),
            ),
            act(
                "LFOs move knobs for you. Drag the LFO 1 pill (top of Modulation) and drop it on the Cutoff knob.",
                "Press on \u{201c}LFO 1\u{201d}, hold, move onto Cutoff, let go.",
                |s| carve(s).is_some_and(|p| p.lfo1.target == LfoTarget::Cutoff),
                |_| Some(Target::LfoPill(1)),
            ),
            act(
                "Turn LFO 1 Depth past 50%: the filter now opens and closes on its own.",
                "Depth is under LFO 1 in Modulation.",
                |s| carve(s).is_some_and(|p| p.lfo1.target == LfoTarget::Cutoff && p.lfo1.depth > 0.5),
                |_| Some(Target::Knob(SynthParam::Lfo1Depth)),
            ),
            act(
                "Slow it down: LFO 1 Rate under 1 Hz, for a long sweep.",
                "Rate is next to Depth.",
                |s| carve(s).is_some_and(|p| lfo_rate_hz(p.lfo1.rate_norm) < 1.0),
                |_| Some(Target::Knob(SynthParam::Lfo1Rate)),
            ),
            act(
                "Now speed it up past 5 Hz: a wobble.",
                "Rate up.",
                |s| carve(s).is_some_and(|p| lfo_rate_hz(p.lfo1.rate_norm) > 5.0),
                |_| Some(Target::Knob(SynthParam::Lfo1Rate)),
            ),
            act(
                "Make it wide: Unison Voices to 3 or more - detuned copies of every note, spread left and right.",
                "Voices, under Unison.",
                |s| carve(s).is_some_and(|p| p.unison.voices >= 3),
                |_| Some(Target::Knob(SynthParam::UnisonVoices)),
            ),
            act(
                "Add space: Reverb mix past 30%.",
                "Reverb, under Effects.",
                |s| carve(s).is_some_and(|p| p.fx.reverb_mix > 0.3),
                |_| Some(Target::Knob(SynthParam::ReverbMix)),
            ),
            info(
                "An LFO is a slow wave that moves another control: on Cutoff it's a sweep or wobble, on Pitch it's vibrato. \
                 Unison and reverb make any sound bigger.",
            ),
        ],
    },
    Lesson {
        id: RECIPE_BASS,
        title: "Recipe: deep bass",
        steps: &[
            act("Press Space: a beat and a plain saw bass.", "Or click the play button at the top.", |s| s.playing, |_| Some(Target::Play)),
            act(
                "Weight first: Oscillator 1 Octave to -1.",
                "The Octave knob under Oscillator 1, one step down.",
                |s| carve(s).is_some_and(|p| p.osc1.octave == -1),
                |_| Some(Target::Knob(SynthParam::Osc1Octave)),
            ),
            act(
                "Sub up to about -6 dB: the sine underneath is what you feel on big speakers.",
                "Above -9 dB.",
                |s| carve(s).is_some_and(|p| p.mix.sub_db > -9.0),
                |_| Some(Target::Knob(SynthParam::SubLevel)),
            ),
            act(
                "Close the filter: Cutoff to about 250 Hz. Bass wants weight, not fizz.",
                "Between 150 and 450 Hz.",
                |s| carve(s).is_some_and(|p| (150.0..=450.0).contains(&p.filter.cutoff_hz)),
                |_| Some(Target::Knob(SynthParam::Cutoff)),
            ),
            act(
                "Make each note punch: Env amount about +2.5 oct and Filter Decay about 200 ms.",
                "Env amount +1.5 to +3.5 oct; Filter Decay 100 to 350 ms.",
                |s| carve(s).is_some_and(|p| (1.5..=3.5).contains(&p.filter.env_amount_oct) && (100.0..=350.0).contains(&p.filter_env.decay_ms)),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((1.5..=3.5).contains(&p.filter.env_amount_oct), Target::Knob(SynthParam::EnvAmount)),
                        ((100.0..=350.0).contains(&p.filter_env.decay_ms), Target::Knob(SynthParam::FilterDecay)),
                    ])
                },
            ),
            act(
                "Grit: Drive past 6 dB.",
                "Drive is in the Filter section.",
                |s| carve(s).is_some_and(|p| p.filter.drive_db > 6.0),
                |_| Some(Target::Knob(SynthParam::Drive)),
            ),
            act(
                "Switch to Mono and set Glide to about 50 ms: notes slide into each other instead of stacking up.",
                "Mono is at the top right of Carve; Glide is under Output (20 to 120 ms).",
                |s| carve(s).is_some_and(|p| p.voice_mode == VoiceMode::Mono && (20.0..=120.0).contains(&p.output.glide_ms)),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        (p.voice_mode == VoiceMode::Mono, Target::VoiceMode),
                        ((20.0..=120.0).contains(&p.output.glide_ms), Target::Knob(SynthParam::Glide)),
                    ])
                },
            ),
            info(
                "Deep bass: a low octave, a sub underneath, a closed filter that punches open, and mono. \
                 Compare with Carve's Deep Rave Bass preset, which adds a detuned second saw.",
            ),
        ],
    },
    Lesson {
        id: RECIPE_FLUTE,
        title: "Recipe: flute",
        steps: &[
            act("Press Space: a slow melody, on a buzzy saw for now.", "Or click the play button at the top.", |s| s.playing, |_| Some(Target::Play)),
            act(
                "A flute is almost pure: set Oscillator 1 to the triangle.",
                "The second wave shape above Oscillator 1.",
                |s| carve(s).is_some_and(|p| p.osc1.waveform == Waveform::Triangle),
                |_| Some(Target::OscWave(1)),
            ),
            act(
                "Breath: Noise to about -26 dB.",
                "Between -34 and -18 dB, in the Mixer.",
                |s| carve(s).is_some_and(|p| (-34.0..=-18.0).contains(&p.mix.noise_db)),
                |_| Some(Target::Knob(SynthParam::NoiseLevel)),
            ),
            act(
                "Soften it: Cutoff about 2.5 kHz, so the breath isn't hissy.",
                "Between 1.5 and 3.5 kHz.",
                |s| carve(s).is_some_and(|p| (1500.0..=3500.0).contains(&p.filter.cutoff_hz)),
                |_| Some(Target::Knob(SynthParam::Cutoff)),
            ),
            act(
                "Blow into it: Amp Attack about 80 ms, so each note breathes in.",
                "Between 50 and 150 ms.",
                |s| carve(s).is_some_and(|p| (50.0..=150.0).contains(&p.amp_env.attack_ms)),
                |_| Some(Target::Knob(SynthParam::AmpAttack)),
            ),
            act(
                "Vibrato: LFO 2 already points at Pitch. Set its Rate to about 5 Hz and Depth to about 20%.",
                "Rate 4 to 7 Hz, Depth 10 to 35%, under LFO 2.",
                |s| carve(s).is_some_and(vibrato),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((4.0..=7.0).contains(&lfo_rate_hz(p.lfo2.rate_norm)), Target::Knob(SynthParam::Lfo2Rate)),
                        ((0.1..=0.35).contains(&p.lfo2.depth), Target::Knob(SynthParam::Lfo2Depth)),
                    ])
                },
            ),
            act(
                "A room to play in: Reverb mix about 30%.",
                "Between 20 and 50%.",
                |s| carve(s).is_some_and(|p| (0.2..=0.5).contains(&p.fx.reverb_mix)),
                |_| Some(Target::Knob(SynthParam::ReverbMix)),
            ),
            info(
                "Flute: a soft wave, a breath of noise, a gentle attack and vibrato. The same idea - soft wave, \
                 slow attack, vibrato - gives you recorders, ocarinas and the bansuri.",
            ),
        ],
    },
    Lesson {
        id: RECIPE_HARP,
        title: "Recipe: Indian harp",
        steps: &[
            act(
                "Press Space. This cascade uses the notes of raga Malkauns (A, C, D, F, G), a late-night raga.",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            act(
                "Strings are bright: keep the saw on Oscillator 1, and bring in Oscillator 2 as a square, one octave up, at about -10 dB.",
                "Osc 2 wave: square. Octave: +1. Mixer Osc 2: above -14 dB.",
                |s| carve(s).is_some_and(|p| p.osc2.waveform == Waveform::Square && p.osc2.octave == 1 && p.mix.osc2_db > -14.0),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        (p.osc2.waveform == Waveform::Square, Target::OscWave(2)),
                        (p.osc2.octave == 1, Target::Knob(SynthParam::Osc2Octave)),
                        (p.mix.osc2_db > -14.0, Target::Knob(SynthParam::Osc2Level)),
                    ])
                },
            ),
            act(
                "The pluck: Amp Attack all the way down (under 5 ms), Sustain to 0, Decay about 1 second.",
                "Decay between 0.6 and 1.6 s.",
                |s| carve(s).is_some_and(|p| p.amp_env.attack_ms < 5.0 && p.amp_env.sustain < 0.05 && (600.0..=1600.0).contains(&p.amp_env.decay_ms)),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        (p.amp_env.attack_ms < 5.0, Target::Knob(SynthParam::AmpAttack)),
                        (p.amp_env.sustain < 0.05, Target::Knob(SynthParam::AmpSustain)),
                        ((600.0..=1600.0).contains(&p.amp_env.decay_ms), Target::Knob(SynthParam::AmpDecay)),
                    ])
                },
            ),
            act(
                "Let the strings ring after each note: Amp Release about 1 second.",
                "Between 0.6 and 1.6 s.",
                |s| carve(s).is_some_and(|p| (600.0..=1600.0).contains(&p.amp_env.release_ms)),
                |_| Some(Target::Knob(SynthParam::AmpRelease)),
            ),
            act(
                "A string is brightest when struck, then mellows: Cutoff about 1 kHz, Env amount about +3 oct, Filter Decay about 300 ms.",
                "Cutoff 0.6 to 1.8 kHz, Env amount +2 to +4 oct, Filter Decay 150 to 500 ms.",
                |s| {
                    carve(s).is_some_and(|p| {
                        (600.0..=1800.0).contains(&p.filter.cutoff_hz)
                            && (2.0..=4.0).contains(&p.filter.env_amount_oct)
                            && (150.0..=500.0).contains(&p.filter_env.decay_ms)
                    })
                },
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((600.0..=1800.0).contains(&p.filter.cutoff_hz), Target::Knob(SynthParam::Cutoff)),
                        ((2.0..=4.0).contains(&p.filter.env_amount_oct), Target::Knob(SynthParam::EnvAmount)),
                        ((150.0..=500.0).contains(&p.filter_env.decay_ms), Target::Knob(SynthParam::FilterDecay)),
                    ])
                },
            ),
            act(
                "Shimmer: Chorus mix about 30%.",
                "Between 20 and 50%, under Effects.",
                |s| carve(s).is_some_and(|p| (0.2..=0.5).contains(&p.fx.chorus_mix)),
                |_| Some(Target::Knob(SynthParam::ChorusMix)),
            ),
            act(
                "Space around it: Reverb mix about 40%, and Size above 70%.",
                "Reverb mix 30 to 60%; Size over 70%.",
                |s| carve(s).is_some_and(|p| (0.3..=0.6).contains(&p.fx.reverb_mix) && p.fx.reverb_size > 0.7),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((0.3..=0.6).contains(&p.fx.reverb_mix), Target::Knob(SynthParam::ReverbMix)),
                        (p.fx.reverb_size > 0.7, Target::Knob(SynthParam::ReverbSize)),
                    ])
                },
            ),
            info(
                "A plucked-string sound in the spirit of the swarmandal and santoor: bright attack, a filter that \
                 mellows, a long ring and a big room. Write your own cascade: Malkauns has no E and no B.",
            ),
        ],
    },
    Lesson {
        id: RECIPE_LEAD,
        title: "Recipe: lead melody",
        steps: &[
            act("Press Space: a beat and a hook on a plain saw.", "Or click the play button at the top.", |s| s.playing, |_| Some(Target::Play)),
            act(
                "A lead has to cut through: Oscillator 1 to the square, and Osc 2 up to about -8 dB, detuned about +7 cents.",
                "Osc 1 wave: square. Mixer Osc 2: above -12 dB. Osc 2 Detune: +3 to +15 cents.",
                |s| {
                    carve(s).is_some_and(|p| {
                        p.osc1.waveform == Waveform::Square && p.mix.osc2_db > -12.0 && (3.0..=15.0).contains(&p.osc2.knob_a_cents)
                    })
                },
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        (p.osc1.waveform == Waveform::Square, Target::OscWave(1)),
                        (p.mix.osc2_db > -12.0, Target::Knob(SynthParam::Osc2Level)),
                        ((3.0..=15.0).contains(&p.osc2.knob_a_cents), Target::Knob(SynthParam::Osc2Detune)),
                    ])
                },
            ),
            act(
                "Bright but not harsh: Cutoff about 3 kHz, Resonance about 30%.",
                "Cutoff 1.8 to 4.5 kHz; Resonance 20 to 45%.",
                |s| carve(s).is_some_and(|p| (1800.0..=4500.0).contains(&p.filter.cutoff_hz) && (0.2..=0.45).contains(&p.filter.resonance)),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((1800.0..=4500.0).contains(&p.filter.cutoff_hz), Target::Knob(SynthParam::Cutoff)),
                        ((0.2..=0.45).contains(&p.filter.resonance), Target::Knob(SynthParam::Resonance)),
                    ])
                },
            ),
            act(
                "Switch to Mono with Glide about 60 ms: notes slide like a voice.",
                "Mono at the top right of Carve; Glide 30 to 120 ms, under Output.",
                |s| carve(s).is_some_and(|p| p.voice_mode == VoiceMode::Mono && (30.0..=120.0).contains(&p.output.glide_ms)),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        (p.voice_mode == VoiceMode::Mono, Target::VoiceMode),
                        ((30.0..=120.0).contains(&p.output.glide_ms), Target::Knob(SynthParam::Glide)),
                    ])
                },
            ),
            act(
                "Expression: vibrato on LFO 2 - Rate about 5 Hz, Depth about 15%.",
                "Rate 4 to 7 Hz, Depth 10 to 35%.",
                |s| carve(s).is_some_and(vibrato),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((4.0..=7.0).contains(&lfo_rate_hz(p.lfo2.rate_norm)), Target::Knob(SynthParam::Lfo2Rate)),
                        ((0.1..=0.35).contains(&p.lfo2.depth), Target::Knob(SynthParam::Lfo2Depth)),
                    ])
                },
            ),
            act(
                "Polish: Chorus mix about 20% and Reverb mix about 25%.",
                "Chorus 10 to 40%; Reverb 15 to 40%.",
                |s| carve(s).is_some_and(|p| (0.1..=0.4).contains(&p.fx.chorus_mix) && (0.15..=0.4).contains(&p.fx.reverb_mix)),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((0.1..=0.4).contains(&p.fx.chorus_mix), Target::Knob(SynthParam::ChorusMix)),
                        ((0.15..=0.4).contains(&p.fx.reverb_mix), Target::Knob(SynthParam::ReverbMix)),
                    ])
                },
            ),
            info(
                "Lead: two detuned oscillators, a bright filter, mono glide and vibrato. Swap the hook for your own \
                 melody - double-click the clip to open it.",
            ),
        ],
    },
];

/// The on-screen Carve patch - only while a Carve track is selected (the
/// panel shows the selected track's patch).
fn carve(s: &Snapshot) -> Option<&SynthState> {
    selected(s).filter(|t| t.instrument == Some(Instrument::Carve)).map(|_| &s.synth)
}

/// The first control in a multi-knob step that isn't set yet.
fn first_unmet(controls: &[(bool, Target)]) -> Option<Target> {
    controls.iter().find(|(done, _)| !done).map(|&(_, t)| t)
}

/// LFO 2 on pitch at a vibrato rate and depth.
fn vibrato(p: &SynthState) -> bool {
    p.lfo2.target == LfoTarget::Pitch && (4.0..=7.0).contains(&lfo_rate_hz(p.lfo2.rate_norm)) && (0.1..=0.35).contains(&p.lfo2.depth)
}

/// Clips on tracks playing `instrument`.
fn clips_on(s: &Snapshot, instrument: Instrument) -> impl Iterator<Item = &Clip> {
    let tracks: Vec<_> = tracks_with(s, instrument).map(|t| t.id).collect();
    s.arrangement.clips.iter().filter(move |c| tracks.contains(&c.track) && matches!(c.content, ClipContent::Midi { .. }))
}

/// Lesson 3's clips: on a Carve track that isn't the pre-built bass.
fn chord_clips(s: &Snapshot) -> impl Iterator<Item = &Clip> {
    let tracks: Vec<_> = tracks_with(s, Instrument::Carve).filter(|t| t.name != "Bass").map(|t| t.id).collect();
    s.arrangement.clips.iter().filter(move |c| tracks.contains(&c.track))
}

/// The row to click - or, with no clip open in the editor, the lane
/// holding the clip (double-clicking it opens the grid).
fn row_or_clip(s: &Snapshot, instrument: Instrument, pitch: u8) -> Option<Target> {
    if s.open_clip.is_some() {
        return Some(Target::PianoRollRow(pitch));
    }
    clips_on(s, instrument).next().map(|c| Target::Lane(c.track))
}

/// Whether `clip`'s pattern has `pitch` at every one of `starts`.
fn has_notes(clip: &Clip, pitch: u8, starts: &[Ticks]) -> bool {
    let ClipContent::Midi { notes, .. } = &clip.content else { return false };
    starts.iter().all(|&at| notes.iter().any(|n| n.pitch == pitch && n.start == at))
}

fn drum_pattern_has(s: &Snapshot, pitch: u8, starts: &[Ticks]) -> bool {
    clips_on(s, Instrument::Drums).any(|c| has_notes(c, pitch, starts))
}

fn carve_pattern_has(s: &Snapshot, pitch: u8, starts: &[Ticks]) -> bool {
    clips_on(s, Instrument::Carve).any(|c| has_notes(c, pitch, starts))
}

fn chord_at(s: &Snapshot, pitches: &[u8], starts: &[Ticks]) -> bool {
    chord_clips(s).any(|c| pitches.iter().all(|&p| has_notes(c, p, starts)))
}

/// The first chord note still to write (for the glow), if any.
fn first_missing(s: &Snapshot, pitches: &[u8], starts: &[Ticks]) -> Option<u8> {
    let clip = chord_clips(s).next()?;
    pitches.iter().copied().find(|&p| !has_notes(clip, p, starts))
}

/// A looping clip at least `bars` long.
fn loops(clip: &Clip, bars: i64) -> bool {
    matches!(clip.content, ClipContent::Midi { loop_len: Some(_), .. }) && clip.length >= bars * BAR
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::arrangement::{Command, MidiNote, Track, TrackKind, DEFAULT_TRACK_HEIGHT};
    use shared::lessons::starting_project;

    /// The app state a lesson starts in.
    fn start(id: &str) -> Snapshot {
        let p = starting_project(id);
        // As `LessonEvent::Begin` leaves it: the last track selected, its
        // patch on screen.
        let selected = p.arrangement.tracks.last().map(|t| t.id);
        let synth = selected
            .and_then(|id| p.instruments.iter().find(|(t, _)| *t == id).map(|(_, patch)| patch.clone()))
            .unwrap_or_else(shared::synth::seed_synth);
        Snapshot { arrangement: p.arrangement, selected_track: selected, playing: false, synth, open_clip: None }
    }

    fn lesson(id: &str) -> &'static Lesson {
        LESSONS.iter().find(|l| l.id == id).unwrap()
    }

    /// Runs `lesson` step by step: before `do_step[i]`, step i must not
    /// pass yet; after it, it must. The last step is the closing info.
    fn walk(id: &str, do_step: &[&dyn Fn(&mut Snapshot)]) {
        let l = lesson(id);
        let mut s = start(id);
        assert_eq!(do_step.len(), l.steps.len() - 1, "{id}: one action per step");
        for (i, (step, act)) in l.steps.iter().zip(do_step).enumerate() {
            let Kind::Action { check, target } = step.kind else { panic!("{id} step {i} should be an action") };
            assert!(!check(&s), "{id} step {} already passes before it's done: {}", i + 1, step.text);
            let _ = target(&s);
            act(&mut s);
            assert!(check(&s), "{id} step {} doesn't pass after doing it: {}", i + 1, step.text);
        }
        assert!(matches!(l.steps.last().unwrap().kind, Kind::Info));
    }

    fn add_midi_track(s: &mut Snapshot, name: &str) {
        add_track(s, name, Instrument::default_for(TrackKind::Midi));
    }

    fn add_track(s: &mut Snapshot, name: &str, instrument: Option<Instrument>) {
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
        };
        let index = s.arrangement.tracks.len();
        Command::InsertTrack { track: Box::new(track), index, clips: vec![], automation: vec![] }.apply(&mut s.arrangement);
        s.selected_track = Some(id);
    }

    /// What a double-click on an empty lane does: a one-bar empty clip on
    /// the selected track.
    fn draw_clip(s: &mut Snapshot) {
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
        };
        Command::InsertClip { clip: Box::new(clip) }.apply(&mut s.arrangement);
    }

    fn add_notes(s: &mut Snapshot, pitch: u8, starts: &[Ticks]) {
        let clip = last_clip(s);
        for &start in starts {
            let note = MidiNote { start, length: PPQ / 4, pitch, velocity: 100 };
            Command::AddMidiNote { clip, note }.apply(&mut s.arrangement);
        }
    }

    /// Dragging the open clip's right edge out to bar 9.
    fn stretch(s: &mut Snapshot) {
        let clip = s.arrangement.clip(last_clip(s)).unwrap().extended_as_loop(8 * BAR).unwrap();
        Command::ReplaceClip { clip: Box::new(clip) }.apply(&mut s.arrangement);
    }

    /// The clip the learner just drew (the newest one).
    fn last_clip(s: &Snapshot) -> shared::arrangement::ClipId {
        s.arrangement.clips.last().unwrap().id
    }

    const OFFBEATS: [Ticks; 4] = [PPQ / 2, PPQ + PPQ / 2, 2 * PPQ + PPQ / 2, 3 * PPQ + PPQ / 2];

    #[test]
    fn lesson_one_can_be_done_step_by_step() {
        walk(
            FIRST_BEAT,
            &[
                &|s| add_track(s, "Drums", Some(Instrument::Drums)),
                &draw_clip,
                &|s| add_notes(s, KICK, &[0, PPQ, 2 * PPQ, 3 * PPQ]),
                &|s| s.playing = true,
                &|s| add_notes(s, CLAP, &[PPQ, 3 * PPQ]),
                &|s| add_notes(s, OPEN_HAT, &OFFBEATS),
                &stretch,
            ],
        );
    }

    #[test]
    fn lesson_two_can_be_done_step_by_step() {
        walk(
            BASSLINE,
            &[
                &|s| add_midi_track(s, "MIDI 1"),
                &|s| s.synth = shared::synth::deep_rave_bass(),
                &|s| s.synth.held_notes.push(48),
                &draw_clip,
                &|s| add_notes(s, BASS_NOTE, &OFFBEATS),
                &|s| s.playing = true,
                &stretch,
            ],
        );
    }

    #[test]
    fn lesson_three_can_be_done_step_by_step() {
        walk(
            CHORDS,
            &[
                &|s| add_midi_track(s, "MIDI 1"),
                &|s| s.synth = shared::synth::PRESETS.iter().find(|p| p.0 == "Soft Pad").unwrap().1(),
                &draw_clip,
                &|s| {
                    let mut clip = s.arrangement.clip(last_clip(s)).unwrap().clone();
                    clip.length = 2 * BAR;
                    clip.content = ClipContent::Midi { notes: vec![], loop_len: Some(2 * BAR), link: None };
                    Command::ReplaceClip { clip: Box::new(clip) }.apply(&mut s.arrangement);
                },
                &|s| {
                    for p in [57, 60, 64] {
                        add_notes(s, p, &[0, 2 * PPQ]);
                    }
                },
                &|s| {
                    for p in [60, 64, 67] {
                        add_notes(s, p, &[4 * PPQ, 6 * PPQ]);
                    }
                },
                &|s| s.playing = true,
            ],
        );
    }

    fn play(s: &mut Snapshot) {
        s.playing = true;
    }

    /// Sets `param` so its knob reads `value` (in the knob's own units,
    /// via the same table the knob uses).
    fn knob(s: &mut Snapshot, param: SynthParam, norm: f32) {
        param.apply_norm(&mut s.synth, norm);
    }

    fn hz_norm(hz: f32) -> f32 {
        (hz / 0.05).ln() / (20.0f32 / 0.05).ln()
    }

    #[test]
    fn carve_waves_can_be_done_step_by_step() {
        walk(
            CARVE_WAVES,
            &[
                &play,
                &|s| s.synth.osc1.waveform = Waveform::Square,
                &|s| s.synth.osc1.waveform = Waveform::Triangle,
                &|s| s.synth.osc1.waveform = Waveform::Sine,
                &|s| s.synth.osc1.waveform = Waveform::Saw,
            ],
        );
    }

    #[test]
    fn carve_mix_can_be_done_step_by_step() {
        walk(
            CARVE_MIX,
            &[
                &play,
                &|s| s.synth.mix.osc2_db = -8.0,
                &|s| s.synth.osc2.knob_a_cents = 10.0,
                &|s| s.synth.osc2.octave = 1,
                &|s| s.synth.mix.sub_db = -8.0,
                &|s| s.synth.mix.noise_db = -30.0,
            ],
        );
    }

    #[test]
    fn carve_filter_can_be_done_step_by_step() {
        walk(
            CARVE_FILTER,
            &[
                &play,
                &|s| s.synth.filter.cutoff_hz = 300.0,
                &|s| s.synth.filter.resonance = 0.8,
                &|s| s.synth.filter.cutoff_hz = 4000.0,
                &|s| s.synth.filter.filter_type = FilterType::Hp,
                &|s| s.synth.filter.filter_type = FilterType::Lp24,
            ],
        );
    }

    #[test]
    fn carve_envelopes_can_be_done_step_by_step() {
        walk(
            CARVE_ENVELOPES,
            &[
                &play,
                &|s| s.synth.amp_env.sustain = 0.0,
                &|s| s.synth.amp_env.decay_ms = 120.0,
                &|s| s.synth.filter.cutoff_hz = 1000.0,
                &|s| s.synth.filter.env_amount_oct = 3.5,
                &|s| s.synth.filter_env.decay_ms = 120.0,
                &|s| s.synth.amp_env.attack_ms = 400.0,
            ],
        );
    }

    #[test]
    fn carve_movement_can_be_done_step_by_step() {
        walk(
            CARVE_MOVEMENT,
            &[
                &play,
                &|s| s.synth.filter.cutoff_hz = 700.0,
                &|s| s.synth.lfo1.target = LfoTarget::Cutoff,
                &|s| s.synth.lfo1.depth = 0.7,
                &|s| knob(s, SynthParam::Lfo1Rate, hz_norm(0.5)),
                &|s| knob(s, SynthParam::Lfo1Rate, hz_norm(6.0)),
                &|s| s.synth.unison.voices = 3,
                &|s| s.synth.fx.reverb_mix = 0.4,
            ],
        );
    }

    #[test]
    fn recipe_bass_can_be_done_step_by_step() {
        walk(
            RECIPE_BASS,
            &[
                &play,
                &|s| s.synth.osc1.octave = -1,
                &|s| s.synth.mix.sub_db = -6.0,
                &|s| s.synth.filter.cutoff_hz = 250.0,
                &|s| {
                    s.synth.filter.env_amount_oct = 2.5;
                    s.synth.filter_env.decay_ms = 200.0;
                },
                &|s| s.synth.filter.drive_db = 9.0,
                &|s| {
                    s.synth.voice_mode = VoiceMode::Mono;
                    s.synth.output.glide_ms = 50.0;
                },
            ],
        );
    }

    #[test]
    fn recipe_flute_can_be_done_step_by_step() {
        walk(
            RECIPE_FLUTE,
            &[
                &play,
                &|s| s.synth.osc1.waveform = Waveform::Triangle,
                &|s| s.synth.mix.noise_db = -26.0,
                &|s| s.synth.filter.cutoff_hz = 2500.0,
                &|s| s.synth.amp_env.attack_ms = 80.0,
                &|s| {
                    knob(s, SynthParam::Lfo2Rate, hz_norm(5.0));
                    s.synth.lfo2.depth = 0.2;
                },
                &|s| s.synth.fx.reverb_mix = 0.3,
            ],
        );
    }

    #[test]
    fn recipe_harp_can_be_done_step_by_step() {
        walk(
            RECIPE_HARP,
            &[
                &play,
                &|s| {
                    s.synth.osc2.waveform = Waveform::Square;
                    s.synth.osc2.octave = 1;
                    s.synth.mix.osc2_db = -10.0;
                },
                &|s| {
                    s.synth.amp_env.attack_ms = 1.0;
                    s.synth.amp_env.sustain = 0.0;
                    s.synth.amp_env.decay_ms = 1000.0;
                },
                &|s| s.synth.amp_env.release_ms = 1000.0,
                &|s| {
                    s.synth.filter.cutoff_hz = 1000.0;
                    s.synth.filter.env_amount_oct = 3.0;
                    s.synth.filter_env.decay_ms = 300.0;
                },
                &|s| s.synth.fx.chorus_mix = 0.3,
                &|s| {
                    s.synth.fx.reverb_mix = 0.4;
                    s.synth.fx.reverb_size = 0.8;
                },
            ],
        );
    }

    #[test]
    fn recipe_lead_can_be_done_step_by_step() {
        walk(
            RECIPE_LEAD,
            &[
                &play,
                &|s| {
                    s.synth.osc1.waveform = Waveform::Square;
                    s.synth.mix.osc2_db = -8.0;
                    s.synth.osc2.knob_a_cents = 7.0;
                },
                &|s| {
                    s.synth.filter.cutoff_hz = 3000.0;
                    s.synth.filter.resonance = 0.3;
                },
                &|s| {
                    s.synth.voice_mode = VoiceMode::Mono;
                    s.synth.output.glide_ms = 60.0;
                },
                &|s| {
                    knob(s, SynthParam::Lfo2Rate, hz_norm(5.0));
                    s.synth.lfo2.depth = 0.15;
                },
                &|s| {
                    s.synth.fx.chorus_mix = 0.2;
                    s.synth.fx.reverb_mix = 0.25;
                },
            ],
        );
    }

    #[test]
    fn carve_steps_need_the_carve_track_selected() {
        // With nothing selected the panel isn't Carve's, so no knob step
        // can pass, whatever the patch holds.
        let mut s = start(CARVE_FILTER);
        s.synth.filter.cutoff_hz = 100.0;
        s.selected_track = None;
        let Kind::Action { check, .. } = lesson(CARVE_FILTER).steps[1].kind else { panic!() };
        assert!(!check(&s));
    }

    #[test]
    fn preset_names_the_checks_use_exist() {
        for name in ["Deep Rave Bass", "Soft Pad"] {
            assert!(shared::synth::PRESETS.iter().any(|p| p.0 == name && p.1().name == name), "{name}");
        }
    }

    #[test]
    fn every_lesson_ends_with_info_and_has_a_starting_project() {
        for l in LESSONS {
            assert!(matches!(l.steps.last().unwrap().kind, Kind::Info), "{}", l.id);
            let _ = starting_project(l.id);
        }
    }
}
