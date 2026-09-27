//! The lesson bar: one strip above the timeline while a lesson runs -
//! lesson title and progress, the current instruction (plus a hint once
//! the step has taken a while), and Continue / Skip / Exit.

use vizia::prelude::*;

use super::course::{Kind, LESSONS};
use super::{LessonEvent, LessonModel};
use crate::project::ProjectEvent;
use crate::tokens;

#[derive(Clone, Copy)]
pub struct LessonBarProps {
    pub active: Signal<Option<(usize, usize)>>,
    pub hint_visible: Signal<bool>,
}

impl LessonBarProps {
    pub fn of(model: &LessonModel) -> Self {
        Self { active: model.active, hint_visible: model.hint_visible }
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
                // Under it: why the step just done sounds the way it does
                // (while the change is still in your ears) - until a hint
                // for this step is due, which takes the line instead.
                let why = step.checked_sub(1).map(|i| l.steps[i].why).unwrap_or("");
                if !why.is_empty() {
                    Label::new(cx, format!("Just now: {why}"))
                        .class("value")
                        .class("lesson-why")
                        .width(Stretch(1.0))
                        .text_wrap(true)
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
