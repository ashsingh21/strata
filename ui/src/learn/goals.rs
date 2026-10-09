//! Learn by goal: seven goals, each a path of lessons and drills in the
//! order to take them. A lesson is one of `lessons::course::LESSONS` (it
//! may sit in more than one goal: done once, done in both); a drill is a
//! session on one of the practice tools, done when most of it is right.
//! Progress is the lessons' "done" list, with drills in it as `drill:` ids.

use shared::ear::Step;
use shared::lessons::*;

use crate::lessons::course::LESSONS;

/// A practice tool, set up for one drill.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drill {
    Ear(Step),
    /// Rhythm exercises at this level (0-based).
    Rhythm(usize),
    /// Melody exercises at this level.
    Melody(usize),
    /// A famous tune, phrase by phrase.
    Classics,
    /// Singing against the tanpura (a session is time, not answers).
    Riyaz,
    /// Tools to explore rather than drill: done once opened.
    TheoryRing,
    VoiceLeading,
}

impl Drill {
    /// Answers in one session (0: it isn't counted in answers).
    pub fn session(self) -> u32 {
        match self {
            Drill::Ear(_) | Drill::Rhythm(_) => 10,
            Drill::Melody(_) | Drill::Classics => 6,
            Drill::Riyaz | Drill::TheoryRing | Drill::VoiceLeading => 0,
        }
    }

    /// Right answers out of a session that count it as done.
    pub fn pass(self) -> u32 {
        (self.session() * 7).div_ceil(10)
    }

