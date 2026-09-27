//! Fixed key -> drum-sample bindings for tapping out a rhythm live,
//! instead of building it clip-by-clip: press a bound key (with the
//! playhead moving, e.g. while looped playback runs) and a one-shot hit
//! lands on that sample's own track at the current tick - the same
//! sample-to-clip mechanics `AddDrumSample` already uses for a dropped-in
//! sample, just at the playhead instead of at the timeline start, and
//! reusing an existing same-named track instead of always making a new
//! one.
//!
//! Digits, not letters: the computer-keyboard note input
//! (`synth::state::key_semitone_offset`) already claims the letter keys
//! (Z/X/C/V/... for white keys, S/D/G/... for black), so digits keep this
//! fully non-overlapping with playing the synth live.

use vizia::prelude::Code;

use shared::arrangement::ClipColor;

#[derive(Clone, Copy)]
pub struct DrumPad {
    pub key: Code,
    pub sample: &'static str,
    pub track_name: &'static str,
    pub color: ClipColor,
}

pub const DRUM_PADS: &[DrumPad] = &[
    DrumPad { key: Code::Digit1, sample: "drums/kick.wav", track_name: "Kick", color: ClipColor::Amber },
    DrumPad { key: Code::Digit2, sample: "drums/snare.wav", track_name: "Snare", color: ClipColor::Teal },
    DrumPad { key: Code::Digit3, sample: "drums/hihat_closed.wav", track_name: "Hihat", color: ClipColor::Violet },
    DrumPad { key: Code::Digit4, sample: "drums/hihat_open.wav", track_name: "Open Hihat", color: ClipColor::Pink },
    DrumPad { key: Code::Digit5, sample: "drums/clap.wav", track_name: "Clap", color: ClipColor::Coral },
];
