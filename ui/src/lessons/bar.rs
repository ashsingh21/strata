//! The lesson bar: one strip above the timeline while a lesson runs -
//! lesson title and progress, the current instruction (plus a hint once
//! the step has taken a while), and Continue / Skip / Exit.

use vizia::prelude::*;

use super::course::{Kind, LESSONS};
use super::preview::Which;
use super::{LessonEvent, LessonModel};
use crate::project::ProjectEvent;
use crate::tokens;

#[derive(Clone, Copy)]
pub struct LessonBarProps {
    pub active: Signal<Option<(usize, usize)>>,
    pub hint_visible: Signal<bool>,
    pub previewing: Signal<Option<Which>>,
    pub has_goal: Signal<bool>,
    pub change_step: Signal<Option<usize>>,
    pub shown_step: Signal<Option<usize>>,
    pub match_target: Signal<Option<std::sync::Arc<shared::analysis::Analysis>>>,
    pub match_yours: Signal<Option<std::sync::Arc<shared::analysis::Analysis>>>,
    pub match_score: Signal<Option<f32>>,
    pub quiz_wrong: Signal<Option<usize>>,
    pub reviewing: Signal<Option<usize>>,
    pub theme: Signal<crate::tokens::ThemeId>,
}

impl LessonBarProps {
    pub fn of(model: &LessonModel, theme: Signal<crate::tokens::ThemeId>) -> Self {
        Self {
            active: model.active,
            hint_visible: model.hint_visible,
            previewing: model.previewing,
            has_goal: model.has_goal,
            change_step: model.change_step,
            shown_step: model.shown_step,
            match_target: model.match_target,
            match_yours: model.match_yours,
            match_score: model.match_score,
            quiz_wrong: model.quiz_wrong,
            reviewing: model.reviewing,
            theme,
        }
    }
}

/// The lesson's title and progress: a filled square per step done, the
/// current one outlined (or, rereading, the one being read).
fn title_and_dots(cx: &mut Context, lesson: usize, marked: usize, current: usize) {
    let l = &LESSONS[lesson];
    VStack::new(cx, move |cx| {
        Label::new(cx, format!("{} \u{b7} {}", l.group, l.title)).class("label");
        let dots: String = (0..l.steps.len())
            .map(|i| if i == marked { '\u{25a3}' } else if i < current { '\u{25a0}' } else { '\u{25a1}' })
            .collect();
        Label::new(cx, dots).class("value").class("lesson-dots");
    })
    .gap(Pixels(2.0))
    .width(Auto)
    .height(Auto);
}

fn nav_button<'a>(cx: &'a mut Context, glyph: &'static str, tip: &'static str) -> Handle<'a, Button> {
    Button::new(cx, move |cx| Label::new(cx, glyph))
        .class("btn")
        .class("quiet")
        .tooltip(move |cx| {
            Tooltip::new(cx, move |cx| {
                Label::new(cx, tip);
            })
            .placement(Placement::Bottom)
            .arrow(false)
        })
}

