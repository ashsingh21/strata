//! The Learn pages, over the arrangement: the home (where to carry on,
//! today's practice, every goal) and a goal's page (its lessons and drills
//! in order). The Learn rail button opens the home; starting anything, or
//! "Back to my project", closes it.

use vizia::prelude::*;

use super::goals::{self, Goal, Item, GOALS};
use super::{LearnEvent, LearnProps, Page};
use crate::browser::icon::{Icon, IconKind};
use crate::tools::{Tool, ToolsEvent};
use crate::tokens;

/// The goal's swatch colour.
pub fn swatch(goal: &Goal) -> Color {
    goal.color.map(crate::timeline::header::clip_color_to_rgb).unwrap_or(Color::rgb(128, 127, 122))
}

/// Mounted over the arrangement column; shows while a page is asked for
/// and no lesson runs.
pub fn learn_page(cx: &mut Context, p: LearnProps) {
    let shown = Memo::new(move |_| if p.lessons_active.get().is_some() { None } else { p.page.get() });
    VStack::new(cx, move |cx| {
        // Rebuilt each time: views shown again after hiding can come back blank.
        Binding::new(cx, shown, move |cx| match shown.get() {
            Some(Page::Home) => {
                ScrollView::new(cx, move |cx| home(cx, p)).show_horizontal_scrollbar(false).width(Stretch(1.0)).height(Stretch(1.0));
            }
            Some(Page::Goal(g)) => {
                ScrollView::new(cx, move |cx| goal_page(cx, p, g)).show_horizontal_scrollbar(false).width(Stretch(1.0)).height(Stretch(1.0));
            }
            None => {}
        });
    })
    .class("course-map")
    .toggle_class("hidden", shown.map(|s| s.is_none()))
    .position_type(PositionType::Absolute)
    .z_index(20)
    .width(Stretch(1.0))
    .height(Stretch(1.0));
}

fn back_to_project(cx: &mut Context) {
    Button::new(cx, |cx| Label::new(cx, "Back to my project")).class("btn").on_press(|cx| cx.emit(LearnEvent::Open(None)));
}

fn home(cx: &mut Context, p: LearnProps) {
    let done = p.done.get();
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            VStack::new(cx, move |cx| {
                Label::new(cx, "Learn").class("display-sm");
                Label::new(cx, "Pick a goal. Each one mixes short lessons with practice you do on your instrument.").class("body").class("learn-muted");
            })
            .gap(Pixels(4.0))
            .width(Stretch(1.0))
            .height(Auto);
            back_to_project(cx);
        })
        .alignment(Alignment::TopLeft)
        .width(Stretch(1.0))
        .height(Auto);

        // Where to carry on, and today's five minutes.
        HStack::new(cx, move |cx| {
            if let Some((g, i)) = goals::continue_at(p.goal.get(), &done) {
                continue_card(cx, g, i);
            }
            today_card(cx, p);
        })
        .class("learn-hero")
        .wrap(LayoutWrap::Wrap)
        .gap(Pixels(24.0))
        .padding(Pixels(20.0))
        .width(Stretch(1.0))
        .height(Auto);

        Label::new(cx, "Goals").class("title");
        HStack::new(cx, move |cx| {
            for g in 0..GOALS.len() {
                goal_card(cx, p, g);
            }
        })
        .wrap(LayoutWrap::Wrap)
        .gap(Pixels(12.0))
        .width(Stretch(1.0))
        .height(Auto);

        // The tools on their own, outside any goal (they're in Search too).
        HStack::new(cx, move |cx| {
            Label::new(cx, "Tools on their own:").class("value");
            for tool in [Tool::Ear, Tool::Exercises, Tool::Theory, Tool::Voicing, Tool::Riyaz] {
                Button::new(cx, move |cx| Label::new(cx, tool.name()))
                    .class("btn")
                    .class("sm")
                    .class("quiet")
                    .on_press(move |cx| {
                        cx.emit(LearnEvent::Open(None));
                        cx.emit(ToolsEvent::Show(tool));
                    });
            }
        })
        .class("learn-footer")
        .wrap(LayoutWrap::Wrap)
        .alignment(Alignment::Left)
        .gap(Pixels(tokens::SPACE_2))
        .padding_top(Pixels(tokens::SPACE_3))
        .width(Stretch(1.0))
        .height(Auto);
    })
    .gap(Pixels(24.0))
    .padding_left(Pixels(40.0))
    .padding_right(Pixels(40.0))
    .padding_top(Pixels(32.0))
    .padding_bottom(Pixels(48.0))
    .width(Stretch(1.0))
    .height(Auto);
}

