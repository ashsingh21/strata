//! The piano roll's note tools. Each works on the chosen notes - the
//! selection, or every note when nothing is selected - and returns the
//! clip's whole new note list plus where the chosen notes ended up (so
//! they stay selected).

use std::collections::HashSet;

use super::model::MidiNote;
use super::time::Ticks;

/// A note's identity in the piano roll: (start, pitch).
pub type NoteKey = (Ticks, u8);

/// What a tool did: the clip's notes, and the chosen ones' new keys.
pub struct Edited {
    pub notes: Vec<MidiNote>,
    pub chosen: HashSet<NoteKey>,
}

/// Runs `tool` on the chosen notes (all of them if `selected` is empty)
/// and puts the rest back around them. A chosen note that lands exactly
/// on an unchosen one replaces it.
fn on_chosen(notes: &[MidiNote], selected: &HashSet<NoteKey>, tool: impl FnOnce(Vec<MidiNote>) -> Vec<MidiNote>) -> Edited {
    let all = selected.is_empty();
    let (chosen, rest): (Vec<MidiNote>, Vec<MidiNote>) =
        notes.iter().partition(|n| all || selected.contains(&(n.start, n.pitch)));
    let mut out = tool(chosen);
    out.retain(|n| n.start >= 0 && n.length > 0);
    out.sort_by_key(|n| (n.start, n.pitch));
    out.dedup_by_key(|n| (n.start, n.pitch));
    let keys: HashSet<NoteKey> = out.iter().map(|n| (n.start, n.pitch)).collect();
    let mut notes: Vec<MidiNote> = rest.into_iter().filter(|n| !keys.contains(&(n.start, n.pitch))).collect();
    notes.extend(out);
    notes.sort_by_key(|n| (n.start, n.pitch));
    Edited { notes, chosen: if all { HashSet::new() } else { keys } }
}

/// Pulls each note's start toward the nearest `grid` line: `strength` 1.0
/// lands it there, 0.5 halfway. Lengths stay as played.
pub fn quantize(notes: &[MidiNote], selected: &HashSet<NoteKey>, grid: Ticks, strength: f32) -> Edited {
    let grid = grid.max(1);
    let strength = strength.clamp(0.0, 1.0) as f64;
    on_chosen(notes, selected, |chosen| {
        chosen
            .into_iter()
            .map(|n| {
                let target = ((n.start as f64 / grid as f64).round() as Ticks) * grid;
                let start = n.start + ((target - n.start) as f64 * strength).round() as Ticks;
                MidiNote { start, ..n }
            })
            .collect()
    })
}

/// Moves the chosen notes later (or earlier, negative) by `by`; none goes
/// before the clip's start.
pub fn nudge(notes: &[MidiNote], selected: &HashSet<NoteKey>, by: Ticks) -> Edited {
    let earliest = chosen_of(notes, selected).map(|n| n.start).min().unwrap_or(0);
    let by = by.max(-earliest);
    on_chosen(notes, selected, |chosen| chosen.into_iter().map(|n| MidiNote { start: n.start + by, ..n }).collect())
}

/// Up (or down) by `semitones`; does nothing if a note would leave 0..=127.
pub fn transpose(notes: &[MidiNote], selected: &HashSet<NoteKey>, semitones: i32) -> Edited {
    let fits = chosen_of(notes, selected).all(|n| (0..=127).contains(&(n.pitch as i32 + semitones)));
    let semitones = if fits { semitones } else { 0 };
    on_chosen(notes, selected, |chosen| {
        chosen.into_iter().map(|n| MidiNote { pitch: (n.pitch as i32 + semitones) as u8, ..n }).collect()
    })
}

