//! A MIDI controller's messages, and what each does in Shor: keys and
//! pads play the selected track, knobs turn Carve's knobs, the sustain
//! pedal holds notes. Pure, so the mapping is tested without hardware.

use crate::synth::SynthParam;

/// One channel message from a controller (channels 0..16).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Message {
    NoteOn { channel: u8, note: u8, velocity: u8 },
    NoteOff { channel: u8, note: u8 },
    Control { channel: u8, cc: u8, value: u8 },
}

/// Reads one message; `None` for anything Shor doesn't use (clock,
/// aftertouch, program change, system messages).
pub fn parse(bytes: &[u8]) -> Option<Message> {
    let (&status, data) = bytes.split_first()?;
    let channel = status & 0x0f;
    match (status & 0xf0, data) {
        // A note-on at velocity 0 is a note-off, by long convention.
        (0x90, [note, 0, ..]) | (0x80, [note, ..]) => Some(Message::NoteOff { channel, note: note & 0x7f }),
        (0x90, [note, velocity, ..]) => Some(Message::NoteOn { channel, note: note & 0x7f, velocity: velocity & 0x7f }),
        (0xb0, [cc, value, ..]) => Some(Message::Control { channel, cc: cc & 0x7f, value: value & 0x7f }),
        _ => None,
    }
}

/// What a message does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Note { note: u8, velocity: u8 },
    NoteOff { note: u8 },
    /// Set a Carve knob (0..1 of its range).
    Knob { param: SynthParam, value: f32 },
    Sustain(bool),
}

/// Which mapping a device gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// Akai MPK Mini: pads on channel 10, knobs on CC 70-77.
    MpkMini,
    /// Any other controller: the usual General MIDI controller numbers.
    Generic,
}

impl Layout {
    /// From a MIDI port's name.
    pub fn for_port(name: &str) -> Self {
        let lower = name.to_lowercase();
        if lower.contains("mpk") && lower.contains("mini") {
            Layout::MpkMini
        } else {
            Layout::Generic
        }
    }
}

/// The MPK Mini's eight knobs, left to right, top row first: the filter,
/// then the envelope, then space and level.
pub const MPK_KNOBS: [SynthParam; 8] = [
    SynthParam::Cutoff,
    SynthParam::Resonance,
    SynthParam::EnvAmount,
    SynthParam::Drive,
    SynthParam::AmpAttack,
    SynthParam::AmpRelease,
    SynthParam::ReverbMix,
    SynthParam::Volume,
];
const MPK_FIRST_KNOB: u8 = 70;
/// The MPK Mini's pads send on channel 10 (9 counting from 0), notes 36-51
/// over its two banks.
const PAD_CHANNEL: u8 = 9;
const FIRST_PAD: u8 = 36;
const PADS: u8 = 16;

/// What `message` from a `layout` device does. `drum_pads` are the
/// selected Drum Kit's notes in pad order (empty when the track isn't a
/// drum kit): the MPK's pads play them by position, whatever note the pad
/// sends.
pub fn map(message: Message, layout: Layout, drum_pads: &[u8]) -> Option<Action> {
    let pad = |channel: u8, note: u8| -> Option<u8> {
        let index = note.checked_sub(FIRST_PAD).filter(|i| *i < PADS)?;
        (layout == Layout::MpkMini && channel == PAD_CHANNEL && !drum_pads.is_empty())
            .then(|| drum_pads[index as usize % drum_pads.len()])
    };
    match message {
        Message::NoteOn { channel, note, velocity } => Some(Action::Note { note: pad(channel, note).unwrap_or(note), velocity: velocity.max(1) }),
        Message::NoteOff { channel, note } => Some(Action::NoteOff { note: pad(channel, note).unwrap_or(note) }),
        Message::Control { cc: 64, value, .. } => Some(Action::Sustain(value >= 64)),
        Message::Control { cc, value, .. } => {
            let param = match (layout, cc) {
                (Layout::MpkMini, c) if (MPK_FIRST_KNOB..MPK_FIRST_KNOB + 8).contains(&c) => MPK_KNOBS[(c - MPK_FIRST_KNOB) as usize],
                (_, 74) => SynthParam::Cutoff,
                (_, 71) => SynthParam::Resonance,
                (_, 73) => SynthParam::AmpAttack,
                (_, 72) => SynthParam::AmpRelease,
                (_, 7) => SynthParam::Volume,
                (_, 91) => SynthParam::ReverbMix,
                (_, 93) => SynthParam::ChorusMix,
                (_, 5) => SynthParam::Glide,
                _ => return None,
            };
            Some(Action::Knob { param, value: value as f32 / 127.0 })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_parse_and_velocity_zero_is_a_release() {
        assert_eq!(parse(&[0x90, 60, 100]), Some(Message::NoteOn { channel: 0, note: 60, velocity: 100 }));
        assert_eq!(parse(&[0x99, 36, 0]), Some(Message::NoteOff { channel: 9, note: 36 }));
        assert_eq!(parse(&[0x80, 60, 40]), Some(Message::NoteOff { channel: 0, note: 60 }));
        assert_eq!(parse(&[0xb0, 70, 127]), Some(Message::Control { channel: 0, cc: 70, value: 127 }));
        assert_eq!(parse(&[0xf8]), None);
        assert_eq!(parse(&[0xe0, 0, 64]), None);
        assert_eq!(parse(&[]), None);
    }

    #[test]
    fn mpk_pads_play_the_drum_kit_by_position() {
        let kit = [36, 38, 39, 42, 46];
        let mpk = Layout::for_port("MPK mini IV MIDI 1");
        assert_eq!(mpk, Layout::MpkMini);
        // Pad 5 (note 40) is the fifth pad: the open hat.
        assert_eq!(map(Message::NoteOn { channel: 9, note: 40, velocity: 90 }, mpk, &kit), Some(Action::Note { note: 46, velocity: 90 }));
        assert_eq!(map(Message::NoteOff { channel: 9, note: 40 }, mpk, &kit), Some(Action::NoteOff { note: 46 }));
        // On a Carve track the pads are just notes; so are the keys anywhere.
        assert_eq!(map(Message::NoteOn { channel: 9, note: 40, velocity: 90 }, mpk, &[]), Some(Action::Note { note: 40, velocity: 90 }));
        assert_eq!(map(Message::NoteOn { channel: 0, note: 40, velocity: 90 }, mpk, &kit), Some(Action::Note { note: 40, velocity: 90 }));
    }

    #[test]
    fn knobs_turn_carve_and_the_pedal_sustains() {
        let mpk = Layout::MpkMini;
        assert_eq!(map(Message::Control { channel: 0, cc: 70, value: 127 }, mpk, &[]), Some(Action::Knob { param: SynthParam::Cutoff, value: 1.0 }));
        assert_eq!(map(Message::Control { channel: 0, cc: 77, value: 0 }, mpk, &[]), Some(Action::Knob { param: SynthParam::Volume, value: 0.0 }));
        let other = Layout::for_port("Keystation 49");
        assert_eq!(other, Layout::Generic);
        assert_eq!(map(Message::Control { channel: 0, cc: 70, value: 64 }, other, &[]), None);
        assert!(matches!(map(Message::Control { channel: 0, cc: 74, value: 64 }, other, &[]), Some(Action::Knob { param: SynthParam::Cutoff, .. })));
        assert_eq!(map(Message::Control { channel: 0, cc: 64, value: 127 }, other, &[]), Some(Action::Sustain(true)));
        assert_eq!(map(Message::Control { channel: 0, cc: 64, value: 0 }, other, &[]), Some(Action::Sustain(false)));
    }
}
