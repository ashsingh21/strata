//! What the chat tells the model about the project, as plain text: the
//! tempo and key, each track and where its clips sit, the chords bar by
//! bar, a drum pattern's hits, and - in full - the notes of the clip
//! that's open (or the selected track's), so "how's my melody?" is about
//! this melody.

use shared::arrangement::{Arrangement, ClipContent, ClipId, Instrument, MidiNote, TrackId, TrackKind, PPQ};
use shared::theory::{chord, note_name_for_key};

const BAR: i64 = 4 * PPQ;
/// At most this many notes of a clip written out.
const MAX_NOTES: usize = 96;

pub struct Scene<'a> {
    pub arrangement: &'a Arrangement,
    pub key: u8,
    pub scale: &'a str,
    pub open_clip: Option<ClipId>,
    pub selected_track: Option<TrackId>,
}

/// "C4", "F#3" (spelled for the key).
fn pitch(p: u8, key: u8) -> String {
    format!("{}{}", note_name_for_key(p % 12, key), p as i32 / 12 - 1)
}

/// A position as bar.beat (1-based), with the 16th when it's between beats:
/// "3.2", "3.2+1/16"... kept short, the way musicians count.
fn place(ticks: i64) -> String {
    let bar = ticks / BAR + 1;
    let beat = ticks % BAR / PPQ + 1;
    let sixteenth = ticks % PPQ / (PPQ / 4);
    let rest = ticks % (PPQ / 4);
    match (sixteenth, rest) {
        (0, 0) => format!("{bar}.{beat}"),
        (2, 0) => format!("{bar}.{beat}&"),
        (s, 0) => format!("{bar}.{beat}+{s}/16"),
        _ => format!("{bar}.{beat}~"),
    }
}

/// A length in beats: "1 beat", "1/2", "1 1/2", "3/4".
fn length(ticks: i64) -> String {
    let sixteenths = (ticks + PPQ / 8) / (PPQ / 4);
    let (beats, rest) = (sixteenths / 4, sixteenths % 4);
    let frac = ["", "1/4", "1/2", "3/4"][rest as usize];
    match (beats, frac) {
        (0, "") => "a tick".to_string(),
        (0, f) => format!("{f} beat"),
        (1, "") => "1 beat".to_string(),
        (b, "") => format!("{b} beats"),
        (b, f) => format!("{b} {f} beats"),
    }
}

/// The chords a clip's notes make, one per point where three or more
/// notes start together: "bar 1: C, bar 2: G, ...".
fn chords(notes: &[MidiNote], key: u8) -> Vec<(i64, String)> {
    let mut starts: Vec<i64> = notes.iter().map(|n| n.start).collect();
    starts.sort_unstable();
    starts.dedup();
    starts
        .into_iter()
        .filter_map(|at| {
            let held: Vec<u8> = notes.iter().filter(|n| n.start == at).map(|n| n.pitch).collect();
            if held.len() < 3 {
                return None;
            }
            let name = chord::recognize(&held).map(|c| c.name).unwrap_or_else(|| held.iter().map(|&p| pitch(p, key)).collect::<Vec<_>>().join("+"));
            Some((at, name))
        })
        .collect()
}

