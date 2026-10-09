//! A song as sections: the places the Map view draws. From the markers if
//! there are any (each runs to the next), else found by listening for where
//! the set of parts playing changes. For each: the parts in it, how busy it
//! is (its energy, 0..1 across the song) and, between two, what comes in
//! and what drops out.

use crate::arrangement::{Arrangement, ClipColor, ClipContent, Ticks, TrackId, PPQ};

const BAR: Ticks = 4 * PPQ;
/// Found sections shorter than this are folded into the one before.
const MIN_BARS: i64 = 4;

#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub track: TrackId,
    pub name: String,
    pub color: ClipColor,
    /// Playing through (nearly) all of the section, not just part of it.
    pub full: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub name: String,
    pub start: Ticks,
    pub end: Ticks,
    pub layers: Vec<Layer>,
    /// How busy, 0 (silent) to 1 (the busiest section of the song).
    pub energy: f32,
}

impl Section {
    pub fn bars(&self) -> i64 {
        (self.end - self.start + BAR - 1) / BAR
    }
}

/// The song's end: its last clip's end, in whole bars.
pub fn song_end(arr: &Arrangement) -> Ticks {
    let end = arr.clips.iter().map(|c| c.end()).max().unwrap_or(0);
    (end + BAR - 1) / BAR * BAR
}

/// For each bar, per track: how many notes start in it, weighted by how
/// hard they're hit (audio clips count as a steady part).
fn activity(arr: &Arrangement, bars: usize) -> Vec<Vec<(TrackId, f32)>> {
    let mut out: Vec<Vec<(TrackId, f32)>> = vec![Vec::new(); bars];
    let add = |bar: usize, track: TrackId, amount: f32, out: &mut Vec<Vec<(TrackId, f32)>>| {
        if bar >= out.len() {
            return;
        }
        match out[bar].iter_mut().find(|(t, _)| *t == track) {
            Some((_, a)) => *a += amount,
            None => out[bar].push((track, amount)),
        }
    };
    for track in &arr.tracks {
        if track.mute {
            continue;
        }
        for clip in arr.clips.iter().filter(|c| c.track == track.id) {
            match &clip.content {
                ClipContent::Midi { .. } => {
                    for n in clip.played_notes() {
                        let at = clip.start + n.start;
                        add((at / BAR) as usize, track.id, n.velocity as f32 / 100.0, &mut out);
                    }
                }
                ClipContent::Audio { .. } => {
                    for bar in clip.start / BAR..(clip.end() + BAR - 1) / BAR {
                        add(bar as usize, track.id, 4.0, &mut out);
                    }
                }
            }
        }
    }
    out
}

fn layers_in(arr: &Arrangement, act: &[Vec<(TrackId, f32)>], from: usize, to: usize) -> Vec<Layer> {
    let bars = (to - from).max(1);
    arr.tracks
        .iter()
        .filter_map(|t| {
            let playing = act[from..to].iter().filter(|bar| bar.iter().any(|(id, a)| *id == t.id && *a > 0.0)).count();
            (playing > 0).then(|| Layer { track: t.id, name: t.name.clone(), color: t.color, full: playing * 4 >= bars * 3 })
        })
        .collect()
}

pub fn sections(arr: &Arrangement) -> Vec<Section> {
    let end = song_end(arr);
    if end == 0 {
        return Vec::new();
    }
    let bars = (end / BAR) as usize;
    let act = activity(arr, bars);
    // Where each section starts (in bars), and its name.
    let mut cuts: Vec<(usize, String)> = Vec::new();
    let mut markers: Vec<_> = arr.markers.iter().filter(|m| m.position < end).collect();
    markers.sort_by_key(|m| m.position);
    if markers.is_empty() {
        // Where the parts playing change, in runs of at least MIN_BARS.
        let set = |bar: usize| {
            let mut s: Vec<TrackId> = act[bar].iter().filter(|(_, a)| *a > 0.0).map(|(t, _)| *t).collect();
            s.sort_unstable();
            s
        };
        let mut start = 0;
        for bar in 1..=bars {
            if bar == bars || set(bar) != set(start) {
                if bar - start >= MIN_BARS as usize || cuts.is_empty() {
                    cuts.push((start, String::new()));
                }
                start = bar;
            }
        }
        for (i, cut) in cuts.iter_mut().enumerate() {
            cut.1 = format!("Part {}", i + 1);
        }
    } else {
        if markers[0].position >= BAR {
            cuts.push((0, "Start".to_string()));
        }
        for m in markers {
            let bar = (m.position / BAR) as usize;
            match cuts.last_mut() {
                // Two markers in one bar: the later name wins.
                Some(last) if last.0 == bar => last.1 = m.name.clone(),
                _ => cuts.push((bar, m.name.clone())),
            }
        }
    }
    let mut out: Vec<Section> = cuts
        .iter()
        .enumerate()
        .map(|(i, (from, name))| {
            let to = cuts.get(i + 1).map(|c| c.0).unwrap_or(bars).max(from + 1).min(bars);
            let busy: f32 = act[*from..to].iter().map(|bar| bar.iter().map(|(_, a)| a).sum::<f32>()).sum::<f32>() / (to - from).max(1) as f32;
            Section {
                name: name.clone(),
                start: *from as Ticks * BAR,
                end: to as Ticks * BAR,
                layers: layers_in(arr, &act, *from, to),
                energy: busy,
            }
        })
        .collect();
    let top = out.iter().map(|s| s.energy).fold(0.0f32, f32::max);
    if top > 0.0 {
        for s in &mut out {
            s.energy /= top;
        }
    }
    out
}