fn continue_card(cx: &mut Context, g: usize, i: usize) {
    let goal = &GOALS[g];
    let item = goal.items[i];
    VStack::new(cx, move |cx| {
        Label::new(cx, format!("Continue \u{b7} {}, step {} of {}", goal.title, i + 1, goal.items.len())).class("label");
        Label::new(cx, item.title()).class("learn-big").text_wrap(true).width(Stretch(1.0));
        HStack::new(cx, move |cx| {
            Button::new(cx, |cx| Label::new(cx, "Continue \u{25b8}"))
                .class("btn")
                .class("lg")
                .class("is-on")
                .on_press(move |cx| cx.emit(LearnEvent::Start { goal: g, item: i }));
            Label::new(cx, format!("{} \u{b7} {}", item.kind(), item.what())).class("value").text_wrap(true).width(Stretch(1.0));
        })
        .gap(Pixels(tokens::SPACE_3))
        .alignment(Alignment::Left)
        .width(Stretch(1.0))
        .height(Auto);
    })
    .gap(Pixels(10.0))
    .min_width(Pixels(360.0))
    .width(Stretch(2.0))
    .height(Auto);
}

fn today_card(cx: &mut Context, p: LearnProps) {
    let plan = goals::today(&p.done.get());
    let t = p.today_done.get();
    let all = t.iter().all(|x| *x);
    let streak = p.streak.get();
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            Label::new(cx, "Today\u{2019}s practice \u{b7} 5 min").class("title").width(Stretch(1.0));
            let s = match streak {
                0 => String::new(),
                1 => "1 day".to_string(),
                n => format!("{n} days in a row"),
            };
            Label::new(cx, s).class("value");
        })
        .alignment(Alignment::Left)
        .width(Stretch(1.0))
        .height(Auto);
        for (slot, (g, i)) in plan.into_iter().enumerate() {
            let item = GOALS[g].items[i];
            HStack::new(cx, move |cx| {
                if t[slot] {
                    Icon::new(cx, IconKind::Check, 12.0, Signal::new(false), p.theme, |pal, _| pal.signal).width(Pixels(14.0));
                } else {
                    Element::new(cx).class("learn-ring").corner_radius(Pixels(6.0)).width(Pixels(12.0)).height(Pixels(12.0));
                }
                Label::new(cx, item.title()).class("body").width(Stretch(1.0));
                Label::new(cx, item.kind()).class("value");
            })
            .gap(Pixels(tokens::SPACE_2))
            .alignment(Alignment::Left)
            .width(Stretch(1.0))
            .height(Pixels(22.0));
        }
        Button::new(cx, move |cx| Label::new(cx, if all { "Done for today: again?" } else { "Start today\u{2019}s practice" }))
            .class("btn")
            .on_press(|cx| cx.emit(LearnEvent::StartToday));
    })
    .class("learn-today")
    .gap(Pixels(tokens::SPACE_2))
    .padding_left(Pixels(24.0))
    .min_width(Pixels(280.0))
    .width(Stretch(1.0))
    .height(Auto);
}

/// A progress bar: `of` of `all`.
fn progress(cx: &mut Context, of: usize, all: usize) {
    HStack::new(cx, move |cx| {
        let pct = if all == 0 { 0.0 } else { of as f32 / all as f32 * 100.0 };
        HStack::new(cx, move |cx| {
            Element::new(cx).class("learn-fill").width(Percentage(pct)).height(Stretch(1.0));
        })
        .class("learn-track")
        .width(Stretch(1.0))
        .height(Pixels(4.0));
        Label::new(cx, format!("{of} of {all}")).class("value").width(Pixels(52.0));
    })
    .gap(Pixels(tokens::SPACE_2))
    .alignment(Alignment::Left)
    .width(Stretch(1.0))
    .height(Auto);
}