    pub fn kind(self) -> &'static str {
        match self {
            Drill::TheoryRing | Drill::VoiceLeading => "Tool",
            Drill::Classics => "Song",
            _ => "Drill",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Lesson(&'static str),
    Drill { id: &'static str, title: &'static str, what: &'static str, drill: Drill },
}

impl Item {
    /// The id it's marked done by.
    pub fn done_id(self) -> String {
        match self {
            Item::Lesson(id) => id.to_string(),
            Item::Drill { id, .. } => format!("drill:{id}"),
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Item::Lesson(id) => lesson(id).map(|i| LESSONS[i].title).unwrap_or(id),
            Item::Drill { title, .. } => title,
        }
    }

    /// A line on what it is.
    pub fn what(self) -> String {
        match self {
            Item::Lesson(id) => lesson(id)
                .map(|i| {
                    let l = &LESSONS[i];
                    format!("{} \u{b7} {} steps", l.group, l.steps.len())
                })
                .unwrap_or_default(),
            Item::Drill { what, .. } => what.to_string(),
        }
    }

    pub fn kind(self) -> &'static str {
        match self {
            Item::Lesson(_) => "Lesson",
            Item::Drill { drill, .. } => drill.kind(),
        }
    }

    pub fn is_lesson(self, lesson_id: &str) -> bool {
        matches!(self, Item::Lesson(id) if id == lesson_id)
    }

    pub fn is_done(self, done: &[String]) -> bool {
        let id = self.done_id();
        done.iter().any(|d| *d == id)
    }
}

/// The lesson's index in the course.
pub fn lesson(id: &str) -> Option<usize> {
    LESSONS.iter().position(|l| l.id == id)
}

pub struct Goal {
    pub id: &'static str,
    pub title: &'static str,
    /// What you'll be able to do.
    pub promise: &'static str,
    /// Its swatch: one of the clip colours (or neutral).
    pub color: Option<shared::arrangement::ClipColor>,
    pub items: &'static [Item],
}

impl Goal {
    pub fn done_count(&self, done: &[String]) -> usize {
        self.items.iter().filter(|i| i.is_done(done)).count()
    }

    /// The first item not done yet.
    pub fn next(&self, done: &[String]) -> Option<usize> {
        self.items.iter().position(|i| !i.is_done(done))
    }

    /// "8 lessons · 3 drills".
    pub fn holds(&self) -> String {
        let lessons = self.items.iter().filter(|i| matches!(i, Item::Lesson(_))).count();
        let drills = self.items.len() - lessons;
        let plural = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        match drills {
            0 => plural(lessons, "lesson", "lessons"),
            _ => format!("{} \u{b7} {}", plural(lessons, "lesson", "lessons"), plural(drills, "drill", "drills")),
        }
    }
}

const fn d(id: &'static str, title: &'static str, what: &'static str, drill: Drill) -> Item {
    Item::Drill { id, title, what, drill }
}
const fn l(id: &'static str) -> Item {
    Item::Lesson(id)
}

pub const EAR_DIRECTION: Item = d("ear-direction", "Higher or lower", "Two notes: which way did it go?", Drill::Ear(Step::Direction));
pub const EAR_DISTANCE: Item = d("ear-distance", "Step or jump", "Is the 2nd note a fret or two away, or further?", Drill::Ear(Step::Distance));
pub const EAR_STRING: Item = d("ear-string", "One string", "The 1st note is lit on the A string: find the 2nd.", Drill::Ear(Step::OneString));
pub const EAR_TUNES: Item = d("ear-tunes", "Short tunes", "Four notes from the lit A, in the box at the 5th fret.", Drill::Ear(Step::Tunes));
pub const EAR_KNOWN: Item = d("ear-known", "Tunes you know", "Happy Birthday, Ode to Joy... from memory, then hear it.", Drill::Ear(Step::Known));
pub const PLAY_BACK: Item = d("melody-1", "Play it back", "Hear a one-bar melody, then play it on time.", Drill::Melody(0));
pub const CLASSICS: Item = d("classics", "Classics, phrase by phrase", "Learn a whole tune: Ode to Joy, Für Elise and more.", Drill::Classics);
pub const TAP_IT: Item = d("rhythm-1", "Tap it back", "Hear a one-bar rhythm, then tap it on any key or pad.", Drill::Rhythm(0));
pub const TAP_IT_HARDER: Item = d("rhythm-3", "Tap it back: offbeats", "Rhythms with 8ths and offbeats, on time.", Drill::Rhythm(2));
pub const RIYAZ: Item = d("riyaz", "Riyaz", "Sing against the tanpura and see how steady each swar is.", Drill::Riyaz);
pub const THEORY_RING: Item = d("theory-ring", "The theory ring", "See any scale and chord on a ring, and play it.", Drill::TheoryRing);
pub const VOICE_LEADING: Item = d("voice-leading", "Voice leading", "Chords that move smoothly from one to the next.", Drill::VoiceLeading);

pub const GOALS: &[Goal] = &[
    Goal {
        id: "start",
        title: "Start here: your first track",
        promise: "A beat, a bassline and chords, then a whole house track in five parts.",
        color: Some(shared::arrangement::ClipColor::Coral),
        items: &[l(FIRST_BEAT), l(BASSLINE), l(CHORDS), l(PROJECT_GROOVE), l(PROJECT_BASS), l(PROJECT_CHORDS), l(PROJECT_ARRANGE), l(PROJECT_FINISH)],
    },
    Goal {
        id: "beats",
        title: "Beats and rhythm",
        promise: "Drums that groove: accents, note length, swing and rolls, and your own timing.",
        color: Some(shared::arrangement::ClipColor::Amber),
        items: &[l(ROLL_DYNAMICS), l(ROLL_LENGTH), TAP_IT, l(ROLL_PAINT), l(ROLL_SWING), TAP_IT_HARDER, l(ROLL_ROLLS), l(TRAP_DRUMS)],
    },
    Goal {
        id: "sound",
        title: "Design sounds",
        promise: "How a synth works, then famous sounds, then match a sound by ear.",
        color: Some(shared::arrangement::ClipColor::Violet),
        items: &[
            l(SOUND_LOUDNESS), l(SOUND_PITCH), l(SOUND_HARMONICS), l(CARVE_WAVES), l(MATCH_WAVE), l(CARVE_MIX), l(CARVE_FILTER),
            l(MATCH_CUTOFF), l(MATCH_RESONANCE), l(CARVE_ENVELOPES), l(MATCH_PLUCK), l(MATCH_SWELL), l(CARVE_MOVEMENT), l(MATCH_SUB),
            l(CARVE_SYNC_FM), l(RECIPE_BASS), l(RECIPE_WOBBLE), l(RECIPE_PAD), l(RECIPE_LEAD), l(RECIPE_KEYS), l(RECIPE_FLUTE),
            l(MATCH_MYSTERY),
        ],
    },
    Goal {
        id: "theory",
        title: "Theory and melody",
        promise: "Scales, keys, chords and writing melodies, learned by hearing them.",
        color: Some(shared::arrangement::ClipColor::Blue),
        items: &[
            l(THEORY_OCTAVES), l(THEORY_SCALES), l(THEORY_KEYS), l(THEORY_MAJOR_MINOR), THEORY_RING, l(THEORY_TRIADS),
            l(THEORY_PROGRESSIONS), VOICE_LEADING, l(THEORY_MELODY), l(MELODY_STEPS), l(MELODY_CALL), l(MELODY_MOTIF), l(MELODY_RHYTHM), l(MELODY_SHAPE),
            l(MELODY_CHORD_TONES), l(MELODY_HOOK), l(MELODY_OWN),
            l(THEORY_SEVENTHS),
        ],
    },
    Goal {
        id: "ear",
        title: "Play by ear",
        promise: "Hear a tune in your head and find it on guitar or keys.",
        color: Some(shared::arrangement::ClipColor::Teal),
        items: &[EAR_DIRECTION, l(THEORY_INTERVALS), EAR_DISTANCE, EAR_STRING, l(THEORY_SCALES), EAR_TUNES, PLAY_BACK, EAR_KNOWN, CLASSICS],
    },
    Goal {
        id: "guitar",
        title: "Guitar styles",
        promise: "Fingerpicking to take to your guitar: Chet Atkins\u{2019} bouncing thumb and John Fahey\u{2019}s drones.",
        color: Some(shared::arrangement::ClipColor::Amber),
        items: &[l(GUITAR_CHET), l(GUITAR_FAHEY), VOICE_LEADING],
    },
    Goal {
        id: "indian",
        title: "Indian classical",
        promise: "Raags, riyaz against the tanpura, and Bollywood lo-fi.",
        color: Some(shared::arrangement::ClipColor::Pink),
        items: &[
            l(THEORY_RAAG), l(RECIPE_TANPURA), RIYAZ, l(THEORY_BHAIRAV), l(RECIPE_REED), l(RECIPE_HARP), l(BOLLY_MELODY), l(BOLLY_DRONE),
            l(ARRANGE_BHAIRAV),
        ],
    },
    Goal {
        id: "finish",
        title: "Finish a song",
        promise: "Arrange, mix and finish: house, lo-fi and trap, start to end.",
        color: None,
        items: &[
            l(ARRANGE_HOUSE), l(MIX_LEVELS), l(MIX_EQ), l(MIX_COMPRESS), l(MIX_FINISH), l(LOFI_BEAT), l(LOFI_KEYS), l(LOFI_BASS),
            l(LOFI_FINISH), l(TRAP_808), l(TRAP_MELODY), l(TRAP_ARRANGE),
        ],
    },
];

/// Where to carry on: the current goal's next item, else the first goal
/// with one left.
pub fn continue_at(goal: usize, done: &[String]) -> Option<(usize, usize)> {
    let goal = goal.min(GOALS.len() - 1);
    GOALS[goal].next(done).map(|i| (goal, i)).or_else(|| GOALS.iter().enumerate().find_map(|(g, x)| x.next(done).map(|i| (g, i))))
}

/// The goals a lesson is in.
pub fn goals_with(lesson_id: &str) -> impl Iterator<Item = usize> + '_ {
    (0..GOALS.len()).filter(move |&g| GOALS[g].items.iter().any(|i| i.is_lesson(lesson_id)))
}

/// Today's practice, about five minutes: an ear drill, a rhythm drill and
/// a melody drill - the first not done in each line, else the last of it
/// (practice goes on once it's done). As (goal, item).
pub fn today(done: &[String]) -> [(usize, usize); 3] {
    let ear = goal_index("ear");
    let beats = goal_index("beats");
    let pick = |goal: usize, line: &[Item]| -> (usize, usize) {
        let items = GOALS[goal].items;
        let chosen = line.iter().copied().find(|i| !i.is_done(done)).unwrap_or(*line.last().unwrap());
        (goal, items.iter().position(|i| *i == chosen).unwrap_or(0))
    };
    [
        pick(ear, &[EAR_DIRECTION, EAR_DISTANCE, EAR_STRING, EAR_TUNES, EAR_KNOWN]),
        pick(beats, &[TAP_IT, TAP_IT_HARDER]),
        pick(ear, &[PLAY_BACK, CLASSICS]),
    ]
}

pub fn goal_index(id: &str) -> usize {
    GOALS.iter().position(|g| g.id == id).unwrap_or(0)
}

/// Days in a row ending today (or yesterday, if today's isn't done yet),
/// from the days today's practice was finished.
pub fn streak(days: &[u64], today: u64) -> usize {
    let mut day = if days.contains(&today) { today } else { today.saturating_sub(1) };
    let mut n = 0;
    while days.contains(&day) {
        n += 1;
        if day == 0 {
            break;
        }
        day -= 1;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_lesson_is_in_a_goal() {
        for l in LESSONS {
            assert!(goals_with(l.id).next().is_some(), "{} ({}) is in no goal", l.title, l.id);
        }
        for g in GOALS {
            for i in g.items {
                if let Item::Lesson(id) = i {
                    assert!(lesson(id).is_some(), "{}: no lesson {id}", g.id);
                }
            }
        }
    }

    #[test]
    fn drill_ids_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for g in GOALS {
            for i in g.items {
                if let Item::Drill { id, .. } = i {
                    // The same drill may sit in two goals, but one id means one drill.
                    let first = seen.insert(*id);
                    let _ = first;
                }
            }
        }
        assert!(seen.len() >= 10);
    }

    #[test]
    fn progress_and_where_to_carry_on() {
        let ear = goal_index("ear");
        let mut done: Vec<String> = Vec::new();
        assert_eq!(continue_at(ear, &done), Some((ear, 0)));
        done.push(EAR_DIRECTION.done_id());
        assert_eq!(GOALS[ear].done_count(&done), 1);
        assert_eq!(continue_at(ear, &done), Some((ear, 1)));
        // A lesson done elsewhere counts here too.
        done.push(THEORY_INTERVALS.to_string());
        assert_eq!(continue_at(ear, &done), Some((ear, 2)));
        assert_eq!(GOALS[goal_index("start")].holds(), "8 lessons");
        assert_eq!(GOALS[ear].holds(), "2 lessons \u{b7} 7 drills");
    }

    #[test]
    fn todays_practice_moves_on_and_then_repeats() {
        let ear = goal_index("ear");
        let t = today(&[]);
        assert_eq!(GOALS[t[0].0].items[t[0].1], EAR_DIRECTION);
        assert_eq!(GOALS[t[1].0].items[t[1].1], TAP_IT);
        assert_eq!(GOALS[t[2].0].items[t[2].1], PLAY_BACK);
        let all: Vec<String> = GOALS[ear].items.iter().map(|i| i.done_id()).chain([TAP_IT.done_id(), TAP_IT_HARDER.done_id()]).collect();
        let t = today(&all);
        assert_eq!(GOALS[t[0].0].items[t[0].1], EAR_KNOWN);
        assert_eq!(GOALS[t[2].0].items[t[2].1], CLASSICS);
    }

    #[test]
    fn a_streak_counts_days_in_a_row() {
        assert_eq!(streak(&[], 100), 0);
        assert_eq!(streak(&[98, 99, 100], 100), 3);
        assert_eq!(streak(&[98, 99], 100), 2, "today not done yet: still counting");
        assert_eq!(streak(&[97, 99], 100), 1);
        assert_eq!(streak(&[95], 100), 0);
        assert_eq!(Drill::Ear(Step::Direction).pass(), 7);
        assert_eq!(Drill::Melody(0).pass(), 5);
    }
}
