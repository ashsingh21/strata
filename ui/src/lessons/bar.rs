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
}

impl LessonBarProps {
    pub fn of(model: &LessonModel) -> Self {
        Self {
            active: model.active,
            hint_visible: model.hint_visible,
            previewing: model.previewing,
            has_goal: model.has_goal,
            change_step: model.change_step,
            shown_step: model.shown_step,
        }
    }
}

pub fn lesson_bar(cx: &mut Context, p: LessonBarProps) {
    Binding::new(cx, p.active, move |cx| {
        let Some((lesson, step)) = p.active.get() else { return };
        let l = &LESSONS[lesson];
        let s = &l.steps[step];
        HStack::new(cx, move |cx| {
            VStack::new(cx, move |cx| {
                Label::new(cx, format!("{} \u{b7} {}", l.group, l.title)).class("label");
                // Progress: a filled square per step done, the current one
                // outlined.
                let dots: String = (0..l.steps.len())
                    .map(|i| if i < step { '\u{25a0}' } else if i == step { '\u{25a3}' } else { '\u{25a1}' })
                    .collect();
                Label::new(cx, dots).class("value").class("lesson-dots");
            })
            .gap(Pixels(2.0))
            .width(Auto)
            .height(Auto);

            Element::new(cx).class("hairline").width(Pixels(1.0)).height(Pixels(28.0));

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

            // Where this lesson is going, to have in your ears first.
            preview_button(cx, p, Which::Goal, "Hear the goal").toggle_class("hidden", p.has_goal.map(|g| !*g));

            let last = step + 1 == l.steps.len();
            match s.kind {
                Kind::Info if last && lesson + 1 < LESSONS.len() => {
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
    });
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
