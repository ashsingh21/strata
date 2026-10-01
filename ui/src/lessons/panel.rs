//! The lesson pane: while a lesson runs, the sidebar's Learn section is
//! this instead of the course list. A header (the lesson, its progress,
//! Exit), then the step: something to listen to first, what to do now,
//! and - once it's done - why it sounds the way it does; every step of the
//! lesson under that. The step's buttons sit in a footer that stays put.

use vizia::prelude::*;

use super::bar::{preview_button, segments, LessonBarProps};
use super::course::{Kind, Lesson, Step, EXPLAINERS, LESSONS};
use super::preview::Which;
use super::LessonEvent;
use crate::project::ProjectEvent;
use crate::tokens;

pub fn lesson_panel(cx: &mut Context, p: LessonBarProps) {
    VStack::new(cx, move |cx| {
        header(cx, p);
        Element::new(cx).class("hairline").width(Stretch(1.0)).height(Pixels(1.0));
        ScrollView::new(cx, move |cx| body(cx, p))
            .show_horizontal_scrollbar(false)
            .width(Stretch(1.0))
            .height(Stretch(1.0));
        Element::new(cx).class("hairline").width(Stretch(1.0)).height(Pixels(1.0));
        footer(cx, p);
    })
    .class("lesson-pane")
    .width(Stretch(1.0))
    .height(Stretch(1.0));
}

/// The lesson's group and title, Exit, and how far along it is.
fn header(cx: &mut Context, p: LessonBarProps) {
    let at = Memo::new(move |_| (p.active.get(), p.reviewing.get(), p.completed.get()));
    Binding::new(cx, at, move |cx| {
        let Some((lesson, step)) = p.active.get() else { return };
        let l = &LESSONS[lesson];
        let marked = p.reviewing.get().filter(|r| *r < step).unwrap_or(step);
        VStack::new(cx, move |cx| {
            HStack::new(cx, move |cx| {
                VStack::new(cx, move |cx| {
                    Label::new(cx, l.group).class("label");
                    Label::new(cx, l.title).class("heading").text_wrap(true).width(Stretch(1.0));
                })
                .gap(Pixels(2.0))
                .width(Stretch(1.0))
                .height(Auto);
                Button::new(cx, |cx| Label::new(cx, "Exit"))
                    .class("btn")
                    .class("sm")
                    .class("quiet")
                    .tooltip(|cx| Tooltip::new(cx, |cx| { Label::new(cx, "Leave the lesson; your project stays as it is"); }).arrow(false))
                    .on_press(|cx| cx.emit(LessonEvent::Exit));
            })
            .alignment(Alignment::TopLeft)
            .width(Stretch(1.0))
            .height(Auto);
            HStack::new(cx, move |cx| {
                segments(cx, lesson, marked, step, true);
                Label::new(cx, format!("Step {} of {}", (step + 1).min(l.steps.len()), l.steps.len())).class("value").text_wrap(false);
            })
            .gap(Pixels(tokens::SPACE_3))
            .alignment(Alignment::Left)
            .width(Stretch(1.0))
            .height(Auto);
        })
        .gap(Pixels(tokens::SPACE_3))
        .padding(Pixels(tokens::SPACE_4))
        .padding_bottom(Pixels(tokens::SPACE_3))
        .width(Stretch(1.0))
        .height(Auto);
    });
}

/// The step on show - the current one, the one just done, or one being
/// reread - and the list of every step.
fn body(cx: &mut Context, p: LessonBarProps) {
    let shown = Memo::new(move |_| (p.active.get(), p.reviewing.get(), p.completed.get()));
    Binding::new(cx, shown, move |cx| {
        let Some((lesson, step)) = p.active.get() else { return };
        let l = &LESSONS[lesson];
        VStack::new(cx, move |cx| {
            let just_done = if p.completed.get() { step.checked_sub(1) } else { None };
            if let Some(i) = just_done {
                done_section(cx, p, l, i);
            } else if let Some(i) = p.reviewing.get().filter(|r| *r < step) {
                reread_section(cx, p, l, i);
            } else {
                current_sections(cx, p, lesson, step);
            }

            Element::new(cx).class("hairline").width(Stretch(1.0)).height(Pixels(1.0));
            Label::new(cx, "Steps").class("label");
            VStack::new(cx, move |cx| {
                for i in 0..l.steps.len() {
                    let done = i < step;
                    let line = step_line(cx, p, i, &l.steps[i], done, i == step && just_done.is_none());
                    if done {
                        line.on_press(move |cx| cx.emit(LessonEvent::Review(Some(i))));
                    }
                }
            })
            .gap(Pixels(2.0))
            .width(Stretch(1.0))
            .height(Auto);
        })
        .gap(Pixels(tokens::SPACE_3))
        .padding(Pixels(tokens::SPACE_4))
        .width(Stretch(1.0))
        .height(Auto);
    });
}

