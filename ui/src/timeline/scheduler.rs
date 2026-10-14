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

use shared::arrangement::TrackId;

use shared::arrangement::{timed_notes_in_range, Arrangement, Ticks};

use crate::synth::state::SynthEvent;

pub struct MidiScheduler {
    last_tick: Cell<Ticks>,
    was_playing: Cell<bool>,
    /// The engine's seek count as of the last frame.
    seen_seeks: Cell<u32>,
    /// (track, pitch) -> how many overlapping notes hold it.
    held: RefCell<HashMap<(TrackId, u8), u32>>,
}

impl MidiScheduler {
    pub fn new() -> Self {
        Self { last_tick: Cell::new(0), was_playing: Cell::new(false), seen_seeks: Cell::new(0), held: RefCell::new(HashMap::new()) }
    }

    /// Call once per frame with the current playhead tick and transport
    /// state.
    pub fn advance(&self, cx: &mut EventContext, arrangement: &Arrangement, tick: Ticks, playing: bool, seeks: u32) {
        if !playing {
            if self.was_playing.get() {
                self.release_all(cx);
                cx.emit(SynthEvent::AllNotesOff);
            }
            self.was_playing.set(false);
            self.last_tick.set(tick);
            return;
        }

        // Just started, or just looped/seeked backward: don't replay
        // history, just resume scheduling from here.
        let jumped = seeks != self.seen_seeks.get();
        self.seen_seeks.set(seeks);
        if !self.was_playing.get() || jumped || tick < self.last_tick.get() {
            if self.was_playing.get() {
                self.release_all(cx);
            }
            self.was_playing.set(true);
            self.last_tick.set(tick.saturating_sub(1));
        }

        let from = self.last_tick.get();
        self.last_tick.set(tick);

        // In time order: when a frame runs long (the UI stalled, or a short
        // note) a note can start and end inside one frame, and handling
        // every off before every on left that note held - a Mono bass
        // then slid back to it under the next notes and droned on.
        let mut held = self.held.borrow_mut();
        for event in timed_notes_in_range(arrangement, from, tick) {
            let key = (event.track, event.pitch);
            if event.on {
                let count = held.entry(key).or_insert(0);
                if *count == 0 {
                    cx.emit(SynthEvent::NoteOn(event.track, event.pitch, event.velocity));
                }
                *count += 1;
            } else if let Some(count) = held.get_mut(&key) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    held.remove(&key);
                    cx.emit(SynthEvent::NoteOff(event.track, event.pitch));
                }
            }
        }
    }

    fn release_all(&self, cx: &mut EventContext) {
        let mut held = self.held.borrow_mut();
        for &(track, pitch) in held.keys() {
            cx.emit(SynthEvent::NoteOff(track, pitch));
        }
        held.clear();
    }
}

impl Default for MidiScheduler {
    fn default() -> Self {
        Self::new()
    }
}
