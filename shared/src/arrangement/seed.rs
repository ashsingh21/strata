//! The demo arrangement used until a project can be loaded/saved: Drums
//! (audio, 3 clips), Bass (MIDI, 2), Lead (audio, 2 clips + a Filter
//! Cutoff automation lane), Pad (MIDI, 1 long clip); 16 bars at 128 BPM,
//! loop bars 5-13, a "Drop" marker at bar 9.

use super::model::{
    Arrangement, AutomationLane, Breakpoint, Clip, ClipColor, ClipContent, LoopRange, Marker,
    Instrument, MidiNote, Track, TrackKind, DEFAULT_VELOCITY, DEFAULT_TRACK_HEIGHT,
};
use super::time::{TempoMap, TimeSignature, PPQ};

const BAR: i64 = PPQ * 4;
const BEAT: i64 = PPQ;

/// A blank starting point: no tracks, clips or automation, same default
/// tempo/time signature as `seed_arrangement`. This is what the app
/// actually starts from - build a project up with "+ Audio"/"+ MIDI"
/// rather than always opening onto a canned demo song.
pub fn empty_arrangement() -> Arrangement {
    Arrangement::new(TempoMap::constant(crate::DEFAULT_BPM, TimeSignature::FOUR_FOUR))
}

pub fn seed_arrangement() -> Arrangement {
    let mut arr = Arrangement::new(TempoMap::constant(crate::DEFAULT_BPM, TimeSignature::FOUR_FOUR));

    let drums = arr.alloc_id();
    let bass = arr.alloc_id();
    let lead = arr.alloc_id();
    let pad = arr.alloc_id();

    arr.tracks.push(Track {
        id: drums,
        name: "Drums".into(),
        color: ClipColor::Coral,
        kind: TrackKind::Audio,
        mute: false,
        solo: false,
        arm: false,
        gain_db: -3.0,
        height: DEFAULT_TRACK_HEIGHT,
        instrument: None,
        effects: vec![],
        effect_slots: vec![],
        fx: crate::arrangement::EffectGraph::new(),
    });
    arr.tracks.push(Track {
        id: bass,
        name: "Bass".into(),
        color: ClipColor::Amber,
        kind: TrackKind::Midi,
        mute: false,
        solo: true,
        arm: false,
        gain_db: -6.0,
        height: DEFAULT_TRACK_HEIGHT,
        instrument: Some(Instrument::Carve),
        effects: vec![],
        effect_slots: vec![],
        fx: crate::arrangement::EffectGraph::new(),
    });
    arr.tracks.push(Track {
        id: lead,
        name: "Lead".into(),
        color: ClipColor::Teal,
        kind: TrackKind::Audio,
        mute: false,
        solo: false,
        arm: true,
        gain_db: -1.5,
        height: DEFAULT_TRACK_HEIGHT,
        instrument: None,
        effects: vec![],
        effect_slots: vec![],
        fx: crate::arrangement::EffectGraph::new(),
    });
    arr.tracks.push(Track {
        id: pad,
        name: "Pad".into(),
        color: ClipColor::Violet,
        kind: TrackKind::Midi,
        mute: false,
        solo: false,
        arm: false,
        gain_db: -9.0,
        height: DEFAULT_TRACK_HEIGHT,
        instrument: Some(Instrument::Carve),
        effects: vec![],
        effect_slots: vec![],
        fx: crate::arrangement::EffectGraph::new(),
    });

    // -- Drums: one continuous take, sliced into three clips. ---------
    let drums_source: std::sync::Arc<str> = "drums.wav".into();
    let sr = 48_000u32;
    for (name, start, length) in
        [("Beat A", 0, BAR * 4), ("Beat B", BAR * 4, BAR * 8), ("Fill", BAR * 12, BAR * 4)]
    {
        let source_offset_samples = arr.tempo_map.ticks_to_samples(start, sr) as u64;
        let id = arr.alloc_id();
        arr.clips.push(Clip {
            id,
            track: drums,
            start,
            length,
            name: name.into(),
            content: ClipContent::Audio {
                source: drums_source.clone(),
                peaks: None,
                source_offset_samples,
            },
            recording: false,
            gain_db: 0.0,
        });
    }

    // -- Bass: two MIDI clips, a simple repeating bass figure. ---------
    for (name, start) in [("Sub 1", 0i64), ("Sub 2", BAR * 8)] {
        let id = arr.alloc_id();
        arr.clips.push(Clip {
            id,
            track: bass,
            start,
            length: BAR * 8,
            name: name.into(),
            content: ClipContent::Midi { notes: bass_pattern() },
            recording: false,
            gain_db: 0.0,
        });
    }

    // -- Lead: an audio "Hook" clip plus a short recording take. -------
    let hook_id = arr.alloc_id();
    arr.clips.push(Clip {
        id: hook_id,
        track: lead,
        start: BAR * 4,
        length: BAR * 8,
        name: "Hook".into(),
        content: ClipContent::Audio {
            source: "lead_hook.wav".into(),
            peaks: None,
            source_offset_samples: 0,
        },
        recording: false,
        gain_db: 0.0,
    });
    let take_id = arr.alloc_id();
    arr.clips.push(Clip {
        id: take_id,
        track: lead,
        start: BAR * 12,
        length: BEAT * 6,
        name: "Take 3".into(),
        content: ClipContent::Audio {
            source: "lead_take3.wav".into(),
            peaks: None,
            source_offset_samples: 0,
        },
        // Not `recording: true` - that makes a clip's rendered length grow
        // to chase the live playhead (see lanes.rs), which is correct for
        // a clip actively being recorded right now but there's no engine
        // support for that yet, so it would just grow forever once
        // playback passed it. This is a finished (fixed-length) take.
        recording: false,
        gain_db: 0.0,
    });

    let cutoff_lane = arr.alloc_id();
    arr.automation.push(AutomationLane {
        id: cutoff_lane,
        track: lead,
        parameter_name: "Filter \u{b7} Cutoff".into(),
        display_value: "2.4 kHz".into(),
        breakpoints: vec![
            Breakpoint { tick: 0, value: 0.25 },
            Breakpoint { tick: BAR * 4, value: 0.25 },
            Breakpoint { tick: BAR * 8, value: 0.8 },
            Breakpoint { tick: BAR * 11, value: 0.8 },
            Breakpoint { tick: BAR * 12, value: 0.35 },
            Breakpoint { tick: BAR * 16, value: 0.35 },
        ],
        target: Some(crate::arrangement::AutomationTarget::Synth(crate::synth::SynthParam::Cutoff)),
    });

    // -- Pad: one long MIDI clip spanning the whole arrangement. -------
    let wash_id = arr.alloc_id();
    arr.clips.push(Clip {
        id: wash_id,
        track: pad,
        start: 0,
        length: BAR * 16,
        name: "Wash".into(),
        content: ClipContent::Midi { notes: pad_pattern() },
        recording: false,
        gain_db: 0.0,
    });

    arr.loop_range = Some(LoopRange { start: BAR * 4, end: BAR * 12 });
    let marker_id = arr.alloc_id();
    arr.markers.push(Marker { id: marker_id, position: BAR * 8, name: "Drop".into() });

    arr
}