/// The current step: something to hear first, then what to do.
fn current_sections(cx: &mut Context, p: LessonBarProps, lesson: usize, i: usize) {
    let l = &LESSONS[lesson];
    let s = &l.steps[i];
    let last = i + 1 == l.steps.len();

    // Listen first.
    match s.kind {
        Kind::Action { .. } => {
            let any = Memo::new(move |_| p.has_example.get() || (p.has_goal.get() && super::sound_match::target(l.id).is_none()));
            VStack::new(cx, move |cx| {
                Label::new(cx, "Listen first").class("title");
                HStack::new(cx, move |cx| {
                    preview_button(cx, p, Which::Example, "Hear it").class("lg").class("is-on").width(Stretch(1.0));
                    preview_button(cx, p, Which::Yours, "Hear yours").class("lg").width(Stretch(1.0));
                })
                .toggle_class("hidden", p.has_example.map(|h| !*h))
                .gap(Pixels(tokens::SPACE_2))
                .width(Stretch(1.0))
                .height(Auto);
                goal(cx, p, l);
            })
            .gap(Pixels(tokens::SPACE_2))
            .toggle_class("hidden", any.map(|a| !*a))
            .width(Stretch(1.0))
            .height(Auto);
        }
        Kind::Quiz { .. } => {
            VStack::new(cx, move |cx| {
                Label::new(cx, "Listen").class("title");
                preview_button(cx, p, Which::Quiz, "Play it").class("lg").class("is-on").width(Stretch(1.0));
            })
            .gap(Pixels(tokens::SPACE_2))
            .width(Stretch(1.0))
            .height(Auto);
        }
        Kind::Info => {}
    }

    // Now you.
    VStack::new(cx, move |cx| {
        let heading = match s.kind {
            Kind::Action { .. } => "Now you",
            Kind::Quiz { .. } => "Your answer",
            Kind::Info if last => "Lesson done",
            Kind::Info => "Read",
        };
        Label::new(cx, heading).class("title");
        text(cx, s.text.to_string(), "body").class("lesson-text");
        if matches!(s.kind, Kind::Action { .. }) {
            text(
                cx,
                p.ghost_progress.map(|g| match g {
                    Some((placed, all)) => format!("{placed} of {all} notes in \u{b7} the dashed outlines show where"),
                    None => String::new(),
                }),
                "value",
            )
            .class("lesson-progress")
            .toggle_class("hidden", p.ghost_progress.map(|g| g.is_none()));
        }
        let words = super::course::new_words(l, i);
        if !words.is_empty() {
            let line = words.iter().map(|(term, meaning)| format!("{term}: {meaning}")).collect::<Vec<_>>().join("\n");
            text(cx, line, "value").class("lesson-words");
        }
        if !s.hint.is_empty() {
            text(cx, s.hint.to_string(), "value").class("lesson-hint").toggle_class("hidden", p.hint_visible.map(|v| !*v));
        }
        if let Kind::Quiz { options, .. } = s.kind {
            for (k, option) in options.iter().enumerate() {
                Button::new(cx, move |cx| Label::new(cx, *option))
                    .class("btn")
                    .class("lg")
                    .toggle_class("quiet", p.quiz_wrong.map(move |w| *w == Some(k)))
                    .width(Stretch(1.0))
                    .on_press(move |cx| cx.emit(LessonEvent::Answer(k)));
            }
            text(
                cx,
                p.quiz_wrong.map(move |w| match w {
                    Some(k) => format!("Not {} - play it again and listen.", options[*k].to_lowercase()),
                    None => String::new(),
                }),
                "value",
            )
            .class("lesson-why")
            .toggle_class("hidden", p.quiz_wrong.map(|w| w.is_none()));
        }
        more(cx, p, s.text.to_string());
        // Just watched "Show me" do the last step: undo it and have a go.
        if i > 0 {
            Button::new(cx, |cx| Label::new(cx, "\u{21b6} Try it yourself"))
                .class("btn")
                .class("is-on")
                .toggle_class("hidden", p.shown_step.map(move |s| *s != Some(i - 1)))
                .on_press(|cx| cx.emit(LessonEvent::TryYourself));
        }
    })
    .gap(Pixels(tokens::SPACE_2))
    .width(Stretch(1.0))
    .height(Auto);

    if last && matches!(s.kind, Kind::Info) {
        finish_section(cx, p, lesson);
    }
}