pub fn describe(s: &Scene) -> String {
    let arr = s.arrangement;
    let bpm = arr.tempo_map.bpm_at(0);
    let mut out = format!("Tempo {bpm:.0} BPM, 4/4. Key: {} {}.\n", shared::theory::note_name(s.key), s.scale.to_lowercase());
    let end = arr.clips.iter().map(|c| c.end()).max().unwrap_or(0);
    out += &format!("The song is {} bars long.\n", (end + BAR - 1) / BAR);
    if arr.tracks.is_empty() {
        out += "There are no tracks yet.\n";
    }
    let focus = s.open_clip.or_else(|| s.selected_track.and_then(|t| arr.clips.iter().find(|c| c.track == t).map(|c| c.id)));
    for track in &arr.tracks {
        let what = match (track.kind, track.instrument) {
            (TrackKind::Audio, _) => "audio".to_string(),
            (_, Some(Instrument::Drums)) => format!("drum kit ({})", track.drum_pads.kit.name()),
            (_, Some(Instrument::Carve)) => "synth".to_string(),
            _ => "MIDI".to_string(),
        };
        let selected = if Some(track.id) == s.selected_track { ", selected" } else { "" };
        out += &format!("\nTrack \"{}\" ({what}{selected}){}:\n", track.name, if track.mute { ", muted" } else { "" });
        let mut clips: Vec<_> = arr.clips.iter().filter(|c| c.track == track.id).collect();
        clips.sort_by_key(|c| c.start);
        if clips.is_empty() {
            out += "  no clips\n";
        }
        for clip in clips {
            let bars = format!("bars {}-{}", clip.start / BAR + 1, (clip.end() + BAR - 1) / BAR);
            let ClipContent::Midi { notes, loop_len, .. } = &clip.content else {
                out += &format!("  \"{}\": audio, {bars}\n", clip.name);
                continue;
            };
            let pattern = loop_len.map(|l| format!(", a {}-bar pattern repeating", (l + BAR - 1) / BAR)).unwrap_or_default();
            let open = if Some(clip.id) == s.open_clip { ", open in the editor" } else { "" };
            out += &format!("  \"{}\": {bars}{pattern}{open}, {} notes\n", clip.name, notes.len());
            let found = chords(notes, s.key);
            if !found.is_empty() {
                let list: Vec<String> = found.iter().map(|(at, name)| format!("{} {name}", place(*at))).collect();
                out += &format!("    chords (from the clip's start): {}\n", list.join(", "));
            }
            if track.instrument == Some(Instrument::Drums) {
                let pads = track.drum_pads.kit.pads();
                for pad in pads {
                    let hits: Vec<String> = notes.iter().filter(|n| n.pitch == pad.note && n.start < BAR).map(|n| place(n.start)).collect();
                    if !hits.is_empty() {
                        out += &format!("    {} in bar 1: {}\n", pad.name, hits.join(" "));
                    }
                }
            } else if Some(clip.id) == focus && found.len() * 3 < notes.len() {
                // The one being worked on, in full: a melody line.
                let mut sorted = notes.clone();
                sorted.sort_by_key(|n| (n.start, n.pitch));
                let list: Vec<String> =
                    sorted.iter().take(MAX_NOTES).map(|n| format!("{} {} ({})", place(n.start), pitch(n.pitch, s.key), length(n.length))).collect();
                out += &format!("    notes (from the clip's start; bar.beat, & = halfway to the next beat): {}\n", list.join(", "));
                if sorted.len() > MAX_NOTES {
                    out += &format!("    ... and {} more\n", sorted.len() - MAX_NOTES);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::arrangement::{empty_arrangement, ClipColor};

    #[test]
    fn places_and_lengths_read_like_a_musician_counts() {
        assert_eq!(place(0), "1.1");
        assert_eq!(place(PPQ / 2), "1.1&");
        assert_eq!(place(BAR + PPQ + PPQ / 4), "2.2+1/16");
        assert_eq!(length(PPQ), "1 beat");
        assert_eq!(length(PPQ * 3 / 2), "1 1/2 beats");
        assert_eq!(length(PPQ / 2), "1/2 beat");
    }

    #[test]
    fn the_lesson_project_reads_as_chords_and_a_melody() {
        let mut p = shared::lessons::starting_project(shared::lessons::MELODY_OWN);
        let melody = p.arrangement.tracks.iter().find(|t| t.name == "Melody").unwrap().id;
        let clip = p.arrangement.clips.iter_mut().find(|c| c.track == melody).unwrap();
        if let ClipContent::Midi { notes, .. } = &mut clip.content {
            notes.push(MidiNote { start: 0, length: PPQ, pitch: 64, velocity: 100 });
            notes.push(MidiNote { start: PPQ + PPQ / 2, length: PPQ / 2, pitch: 67, velocity: 100 });
        }
        let id = clip.id;
        let text = describe(&Scene { arrangement: &p.arrangement, key: 0, scale: "Major", open_clip: Some(id), selected_track: Some(melody) });
        assert!(text.contains("Key: C major"), "{text}");
        assert!(text.contains("1.1 C, 2.1 G, 3.1 Am, 4.1 F"), "{text}");
        assert!(text.contains("1.1 E4 (1 beat), 1.2& G4 (1/2 beat)"), "{text}");
        assert!(text.contains("open in the editor"));
    }

    #[test]
    fn an_empty_project_says_so() {
        let arr = empty_arrangement();
        let text = describe(&Scene { arrangement: &arr, key: 9, scale: "Minor pentatonic", open_clip: None, selected_track: None });
        assert!(text.contains("no tracks yet"));
        let _ = ClipColor::Coral;
    }
}
