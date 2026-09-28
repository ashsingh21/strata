//! The piano roll's own (small) UI state: which clip is open, edit mode,
//! label mode and the current note selection. Actually editing notes goes
//! through `TimelineEvent` (see `timeline::state`) since that's what owns
//! the arrangement and undo stack - this model only tracks what's shown
//! and selected, none of which is itself undo-able.

use std::collections::{HashMap, HashSet};

use vizia::prelude::*;

use shared::arrangement::{ClipId, Ticks};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditMode {
    /// Click a note to select it; click empty space to clear the
    /// selection. Delete/Backspace removes whatever's selected.
    Select,
    /// Click empty space to add a note (snapped, one grid step long);
    /// click an existing note to remove it.
    Draw,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelMode {
    /// Row/note labels lead with the note name (e.g. "A4").
    Notes,
    /// Row/note labels lead with the scale-degree interval (e.g. "b3").
    Intervals,
    /// Row/note labels lead with the sargam name (Sa, re, Re...).
    Sargam,
}

impl LabelMode {
    /// The next one, as clicking the row labels' heading cycles them.
    pub fn next(self) -> Self {
        match self {
            LabelMode::Intervals => LabelMode::Notes,
            LabelMode::Notes => LabelMode::Sargam,
            LabelMode::Sargam => LabelMode::Intervals,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            LabelMode::Intervals => "Interval",
            LabelMode::Notes => "Note",
            LabelMode::Sargam => "Sargam",
        }
    }
}

/// What one Draw click writes: a note, or a chord stacked on it in thirds
/// of the current scale (so it's always in key).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChordShape {
    Note,
    Triad,
    Seventh,
}

impl ChordShape {
    pub const ALL: [ChordShape; 3] = [ChordShape::Note, ChordShape::Triad, ChordShape::Seventh];

    pub fn label(self) -> &'static str {
        match self {
            ChordShape::Note => "Note",
            ChordShape::Triad => "Triad",
            ChordShape::Seventh => "7th",
        }
    }

    /// The pitches a click on `root` writes: the root, then every other
    /// scale note above it (a third each). Notes past MIDI's top are left
    /// out.
    pub fn pitches(self, root: u8, key: u8, mask: u16) -> Vec<u8> {
        let size = match self {
            ChordShape::Note => 1,
            ChordShape::Triad => 3,
            ChordShape::Seventh => 4,
        };
        std::iter::once(Some(root))
            .chain((1..size).map(|i| shared::theory::scale_step(root, key, mask, 2 * i)))
            .flatten()
            .collect()
    }
}

/// A note's identity within its clip: `(start tick, pitch)`. Matches how
/// `Command::RemoveMidiNote` already identifies a note, so no separate id
/// scheme is needed.
pub type NoteKey = (Ticks, u8);

pub struct PianoRollModel {
    /// `None` when the piano roll is closed; the clip being edited
    /// otherwise. This one signal is both "is it open" and "which clip".
    pub open_clip: Signal<Option<ClipId>>,
    pub mode: Signal<EditMode>,
    pub label_mode: Signal<LabelMode>,
    pub selected: Signal<HashSet<NoteKey>>,
    /// The open clip's row window, in octaves from its default (see
    /// `grid::row_pitches`), and where each clip's was left.
    pub octave: Signal<i32>,
    pub chord: Signal<ChordShape>,
    octaves: HashMap<ClipId, i32>,
}

pub enum PianoRollEvent {
    Open(ClipId),
    Close,
    SetMode(EditMode),
    SetLabelMode(LabelMode),
    /// Click on a note: replaces the selection, or toggles membership when
    /// `extend` (shift-click).
    SelectNote { key: NoteKey, extend: bool },
    ClearSelection,
    /// Replaces the selection - after moving notes, so they stay selected.
    SetSelection(HashSet<NoteKey>),
    /// Moves the rows' window up or down by whole octaves.
    ShiftOctave(i32),
    SetChord(ChordShape),
}

impl PianoRollModel {
    pub fn new() -> Self {
        Self {
            open_clip: Signal::new(None),
            mode: Signal::new(EditMode::Draw),
            label_mode: Signal::new(LabelMode::Intervals),
            selected: Signal::new(HashSet::new()),
            octave: Signal::new(0),
            chord: Signal::new(ChordShape::Note),
            octaves: HashMap::new(),
        }
    }
}

impl Default for PianoRollModel {
    fn default() -> Self {
        Self::new()
    }
}

impl Model for PianoRollModel {
    fn event(&mut self, _cx: &mut EventContext, event: &mut Event) {
        event.map(|event, _| match event {
            PianoRollEvent::Open(clip) => {
                self.open_clip.set(Some(*clip));
                self.selected.set(HashSet::new());
                self.octave.set(self.octaves.get(clip).copied().unwrap_or(0));
            }
            PianoRollEvent::Close => {
                self.open_clip.set(None);
                self.selected.set(HashSet::new());
            }
            PianoRollEvent::SetMode(mode) => self.mode.set(*mode),
            PianoRollEvent::SetLabelMode(mode) => self.label_mode.set(*mode),
            PianoRollEvent::SelectNote { key, extend } => {
                self.selected.update(|sel| {
                    if *extend {
                        if !sel.remove(key) {
                            sel.insert(*key);
                        }
                    } else {
                        *sel = std::iter::once(*key).collect();
                    }
                });
            }
            PianoRollEvent::ClearSelection => self.selected.set(HashSet::new()),
            PianoRollEvent::SetChord(shape) => self.chord.set(*shape),
            PianoRollEvent::SetSelection(keys) => self.selected.set(keys.clone()),
            PianoRollEvent::ShiftOctave(by) => {
                let Some(clip) = self.open_clip.get() else { return };
                // MIDI's range: A0 up to about C8 is plenty either way.
                let octave = (self.octave.get() + by).clamp(-3, 3);
                self.octave.set(octave);
                self.octaves.insert(clip, octave);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: u8 = 0;
    const C_MINOR: u16 = 0b0101_1010_1101; // 0 2 3 5 7 8 10

    #[test]
    fn chords_stack_thirds_in_the_key() {
        assert_eq!(ChordShape::Note.pitches(60, C, C_MINOR), vec![60]);
        // C minor: C Eb G, and with the 7th Bb.
        assert_eq!(ChordShape::Triad.pitches(60, C, C_MINOR), vec![60, 63, 67]);
        assert_eq!(ChordShape::Seventh.pitches(60, C, C_MINOR), vec![60, 63, 67, 70]);
        // On the 3rd degree (Eb): Eb G Bb.
        assert_eq!(ChordShape::Triad.pitches(63, C, C_MINOR), vec![63, 67, 70]);
        // Nothing past 127.
        assert_eq!(ChordShape::Seventh.pitches(120, C, C_MINOR), vec![120, 123, 127]);
    }
}
