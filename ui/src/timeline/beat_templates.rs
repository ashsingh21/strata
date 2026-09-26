//! Built-in drum patterns, offered from the sidebar next to the plain
//! one-shot samples: one click lays a finished multi-bar groove across
//! fresh tracks, the same way a single sample becomes one track+clip via
//! `TimelineEvent::AddDrumSample` - just several hits, on several tracks,
//! in one undo step instead of one.

use shared::arrangement::ClipColor;

/// One drum hit's position within a single bar of 4/4 - repeated for
/// every bar the template spans.
pub struct DrumHit {
    pub sample: &'static str,
    pub track_name: &'static str,
    pub color: ClipColor,
    /// 0-based beat within the bar (0..4 in 4/4).
    pub beat: i64,
}

pub struct BeatTemplate {
    pub name: &'static str,
    pub bars: i64,
    pub hits: &'static [DrumHit],
}

pub const TEMPLATES: &[BeatTemplate] = &[BeatTemplate {
    name: "Basic backbeat",
    bars: 2,
    hits: &[
        DrumHit { sample: "drums/kick.wav", track_name: "Kick", color: ClipColor::Amber, beat: 0 },
        DrumHit { sample: "drums/snare.wav", track_name: "Snare", color: ClipColor::Teal, beat: 1 },
        DrumHit { sample: "drums/kick.wav", track_name: "Kick", color: ClipColor::Amber, beat: 2 },
        DrumHit { sample: "drums/snare.wav", track_name: "Snare", color: ClipColor::Teal, beat: 3 },
    ],
}];