/// Every note outside the scale (`key` 0..12, `mask` bit n = n semitones
/// above the key) moved to the nearest note in it - down on a tie.
pub fn fit_to_scale(notes: &[MidiNote], selected: &HashSet<NoteKey>, key: u8, mask: u16) -> Edited {
    let in_scale = |p: i32| mask & (1 << ((p - key as i32).rem_euclid(12))) != 0;
    on_chosen(notes, selected, |chosen| {
        chosen
            .into_iter()
            .map(|n| {
                let p = n.pitch as i32;
                let fitted = (0..=6)
                    .flat_map(|d| [p - d, p + d])
                    .find(|q| (0..=127).contains(q) && in_scale(*q))
                    .unwrap_or(p);
                MidiNote { pitch: fitted as u8, ..n }
            })
            .collect()
    })
}

/// Each note held until the next chosen note starts (a chord's notes all
/// reach the next chord); the last keeps its length.
pub fn legato(notes: &[MidiNote], selected: &HashSet<NoteKey>) -> Edited {
    on_chosen(notes, selected, |chosen| {
        let mut starts: Vec<Ticks> = chosen.iter().map(|n| n.start).collect();
        starts.sort();
        starts.dedup();
        chosen
            .into_iter()
            .map(|n| match starts.iter().find(|s| **s > n.start) {
                Some(next) => MidiNote { length: next - n.start, ..n },
                None => n,
            })
            .collect()
    })
}

/// Each note cut into repeats `step` long (a roll), the last one ending
/// where the note did.
pub fn chop(notes: &[MidiNote], selected: &HashSet<NoteKey>, step: Ticks) -> Edited {
    let step = step.max(1);
    on_chosen(notes, selected, |chosen| {
        chosen
            .into_iter()
            .flat_map(|n| {
                let end = n.start + n.length;
                (0..)
                    .map(move |i| n.start + i * step)
                    .take_while(move |s| *s < end)
                    .map(move |s| MidiNote { start: s, length: step.min(end - s), ..n })
            })
            .collect()
    })
}

/// Chords spread out like a strum: in each group of notes starting
/// together, each note `spread` later than the one below it (a negative
/// spread strums down from the top). Each still ends where it did.
pub fn strum(notes: &[MidiNote], selected: &HashSet<NoteKey>, spread: Ticks) -> Edited {
    on_chosen(notes, selected, |chosen| {
        let mut out = Vec::with_capacity(chosen.len());
        let mut starts: Vec<Ticks> = chosen.iter().map(|n| n.start).collect();
        starts.sort();
        starts.dedup();
        for start in starts {
            let mut chord: Vec<MidiNote> = chosen.iter().filter(|n| n.start == start).copied().collect();
            chord.sort_by_key(|n| n.pitch);
            if spread < 0 {
                chord.reverse();
            }
            for (i, n) in chord.into_iter().enumerate() {
                let delay = (i as Ticks * spread.abs()).min(n.length - 1);
                out.push(MidiNote { start: n.start + delay, length: n.length - delay, ..n });
            }
        }
        out
    })
}

/// The chosen notes played backwards, over the span they cover.
pub fn reverse(notes: &[MidiNote], selected: &HashSet<NoteKey>) -> Edited {
    on_chosen(notes, selected, |chosen| {
        let Some(first) = chosen.iter().map(|n| n.start).min() else { return chosen };
        let last = chosen.iter().map(|n| n.start + n.length).max().unwrap_or(first);
        chosen.into_iter().map(|n| MidiNote { start: first + last - (n.start + n.length), ..n }).collect()
    })
}