/// An earlier step, to read again: what it asked, the words it brought in
/// and why it sounds as it does - with ‹ › through the steps before the
/// current one, and back to it.
fn review_bar(cx: &mut Context, lesson: usize, viewing: usize, current: usize) {
    let l = &LESSONS[lesson];
    let s = &l.steps[viewing];
    HStack::new(cx, move |cx| {
        title_and_dots(cx, lesson, viewing, current);
        Element::new(cx).class("hairline").width(Pixels(1.0)).height(Pixels(28.0));
        VStack::new(cx, move |cx| {
            Label::new(cx, format!("Step {} of {}, done", viewing + 1, l.steps.len())).class("label");
            Label::new(cx, s.text).class("body").class("lesson-text").width(Stretch(1.0)).text_wrap(true);
            let words = super::course::new_words(l, viewing);
            if !words.is_empty() {
                let line = words.iter().map(|(term, meaning)| format!("{term}: {meaning}")).collect::<Vec<_>>().join("   \u{b7}   ");
                Label::new(cx, line).class("value").class("lesson-words").width(Stretch(1.0)).text_wrap(true);
            }
            if !s.why.is_empty() {
                Label::new(cx, format!("Why: {}", s.why)).class("value").class("lesson-why").width(Stretch(1.0)).text_wrap(true);
            }
        })
        .gap(Pixels(2.0))
        .width(Stretch(1.0))
        .height(Auto);

        if viewing > 0 {
            nav_button(cx, "\u{2039} Back", "The step before").on_press(move |cx| cx.emit(LessonEvent::Review(Some(viewing - 1))));
        }
        nav_button(cx, "Next \u{203a}", "The step after").on_press(move |cx| {
            cx.emit(LessonEvent::Review(if viewing + 1 < current { Some(viewing + 1) } else { None }))
        });
        Button::new(cx, move |cx| Label::new(cx, format!("Back to step {}", current + 1)))
            .class("btn")
            .class("is-on")
            .on_press(|cx| cx.emit(LessonEvent::Review(None)));
        Button::new(cx, |cx| Label::new(cx, "Exit"))
            .class("btn")
            .class("quiet")
            .on_press(|cx| cx.emit(LessonEvent::Exit));
    })
    .class("lesson-bar")
    .gap(Pixels(tokens::SPACE_3))
    .padding_left(Pixels(tokens::SPACE_3))
    .padding_right(Pixels(tokens::SPACE_3))
    .alignment(Alignment::Left)
    .width(Stretch(1.0))
    .height(Auto)
    .min_height(Pixels(48.0))
    .padding_top(Pixels(6.0))
    .padding_bottom(Pixels(6.0));
}