/// The end of a lesson: what you made, to hear again, and the next one.
fn finish_section(cx: &mut Context, p: LessonBarProps, lesson: usize) {
    let l = &LESSONS[lesson];
    VStack::new(cx, move |cx| {
        Label::new(cx, "What you made").class("title");
        HStack::new(cx, move |cx| {
            preview_button(cx, p, Which::Yours, "Hear yours").class("lg").class("is-on").width(Stretch(1.0));
            if super::sound_match::target(l.id).is_none() {
                preview_button(cx, p, Which::Goal, "The example")
                    .class("lg")
                    .width(Stretch(1.0))
                    .toggle_class("hidden", p.has_goal.map(|g| !*g));
            }
        })
        .gap(Pixels(tokens::SPACE_2))
        .width(Stretch(1.0))
        .height(Auto);
        if let Some(next) = next_after(p, lesson) {
            let n = &LESSONS[next];
            VStack::new(cx, move |cx| {
                Label::new(cx, "Up next").class("label");
                Label::new(cx, n.title).class("title");
                Label::new(cx, format!("{} \u{b7} {} steps", n.group, n.steps.len())).class("value");
            })
            .class("lesson-card")
            .gap(Pixels(2.0))
            .padding(Pixels(tokens::SPACE_3))
            .width(Stretch(1.0))
            .height(Auto);
        }
    })
    .gap(Pixels(tokens::SPACE_2))
    .width(Stretch(1.0))
    .height(Auto);
}

/// The lesson to take after `lesson`: the first unfinished one on the path.
fn next_after(p: LessonBarProps, lesson: usize) -> Option<usize> {
    super::course::next_lesson(&p.done.get()).filter(|&n| n != lesson)
}

/// The step just done, held until Next step: what you did, why it sounds
/// that way, and Before / After to hear it.
fn done_section(cx: &mut Context, p: LessonBarProps, l: &'static Lesson, i: usize) {
    let s = &l.steps[i];
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            check(cx, p);
            Label::new(cx, format!("Step {} done", i + 1)).class("title").class("lesson-done");
        })
        .gap(Pixels(6.0))
        .alignment(Alignment::Left)
        .height(Auto);
        text(cx, s.text.to_string(), "value");
        Label::new(cx, "Why it sounds this way").class("title");
        text(cx, s.why.to_string(), "body").class("lesson-text");
        HStack::new(cx, move |cx| {
            let unheard = p.change_step.map(move |c| *c != Some(i));
            for (which, label) in [(Which::Before, "Before"), (Which::After, "After")] {
                preview_button(cx, p, which, label).class("lg").width(Stretch(1.0)).toggle_class("hidden", unheard);
            }
        })
        .gap(Pixels(tokens::SPACE_2))
        .width(Stretch(1.0))
        .height(Auto);
        more(cx, p, format!("{} {}", s.text, s.why));
        Button::new(cx, |cx| Label::new(cx, "\u{21b6} Try it yourself"))
            .class("btn")
            .toggle_class("hidden", p.shown_step.map(move |s| *s != Some(i)))
            .on_press(|cx| cx.emit(LessonEvent::TryYourself));
    })
    .gap(Pixels(tokens::SPACE_2))
    .width(Stretch(1.0))
    .height(Auto);
}

