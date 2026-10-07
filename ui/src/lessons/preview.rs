//! "Hear it": short renders a lesson can play without touching the
//! project - where the lesson is going (the goal), and the step just
//! done, before and after. Rendered offline with the export renderer and
//! mixed into the output by the engine's preview player.

use std::collections::BTreeMap;

use shared::arrangement::{Arrangement, Ticks, TrackId};
use shared::lessons::{
    BAR, BASSLINE, CHORDS, FIRST_BEAT, PROJECT_ARRANGE, PROJECT_BASS, PROJECT_CHORDS, PROJECT_FINISH, PROJECT_GROOVE,
    ARRANGE_BHAIRAV, ARRANGE_HOUSE, RECIPE_BASS, RECIPE_WOBBLE, RECIPE_FLUTE, RECIPE_HARP, RECIPE_LEAD, RECIPE_PAD, RECIPE_REED, RECIPE_TANPURA,
};
use shared::project::Project;
use shared::synth::SynthState;

use super::Snapshot;

/// How much a take plays by default.
const TAKE_BARS: i64 = 2;
/// Let the last notes ring out.
pub const TAIL_SECONDS: f64 = 1.0;

/// A stretch of a song to play: the arrangement, its patches and the
/// range.
#[derive(Clone)]
pub struct Take {
    pub arrangement: Arrangement,
    pub patches: BTreeMap<TrackId, SynthState>,
    pub from: Ticks,
    pub to: Ticks,
}

/// Which preview is playing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    Goal,
    Before,
    After,
    /// Sound match: your patch now.
    Yours,
    /// A quiz step's question.
    Quiz,
    /// The current step, done (its "Show me").
    Example,
}

/// A quiz question's notes - (16th, pitch, 16ths) - played by the theory
/// lessons' Keys sound, on their own.
pub fn quiz_take(notes: &[(i64, u8, i64)]) -> Take {
    let mut project = shared::lessons::starting_project(shared::lessons::THEORY_TRIADS);
    project.migrate();
    let sixteenth = shared::lessons::SIXTEENTH;
    let end = notes.iter().map(|&(at, _, len)| at + len).max().unwrap_or(4);
    let clip = project.arrangement.clips.first_mut().expect("the Keys clip");
    clip.length = end * sixteenth;
    clip.content = shared::arrangement::ClipContent::Midi {
        notes: notes
            .iter()
            .map(|&(at, pitch, len)| shared::arrangement::MidiNote { start: at * sixteenth, length: len * sixteenth, pitch, velocity: 100 })
            .collect(),
        loop_len: None,
        link: None,
    };
    Take { arrangement: project.arrangement, patches: project.instruments.into_iter().collect(), from: 0, to: end * sixteenth }
}

/// The app as it stands in `snap`: the on-screen patch is the selected
/// track's (it may not be stored back yet), played around the part being
/// worked on - the clip open in the editor, else the selected track's
/// first clip.
pub fn take_of(snap: &Snapshot, patches: &BTreeMap<TrackId, SynthState>) -> Take {
    let mut patches = patches.clone();
    if let Some(track) = super::selected(snap).filter(|t| patches.contains_key(&t.id) || t.instrument == Some(shared::arrangement::Instrument::Carve)) {
        patches.insert(track.id, snap.synth.clone());
    }
    let arr = &snap.arrangement;
    let from = snap
        .open_clip
        .and_then(|id| arr.clip(id))
        .map(|c| c.start)
        .or_else(|| snap.selected_track.and_then(|t| arr.clips.iter().filter(|c| c.track == t).map(|c| c.start).min()))
        .unwrap_or(0);
    Take { arrangement: snap.arrangement.clone(), patches, from, to: from + TAKE_BARS * BAR }
}

fn project_take(mut project: Project, from_bar: i64, bars: i64) -> Take {
    project.migrate();
    Take {
        arrangement: project.arrangement,
        patches: project.instruments.into_iter().collect(),
        from: from_bar * BAR,
        to: (from_bar + bars) * BAR,
    }
}

/// The app as a lesson starts it: its starting project, the last track
/// selected with its patch on screen (what `LessonEvent::Begin` does).
pub fn starting_snapshot(lesson: &str) -> (Snapshot, BTreeMap<TrackId, SynthState>) {
    let mut project = shared::lessons::starting_project(lesson);
    project.migrate();
    let patches: BTreeMap<_, _> = project.instruments.iter().cloned().collect();
    let last = project.arrangement.tracks.last().map(|t| t.id);
    let synth = last.and_then(|t| patches.get(&t).cloned()).unwrap_or_else(shared::synth::seed_synth);
    let (key, scale_mask) = lesson_key_mask(lesson);
    let snap = Snapshot {
        arrangement: project.arrangement,
        selected_track: last,
        playing: false,
        synth,
        open_clip: None,
        playhead: 0,
        match_score: 0.0,
        key,
        scale_mask,
        exported: false,
        analyzer_open: false,
        snap: shared::arrangement::SnapGrid::Sixteenth,
    };
    (snap, patches)
}