fn chosen_of<'a>(notes: &'a [MidiNote], selected: &'a HashSet<NoteKey>) -> impl Iterator<Item = &'a MidiNote> {
    notes.iter().filter(move |n| selected.is_empty() || selected.contains(&(n.start, n.pitch)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrangement::time::PPQ;

    fn n(start: Ticks, length: Ticks, pitch: u8) -> MidiNote {
        MidiNote { start, length, pitch, velocity: 100 }
    }

    fn none() -> HashSet<NoteKey> {
        HashSet::new()
    }

    #[test]
    fn quantize_lands_on_the_grid_or_part_way() {
        let notes = [n(PPQ + 30, 100, 60), n(2 * PPQ - 20, 100, 62)];
        let full = quantize(&notes, &none(), PPQ, 1.0).notes;
        assert_eq!(full.iter().map(|n| n.start).collect::<Vec<_>>(), vec![PPQ, 2 * PPQ]);
        let half = quantize(&notes, &none(), PPQ, 0.5).notes;
        assert_eq!(half.iter().map(|n| n.start).collect::<Vec<_>>(), vec![PPQ + 15, 2 * PPQ - 10]);
    }

    #[test]
    fn only_the_selection_moves_and_stays_selected() {
        let notes = [n(10, 100, 60), n(PPQ + 10, 100, 62)];
        let sel: HashSet<NoteKey> = [(PPQ + 10, 62)].into();
        let e = quantize(&notes, &sel, PPQ, 1.0);
        assert_eq!(e.notes.iter().map(|n| n.start).collect::<Vec<_>>(), vec![10, PPQ]);
        assert_eq!(e.chosen, [(PPQ, 62)].into());
    }

    #[test]
    fn nudge_stops_at_the_clip_start() {
        let e = nudge(&[n(30, 10, 60), n(100, 10, 62)], &none(), -50);
        assert_eq!(e.notes.iter().map(|n| n.start).collect::<Vec<_>>(), vec![0, 70]);
    }

    #[test]
    fn fit_to_scale_moves_only_wrong_notes() {
        // C major: C# goes down to C, F# down to F; E stays.
        let major = 0b1010_1011_0101;
        let e = fit_to_scale(&[n(0, 1, 61), n(1, 1, 64), n(2, 1, 66)], &none(), 0, major);
        assert_eq!(e.notes.iter().map(|n| n.pitch).collect::<Vec<_>>(), vec![60, 64, 65]);
    }

    #[test]
    fn legato_holds_each_chord_to_the_next() {
        let e = legato(&[n(0, 10, 60), n(0, 10, 64), n(PPQ, 10, 62)], &none());
        assert_eq!(e.notes.iter().map(|n| n.length).collect::<Vec<_>>(), vec![PPQ, PPQ, 10]);
    }

    #[test]
    fn chop_makes_a_roll_ending_with_the_note() {
        let e = chop(&[n(0, PPQ + PPQ / 2, 60)], &none(), PPQ / 2);
        assert_eq!(e.notes.iter().map(|n| (n.start, n.length)).collect::<Vec<_>>(), vec![(0, PPQ / 2), (PPQ / 2, PPQ / 2), (PPQ, PPQ / 2)]);
    }

    #[test]
    fn strum_spreads_a_chord_from_the_bottom() {
        let e = strum(&[n(0, PPQ, 64), n(0, PPQ, 60), n(0, PPQ, 67)], &none(), 20);
        let by_pitch = |p| e.notes.iter().find(|n| n.pitch == p).map(|n| (n.start, n.start + n.length)).unwrap();
        assert_eq!((by_pitch(60), by_pitch(64), by_pitch(67)), ((0, PPQ), (20, PPQ), (40, PPQ)));
    }

    #[test]
    fn reverse_plays_it_backwards() {
        let e = reverse(&[n(0, PPQ, 60), n(PPQ, 2 * PPQ, 62)], &none());
        assert_eq!(e.notes.iter().map(|n| (n.start, n.pitch)).collect::<Vec<_>>(), vec![(0, 62), (2 * PPQ, 60)]);
    }

    #[test]
    fn transpose_refuses_to_leave_the_keyboard() {
        let e = transpose(&[n(0, 1, 120), n(1, 1, 60)], &none(), 12);
        assert_eq!(e.notes.iter().map(|n| n.pitch).collect::<Vec<_>>(), vec![120, 60]);
    }
}