/// A done step opened again: all of it, and why.
fn reread_section(cx: &mut Context, p: LessonBarProps, l: &'static Lesson, i: usize) {
    let s = &l.steps[i];
    VStack::new(cx, move |cx| {
        Label::new(cx, format!("Step {} (done)", i + 1)).class("title");
        text(cx, s.text.to_string(), "body").class("lesson-text");
        let words = super::course::new_words(l, i);
        if !words.is_empty() {
            let line = words.iter().map(|(term, meaning)| format!("{term}: {meaning}")).collect::<Vec<_>>().join("\n");
            text(cx, line, "value").class("lesson-words");
        }
        if !s.why.is_empty() {
            Label::new(cx, "Why it sounds this way").class("title");
            text(cx, s.why.to_string(), "value").class("lesson-why");
        }
        more(cx, p, format!("{} {}", s.text, s.why));
        Button::new(cx, |cx| Label::new(cx, "Back to the current step"))
            .class("btn")
            .class("is-on")
            .on_press(|cx| cx.emit(LessonEvent::Review(None)));
    })
    .gap(Pixels(tokens::SPACE_2))
    .width(Stretch(1.0))
    .height(Auto);
}

/// The step's buttons, kept at the bottom of the pane.
fn footer(cx: &mut Context, p: LessonBarProps) {
    let at = Memo::new(move |_| (p.active.get(), p.completed.get(), p.reviewing.get(), p.done.get().len()));
    Binding::new(cx, at, move |cx| {
        let Some((lesson, step)) = p.active.get() else { return };
        let l = &LESSONS[lesson];
        let Some(s) = l.steps.get(step) else { return };
        let last = step + 1 == l.steps.len();
        HStack::new(cx, move |cx| {
            if p.completed.get() {
                Button::new(cx, |cx| Label::new(cx, "Next step \u{203a}"))
                    .class("btn")
                    .class("is-on")
                    .on_press(|cx| cx.emit(LessonEvent::NextStep));
            } else {
                match s.kind {
                    Kind::Action { .. } => {
                        Button::new(cx, |cx| Label::new(cx, "Show me"))
                            .class("btn")
                            .tooltip(|cx| {
                                Tooltip::new(cx, |cx| {
                                    Label::new(cx, "Does this step for you - then \u{201c}Try it yourself\u{201d} takes it back.");
                                })
                                .arrow(false)
                            })
                            .on_press(|cx| cx.emit(LessonEvent::ShowMe));
                        skip(cx);
                    }
                    Kind::Quiz { .. } => skip(cx),
                    Kind::Info if last => {
                        if let Some(next) = next_after(p, lesson) {
                            Button::new(cx, |cx| Label::new(cx, "Start next"))
                                .class("btn")
                                .class("is-on")
                                .on_press(move |cx| cx.emit(ProjectEvent::StartLesson(next)));
                        }
                        Button::new(cx, |cx| Label::new(cx, "Done")).class("btn").on_press(|cx| cx.emit(LessonEvent::Continue));
                    }
                    Kind::Info => {
                        Button::new(cx, |cx| Label::new(cx, "Continue"))
                            .class("btn")
                            .class("is-on")
                            .on_press(|cx| cx.emit(LessonEvent::Continue));
                    }
                }
            }
            Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
            Button::new(cx, |cx| Label::new(cx, "All lessons"))
                .class("btn")
                .class("quiet")
                .on_press(|cx| {
                    cx.emit(LessonEvent::Exit);
                    cx.emit(LessonEvent::ShowMap(true));
                });
        })
        .gap(Pixels(tokens::SPACE_2))
        .alignment(Alignment::Left)
        .padding_left(Pixels(tokens::SPACE_4))
        .padding_right(Pixels(tokens::SPACE_4))
        .width(Stretch(1.0))
        .height(Pixels(48.0));
    });
}

/// A done mark (the font has no ✓ of its own).
fn check(cx: &mut Context, p: LessonBarProps) -> Handle<'_, crate::browser::icon::Icon<Signal<bool>>> {
    crate::browser::icon::Icon::new(cx, crate::browser::icon::IconKind::Check, 12.0, Signal::new(false), p.theme, |pal, _| pal.signal)
}

