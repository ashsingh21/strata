//! The drill bar, over a practice tool opened from a goal or today's
//! practice: which drill and where it sits in its goal, each answer so
//! far, and - once the session is done - how it went and what's next.

use vizia::prelude::*;

use super::goals::{Drill, GOALS};
use super::{LearnEvent, LearnProps};
use crate::tokens;

pub fn drill_bar(cx: &mut Context, p: LearnProps) {
    VStack::new(cx, move |cx| {
        Binding::new(cx, p.run, move |cx| {
            let Some(run) = p.run.get() else { return };
            let goal = &GOALS[run.goal];
            HStack::new(cx, move |cx| {
                Label::new(cx, run.title()).class("heading");
                let place = match run.today {
                    Some(slot) => format!("Today\u{2019}s practice, {} of 3", slot + 1),
                    None => format!("{} \u{b7} step {} of {}", goal.title, run.item + 1, goal.items.len()),
                };
                Label::new(cx, place).class("value");
                let session = run.drill.session() as usize;
                if session > 0 {
                    let (answers, finished) = (run.answers.clone(), run.finished);
                    HStack::new(cx, move |cx| {
                        for k in 0..session {
                            let class = match answers.get(k) {
                                Some(true) => "is-right",
                                Some(false) => "is-wrong",
                                None if k == answers.len() && !finished => "is-now",
                                None => "is-later",
                            };
                            Element::new(cx).class("learn-dot").class(class).width(Pixels(14.0)).height(Pixels(4.0));
                        }
                    })
                    .gap(Pixels(3.0))
                    .alignment(Alignment::Left)
                    .size(Auto);
                }
                let status = if run.finished {
                    match run.drill {
                        Drill::TheoryRing | Drill::VoiceLeading => "Have a look around, then on to the next step.".to_string(),
                        Drill::Riyaz => "Two minutes done.".to_string(),
                        _ if run.passed() => format!("{} of {} right: done", run.right(), session),
                        _ => format!("{} of {} right: {} to pass, try again?", run.right(), session, run.drill.pass()),
                    }
                } else if session > 0 {
                    format!("{} of {}", run.answers.len() + 1, session)
                } else {
                    "Two minutes of singing".to_string()
                };
                Label::new(cx, status).class("value").toggle_class("lesson-done", run.passed());
                Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
                if run.finished {
                    if !run.passed() {
                        Button::new(cx, |cx| Label::new(cx, "Again")).class("btn").class("is-on").on_press(|cx| cx.emit(LearnEvent::Again));
                    }
                    Button::new(cx, |cx| Label::new(cx, "Next step \u{203a}"))
                        .class("btn")
                        .toggle_class("is-on", run.passed())
                        .on_press(|cx| cx.emit(LearnEvent::Next));
                }
                Button::new(cx, |cx| Label::new(cx, "Leave drill"))
                    .class("btn")
                    .class("quiet")
                    .on_press(|cx| cx.emit(LearnEvent::Leave));
            })
            .gap(Pixels(tokens::SPACE_3))
            .alignment(Alignment::Left)
            .padding_left(Pixels(tokens::SPACE_1))
            .width(Stretch(1.0))
            .height(Pixels(30.0));
        });
    })
    .width(Stretch(1.0))
    .height(Auto);
}