/// What changes from `a` to `b`: the parts that come in, and those that
/// drop out (by name).
pub fn changes(a: &Section, b: &Section) -> (Vec<String>, Vec<String>) {
    let has = |s: &Section, t: TrackId| s.layers.iter().any(|l| l.track == t);
    let entering = b.layers.iter().filter(|l| !has(a, l.track)).map(|l| l.name.clone()).collect();
    let leaving = a.layers.iter().filter(|l| !has(b, l.track)).map(|l| l.name.clone()).collect();
    (entering, leaving)
}

/// The section playing at `tick`.
pub fn at(sections: &[Section], tick: Ticks) -> Option<usize> {
    sections.iter().position(|s| tick >= s.start && tick < s.end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrangement::{empty_arrangement, Instrument, Marker, MidiNote};
    use crate::lessons::{add_clip, add_track};

    fn beat(pitch: u8) -> Vec<MidiNote> {
        (0..4).map(|b| MidiNote { start: b * PPQ, length: PPQ / 2, pitch, velocity: 100 }).collect()
    }

    /// Drums all through; a melody only in bars 9-16.
    fn song() -> Arrangement {
        let mut arr = empty_arrangement();
        let drums = add_track(&mut arr, "Drums", ClipColor::Coral, Instrument::Drums, 0.0);
        add_clip(&mut arr, drums, "Beat", 0, 24, 1, beat(36));
        let melody = add_track(&mut arr, "Melody", ClipColor::Blue, Instrument::Carve, 0.0);
        add_clip(&mut arr, melody, "Tune", 8, 16, 1, beat(72));
        arr
    }

    #[test]
    fn without_markers_the_parts_playing_mark_the_sections() {
        let s = sections(&song());
        assert_eq!(s.len(), 3, "{s:?}");
        assert_eq!((s[0].start / BAR, s[0].end / BAR), (0, 8));
        assert_eq!((s[1].start / BAR, s[1].end / BAR), (8, 16));
        assert_eq!(s[1].layers.len(), 2);
        assert!((s[1].energy - 1.0).abs() < 1e-6, "the busiest is 1");
        assert!(s[0].energy < s[1].energy);
        let (inn, out) = changes(&s[0], &s[1]);
        assert_eq!((inn, out), (vec!["Melody".to_string()], vec![]));
        let (inn, out) = changes(&s[1], &s[2]);
        assert_eq!((inn, out), (vec![], vec!["Melody".to_string()]));
        assert_eq!(at(&s, 9 * BAR), Some(1));
    }

    #[test]
    fn markers_name_and_cut_the_sections() {
        let mut arr = song();
        arr.markers.push(Marker { id: 900, position: 4 * BAR, name: "Build".into() });
        arr.markers.push(Marker { id: 901, position: 8 * BAR, name: "Drop".into() });
        let s = sections(&arr);
        let names: Vec<_> = s.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["Start", "Build", "Drop"]);
        assert_eq!(s[2].bars(), 16);
        assert!(!s[2].layers.iter().find(|l| l.name == "Melody").unwrap().full, "the melody plays only half the drop");
    }

    #[test]
    fn an_empty_song_has_no_sections() {
        assert!(sections(&empty_arrangement()).is_empty());
    }
}
