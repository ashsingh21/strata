//! The course map: every lesson as a card, by group, with where to go on
//! at the top. It covers the arrangement while it's open (the Learn rail
//! button opens it; starting a lesson or "Back to my project" closes it).

use vizia::prelude::*;

use super::bar::LessonBarProps;
use super::course::{Kind, Lesson, LESSONS};
use super::LessonEvent;
use crate::browser::icon::{Icon, IconKind};
use crate::project::ProjectEvent;
use crate::tokens;

const CARD_W: f32 = 236.0;

/// Mounted over the arrangement column; shows itself while asked for and
/// no lesson runs.
pub fn course_map(cx: &mut Context, p: LessonBarProps) {
    let shown = Memo::new(move |_| p.map_open.get() && p.active.get().is_none());
    VStack::new(cx, move |cx| {
        // Rebuilt each time it opens: views shown again after hiding can
        // come back blank.
        Binding::new(cx, shown, move |cx| {
            if shown.get() {
                ScrollView::new(cx, move |cx| page(cx, p)).show_horizontal_scrollbar(false).width(Stretch(1.0)).height(Stretch(1.0));
            }
        });
    })
    .class("course-map")
    .toggle_class("hidden", shown.map(|s| !*s))
    .position_type(PositionType::Absolute)
    .z_index(20)
    .width(Stretch(1.0))
    .height(Stretch(1.0));
}

fn page(cx: &mut Context, p: LessonBarProps) {
    let done = p.done.get();
    let finished = |l: &Lesson| done.iter().any(|d| d == l.id);
    let total_done = LESSONS.iter().filter(|l| finished(l)).count();
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            Label::new(cx, "Learn").class("display-sm");
            Label::new(cx, format!("{total_done} of {} lessons done", LESSONS.len())).class("value");
            Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
            Button::new(cx, |cx| Label::new(cx, "Back to my project"))
                .class("btn")
                .on_press(|cx| cx.emit(LessonEvent::ShowMap(false)));
        })
        .gap(Pixels(tokens::SPACE_3))
        .alignment(Alignment::Left)
        .width(Stretch(1.0))
        .height(Auto);

        if let Some(next) = super::course::next_lesson(&p.done.get()) {
            continue_card(cx, next, total_done == 0);
        }

        let mut groups: Vec<&'static str> = Vec::new();
        for l in LESSONS {
            if !groups.contains(&l.group) {
                groups.push(l.group);
            }
        }
        for group in groups {
            let lessons: Vec<usize> = (0..LESSONS.len()).filter(|&i| LESSONS[i].group == group).collect();
            let count_done = lessons.iter().filter(|&&i| p.done.get().iter().any(|d| d == LESSONS[i].id)).count();
            let all = lessons.len();
            VStack::new(cx, move |cx| {
                HStack::new(cx, move |cx| {
                    Label::new(cx, group).class("heading");
                    Label::new(cx, format!("{count_done} / {all}")).class("value").toggle_class("lesson-done", count_done == all);
                })
                .gap(Pixels(tokens::SPACE_3))
                .alignment(Alignment::BottomLeft)
                .height(Auto);
                HStack::new(cx, move |cx| {
                    for &i in &lessons {
                        card(cx, p, i);
                    }
                })
                .wrap(LayoutWrap::Wrap)
                .gap(Pixels(tokens::SPACE_3))
                .width(Stretch(1.0))
                .height(Auto);
            })
            .gap(Pixels(tokens::SPACE_3))
            .width(Stretch(1.0))
            .height(Auto);
        }
    })
    .gap(Pixels(28.0))
    .padding(Pixels(32.0))
    .width(Stretch(1.0))
    .height(Auto);
}

/// Where to go on: the next lesson on the path, big.
fn continue_card(cx: &mut Context, next: usize, first: bool) {
    let l = &LESSONS[next];
    HStack::new(cx, move |cx| {
        VStack::new(cx, move |cx| {
            Label::new(cx, if first { "Start here" } else { "Up next" }).class("label");
            Label::new(cx, l.title).class("heading");
            Label::new(cx, format!("{} \u{b7} {}", l.group, length(l))).class("value");
        })
        .gap(Pixels(4.0))
        .width(Stretch(1.0))
        .height(Auto);
        Button::new(cx, |cx| Label::new(cx, "Start"))
            .class("btn")
            .class("lg")
            .class("is-on")
            .on_press(move |cx| cx.emit(ProjectEvent::StartLesson(next)));
    })
    .class("lesson-card")
    .alignment(Alignment::Left)
    .gap(Pixels(tokens::SPACE_4))
    .padding(Pixels(tokens::SPACE_4))
    .width(Stretch(1.0))
    .height(Auto);
}

/// One lesson: its title, how long it is, and a mark once done. A click
/// starts it.
fn card(cx: &mut Context, p: LessonBarProps, i: usize) {
    let l = &LESSONS[i];
    let finished = p.done.get().iter().any(|d| d == l.id);
    let is_next = super::course::next_lesson(&p.done.get()) == Some(i);
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            Label::new(cx, l.title).class("title").text_wrap(true).width(Stretch(1.0)).hoverable(false);
            if finished {
                Icon::new(cx, IconKind::Check, 12.0, Signal::new(false), p.theme, |pal, _| pal.signal).hoverable(false);
            }
        })
        .gap(Pixels(tokens::SPACE_2))
        .alignment(Alignment::TopLeft)
        .width(Stretch(1.0))
        .height(Auto);
        Element::new(cx).height(Stretch(1.0)).width(Pixels(1.0)).hoverable(false);
        Label::new(cx, length(l)).class("value").hoverable(false);
    })
    .class("map-card")
    .toggle_class("is-next", is_next)
    .cursor(CursorIcon::Hand)
    .gap(Pixels(6.0))
    .padding(Pixels(tokens::SPACE_3))
    .width(Pixels(CARD_W))
    .height(Pixels(84.0))
    .on_press(move |cx| cx.emit(ProjectEvent::StartLesson(i)));
}

/// "6 steps · 2 listening questions".
fn length(l: &Lesson) -> String {
    let quizzes = l.steps.iter().filter(|s| matches!(s.kind, Kind::Quiz { .. })).count();
    match quizzes {
        0 => format!("{} steps", l.steps.len()),
        1 => format!("{} steps \u{b7} 1 listening question", l.steps.len()),
        n => format!("{} steps \u{b7} {n} listening questions", l.steps.len()),
    }
}
