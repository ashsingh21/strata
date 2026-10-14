//! Playing a MIDI clip in live: notes held and let go while the song
//! plays, stamped with the playhead's tick, and - once playback stops -
//! one clip on whole bars holding them.

use std::collections::HashMap;

use super::model::{Clip, ClipContent, ClipId, ControlPoint, MidiControls, MidiNote, TrackId};
use super::time::Ticks;

/// A take in progress: the notes played so far, at song ticks.
#[derive(Clone, Debug, Default)]
pub struct LiveTake {
    pub track: TrackId,
    /// Notes let go, at song ticks.
    pub notes: Vec<MidiNote>,
    /// Keys still down: pitch -> (tick pressed, velocity).
    held: HashMap<u8, (Ticks, u8)>,
    /// Wheel moves, at song ticks.
    pub controls: MidiControls,
}

impl LiveTake {
    pub fn new(track: TrackId) -> Self {
        Self { track, notes: Vec::new(), held: HashMap::new(), controls: MidiControls::default() }
    }

    pub fn press(&mut self, pitch: u8, velocity: u8, at: Ticks) {
        // A key pressed again before its release reached us: end the old one.
        self.release(pitch, at);
        self.held.insert(pitch, (at, velocity));
    }

    pub fn release(&mut self, pitch: u8, at: Ticks) {
        if let Some((start, velocity)) = self.held.remove(&pitch) {
            self.notes.push(MidiNote { start, length: (at - start).max(1), pitch, velocity });
        }
    }

    /// A wheel moved while recording.
    pub fn wheel(&mut self, wheel: super::schedule::Wheel, value: f32, at: Ticks) {
        let points = match wheel {
            super::schedule::Wheel::Bend => &mut self.controls.bend,
            super::schedule::Wheel::Mod => &mut self.controls.modwheel,
        };
        points.retain(|p| p.tick != at);
        points.push(ControlPoint { tick: at, value });
    }

    /// The loop jumped back (or playback stopped): every key still down
    /// ends where the playhead was.
    pub fn release_all(&mut self, at: Ticks) {
        let pitches: Vec<u8> = self.held.keys().copied().collect();
        for pitch in pitches {
            self.release(pitch, at);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.notes.is_empty() && self.held.is_empty()
    }

    /// The take as a clip on whole bars (`None` if nothing was played):
    /// from the bar the first note is in to the end of the bar the last
    /// one ends in.
    pub fn into_clip(mut self, id: ClipId, ticks_per_bar: Ticks, end: Ticks) -> Option<Clip> {
        self.release_all(end);
        let first = self.notes.iter().map(|n| n.start).min()?;
        let last = self.notes.iter().map(|n| n.start + n.length).max()?;
        let bar = ticks_per_bar.max(1);
        let start = first.max(0).div_euclid(bar) * bar;
        let end = (last + bar - 1).div_euclid(bar) * bar;
        let mut notes: Vec<MidiNote> = self.notes.into_iter().map(|n| MidiNote { start: n.start - start, ..n }).collect();
        notes.sort_by_key(|n| (n.start, n.pitch));
        let relative = |points: Vec<ControlPoint>| {
            let mut points: Vec<ControlPoint> =
                points.into_iter().filter(|p| p.tick >= start && p.tick < end).map(|p| ControlPoint { tick: p.tick - start, ..p }).collect();
            points.sort_by_key(|p| p.tick);
            points
        };
        let controls = MidiControls { bend: relative(self.controls.bend), modwheel: relative(self.controls.modwheel) };
        Some(Clip {
            id,
            track: self.track,
            start,
            length: (end - start).max(bar),
            name: "Played".into(),
            content: ClipContent::Midi { notes, loop_len: None, link: None, controls },
            recording: false,
            gain_db: 0.0,
            swing: 0.0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrangement::time::PPQ;

    const BAR: Ticks = PPQ * 4;

    #[test]
    fn a_take_becomes_a_clip_on_whole_bars() {
        let mut take = LiveTake::new(3);
        take.press(60, 90, BAR + PPQ);
        take.release(60, BAR + PPQ * 2);
        take.press(64, 70, BAR * 2 + PPQ * 3);
        let clip = take.into_clip(7, BAR, BAR * 3 - 10).unwrap();
        assert_eq!((clip.track, clip.start, clip.length), (3, BAR, BAR * 2));
        let ClipContent::Midi { notes, .. } = clip.content else { panic!() };
        // Relative to the clip; the key still down ends where playback did.
        assert_eq!(notes[0], MidiNote { start: PPQ, length: PPQ, pitch: 60, velocity: 90 });
        assert_eq!((notes[1].start, notes[1].length), (BAR + PPQ * 3, PPQ - 10));
    }

    #[test]
    fn nothing_played_is_no_clip() {
        assert!(LiveTake::new(1).into_clip(2, BAR, BAR).is_none());
    }

    #[test]
    fn a_loop_pass_layers_onto_the_same_take() {
        let mut take = LiveTake::new(1);
        take.press(36, 100, 0);
        take.release(36, PPQ);
        // The loop came round: a second pass over the same bar.
        take.press(42, 100, PPQ / 2);
        take.release(42, PPQ);
        let clip = take.into_clip(5, BAR, BAR).unwrap();
        assert_eq!(clip.length, BAR);
        assert!(matches!(clip.content, ClipContent::Midi { ref notes, .. } if notes.len() == 2));
    }
}
