//! Drives Carve from the timeline during playback: each frame, schedules
//! whatever MIDI notes the playhead crossed since the last frame as real
//! note on/off through [`SynthEvent`]. Reference-counts overlapping
//! same-pitch notes (from different clips or tracks) so one ending early
//! doesn't cut a still-sounding one short.
//!
//! Coarse (~16ms, the UI timer's rate) rather than sample-accurate - fine
//! for a first pass, since nothing else in the timeline is sample-accurate
//! yet either.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use vizia::prelude::*;

use shared::arrangement::{notes_in_range, Arrangement, Ticks};

use crate::synth::state::SynthEvent;

pub struct MidiScheduler {
    last_tick: Cell<Ticks>,
    was_playing: Cell<bool>,
    held: RefCell<HashMap<u8, u32>>,
}

impl MidiScheduler {
    pub fn new() -> Self {
        Self { last_tick: Cell::new(0), was_playing: Cell::new(false), held: RefCell::new(HashMap::new()) }
    }

    /// Call once per frame with the current playhead tick and transport
    /// state.
    pub fn advance(&self, cx: &mut EventContext, arrangement: &Arrangement, tick: Ticks, playing: bool) {
        if !playing {
            if self.was_playing.get() {
                self.release_all(cx);
            }
            self.was_playing.set(false);
            self.last_tick.set(tick);
            return;
        }

        // Just started, or just looped/seeked backward: don't replay
        // history, just resume scheduling from here.
        if !self.was_playing.get() || tick < self.last_tick.get() {
            if self.was_playing.get() {
                self.release_all(cx);
            }
            self.was_playing.set(true);
            self.last_tick.set(tick.saturating_sub(1));
        }

        let from = self.last_tick.get();
        self.last_tick.set(tick);

        let scheduled = notes_in_range(arrangement, from, tick);
        let mut held = self.held.borrow_mut();
        for pitch in scheduled.note_off {
            if let Some(count) = held.get_mut(&pitch) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    held.remove(&pitch);
                    cx.emit(SynthEvent::NoteOff(pitch));
                }
            }
        }
        for pitch in scheduled.note_on {
            let count = held.entry(pitch).or_insert(0);
            if *count == 0 {
                cx.emit(SynthEvent::NoteOn(pitch));
            }
            *count += 1;
        }
    }

    fn release_all(&self, cx: &mut EventContext) {
        let mut held = self.held.borrow_mut();
        for &pitch in held.keys() {
            cx.emit(SynthEvent::NoteOff(pitch));
        }
        held.clear();
    }
}

impl Default for MidiScheduler {
    fn default() -> Self {
        Self::new()
    }
}
