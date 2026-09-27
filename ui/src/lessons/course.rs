//! The course: every lesson's steps, their text, what each one checks and
//! which control glows. Text lives only here, so translating the course
//! is a change to this table.

use shared::arrangement::{Clip, ClipContent, Instrument, Ticks, PPQ};
use shared::drums::{CLAP, CLOSED_HAT, KICK, OPEN_HAT, SNARE};
use shared::lessons::{
    BAR, BASSLINE, BASS_NOTE, CARVE_ENVELOPES, CARVE_FILTER, CARVE_MIX, CARVE_MOVEMENT, CARVE_WAVES, CHORDS, FIRST_BEAT,
    RECIPE_BASS, RECIPE_FLUTE, RECIPE_HARP, RECIPE_LEAD, RECIPE_PAD, RECIPE_TANPURA, RECIPE_REED, ARRANGE_HOUSE, ARRANGE_BHAIRAV,
    PROJECT_ARRANGE, PROJECT_BASS, PROJECT_BASS_ROOTS, PROJECT_CHORDS, PROJECT_CHORDS_NOTES, PROJECT_FINISH, PROJECT_GROOVE,
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
    /// Why this step sounds the way it does - shown once it's done, while
    /// the change is still in your ears. Empty for most non-recipe steps.
    pub why: &'static str,
    /// Shown if the step hasn't been done after a while.
    pub hint: &'static str,
    pub kind: Kind,
}

pub struct Lesson {
    /// Matches `shared::lessons` (starting project) and the saved "done" list.
    pub id: &'static str,
    /// The sidebar heading it's listed under.
    pub group: &'static str,
    pub title: &'static str,
    pub steps: &'static [Step],
}

pub const BASICS: &str = "Basics";
pub const CARVE: &str = "Carve synth";
pub const RECIPES: &str = "Recipes";
pub const ARRANGEMENT: &str = "Arrangement";
pub const PROJECTS: &str = "Projects";

const fn act(text: &'static str, hint: &'static str, check: fn(&Snapshot) -> bool, target: fn(&Snapshot) -> Option<Target>) -> Step {
    Step { text, why: "", hint, kind: Kind::Action { check, target } }
}

/// A recipe step: like `act`, plus the reason it sounds that way.
const fn recipe(
    text: &'static str,
    why: &'static str,
    hint: &'static str,
    check: fn(&Snapshot) -> bool,
    target: fn(&Snapshot) -> Option<Target>,
) -> Step {
    Step { text, why, hint, kind: Kind::Action { check, target } }
}

const fn info(text: &'static str) -> Step {
    Step { text, why: "", hint: "", kind: Kind::Info }
}