fn bass_pattern() -> Vec<MidiNote> {
    let pitches = [36u8, 36, 39, 41, 36, 38, 41, 43];
    (0..16)
        .map(|i| MidiNote {
            start: BEAT / 2 * i as i64,
            length: BEAT / 2 - PPQ / 16,
            pitch: pitches[i % pitches.len()],
            // Accent the downbeats so the velocity lane has something to show.
            velocity: if i % 4 == 0 { 118 } else { 88 },
        })
        .collect()
}

fn pad_pattern() -> Vec<MidiNote> {
    let chord = [48u8, 52, 55];
    (0..8)
        .flat_map(|bar| chord.iter().map(move |&pitch| MidiNote { start: BAR * bar, length: BAR, pitch, velocity: DEFAULT_VELOCITY }))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_spec_shape() {
        let arr = seed_arrangement();
        assert_eq!(arr.tracks.len(), 4);
        assert_eq!(arr.clips.iter().filter(|c| c.track == arr.tracks[0].id).count(), 3);
        assert_eq!(arr.clips.iter().filter(|c| c.track == arr.tracks[1].id).count(), 2);
        assert_eq!(arr.clips.iter().filter(|c| c.track == arr.tracks[2].id).count(), 2);
        assert_eq!(arr.clips.iter().filter(|c| c.track == arr.tracks[3].id).count(), 1);
        assert_eq!(arr.automation.len(), 1);
        assert_eq!(arr.loop_range, Some(LoopRange { start: BAR * 4, end: BAR * 12 }));
        assert_eq!(arr.markers[0].position, BAR * 8);
        assert_eq!(arr.markers[0].name, "Drop");

        // All clip/track/lane/marker ids are unique.
        let mut ids: Vec<u32> = arr.tracks.iter().map(|t| t.id).collect();
        ids.extend(arr.clips.iter().map(|c| c.id));
        ids.extend(arr.automation.iter().map(|a| a.id));
        ids.extend(arr.markers.iter().map(|m| m.id));
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len());
    }
}