/// The key and scale a lesson starts in (as `ProjectEvent::StartLesson`
/// sets them): its own, or the app's default, A minor pentatonic.
pub fn lesson_key_mask(lesson: &str) -> (u8, u16) {
    let preset = |name: &str| shared::theory::SCALE_PRESETS.iter().find(|p| p.name == name).map(|p| p.mask);
    shared::lessons::lesson_key(lesson)
        .and_then(|(root, scale)| Some((root, preset(scale)?)))
        .unwrap_or((9, preset("Minor pentatonic").unwrap_or(1)))
}

/// Where `lesson` ends up, to hear before starting it - `None` for the
/// arrangement walk-throughs (they play the finished songs anyway).
pub fn goal(lesson: &str, snap: &Snapshot, patches: &BTreeMap<TrackId, SynthState>) -> Option<Take> {
    let preset = match lesson {
        RECIPE_BASS => Some("Deep Bass"),
        RECIPE_WOBBLE => Some("Wobble Bass"),
        RECIPE_PAD => Some("Soft Pad"),
        RECIPE_FLUTE => Some("Flute"),
        RECIPE_HARP => Some("Indian Harp"),
        RECIPE_TANPURA => Some("Tanpura"),
        RECIPE_REED => Some("Reed"),
        RECIPE_LEAD => Some("Lead"),
        _ => None,
    };
    if let Some(preset) = preset {
        // The lesson's riff, played by the finished sound.
        let build = shared::synth::PRESETS.iter().find(|p| p.0 == preset)?.1;
        let mut done = snap.clone();
        done.synth = build();
        return Some(take_of(&done, patches));
    }
    use shared::lessons::{project_after, starting_project};
    Some(match lesson {
        FIRST_BEAT => project_take(starting_project(BASSLINE), 0, 2),
        BASSLINE => project_take(starting_project(CHORDS), 0, 2),
        PROJECT_GROOVE => project_take(project_after(1), 0, 2),
        PROJECT_BASS => project_take(project_after(2), 4, 2),
        PROJECT_CHORDS => project_take(project_after(3), 8, 2),
        // The breakdown dropping back into the groove.
        PROJECT_ARRANGE => project_take(project_after(4), 22, 4),
        PROJECT_FINISH => project_take(project_after(5), 22, 4),
        ARRANGE_HOUSE | ARRANGE_BHAIRAV => return None,
        // Any other lesson: every step done by "Show me", from its start.
        _ => {
            let (mut done, patches) = starting_snapshot(lesson);
            for show in super::show::steps(lesson) {
                if !super::show::run(&*show, &mut done) {
                    return None;
                }
            }
            take_of(&done, &patches)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lessons::course::LESSONS;

    fn start(id: &str) -> (Snapshot, BTreeMap<TrackId, SynthState>) {
        starting_snapshot(id)
    }

    #[test]
    fn every_goal_has_something_to_hear() {
        for lesson in LESSONS {
            let (snap, patches) = start(lesson.id);
            let Some(take) = goal(lesson.id, &snap, &patches) else {
                assert!(lesson.group == crate::lessons::course::ARRANGEMENT, "{} has no goal", lesson.id);
                continue;
            };
            assert!(take.to > take.from, "{}", lesson.id);
            assert!(
                take.arrangement.clips.iter().any(|c| c.start < take.to && c.start + c.length > take.from),
                "{}: nothing plays in the goal's range",
                lesson.id
            );
        }
    }

    #[test]
    fn a_recipe_goal_is_the_finished_preset_on_the_lesson_track() {
        let (snap, patches) = start(RECIPE_FLUTE);
        let take = goal(RECIPE_FLUTE, &snap, &patches).unwrap();
        assert_eq!(take.patches[&snap.selected_track.unwrap()].name, "Flute");
        // Starting the lesson hasn't changed: the goal is a copy.
        assert_ne!(snap.synth.name, "Flute");
    }

    #[test]
    fn a_quiz_question_renders_to_sound() {
        let take = quiz_take(&[(0, 60, 4), (4, 67, 6)]);
        let job = engine::render::RenderJob { arrangement: take.arrangement, patches: take.patches, sources: vec![], sample_rate: 48_000 };
        let audio = engine::render::render_between(&job, take.from, take.to, TAIL_SECONDS);
        let peak = audio.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak > 0.05, "the question is silent (peak {peak})");
    }

    #[test]
    fn a_goal_renders_to_sound() {
        let (snap, patches) = start(RECIPE_FLUTE);
        let take = goal(RECIPE_FLUTE, &snap, &patches).unwrap();
        let job = engine::render::RenderJob { arrangement: take.arrangement, patches: take.patches, sources: vec![], sample_rate: 48_000 };
        let audio = engine::render::render_between(&job, take.from, take.to, TAIL_SECONDS);
        let peak = audio.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak > 0.05, "the flute goal is silent (peak {peak})");
    }
}