fn goal_card(cx: &mut Context, p: LearnProps, g: usize) {
    let goal = &GOALS[g];
    let done = p.done.get();
    let of = goal.done_count(&done);
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            Element::new(cx).class("learn-swatch").background_color(swatch(goal)).width(Pixels(10.0)).height(Pixels(10.0)).hoverable(false);
            Label::new(cx, goal.title).class("title").text_wrap(true).width(Stretch(1.0)).hoverable(false);
        })
        .gap(Pixels(tokens::SPACE_2))
        .alignment(Alignment::Left)
        .width(Stretch(1.0))
        .height(Auto)
        .hoverable(false);
        Label::new(cx, goal.promise).class("body").class("learn-muted").text_wrap(true).width(Stretch(1.0)).hoverable(false);
        Element::new(cx).height(Stretch(1.0)).width(Pixels(1.0)).hoverable(false);
        Label::new(cx, goal.holds()).class("value").hoverable(false);
        VStack::new(cx, move |cx| progress(cx, of, goal.items.len())).height(Auto).hoverable(false);
    })
    .class("map-card")
    .cursor(CursorIcon::Hand)
    .gap(Pixels(tokens::SPACE_2))
    .padding(Pixels(16.0))
    .width(Pixels(300.0))
    .height(Pixels(156.0))
    .on_press(move |cx| cx.emit(LearnEvent::Open(Some(Page::Goal(g)))));
}

fn goal_page(cx: &mut Context, p: LearnProps, g: usize) {
    let goal = &GOALS[g];
    let done = p.done.get();
    let next = goal.next(&done);
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            Button::new(cx, |cx| Label::new(cx, "\u{2039}  Learn"))
                .class("btn")
                .class("quiet")
                .on_press(|cx| cx.emit(LearnEvent::Open(Some(Page::Home))));
            Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
            back_to_project(cx);
        })
        .alignment(Alignment::Left)
        .width(Stretch(1.0))
        .height(Auto);
        HStack::new(cx, move |cx| {
            Element::new(cx).class("learn-swatch").background_color(swatch(goal)).width(Pixels(12.0)).height(Pixels(12.0));
            Label::new(cx, goal.title).class("display-sm");
        })
        .gap(Pixels(10.0))
        .alignment(Alignment::Left)
        .height(Auto);
        Label::new(cx, goal.promise).class("body").class("learn-muted").text_wrap(true).width(Stretch(1.0));
        let of = goal.done_count(&done);
        VStack::new(cx, move |cx| progress(cx, of, goal.items.len())).width(Pixels(360.0)).height(Auto);

        VStack::new(cx, move |cx| {
            for (i, &item) in goal.items.iter().enumerate() {
                item_row(cx, p, g, i, item, item.is_done(&done), next == Some(i));
            }
        })
        .class("learn-list")
        .width(Stretch(1.0))
        .height(Auto);
    })
    .gap(Pixels(tokens::SPACE_4))
    .padding_left(Pixels(40.0))
    .padding_right(Pixels(40.0))
    .padding_top(Pixels(24.0))
    .padding_bottom(Pixels(48.0))
    .width(Stretch(1.0))
    .height(Auto);
}

/// One step of a goal: done (✓), the one to do next, or one to come.
fn item_row(cx: &mut Context, p: LearnProps, g: usize, i: usize, item: Item, done: bool, current: bool) {
    HStack::new(cx, move |cx| {
        if done {
            Icon::new(cx, IconKind::Check, 12.0, Signal::new(false), p.theme, |pal, _| pal.signal).width(Pixels(24.0)).hoverable(false);
        } else {
            Label::new(cx, format!("{}", i + 1)).class("learn-num").toggle_class("is-current", current).alignment(Alignment::Center).corner_radius(Pixels(12.0)).hoverable(false);
        }
        VStack::new(cx, move |cx| {
            HStack::new(cx, move |cx| {
                Label::new(cx, item.title()).class("title").hoverable(false);
                Label::new(cx, item.kind()).class("value").hoverable(false);
            })
            .gap(Pixels(tokens::SPACE_2))
            .alignment(Alignment::BottomLeft)
            .height(Auto)
            .hoverable(false);
            Label::new(cx, item.what()).class("body").class("learn-muted").hoverable(false);
        })
        .gap(Pixels(2.0))
        .width(Stretch(1.0))
        .height(Auto)
        .hoverable(false);
        Button::new(cx, move |cx| Label::new(cx, if done { "Again" } else { "Start \u{25b8}" }))
            .class("btn")
            .toggle_class("is-on", current)
            .on_press(move |cx| cx.emit(LearnEvent::Start { goal: g, item: i }));
    })
    .class("learn-row")
    .toggle_class("is-current", current)
    .cursor(CursorIcon::Hand)
    .gap(Pixels(14.0))
    .alignment(Alignment::Left)
    .padding_left(Pixels(12.0))
    .padding_right(Pixels(12.0))
    .width(Stretch(1.0))
    .height(Pixels(58.0))
    .on_press(move |cx| cx.emit(LearnEvent::Start { goal: g, item: i }));
}