pub fn lesson_bar(cx: &mut Context, p: LessonBarProps) {
    let shown = Memo::new(move |_| (p.active.get(), p.reviewing.get()));
    Binding::new(cx, shown, move |cx| {
        let Some((lesson, step)) = p.active.get() else { return };
        if let Some(viewing) = p.reviewing.get().filter(|v| *v < step) {
            review_bar(cx, lesson, viewing, step);
            return;
        }
        let l = &LESSONS[lesson];
        let s = &l.steps[step];
        HStack::new(cx, move |cx| {
            title_and_dots(cx, lesson, step, step);

            Element::new(cx).class("hairline").width(Pixels(1.0)).height(Pixels(28.0));

            // Reread the steps already done.
            if step > 0 {
                nav_button(cx, "\u{2039} Back", "Read the step before again").on_press(move |cx| cx.emit(LessonEvent::Review(Some(step - 1))));
            }

            VStack::new(cx, move |cx| {
                // Wrapped to the space between the title and the buttons.
                Label::new(cx, s.text).class("body").class("lesson-text").width(Stretch(1.0)).text_wrap(true);
                // New music words in this step, in plain language.
                let words = super::course::new_words(l, step);
                if !words.is_empty() {
                    let line = words.iter().map(|(term, meaning)| format!("{term}: {meaning}")).collect::<Vec<_>>().join("   \u{b7}   ");
                    Label::new(cx, line).class("value").class("lesson-words").width(Stretch(1.0)).text_wrap(true);
                }
                // Under it: why the step just done sounds the way it does
                // (while the change is still in your ears) - until a hint
                // for this step is due, which takes the line instead.
                let why = step.checked_sub(1).map(|i| l.steps[i].why).unwrap_or("");
                if !why.is_empty() {
                    HStack::new(cx, move |cx| {
                        Label::new(cx, format!("Just now: {why}"))
                            .class("value")
                            .class("lesson-why")
                            .width(Stretch(1.0))
                            .text_wrap(true);
                        // Hear the step just done: the sound before it and
                        // after it, back to back, is the lesson.
                        let unheard = p.change_step.map(move |c| step.checked_sub(1).is_none_or(|prev| *c != Some(prev)));
                        for (which, label) in [(Which::Before, "Before"), (Which::After, "After")] {
                            preview_button(cx, p, which, label).toggle_class("hidden", unheard);
                        }
                    })
                    .gap(Pixels(tokens::SPACE_2))
                    .alignment(Alignment::Left)
                    .height(Auto)
                    .toggle_class("hidden", p.hint_visible.map(move |v| *v && !s.hint.is_empty()));
                }
                // A quiz answered wrong: try again, by ear.
                if let Kind::Quiz { options, .. } = s.kind {
                    Label::new(
                        cx,
                        p.quiz_wrong.map(move |w| match w {
                            Some(i) => format!("Not {} - play it again and listen.", options[*i].to_lowercase()),
                            None => String::new(),
                        }),
                    )
                    .class("value")
                    .class("lesson-why")
                    .toggle_class("hidden", p.quiz_wrong.map(|w| w.is_none()));
                }
                if !s.hint.is_empty() {
                    Label::new(cx, s.hint)
                        .class("value")
                        .width(Stretch(1.0))
                        .text_wrap(true)
                        .toggle_class("hidden", p.hint_visible.map(|v| !*v));
                }
            })
            .gap(Pixels(2.0))
            .width(Stretch(1.0))
            .height(Auto);

            // Just watched "Show me" do the last step: undo it and have a go.
            if step > 0 {
                Button::new(cx, |cx| Label::new(cx, "\u{21b6} Try it yourself"))
                    .class("btn")
                    .class("is-on")
                    .toggle_class("hidden", p.shown_step.map(move |s| *s != Some(step - 1)))
                    .on_press(|cx| cx.emit(LessonEvent::TryYourself));
            }

            // Where this lesson is going, to have in your ears first (a
            // Sound match has its own Target / Yours buttons instead).
            // A quiz has its own Play.
            let is_match = super::sound_match::target(l.id).is_some();
            if !is_match && !matches!(s.kind, Kind::Quiz { .. }) {
                preview_button(cx, p, Which::Goal, "Hear the goal").toggle_class("hidden", p.has_goal.map(|g| !*g));
            }

            let last = step + 1 == l.steps.len();
            match s.kind {
                Kind::Info if last && lesson + 1 < LESSONS.len() => {
                    // Back a lesson, within the same group (Carve, Theory...).
                    if lesson > 0 && LESSONS[lesson - 1].group == l.group {
                        Button::new(cx, |cx| Label::new(cx, "Previous lesson"))
                            .class("btn")
                            .class("quiet")
                            .on_press(move |cx| cx.emit(ProjectEvent::StartLesson(lesson - 1)));
                    }
                    Button::new(cx, |cx| Label::new(cx, "Next lesson"))
                        .class("btn")
                        .class("is-on")
                        .on_press(move |cx| cx.emit(ProjectEvent::StartLesson(lesson + 1)));
                    Button::new(cx, |cx| Label::new(cx, "Done")).class("btn").on_press(|cx| cx.emit(LessonEvent::Continue));
                }
                Kind::Info => {
                    let label = if last { "Done" } else { "Continue" };
                    Button::new(cx, move |cx| Label::new(cx, label))
                        .class("btn")
                        .class("is-on")
                        .on_press(|cx| cx.emit(LessonEvent::Continue));
                }
                Kind::Quiz { options, .. } => {
                    preview_button(cx, p, Which::Quiz, "Play").class("is-on");
                    Element::new(cx).class("hairline").width(Pixels(1.0)).height(Pixels(22.0));
                    for (i, option) in options.iter().enumerate() {
                        Button::new(cx, move |cx| Label::new(cx, *option))
                            .class("btn")
                            .toggle_class("quiet", p.quiz_wrong.map(move |w| *w == Some(i)))
                            .on_press(move |cx| cx.emit(LessonEvent::Answer(i)));
                    }
                    Button::new(cx, |cx| Label::new(cx, "Skip"))
                        .class("btn")
                        .class("quiet")
                        .on_press(|cx| cx.emit(LessonEvent::Skip));
                }
                Kind::Action { .. } => {
                    // Stuck? Watch it done - then Undo to try it yourself.
                    Button::new(cx, |cx| Label::new(cx, "Show me"))
                        .class("btn")
                        .class("quiet")
                        .tooltip(|cx| {
                            Tooltip::new(cx, |cx| {
                                Label::new(cx, "Does this step for you - then \u{201c}Try it yourself\u{201d} takes it back.");
                            })
                            .placement(Placement::Bottom)
                            .arrow(false)
                        })
                        .on_press(|cx| cx.emit(LessonEvent::ShowMe));
                    Button::new(cx, |cx| Label::new(cx, "Skip"))
                        .class("btn")
                        .class("quiet")
                        .on_press(|cx| cx.emit(LessonEvent::Skip));
                }
            }
            Button::new(cx, |cx| Label::new(cx, "Exit"))
                .class("btn")
                .class("quiet")
                .on_press(|cx| cx.emit(LessonEvent::Exit));
        })
        .class("lesson-bar")
        .gap(Pixels(tokens::SPACE_3))
        .padding_left(Pixels(tokens::SPACE_3))
        .padding_right(Pixels(tokens::SPACE_3))
        .alignment(Alignment::Left)
        .width(Stretch(1.0))
        // Grows for a two-line explanation.
        .height(Auto)
        .min_height(Pixels(48.0))
        .padding_top(Pixels(6.0))
        .padding_bottom(Pixels(6.0));

        if super::sound_match::target(l.id).is_some() {
            match_panel(cx, p);
        }
    });
}

