//! The sidebar's Learn section while no lesson runs: the goal you're
//! working through, as a path - done, the one you're on, the ones to come -
//! so it stays in view while a drill runs under the devices.

use vizia::prelude::*;

use super::goals::GOALS;
use super::home::swatch;
use super::{LearnEvent, LearnProps, Page};
use crate::browser::icon::{Icon, IconKind};
use crate::tokens;

pub fn goal_path(cx: &mut Context, p: LearnProps) {
    let shape = Memo::new(move |_| (p.goal.get(), p.done.get(), p.run.get().map(|r| (r.goal, r.item))));
    VStack::new(cx, move |cx| {
        Binding::new(cx, shape, move |cx| {
            let (g, done, running) = shape.get();
            let goal = &GOALS[g.min(GOALS.len() - 1)];
            let g = g.min(GOALS.len() - 1);
            let on = running.filter(|r| r.0 == g).map(|r| r.1).or_else(|| goal.next(&done));
            let of = goal.done_count(&done);
            VStack::new(cx, move |cx| {
                Button::new(cx, |cx| Label::new(cx, "\u{2039}  All goals"))
                    .class("btn")
                    .class("sm")
                    .class("quiet")
                    .on_press(|cx| cx.emit(LearnEvent::Open(Some(Page::Home))));
                HStack::new(cx, move |cx| {
                    Element::new(cx).class("learn-swatch").background_color(swatch(goal)).width(Pixels(10.0)).height(Pixels(10.0));
                    Label::new(cx, goal.title).class("title").text_wrap(true).width(Stretch(1.0));
                    Label::new(cx, format!("{of} of {}", goal.items.len())).class("value");
                })
                .gap(Pixels(tokens::SPACE_2))
                .alignment(Alignment::Left)
                .width(Stretch(1.0))
                .height(Auto);
                ScrollView::new(cx, move |cx| {
                    VStack::new(cx, move |cx| {
                        for (i, &item) in goal.items.iter().enumerate() {
                            let is_done = item.is_done(&done);
                            HStack::new(cx, move |cx| {
                                if is_done {
                                    Icon::new(cx, IconKind::Check, 12.0, Signal::new(false), p.theme, |pal, _| pal.signal).width(Pixels(16.0)).hoverable(false);
                                } else {
                                    Label::new(cx, format!("{}", i + 1)).class("value").width(Pixels(16.0)).hoverable(false);
                                }
                                Label::new(cx, item.title())
                                    .class("body")
                                    .text_wrap(false)
                                    .text_overflow(TextOverflow::Ellipsis)
                                    .width(Stretch(1.0))
                                    .hoverable(false);
                                Label::new(cx, item.kind()).class("value").hoverable(false);
                            })
                            .class("learn-step")
                            .toggle_class("is-current", on == Some(i))
                            .cursor(CursorIcon::Hand)
                            .gap(Pixels(tokens::SPACE_2))
                            .alignment(Alignment::Left)
                            .padding_left(Pixels(6.0))
                            .padding_right(Pixels(6.0))
                            .width(Stretch(1.0))
                            .height(Pixels(26.0))
                            .on_press(move |cx| cx.emit(LearnEvent::Start { goal: g, item: i }));
                        }
                    })
                    .gap(Pixels(2.0))
                    .width(Stretch(1.0))
                    .height(Auto);
                })
                .show_horizontal_scrollbar(false)
                .width(Stretch(1.0))
                .height(Stretch(1.0));
                Button::new(cx, |cx| Label::new(cx, "The goal\u{2019}s page"))
                    .class("btn")
                    .class("sm")
                    .on_press(move |cx| cx.emit(LearnEvent::Open(Some(Page::Goal(g)))));
            })
            .gap(Pixels(tokens::SPACE_3))
            .padding(Pixels(tokens::SPACE_3))
            .width(Stretch(1.0))
            .height(Stretch(1.0));
        });
    })
    .width(Stretch(1.0))
    .height(Stretch(1.0));
}
