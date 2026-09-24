//! The piano roll's own (small) UI state: which clip is open, edit mode,
//! label mode and the current note selection. Actually editing notes goes
//! through `TimelineEvent` (see `timeline::state`) since that's what owns
//! the arrangement and undo stack - this model only tracks what's shown
//! and selected, none of which is itself undo-able.

use std::collections::HashSet;

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
}

impl PianoRollModel {
    pub fn new() -> Self {
        Self {
            open_clip: Signal::new(None),
            mode: Signal::new(EditMode::Draw),
            label_mode: Signal::new(LabelMode::Intervals),
            selected: Signal::new(HashSet::new()),
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
        });
    }
}