fn skip(cx: &mut Context) {
    Button::new(cx, |cx| Label::new(cx, "Skip")).class("btn").class("quiet").on_press(|cx| cx.emit(LessonEvent::Skip));
}

/// A step as one line in the list: done (✓, click to read it again), the
/// current one, or one to come.
fn step_line<'a>(cx: &'a mut Context, p: LessonBarProps, i: usize, s: &'static Step, done: bool, current: bool) -> Handle<'a, HStack> {
    HStack::new(cx, move |cx| {
        if done {
            check(cx, p).width(Pixels(16.0)).hoverable(false);
        } else {
            Label::new(cx, format!("{}", i + 1)).class("value").width(Pixels(16.0)).hoverable(false);
        }
        Label::new(cx, s.text)
            .class("value")
            .text_wrap(false)
            .text_overflow(TextOverflow::Ellipsis)
            .width(Stretch(1.0))
            .hoverable(false);
    })
    .class("lesson-step")
    .toggle_class("is-done", done)
    .toggle_class("is-current", current)
    .toggle_class("is-future", !done && !current)
    .cursor(if done { CursorIcon::Hand } else { CursorIcon::Default })
    .gap(Pixels(tokens::SPACE_1))
    .alignment(Alignment::Left)
    .padding_left(Pixels(tokens::SPACE_1))
    .padding_right(Pixels(tokens::SPACE_1))
    .width(Stretch(1.0))
    .height(Pixels(22.0))
}

/// A wrapped line of text.
fn text<'a>(cx: &'a mut Context, s: impl Res<String> + Clone + 'static, class: &'static str) -> Handle<'a, Label> {
    Label::new(cx, s).class(class).width(Stretch(1.0)).text_wrap(true)
}

/// A row of buttons that fits the column.
fn buttons(cx: &mut Context, content: impl FnOnce(&mut Context)) {
    HStack::new(cx, content).gap(Pixels(tokens::SPACE_1)).alignment(Alignment::Left).width(Stretch(1.0)).height(Auto);
}

/// "More about" the ideas a step mentions - two to a row, to fit the
/// column - and the one that's open, inline.
fn more(cx: &mut Context, p: LessonBarProps, about: String) {
    let found = super::course::explainers_for(&about);
    if !found.is_empty() {
        Label::new(cx, "More about").class("label");
        for pair in found.chunks(2) {
            let pair = pair.to_vec();
            buttons(cx, move |cx| {
                for i in pair {
                    Button::new(cx, move |cx| Label::new(cx, EXPLAINERS[i].1))
                        .class("btn")
                        .class("sm")
                        .toggle_class("is-on", p.explaining.map(move |e| *e == Some(i)))
                        .on_press(move |cx| cx.emit(LessonEvent::Explain(Some(i))));
                }
            });
        }
    }
    Binding::new(cx, p.explaining, move |cx| {
        let Some(i) = p.explaining.get() else { return };
        let (_, title, body) = EXPLAINERS[i];
        VStack::new(cx, move |cx| {
            HStack::new(cx, move |cx| {
                Label::new(cx, title).class("label").width(Stretch(1.0));
                Button::new(cx, |cx| Label::new(cx, "Close"))
                    .class("btn")
                    .class("sm")
                    .class("quiet")
                    .on_press(|cx| cx.emit(LessonEvent::Explain(None)));
            })
            .alignment(Alignment::Left)
            .width(Stretch(1.0))
            .height(Auto);
            text(cx, body.to_string(), "body");
        })
        .class("lesson-bar")
        .gap(Pixels(4.0))
        .padding(Pixels(tokens::SPACE_2))
        .width(Stretch(1.0))
        .height(Auto);
    });
}

/// Where the lesson is going, to have in your ears first (a Sound match
/// has its own Target / Yours instead).
fn goal(cx: &mut Context, p: LessonBarProps, l: &'static Lesson) {
    if super::sound_match::target(l.id).is_none() {
        preview_button(cx, p, Which::Goal, "Hear the goal").toggle_class("hidden", p.has_goal.map(|g| !*g));
    }
}