/// Sound match: the Target against yours - spectrum and loudness - with
/// the score and buttons to hear each.
fn match_panel(cx: &mut Context, p: LessonBarProps) {
    use super::match_view::{Graph, MatchGraph};
    HStack::new(cx, move |cx| {
        MatchGraph::new(cx, Graph::Spectrum, p.match_target, p.match_yours, p.theme).width(Stretch(2.0)).height(Stretch(1.0));
        MatchGraph::new(cx, Graph::Loudness, p.match_target, p.match_yours, p.theme).width(Stretch(1.0)).height(Stretch(1.0));
        VStack::new(cx, move |cx| {
            Label::new(
                cx,
                p.match_score.map(|s| match s {
                    Some(s) => format!("{:.0}%", s * 100.0),
                    None => "\u{2026}".to_string(),
                }),
            )
            .class("match-score")
            .toggle_class("is-matched", p.match_score.map(|s| s.is_some_and(|s| s >= super::sound_match::WIN)));
            Label::new(cx, "match").class("value");
            preview_button(cx, p, Which::Goal, "Target").width(Stretch(1.0));
            preview_button(cx, p, Which::Yours, "Yours").width(Stretch(1.0));
        })
        .gap(Pixels(4.0))
        .alignment(Alignment::TopCenter)
        .width(Pixels(110.0))
        .height(Stretch(1.0));
    })
    .class("lesson-bar")
    .gap(Pixels(tokens::SPACE_3))
    .padding(Pixels(tokens::SPACE_3))
    .padding_top(Pixels(0.0))
    .width(Stretch(1.0))
    .height(Pixels(150.0));
}

/// "▸ {label}", or "■ Stop" while `which` plays.
fn preview_button<'a>(cx: &'a mut Context, p: LessonBarProps, which: Which, label: &'static str) -> Handle<'a, Button> {
    Button::new(cx, move |cx| {
        Label::new(cx, p.previewing.map(move |now| {
            if *now == Some(which) { "\u{25a0} Stop".to_string() } else { format!("\u{25b8} {label}") }
        }))
    })
    .class("btn")
    .class("quiet")
    .on_press(move |cx| cx.emit(LessonEvent::Hear(which)))
}