pub const LESSONS: &[Lesson] = &[
    Lesson {
        id: FIRST_BEAT,
        group: BASICS,
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
        group: BASICS,
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
        group: BASICS,
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
        group: CARVE,
        title: "Waves",
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
        group: CARVE,
        title: "Mixing oscillators",
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
        group: CARVE,
        title: "The filter",
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
        group: CARVE,
        title: "Envelopes",
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
        group: CARVE,
        title: "Movement",
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
        group: RECIPES,
        title: "Deep bass",
        steps: &[
            act("Press Space: a beat and a plain saw bass.", "Or click the play button at the top.", |s| s.playing, |_| Some(Target::Play)),
            recipe(
                "Weight first: Oscillator 1 Octave to -1.",
                "An octave down halves the pitch, moving the note into the range you feel in your chest more than hear.",
                "The Octave knob under Oscillator 1, one step down.",
                |s| carve(s).is_some_and(|p| p.osc1.octave == -1),
                |_| Some(Target::Knob(SynthParam::Osc1Octave)),
            ),
            recipe(
                "Sub up to about -6 dB: the sine underneath is what you feel on big speakers.",
                "The sub is a pure sine an octave below. It adds weight without buzz, because a sine has no harmonics to clutter the mix.",
                "Above -9 dB.",
                |s| carve(s).is_some_and(|p| p.mix.sub_db > -9.0),
                |_| Some(Target::Knob(SynthParam::SubLevel)),
            ),
            recipe(
                "Close the filter: Cutoff to about 250 Hz. Bass wants weight, not fizz.",
                "A saw's upper harmonics are the \u{201c}fizz\u{201d}. Closing the low-pass filter removes them and leaves the round low end.",
                "Between 150 and 450 Hz.",
                |s| carve(s).is_some_and(|p| (150.0..=450.0).contains(&p.filter.cutoff_hz)),
                |_| Some(Target::Knob(SynthParam::Cutoff)),
            ),
            recipe(
                "Make each note punch: Env amount about +2.5 oct and Filter Decay about 200 ms.",
                "The filter envelope throws the filter open at each note and closes it within 200 ms: a burst of brightness your ear reads as a punch.",
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
            recipe(
                "Grit: Drive past 6 dB.",
                "Drive pushes the sound into the filter harder, adding harmonics back as grit, so the bass still cuts through on phone and laptop speakers.",
                "Drive is in the Filter section.",
                |s| carve(s).is_some_and(|p| p.filter.drive_db > 6.0),
                |_| Some(Target::Knob(SynthParam::Drive)),
            ),
            recipe(
                "Switch to Mono and set Glide to about 50 ms: notes slide into each other instead of stacking up.",
                "Mono plays one note at a time, so overlaps can't pile into mud; glide slides the pitch between notes, like a bassist's finger.",
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
        id: RECIPE_PAD,
        group: RECIPES,
        title: "Soft pad",
        steps: &[
            act("Press Space: two held chords on a plain saw.", "Or click the play button at the top.", |s| s.playing, |_| Some(Target::Play)),
            recipe(
                "Two saws, slightly apart: Osc 2 up to about -6 dB, and Osc 2 Detune about +9 cents.",
                "Two saws a few cents apart drift in and out of step with each other - a slow, breathing movement that makes a pad feel alive rather than static.",
                "Mixer Osc 2 above -9 dB; Detune +5 to +15 cents.",
                |s| carve(s).is_some_and(|p| p.mix.osc2_db > -9.0 && (5.0..=15.0).contains(&p.osc2.knob_a_cents)),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        (p.mix.osc2_db > -9.0, Target::Knob(SynthParam::Osc2Level)),
                        ((5.0..=15.0).contains(&p.osc2.knob_a_cents), Target::Knob(SynthParam::Osc2Detune)),
                    ])
                },
            ),
            recipe(
                "Take the edge off: Cutoff about 900 Hz, Resonance low (under 20%).",
                "A pad sits behind everything else, so it shouldn't fight the lead for the highs. Low-pass it, and keep resonance down so no single frequency pokes out.",
                "Cutoff 600 Hz to 1.3 kHz; Resonance under 20%.",
                |s| carve(s).is_some_and(|p| (600.0..=1300.0).contains(&p.filter.cutoff_hz) && p.filter.resonance < 0.2),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((600.0..=1300.0).contains(&p.filter.cutoff_hz), Target::Knob(SynthParam::Cutoff)),
                        (p.filter.resonance < 0.2, Target::Knob(SynthParam::Resonance)),
                    ])
                },
            ),
            recipe(
                "Swell in, fade out: Amp Attack about 500 ms and Release about 1.2 s.",
                "A slow attack means no hard start - each chord fades in like a string section. The long release lets it hang over into the next chord, so the changes blur together.",
                "Attack 300 to 900 ms; Release 0.8 to 2 s.",
                |s| carve(s).is_some_and(|p| (300.0..=900.0).contains(&p.amp_env.attack_ms) && (800.0..=2000.0).contains(&p.amp_env.release_ms)),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((300.0..=900.0).contains(&p.amp_env.attack_ms), Target::Knob(SynthParam::AmpAttack)),
                        ((800.0..=2000.0).contains(&p.amp_env.release_ms), Target::Knob(SynthParam::AmpRelease)),
                    ])
                },
            ),
            recipe(
                "Let it bloom: Env amount about +1.5 oct, and Filter Attack about 800 ms.",
                "With a slow filter attack, the filter opens gradually while each chord holds, so the pad gets brighter over time - movement without touching a knob.",
                "Env amount +1 to +2.5 oct; Filter Attack 400 ms to 1.5 s.",
                |s| carve(s).is_some_and(|p| (1.0..=2.5).contains(&p.filter.env_amount_oct) && (400.0..=1500.0).contains(&p.filter_env.attack_ms)),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((1.0..=2.5).contains(&p.filter.env_amount_oct), Target::Knob(SynthParam::EnvAmount)),
                        ((400.0..=1500.0).contains(&p.filter_env.attack_ms), Target::Knob(SynthParam::FilterAttack)),
                    ])
                },
            ),
            recipe(
                "Make it wide: Unison Voices to 3 or more, Unison Detune about 18 cents.",
                "Unison stacks several detuned copies of every note and spreads them left and right - the width that makes a pad surround you instead of sitting in the middle.",
                "Voices 3+; Detune 15 to 30 cents.",
                |s| carve(s).is_some_and(|p| p.unison.voices >= 3 && (15.0..=30.0).contains(&p.unison.detune_cents)),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        (p.unison.voices >= 3, Target::Knob(SynthParam::UnisonVoices)),
                        ((15.0..=30.0).contains(&p.unison.detune_cents), Target::Knob(SynthParam::UnisonDetune)),
                    ])
                },
            ),
            recipe(
                "Shimmer and space: Chorus mix about 35%, Reverb mix about 35%, Reverb Size above 70%.",
                "Chorus adds a gentle shimmer and a big reverb puts the pad in a large room. Both blur its edges - exactly what a background sound should do.",
                "Chorus 25 to 50%; Reverb 25 to 50%; Size over 70%.",
                |s| {
                    carve(s).is_some_and(|p| {
                        (0.25..=0.5).contains(&p.fx.chorus_mix) && (0.25..=0.5).contains(&p.fx.reverb_mix) && p.fx.reverb_size > 0.7
                    })
                },
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((0.25..=0.5).contains(&p.fx.chorus_mix), Target::Knob(SynthParam::ChorusMix)),
                        ((0.25..=0.5).contains(&p.fx.reverb_mix), Target::Knob(SynthParam::ReverbMix)),
                        (p.fx.reverb_size > 0.7, Target::Knob(SynthParam::ReverbSize)),
                    ])
                },
            ),
            info(
                "Soft pad: two detuned saws, a darker filter that blooms, a slow swell, width and space. \
                 Compare it with Carve's Soft Pad preset - same ideas, a few different settings.",
            ),
        ],
    },
    Lesson {
        id: RECIPE_FLUTE,
        group: RECIPES,
        title: "Flute",
        steps: &[
            act("Press Space: a slow melody, on a buzzy saw for now.", "Or click the play button at the top.", |s| s.playing, |_| Some(Target::Play)),
            recipe(
                "A flute is almost pure: set Oscillator 1 to the triangle.",
                "A triangle's harmonics fade quickly, so it's close to a pure tone - which is what a flute's column of air produces.",
                "The second wave shape above Oscillator 1.",
                |s| carve(s).is_some_and(|p| p.osc1.waveform == Waveform::Triangle),
                |_| Some(Target::OscWave(1)),
            ),
            recipe(
                "Breath: Noise to about -26 dB.",
                "A real flute is air rushing across an edge. A little noise under the tone is that breath.",
                "Between -34 and -18 dB, in the Mixer.",
                |s| carve(s).is_some_and(|p| (-34.0..=-18.0).contains(&p.mix.noise_db)),
                |_| Some(Target::Knob(SynthParam::NoiseLevel)),
            ),
            recipe(
                "Soften it: Cutoff about 2.5 kHz, so the breath isn't hissy.",
                "Noise has energy at every frequency; cutting above ~2.5 kHz keeps the soft part that reads as breath and drops the hiss.",
                "Between 1.5 and 3.5 kHz.",
                |s| carve(s).is_some_and(|p| (1500.0..=3500.0).contains(&p.filter.cutoff_hz)),
                |_| Some(Target::Knob(SynthParam::Cutoff)),
            ),
            recipe(
                "Blow into it: Amp Attack about 80 ms, so each note breathes in.",
                "It takes a moment of breath before a flute speaks. A short fade-in copies that; an instant attack gives it away as a synth.",
                "Between 50 and 150 ms.",
                |s| carve(s).is_some_and(|p| (50.0..=150.0).contains(&p.amp_env.attack_ms)),
                |_| Some(Target::Knob(SynthParam::AmpAttack)),
            ),
            recipe(
                "Vibrato: LFO 2 already points at Pitch. Set its Rate to about 5 Hz and Depth to about 20%.",
                "Flautists add vibrato with their breath, about five wobbles a second. LFO 2 moving the pitch that fast, and only slightly, is exactly that.",
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
            recipe(
                "A room to play in: Reverb mix about 30%.",
                "Wind instruments are almost always heard in a room, and the room's echoes are part of their sound.",
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
        group: RECIPES,
        title: "Indian harp",
        steps: &[
            act(
                "Press Space. This cascade uses the notes of raga Malkauns (A, C, D, F, G), a late-night raga.",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            recipe(
                "Strings are bright: keep the saw on Oscillator 1, and bring in Oscillator 2 as a square, one octave up, at about -10 dB.",
                "A struck string is richest in harmonics at the moment it's hit. A square an octave up adds bright upper harmonics on top of the saw.",
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
            recipe(
                "The pluck: Amp Attack all the way down (under 5 ms), Sustain to 0, Decay about 1 second.",
                "A pluck is all start and fade: it speaks instantly and nothing is held, so attack near zero and sustain at zero.",
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
            recipe(
                "Let the strings ring after each note: Amp Release about 1 second.",
                "Strings keep ringing after the finger leaves them. A long release does the same, so the cascade blurs into a shimmer.",
                "Between 0.6 and 1.6 s.",
                |s| carve(s).is_some_and(|p| (600.0..=1600.0).contains(&p.amp_env.release_ms)),
                |_| Some(Target::Knob(SynthParam::AmpRelease)),
            ),
            recipe(
                "A string is brightest when struck, then mellows: Cutoff about 1 kHz, Env amount about +3 oct, Filter Decay about 300 ms.",
                "Real strings lose their high harmonics first as they ring. A filter that starts open and closes over 300 ms copies that.",
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
            recipe(
                "Shimmer: Chorus mix about 30%.",
                "Swarmandal and santoor strings come in courses of nearly-matching strings; chorus imitates that slightly-out-of-tune doubling.",
                "Between 20 and 50%, under Effects.",
                |s| carve(s).is_some_and(|p| (0.2..=0.5).contains(&p.fx.chorus_mix)),
                |_| Some(Target::Knob(SynthParam::ChorusMix)),
            ),
            recipe(
                "Space around it: Reverb mix about 40%, and Size above 70%.",
                "A big reverb lets each note hang into the next - the sustaining wash these instruments are known for.",
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
        id: RECIPE_TANPURA,
        group: RECIPES,
        title: "Tanpura",
        steps: &[
            act(
                "Press Space. The tanpura plucks its four strings in turn - Pa, Sa, Sa, low Sa - over and over, under the whole performance.",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            recipe(
                "Pluck and ring: Amp Attack under 10 ms, Decay near the top (over 1.5 s), Sustain about 40%, Release over 1.5 s.",
                "A tanpura string is plucked and then rings for seconds. With long decay and release, each string is still sounding when the next is plucked - so the four blur into one continuous drone instead of four notes.",
                "Attack is already short. Decay and Release: drag them nearly all the way up.",
                |s| {
                    carve(s).is_some_and(|p| {
                        p.amp_env.attack_ms < 10.0
                            && p.amp_env.decay_ms >= 1500.0
                            && (0.3..=0.6).contains(&p.amp_env.sustain)
                            && p.amp_env.release_ms >= 1500.0
                    })
                },
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        (p.amp_env.attack_ms < 10.0, Target::Knob(SynthParam::AmpAttack)),
                        (p.amp_env.decay_ms >= 1500.0, Target::Knob(SynthParam::AmpDecay)),
                        ((0.3..=0.6).contains(&p.amp_env.sustain), Target::Knob(SynthParam::AmpSustain)),
                        (p.amp_env.release_ms >= 1500.0, Target::Knob(SynthParam::AmpRelease)),
                    ])
                },
            ),
            recipe(
                "Deep and round: Cutoff about 700 Hz, Resonance about 40%.",
                "A tanpura is dark and warm, not bright. The resonance picks out a narrow band of harmonics at the cutoff - the band the next step will set in motion.",
                "Cutoff 450 Hz to 1.1 kHz; Resonance 30 to 55%.",
                |s| carve(s).is_some_and(|p| (450.0..=1100.0).contains(&p.filter.cutoff_hz) && (0.3..=0.55).contains(&p.filter.resonance)),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((450.0..=1100.0).contains(&p.filter.cutoff_hz), Target::Knob(SynthParam::Cutoff)),
                        ((0.3..=0.55).contains(&p.filter.resonance), Target::Knob(SynthParam::Resonance)),
                    ])
                },
            ),
            recipe(
                "The jawari bloom: Env amount about +2 oct, Filter Attack about 500 ms, Filter Decay over 1.2 s.",
                "A tanpura's bridge (the jawari) is curved, so the string buzzes against it and its upper harmonics swell a moment after the pluck. A slow filter attack does the same thing: every note starts dark and blooms bright.",
                "Env amount +1.5 to +3 oct; Filter Attack 300 to 900 ms; Filter Decay over 1.2 s.",
                |s| {
                    carve(s).is_some_and(|p| {
                        (1.5..=3.0).contains(&p.filter.env_amount_oct)
                            && (300.0..=900.0).contains(&p.filter_env.attack_ms)
                            && p.filter_env.decay_ms >= 1200.0
                    })
                },
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((1.5..=3.0).contains(&p.filter.env_amount_oct), Target::Knob(SynthParam::EnvAmount)),
                        ((300.0..=900.0).contains(&p.filter_env.attack_ms), Target::Knob(SynthParam::FilterAttack)),
                        (p.filter_env.decay_ms >= 1200.0, Target::Knob(SynthParam::FilterDecay)),
                    ])
                },
            ),
            recipe(
                "The buzz: Drive past 6 dB.",
                "That jawari buzz is the string rattling against wood - a gentle distortion. Drive adds the same rasp of extra harmonics.",
                "Drive is in the Filter section.",
                |s| carve(s).is_some_and(|p| p.filter.drive_db > 6.0),
                |_| Some(Target::Knob(SynthParam::Drive)),
            ),
            recipe(
                "The swirl: drag LFO 1 onto Cutoff, then set its Rate under 0.5 Hz and Depth about 30%.",
                "Listen to a real tanpura and its overtones seem to rotate slowly, even between plucks. A slow LFO sweeping the filter keeps the harmonics moving the same way.",
                "Press on \u{201c}LFO 1\u{201d}, drop it on Cutoff, then Rate and Depth under LFO 1.",
                |s| {
                    carve(s).is_some_and(|p| {
                        p.lfo1.target == LfoTarget::Cutoff && lfo_rate_hz(p.lfo1.rate_norm) < 0.5 && (0.15..=0.5).contains(&p.lfo1.depth)
                    })
                },
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        (p.lfo1.target == LfoTarget::Cutoff, Target::LfoPill(1)),
                        (lfo_rate_hz(p.lfo1.rate_norm) < 0.5, Target::Knob(SynthParam::Lfo1Rate)),
                        ((0.15..=0.5).contains(&p.lfo1.depth), Target::Knob(SynthParam::Lfo1Depth)),
                    ])
                },
            ),
            recipe(
                "Strings and a room: Chorus mix about 30%, Reverb mix about 45%, Reverb Size above 80%.",
                "The four strings are never perfectly in tune with one another; chorus adds that gentle beating. A large reverb gives the drone the resonant space it's usually heard in.",
                "Chorus 20 to 45%; Reverb 30 to 60%; Size over 80%.",
                |s| {
                    carve(s).is_some_and(|p| {
                        (0.2..=0.45).contains(&p.fx.chorus_mix) && (0.3..=0.6).contains(&p.fx.reverb_mix) && p.fx.reverb_size > 0.8
                    })
                },
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        ((0.2..=0.45).contains(&p.fx.chorus_mix), Target::Knob(SynthParam::ChorusMix)),
                        ((0.3..=0.6).contains(&p.fx.reverb_mix), Target::Knob(SynthParam::ReverbMix)),
                        (p.fx.reverb_size > 0.8, Target::Knob(SynthParam::ReverbSize)),
                    ])
                },
            ),
            info(
                "A tanpura: long-ringing plucks that overlap into a drone, a bloom and buzz from the jawari, and a slow \
                 swirl of overtones. It's tuned to Sa and Pa, so it sits under any raag - try it under the Bhairav rave demo.",
            ),
        ],
    },
    Lesson {
        id: RECIPE_REED,
        group: RECIPES,
        title: "Reed (shehnai)",
        steps: &[
            act(
                "Press Space: Raag Bhairav's signature phrase, Ga ma dha Pa, Ga ma re Sa - on a plain saw for now.",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            recipe(
                "The reed: Oscillator 1 to the square, and Osc 2 (a saw) up to about -9 dB.",
                "A reed is a thin tongue snapping open and shut, and that on-off motion makes strong odd harmonics - a square wave's recipe. The saw underneath adds the even ones, for a richer, more complex reed.",
                "Osc 1 wave: square. Mixer Osc 2: above -12 dB.",
                |s| carve(s).is_some_and(|p| p.osc1.waveform == Waveform::Square && p.mix.osc2_db > -12.0),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        (p.osc1.waveform == Waveform::Square, Target::OscWave(1)),
                        (p.mix.osc2_db > -12.0, Target::Knob(SynthParam::Osc2Level)),
                    ])
                },
            ),
            recipe(
                "The nasal honk: filter type BP (band-pass), Cutoff about 1.7 kHz, Resonance about 40%.",
                "A shehnai's narrow bore amplifies one band of frequencies, around 1-2 kHz, and that band is the honk. A band-pass filter keeps just that band - cutting both the lows and the highs, as if the sound came through a narrow pipe.",
                "BP on the filter's type switch; Cutoff 1.2 to 2.5 kHz; Resonance 25 to 55%.",
                |s| {
                    carve(s).is_some_and(|p| {
                        p.filter.filter_type == FilterType::Bp
                            && (1200.0..=2500.0).contains(&p.filter.cutoff_hz)
                            && (0.25..=0.55).contains(&p.filter.resonance)
                    })
                },
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        (p.filter.filter_type == FilterType::Bp, Target::FilterType),
                        ((1200.0..=2500.0).contains(&p.filter.cutoff_hz), Target::Knob(SynthParam::Cutoff)),
                        ((0.25..=0.55).contains(&p.filter.resonance), Target::Knob(SynthParam::Resonance)),
                    ])
                },
            ),
            recipe(
                "Breath: Noise to about -32 dB.",
                "Air forced through a reed always hisses a little; a touch of noise under the tone is that breath.",
                "Between -40 and -24 dB, in the Mixer.",
                |s| carve(s).is_some_and(|p| (-40.0..=-24.0).contains(&p.mix.noise_db)),
                |_| Some(Target::Knob(SynthParam::NoiseLevel)),
            ),
            recipe(
                "Let it speak: Amp Attack about 35 ms.",
                "A reed needs a moment of breath pressure before it starts to vibrate, so notes begin with a soft push rather than a click.",
                "Between 20 and 80 ms.",
                |s| carve(s).is_some_and(|p| (20.0..=80.0).contains(&p.amp_env.attack_ms)),
                |_| Some(Target::Knob(SynthParam::AmpAttack)),
            ),
            recipe(
                "Meend: switch to Mono and set Glide to about 70 ms.",
                "Shehnai players slide between notes - meend - instead of jumping. Mono with glide does exactly that. Listen to the fall from re to Sa at the end of the phrase: in Bhairav that slide is the signature.",
                "Mono at the top right of Carve; Glide 40 to 150 ms, under Output.",
                |s| carve(s).is_some_and(|p| p.voice_mode == VoiceMode::Mono && (40.0..=150.0).contains(&p.output.glide_ms)),
                |s| {
                    let p = carve(s)?;
                    first_unmet(&[
                        (p.voice_mode == VoiceMode::Mono, Target::VoiceMode),
                        ((40.0..=150.0).contains(&p.output.glide_ms), Target::Knob(SynthParam::Glide)),
                    ])
                },
            ),
            recipe(
                "Vibrato: LFO 2 (on Pitch) at about 5.5 Hz, Depth about 15%.",
                "Reed players shape held notes with breath vibrato, a little over five wobbles a second - it's what makes a line sound sung rather than typed.",
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
            recipe(
                "A hall to play in: Reverb mix about 30%.",
                "The shehnai is an outdoor, ceremonial instrument that carries far; reverb gives it the space it's meant to fill.",
                "Between 20 and 45%.",
                |s| carve(s).is_some_and(|p| (0.2..=0.45).contains(&p.fx.reverb_mix)),
                |_| Some(Target::Knob(SynthParam::ReverbMix)),
            ),
            info(
                "A reed: square and saw for the vibrating tongue, a band-pass filter for the nasal bore, breath, a soft \
                 attack, meend and vibrato. Widen the filter (LP 24, higher cutoff) and it becomes a clarinet or a sax.",
            ),
        ],
    },
    Lesson {
        id: RECIPE_LEAD,
        group: RECIPES,
        title: "Lead melody",
        steps: &[
            act("Press Space: a beat and a hook on a plain saw.", "Or click the play button at the top.", |s| s.playing, |_| Some(Target::Play)),
            recipe(
                "A lead has to cut through: Oscillator 1 to the square, and Osc 2 up to about -8 dB, detuned about +7 cents.",
                "Square plus saw gives a hollow body with a bright edge, and detuning them a few cents thickens it so it stands apart from the chords.",
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
            recipe(
                "Bright but not harsh: Cutoff about 3 kHz, Resonance about 30%.",
                "Our ears are most sensitive around 2-4 kHz. A lead that lives there, with a little resonance, cuts through without being loud.",
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
            recipe(
                "Switch to Mono with Glide about 60 ms: notes slide like a voice.",
                "Mono with glide makes each note connect to the next, the way a singer or a guitar bend moves between pitches.",
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
            recipe(
                "Expression: vibrato on LFO 2 - Rate about 5 Hz, Depth about 15%.",
                "Singers and guitarists add vibrato to held notes. It's what makes a line sound performed rather than programmed.",
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
            recipe(
                "Polish: Chorus mix about 20% and Reverb mix about 25%.",
                "Chorus widens it and reverb gives it a place in the room - both kept small, so the lead stays up front.",
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
    Lesson {
        id: ARRANGE_HOUSE,
        group: ARRANGEMENT,
        title: "House: how a track is built",
        steps: &[
            info(
                "This is the House demo, laid out the way most house and techno is: in blocks of 8 bars. The markers on the ruler \
                 name each section - Intro, Build, Groove, Breakdown, Drop, Outro.",
            ),
            act(
                "Press Space and listen to the Intro: a kick and a quiet pad, little else. Intros are sparse on purpose - DJs mix a new track in over the old one here.",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            act(
                "Keep listening until bar 9, the Build: the clap and the bass arrive. Every 8 bars something comes in or drops out - that's what keeps a loop feeling like a journey.",
                "The glowing bar on the ruler is bar 9. Or click it to jump there.",
                |s| s.playhead >= bars(8),
                |_| Some(Target::RulerBar(8)),
            ),
            act(
                "Jump to the Breakdown: click the ruler at bar 33. The kick stops and the chords take over - the tension comes from waiting for the kick to return.",
                "Click the glowing bar near the Breakdown marker.",
                |s| in_bars(s, 32, 40),
                |_| Some(Target::RulerBar(32)),
            ),
            act(
                "Feel how much the kick does: mute the Drums track (its M button) for a moment.",
                "M is on the Drums track's header, bottom left.",
                |s| track_named(s, "Drums").is_some_and(|t| t.mute),
                |s| track_named(s, "Drums").map(|t| Target::Mute(t.id)),
            ),
            act(
                "Bring it back: click M again.",
                "The same M button.",
                |s| track_named(s, "Drums").is_some_and(|t| !t.mute),
                |s| track_named(s, "Drums").map(|t| Target::Mute(t.id)),
            ),
            act(
                "Now jump to the Drop at bar 41: kick, bass, stabs and the lead hook all at once - the payoff the breakdown made you wait for.",
                "Click the glowing bar near the Drop marker.",
                |s| in_bars(s, 40, 56),
                |_| Some(Target::RulerBar(40)),
            ),
            act(
                "Mark a spot of your own: right-click the ruler at bar 25 and choose Add marker. Markers are how you plan a song before it's written.",
                "Right-click the glowing bar, then \u{201c}Add marker here\u{201d}.",
                |s| s.arrangement.markers.len() > 6,
                |_| Some(Target::RulerBar(24)),
            ),
            info(
                "The shape: Intro 8, Build 8, Groove 16, Breakdown 8, Drop 16, Outro 8 bars - 64 bars, two minutes at 128 BPM. \
                 Club versions double every section (5 to 7 minutes) so DJs have room to mix. Rave, techno and hardcore use the \
                 same bones, faster (130 to 175 BPM), with snare rolls and risers for the build and a harder drop.",
            ),
        ],
    },
    Lesson {
        id: ARRANGE_BHAIRAV,
        group: ARRANGEMENT,
        title: "Bhairav: a raag as a rave",
        steps: &[
            info(
                "A raag performance starts slowly: the alap, just the drone and a voice finding Sa, before any rhythm. \
                 The Bhairav rave borrows that shape and dresses it as a dance track.",
            ),
            act(
                "Press Space. First the tanpura alone, then hats: Sa is established before anything else, the way a raag begins.",
                "Or click the play button at the top.",
                |s| s.playing,
                |_| Some(Target::Play),
            ),
            act(
                "Solo the Tanpura (its S button): Sa and Pa, held for the whole song. Every other part is heard against this.",
                "S is on the Tanpura track's header.",
                |s| track_named(s, "Tanpura").is_some_and(|t| t.solo),
                |s| track_named(s, "Tanpura").map(|t| Target::Solo(t.id)),
            ),
            act(
                "Un-solo it: click S again.",
                "The same S button.",
                |s| track_named(s, "Tanpura").is_some_and(|t| !t.solo),
                |s| track_named(s, "Tanpura").map(|t| Target::Solo(t.id)),
            ),
            act(
                "Jump to the Build at bar 9: the bass arrives, pulling from komal re back to Sa on every beat - Bhairav's gravity, as a groove.",
                "Click the glowing bar near the Build marker.",
                |s| in_bars(s, 8, 16),
                |_| Some(Target::RulerBar(8)),
            ),
            act(
                "Jump to the Break at bar 33: the beat stops and the reed sings Bhairav's phrase over the drone - the melodic heart, like a vocal breakdown in house.",
                "Click the glowing bar near the Break marker.",
                |s| in_bars(s, 32, 40),
                |_| Some(Target::RulerBar(32)),
            ),
            act(
                "Mute the Tanpura and listen to the reed: without Sa underneath, the melody loses its home.",
                "M on the Tanpura track's header.",
                |s| track_named(s, "Tanpura").is_some_and(|t| t.mute),
                |s| track_named(s, "Tanpura").map(|t| Target::Mute(t.id)),
            ),
            act(
                "Bring the drone back: click M again.",
                "The same M button.",
                |s| track_named(s, "Tanpura").is_some_and(|t| !t.mute),
                |s| track_named(s, "Tanpura").map(|t| Target::Mute(t.id)),
            ),
            act(
                "Jump to the Drop at bar 41: beat, bass, drone and reed together.",
                "Click the glowing bar near the Drop marker.",
                |s| in_bars(s, 40, 56),
                |_| Some(Target::RulerBar(40)),
            ),
            info(
                "Drone first, then rhythm, the melody alone in the break, everything in the drop - and back to the drone at the \
                 end, finishing on Sa where it began, as a raag does. Try the shape with another raag: change the Key to its \
                 scale and rewrite the bass and reed in its notes.",
            ),
        ],
    },
    Lesson {
        id: PROJECT_GROOVE,
        group: PROJECTS,
        title: "House track 1: the groove",
        steps: &[
            info(
                "Over five parts you'll build a whole house track yourself - beat, bass, chords, arrangement and mix. \
                 Each part starts where the last one ended. Part 1: the groove every house track stands on.",
            ),
            act(
                "Add a drum track: \u{201c}+ Drums\u{201d} under the tracks.",
                "Below the track list, on the left of the timeline.",
                |s| tracks_with(s, Instrument::Drums).next().is_some(),
                |_| Some(Target::AddDrumTrack),
            ),
            act(
                "Double-click bar 1 of the Drums track to make a clip.",
                "Two quick clicks on the empty lane. The clip opens below.",
                |s| clips_on(s, Instrument::Drums).next().is_some(),
                |s| tracks_with(s, Instrument::Drums).next().map(|t| Target::Lane(t.id)),
            ),
            act(
                "Kick on every beat: the Kick row under 1, 2, 3 and 4.",
                "Grid not showing? Double-click the clip.",
                |s| drum_pattern_has(s, KICK, &[0, PPQ, 2 * PPQ, 3 * PPQ]),
                |s| row_or_clip(s, Instrument::Drums, KICK),
            ),
            act(
                "Clap on beats 2 and 4.",
                "The Clap row, under 2 and 4.",
                |s| drum_pattern_has(s, CLAP, &[PPQ, 3 * PPQ]),
                |s| row_or_clip(s, Instrument::Drums, CLAP),
            ),
            act(
                "Closed hats drive it - your pick: the classic is the 2nd and 4th square of every beat (the \u{201c}e\u{201d} \
                 and \u{201c}a\u{201d}), but every square, or a pattern of your own, works too. At least four hits.",
                "Each beat has four squares: 1 e and a. The e-and-a pattern leaves the 3rd square free for the open hat.",
                |s| clips_on(s, Instrument::Drums).any(|c| pitch_count(c, CLOSED_HAT) >= 4),
                |s| row_or_clip(s, Instrument::Drums, CLOSED_HAT),
            ),
            act(
                "Open hat on the \u{201c}and\u{201d} of every beat - the 3rd square.",
                "The Open Hat row, halfway between the beats.",
                |s| drum_pattern_has(s, OPEN_HAT, &OFFBEATS),
                |s| row_or_clip(s, Instrument::Drums, OPEN_HAT),
            ),
            act("Press Space: that's a house groove.", "Or the play button at the top.", |s| s.playing, |_| Some(Target::Play)),
            recipe(
                "Your turn: make it yours. Add one hit the groove doesn't have yet - a Snare on the very last square, a \
                 Kick just before the bar ends, a Clap somewhere new. Listen while it plays; keep what you like.",
                "One hit away from the obvious places is what makes a groove sound played rather than programmed.",
                "Anything outside the pattern counts. Don't like it? Click the note again to remove it and try another.",
                groove_has_extra,
                |s| row_or_clip(s, Instrument::Drums, SNARE),
            ),
            act(
                "Stretch the clip out to bar 17: drag its right edge. Sixteen bars of groove to build on.",
                "Grab the very end of the clip in the timeline.",
                |s| clips_on(s, Instrument::Drums).any(|c| loops(c, 16)),
                |s| tracks_with(s, Instrument::Drums).next().map(|t| Target::Lane(t.id)),
            ),
            info(
                "Kick for the pulse, clap for the backbeat, closed hats for the drive and the open hat for the bounce. \
                 Next: a bassline that locks to it.",
            ),
        ],
    },
    Lesson {
        id: PROJECT_BASS,
        group: PROJECTS,
        title: "House track 2: the bassline",
        steps: &[
            act(
                "Add a MIDI track for the bass: \u{201c}+ MIDI track\u{201d}.",
                "Below the track list.",
                |s| tracks_with(s, Instrument::Carve).next().is_some(),
                |_| Some(Target::AddMidiTrack),
            ),
            act(
                "Pick its sound: click the preset name at the top of Carve and choose Deep Bass - the one you built in the recipe.",
                "Or step through with the \u{2039} \u{203a} arrows.",
                |s| selected(s).is_some_and(|t| t.instrument == Some(Instrument::Carve)) && s.synth.name == "Deep Bass",
                |_| Some(Target::Preset("Deep Bass")),
            ),
            act(
                "The bass comes in after the drums: double-click bar 5 of the bass track.",
                "Bar 5, not bar 1 - four bars of drums alone first.",
                |s| project_bass(s).is_some_and(|b| clips_of(s, b.id).any(|c| c.start == bars(4))),
                |s| s.selected_track.map(Target::Lane),
            ),
            act(
                "The line is four bars long, one bar per chord: click + next to Pattern until it says 4 bars.",
                "Above the grid.",
                |s| project_bass(s).is_some_and(|b| clips_of(s, b.id).any(|c| c.content_len() == bars(4))),
                |_| Some(Target::PatternPlus),
            ),
            act(
                "Bar 1: A, the bottom row. House bass usually sits on the four off-beats, between the kicks - but the \
                 rhythm is yours: at least two A's in bar 1.",
                "Off-beats: two squares after each beat number, where the open hat plays.",
                |s| bass_bar(s, 0),
                |s| row_or_clip(s, Instrument::Carve, PROJECT_BASS_ROOTS[0]),
            ),
            act(
                "Bar 2: C, in the same rhythm (or a new one).",
                "Bar 2 starts at the 2 mark along the top.",
                |s| bass_bar(s, 1),
                |s| row_or_clip(s, Instrument::Carve, PROJECT_BASS_ROOTS[1]),
            ),
            act("Bar 3: on D.", "Bar 3 starts at the 3 mark.", |s| bass_bar(s, 2), |s| row_or_clip(s, Instrument::Carve, PROJECT_BASS_ROOTS[2])),
            act("Bar 4: back to C.", "Bar 4 starts at the 4 mark.", |s| bass_bar(s, 3), |s| row_or_clip(s, Instrument::Carve, PROJECT_BASS_ROOTS[3])),
            act(
                "Stretch the bass clip to bar 17, level with the drums.",
                "Drag its right edge.",
                |s| project_bass(s).is_some_and(|b| clips_of(s, b.id).any(|c| c.end() >= bars(16) && looping(c))),
                |s| project_bass(s).map(|b| Target::Lane(b.id)),
            ),
            act("Press Space.", "Or the play button.", |s| s.playing, |_| Some(Target::Play)),
            recipe(
                "Your turn: swap one bass note for another note of its chord. The chords you'll add are A minor (A C E), \
                 C major (C E G), D sus (D G A) and C again - so in bar 1, try an E instead of one A.",
                "Roots make the bass solid; other notes of the chord make it move. Both belong to the chord, so neither clashes.",
                "Bar 1: E, a few rows above A. Bar 2: E or G. Bar 3: G or A. Click a note to remove it, click the new row to add one.",
                bass_moves,
                |s| row_or_clip(s, Instrument::Carve, PROJECT_CHORDS_NOTES[0][2]),
            ),
            info(
                "The bass plays the root of each chord you'll add next - A, C, D, C - in the gaps between the kicks. \
                 Next: the chords themselves.",
            ),
        ],
    },
    Lesson {
        id: PROJECT_CHORDS,
        group: PROJECTS,
        title: "House track 3: chords",
        steps: &[
            act(
                "One more MIDI track, for chords.",
                "\u{201c}+ MIDI track\u{201d}.",
                |s| tracks_with(s, Instrument::Carve).count() >= 2,
                |_| Some(Target::AddMidiTrack),
            ),
            act(
                "Preset: Soft Pad - its slow swell and long tail turn short hits into lush chords.",
                "The preset name at the top of Carve.",
                |s| project_chord_track(s).is_some_and(|t| Some(t.id) == s.selected_track) && s.synth.name == "Soft Pad",
                |_| Some(Target::Preset("Soft Pad")),
            ),
            act(
                "The chords arrive at bar 9: double-click bar 9 of the new track.",
                "Eight bars of drums and bass first.",
                |s| project_chord_track(s).is_some_and(|t| clips_of(s, t.id).any(|c| c.start == bars(8))),
                |s| s.selected_track.map(Target::Lane),
            ),
            act(
                "Four chords, four bars: Pattern + until it says 4 bars.",
                "Above the grid.",
                |s| project_chord_track(s).is_some_and(|t| clips_of(s, t.id).any(|c| c.content_len() == bars(4))),
                |_| Some(Target::PatternPlus),
            ),
            act(
                "Bar 1, A minor: A, C and E, stacked on the very first square. (A chord is its three notes: anywhere in \
                 the bar, in any octave, still counts.)",
                "Three clicks in the same column: A (bottom row), C, E.",
                |s| chord_bar(s, 0),
                |s| chord_target(s, 0),
            ),
            act(
                "Bar 2, C major: C, E and G on its first square.",
                "Bar 2 starts at the 2 mark.",
                |s| chord_bar(s, 1),
                |s| chord_target(s, 1),
            ),
            act(
                "Bar 3, D suspended: D, G and A.",
                "Bar 3 starts at the 3 mark. \u{201c}Suspended\u{201d}: no third, so it floats.",
                |s| chord_bar(s, 2),
                |s| chord_target(s, 2),
            ),
            act("Bar 4, C major again: C, E, G.", "Bar 4 starts at the 4 mark.", |s| chord_bar(s, 3), |s| chord_target(s, 3)),
            act(
                "Stretch the chords to bar 17.",
                "Drag the clip's right edge.",
                |s| project_chord_track(s).is_some_and(|t| clips_of(s, t.id).any(|c| c.end() >= bars(16) && looping(c))),
                |s| project_chord_track(s).map(|t| Target::Lane(t.id)),
            ),
            act("Press Space: drums, bass and chords.", "Or the play button.", |s| s.playing, |_| Some(Target::Play)),
            recipe(
                "Your turn: give the chords a rhythm. Add a second hit of a chord later in its bar - try A, C and E again \
                 on the \u{201c}and\u{201d} of beat 2 in bar 1.",
                "Chords that only land on the one sit still; a second hit answers the beat, and the gaps between hits become the groove.",
                "The \u{201c}and\u{201d} of 2: two squares after the 2 mark. Or any square you like.",
                chords_have_rhythm,
                |s| chord_target(s, 0),
            ),
            info(
                "Am, C, Dsus, C - and the bass under them plays each chord's root. Notice the chords share notes \
                 (C and E in the first two, G in the next), so they flow. Next: turning 16 bars into a song.",
            ),
        ],
    },
    Lesson {
        id: PROJECT_ARRANGE,
        group: PROJECTS,
        title: "House track 4: arrangement",
        steps: &[
            info(
                "Right now the track only builds. A song needs tension and release: you'll extend it to 32 bars and \
                 cut a breakdown in the middle, where the drums and bass drop out.",
            ),
            act(
                "Plan it with markers: right-click the ruler at bars 1, 9, 17 and 25 and Add marker at each (Intro, Groove, Breakdown, Drop).",
                "Right-click the glowing bar, then \u{201c}Add marker here\u{201d}. Double-click a marker to rename it.",
                |s| MARKER_BARS.iter().all(|&b| has_marker_at(s, b)),
                |s| MARKER_BARS.iter().find(|&&b| !has_marker_at(s, b)).map(|&b| Target::RulerBar(b)),
            ),
            act(
                "Extend everything to bar 33: stretch the drums, bass and chords clips.",
                "Drag each clip's right edge to bar 33.",
                |s| ["Drums", "Bass", "Chords"].iter().all(|n| reaches(s, n, 32)),
                |s| ["Drums", "Bass", "Chords"].iter().find(|n| !reaches(s, n, 32)).and_then(|n| track_named(s, n)).map(|t| Target::Lane(t.id)),
            ),
            act(
                "Cut at the breakdown: click the ruler at bar 17, then press Ctrl+E (\u{2318}E on a Mac). Every clip there splits in two.",
                "Click the glowing bar first - the split happens at the playhead.",
                |s| starts_at(s, "Drums", 16),
                |_| Some(Target::RulerBar(16)),
            ),
            act(
                "And where the drop comes back: click the ruler at bar 25 and Ctrl+E again.",
                "The glowing bar.",
                |s| starts_at(s, "Drums", 24),
                |_| Some(Target::RulerBar(24)),
            ),
            act(
                "Empty the breakdown: click the Drums piece between bars 17 and 25 and press Delete.",
                "Just that middle piece.",
                |s| silent_in(s, "Drums", 16, 24) && !silent_in(s, "Drums", 24, 32),
                |s| track_named(s, "Drums").map(|t| Target::Lane(t.id)),
            ),
            act(
                "Same for the Bass piece in bars 17 to 25 - only the chords remain there.",
                "Click it, then Delete.",
                |s| silent_in(s, "Bass", 16, 24) && !silent_in(s, "Bass", 24, 32),
                |s| track_named(s, "Bass").map(|t| Target::Lane(t.id)),
            ),
            act(
                "Hear it: click the ruler at bar 13 and press Space. The groove falls away into the breakdown, then everything slams back at bar 25.",
                "Click the glowing bar, then Space.",
                |s| s.playing && s.playhead >= bars(12),
                |_| Some(Target::RulerBar(12)),
            ),
            info(
                "That drop only hits because the breakdown took the kick and bass away first. Taking things out is \
                 as important as putting them in. Last part: movement and the final mix.",
            ),
        ],
    },
    Lesson {
        id: PROJECT_FINISH,
        group: PROJECTS,
        title: "House track 5: movement and mix",
        steps: &[
            act(
                "The Chords track is selected, so its Carve is below. Automate its filter: right-click the Cutoff knob and choose Automate.",
                "A lane appears under the Chords track. (If Carve isn't showing, click the Chords track's name first.)",
                |s| chords_cutoff_lane(s).is_some(),
                |_| Some(Target::Knob(SynthParam::Cutoff)),
            ),
            act(
                "Build the breakdown: in the new lane, click a point low at bar 17 and another high at bar 25 - the filter opens as the drop approaches.",
                "Click in the lane to add a point; drag a point to move it.",
                |s| chords_cutoff_lane(s).is_some_and(|l| rises(l, 16, 24)),
                |s| track_named(s, "Chords").map(|t| Target::Automation(t.id)),
            ),
            act(
                "Balance: pads sit behind the beat. Drag the Chords track's fader down to about -10 dB.",
                "The fader is on the right of the track header; its level shows beside it.",
                |s| track_named(s, "Chords").is_some_and(|t| (-13.0..=-7.0).contains(&t.gain_db)),
                |_| None,
            ),
            act(
                "Listen from the top: press Home, then Space - the whole track.",
                "Home jumps to the start.",
                |s| s.playing && s.playhead < bars(4),
                |_| Some(Target::Play),
            ),
            info(
                "You built a house track: groove, bass, chords, an arrangement with a breakdown and drop, and \
                 automation for movement. Save it (Ctrl+S, \u{2318}S on a Mac) and export it (File \u{2192} Export \
                 Audio) to play it anywhere.",
            ),
        ],
    },
];

/// The "and" of every beat.
const OFFBEATS: [Ticks; 4] = [PPQ / 2, PPQ + PPQ / 2, 2 * PPQ + PPQ / 2, 3 * PPQ + PPQ / 2];
/// Where the arrangement's markers go (0-based bars).
pub(super) const MARKER_BARS: [i64; 4] = [0, 8, 16, 24];

fn clips_of(s: &Snapshot, track: shared::arrangement::TrackId) -> impl Iterator<Item = &Clip> {
    s.arrangement.clips.iter().filter(move |c| c.track == track)
}

fn looping(c: &Clip) -> bool {
    matches!(c.content, ClipContent::Midi { loop_len: Some(_), .. })
}

/// The project's bass track: "Bass" once it exists, else the (only) Carve track.
fn project_bass(s: &Snapshot) -> Option<&shared::arrangement::Track> {
    track_named(s, "Bass").or_else(|| tracks_with(s, Instrument::Carve).next())
}

/// The project's chords track: a Carve track that isn't the bass.
fn project_chord_track(s: &Snapshot) -> Option<&shared::arrangement::Track> {
    track_named(s, "Chords").or_else(|| tracks_with(s, Instrument::Carve).find(|t| t.name != "Bass"))
}

/// The pattern notes starting in bar `bar` (0-based) of `clip`.
fn notes_in_bar(clip: &Clip, bar: usize) -> impl Iterator<Item = &shared::arrangement::MidiNote> {
    let range = bars(bar as i64)..bars(bar as i64 + 1);
    let notes = match &clip.content {
        ClipContent::Midi { notes, .. } => notes.as_slice(),
        _ => &[],
    };
    notes.iter().filter(move |n| range.contains(&n.start))
}

/// Same note name, any octave.
fn same_class(a: u8, b: u8) -> bool {
    a % 12 == b % 12
}

/// Bar `bar` of the bass pattern has its root at least twice - any
/// rhythm, any octave.
fn bass_bar(s: &Snapshot, bar: usize) -> bool {
    let root = PROJECT_BASS_ROOTS[bar];
    project_bass(s).is_some_and(|b| clips_of(s, b.id).any(|c| notes_in_bar(c, bar).filter(|n| same_class(n.pitch, root)).count() >= 2))
}

/// Somewhere in the bass pattern, a note of its bar's chord other than
/// the root.
fn bass_moves(s: &Snapshot) -> bool {
    project_bass(s).is_some_and(|b| {
        clips_of(s, b.id).any(|c| {
            (0..4).any(|bar| {
                notes_in_bar(c, bar).any(|n| {
                    !same_class(n.pitch, PROJECT_BASS_ROOTS[bar])
                        && PROJECT_CHORDS_NOTES[bar].iter().any(|&p| same_class(n.pitch, p))
                })
            })
        })
    })
}

/// The chord's notes still missing from bar `bar`, in the order to click.
fn chord_missing(clip: &Clip, bar: usize) -> impl Iterator<Item = u8> + '_ {
    PROJECT_CHORDS_NOTES[bar].into_iter().filter(move |&p| !notes_in_bar(clip, bar).any(|n| same_class(n.pitch, p)))
}

/// Bar `bar` of the chords pattern has all three of its chord's notes -
/// stacked or spread, in any octave.
fn chord_bar(s: &Snapshot, bar: usize) -> bool {
    project_chord_track(s).is_some_and(|t| clips_of(s, t.id).any(|c| chord_missing(c, bar).next().is_none()))
}

/// Some bar of the chords plays its chord's notes at two different times.
fn chords_have_rhythm(s: &Snapshot) -> bool {
    project_chord_track(s).is_some_and(|t| {
        clips_of(s, t.id).any(|c| {
            (0..4).any(|bar| {
                let mut starts =
                    notes_in_bar(c, bar).filter(|n| PROJECT_CHORDS_NOTES[bar].iter().any(|&p| same_class(n.pitch, p))).map(|n| n.start);
                starts.next().is_some_and(|first| starts.any(|t| t != first))
            })
        })
    })
}

/// The row of the first chord note still missing (or the clip's lane).
fn chord_target(s: &Snapshot, bar: usize) -> Option<Target> {
    let t = project_chord_track(s)?;
    if s.open_clip.is_none() {
        return Some(Target::Lane(t.id));
    }
    let clip = clips_of(s, t.id).next()?;
    Some(Target::PianoRollRow(chord_missing(clip, bar).next().unwrap_or(PROJECT_CHORDS_NOTES[bar][0])))
}

/// How many times `pitch` plays in `clip`'s pattern.
fn pitch_count(clip: &Clip, pitch: u8) -> usize {
    match &clip.content {
        ClipContent::Midi { notes, .. } => notes.iter().filter(|n| n.pitch == pitch).count(),
        _ => 0,
    }
}

/// A drum hit the groove lesson didn't ask for: a snare anywhere, or a
/// kick, clap or open hat off its taught squares (closed hats were a free
/// choice already).
fn groove_has_extra(s: &Snapshot) -> bool {
    let beats = [0, PPQ, 2 * PPQ, 3 * PPQ];
    clips_on(s, Instrument::Drums).any(|c| {
        let ClipContent::Midi { notes, .. } = &c.content else { return false };
        notes.iter().any(|n| {
            let at = n.start % BAR;
            match n.pitch {
                KICK => !beats.contains(&at),
                CLAP => at != PPQ && at != 3 * PPQ,
                OPEN_HAT => !OFFBEATS.contains(&at),
                CLOSED_HAT => false,
                _ => true,
            }
        })
    })
}

fn has_marker_at(s: &Snapshot, bar: i64) -> bool {
    s.arrangement.markers.iter().any(|m| m.position >= bars(bar) && m.position < bars(bar + 1))
}

/// The named track has a clip reaching bar `bar` (0-based end).
fn reaches(s: &Snapshot, name: &str, bar: i64) -> bool {
    track_named(s, name).is_some_and(|t| clips_of(s, t.id).any(|c| c.end() >= bars(bar)))
}

/// The named track has a clip starting exactly at bar `bar`.
fn starts_at(s: &Snapshot, name: &str, bar: i64) -> bool {
    track_named(s, name).is_some_and(|t| clips_of(s, t.id).any(|c| c.start == bars(bar)))
}

/// No note of the named track sounds anywhere in bars `from..to`.
fn silent_in(s: &Snapshot, name: &str, from: i64, to: i64) -> bool {
    let Some(t) = track_named(s, name) else { return false };
    !clips_of(s, t.id).any(|c| c.played_notes().iter().any(|n| (bars(from)..bars(to)).contains(&(c.start + n.start))))
}

fn chords_cutoff_lane(s: &Snapshot) -> Option<&shared::arrangement::AutomationLane> {
    let t = track_named(s, "Chords")?;
    s.arrangement
        .automation
        .iter()
        .find(|l| l.track == t.id && l.target == Some(shared::arrangement::AutomationTarget::Synth(SynthParam::Cutoff)))
}

/// The lane is clearly higher at bar `to` than at bar `from`.
fn rises(lane: &shared::arrangement::AutomationLane, from: i64, to: i64) -> bool {
    match (lane.value_at(bars(from)), lane.value_at(bars(to))) {
        (Some(a), Some(b)) => b - a > 0.25,
        _ => false,
    }
}

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

/// `n` bars in ticks (4/4 throughout the course).
const fn bars(n: i64) -> Ticks {
    n * BAR
}

/// The playhead is somewhere in bars `from..to` (0-based).
fn in_bars(s: &Snapshot, from: i64, to: i64) -> bool {
    (bars(from)..bars(to)).contains(&s.playhead)
}

fn track_named<'a>(s: &'a Snapshot, name: &str) -> Option<&'a shared::arrangement::Track> {
    s.arrangement.tracks.iter().find(|t| t.name == name)
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
    use crate::lessons::show::{add_midi_track, add_notes, add_track, draw_clip_at, pattern_bars};
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
        Snapshot { arrangement: p.arrangement, selected_track: selected, playing: false, synth, open_clip: None, playhead: 0 }
    }

    fn lesson(id: &str) -> &'static Lesson {
        LESSONS.iter().find(|l| l.id == id).unwrap()
    }


    #[test]
    fn every_recipe_step_explains_itself() {
        for l in LESSONS.iter().filter(|l| l.group == RECIPES) {
            for step in l.steps {
                if let Kind::Action { .. } = step.kind {
                    let plays = step.text.starts_with("Press Space");
                    assert!(plays || !step.why.is_empty(), "{}: no why for \"{}\"", l.id, step.text);
                }
            }
        }
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

    /// Every lesson done start to finish by "Show me": before each action
    /// step's entry runs, the step must not pass yet; after it, it must.
    #[test]
    fn show_me_does_every_step() {
        for l in LESSONS {
            let id = l.id;
            let mut s = start(id);
            let shows = crate::lessons::show::steps(id);
            let actions: Vec<&Step> = l.steps.iter().filter(|st| matches!(st.kind, Kind::Action { .. })).collect();
            assert_eq!(shows.len(), actions.len(), "{id}: one Show me per action step");
            for (i, (step, show)) in actions.iter().zip(&shows).enumerate() {
                let Kind::Action { check, target } = step.kind else { unreachable!() };
                assert!(!check(&s), "{id} action {} already passes: {}", i + 1, step.text);
                let _ = target(&s);
                show(&mut s);
                assert!(check(&s), "{id} action {} doesn't pass after Show me: {}", i + 1, step.text);
            }
        }
    }

    #[test]
    fn each_recipe_preset_is_what_its_lesson_builds() {
        // Loading the preset should satisfy every knob step of its recipe.
        for (id, preset) in [
            (RECIPE_BASS, "Deep Bass"),
            (RECIPE_FLUTE, "Flute"),
            (RECIPE_HARP, "Indian Harp"),
            (RECIPE_TANPURA, "Tanpura"),
            (RECIPE_REED, "Reed"),
            (RECIPE_LEAD, "Lead"),
        ] {
            let build = shared::synth::PRESETS.iter().find(|p| p.0 == preset).unwrap_or_else(|| panic!("no preset {preset}")).1;
            let mut s = start(id);
            s.synth = build();
            assert_eq!(s.synth.name, preset);
            s.playing = true;
            for step in lesson(id).steps {
                if let Kind::Action { check, .. } = step.kind {
                    assert!(check(&s), "{preset} doesn't satisfy {id}: {}", step.text);
                }
            }
        }
    }


    #[test]
    fn project_parts_take_other_choices_too() {
        // Bass: a rhythm of its own, an octave up.
        let mut s = start(PROJECT_BASS);
        add_midi_track(&mut s, "MIDI 1");
        draw_clip_at(&mut s, 4);
        pattern_bars(&mut s, 4);
        add_notes(&mut s, PROJECT_BASS_ROOTS[0] + 12, &[0, 3 * PPQ]);
        assert!(bass_bar(&s, 0));
        assert!(!bass_bar(&s, 1));
        // A note outside the chord isn't the "your turn" swap.
        add_notes(&mut s, 62, &[PPQ]);
        assert!(!bass_moves(&s));
        // Chords: A minor spread across bar 1 as an arpeggio.
        let mut s = start(PROJECT_CHORDS);
        add_midi_track(&mut s, "MIDI 1");
        draw_clip_at(&mut s, 8);
        pattern_bars(&mut s, 4);
        for (i, p) in [69u8, 60, 64].into_iter().enumerate() {
            add_notes(&mut s, p, &[i as i64 * PPQ]);
        }
        assert!(chord_bar(&s, 0));
        assert!(!chord_bar(&s, 1));
        // Groove: hats on every square are a fine choice, and aren't an
        // "extra" hit.
        let mut s = start(PROJECT_GROOVE);
        add_track(&mut s, "Drums", Some(Instrument::Drums));
        draw_clip_at(&mut s, 0);
        add_notes(&mut s, CLOSED_HAT, &(0..16).map(|i| i * PPQ / 4).collect::<Vec<_>>());
        assert!(clips_on(&s, Instrument::Drums).any(|c| pitch_count(c, CLOSED_HAT) >= 4));
        assert!(!groove_has_extra(&s));
        add_notes(&mut s, KICK, &[3 * PPQ + 3 * PPQ / 4]);
        assert!(groove_has_extra(&s));
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
