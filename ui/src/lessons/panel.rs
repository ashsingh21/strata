//! The lesson panel: the running lesson in the sidebar, in place of the
//! Learn list. Every step is there - done ones collapsed to a line (click
//! one to read it again, with why it sounds the way it does), the current
//! one as a card with its buttons, the ones to come faint - in a column
//! narrow enough to read, beside the controls the steps point at. The bar
//! above the timeline keeps one line for the current step.

use vizia::prelude::*;

use super::bar::{preview_button, title_and_dots, LessonBarProps};
use super::course::{Kind, Lesson, Step, EXPLAINERS, LESSONS};
use super::preview::Which;
use super::LessonEvent;
use crate::project::ProjectEvent;
use crate::tokens;

pub fn lesson_panel(cx: &mut Context, p: LessonBarProps) {
    let shown = Memo::new(move |_| (p.active.get(), p.reviewing.get(), p.completed.get()));
    Binding::new(cx, shown, move |cx| {
        let Some((lesson, step)) = p.active.get() else { return };
        let l = &LESSONS[lesson];
        VStack::new(cx, move |cx| {
            HStack::new(cx, move |cx| {
                title_and_dots(cx, lesson, step, step);
                Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
                Button::new(cx, |cx| Label::new(cx, "Exit"))
                    .class("btn")
                    .class("sm")
                    .class("quiet")
                    .on_press(|cx| cx.emit(LessonEvent::Exit));
            })
            .alignment(Alignment::TopLeft)
            .width(Stretch(1.0))
            .height(Auto);

            let just_done = if p.completed.get() { step.checked_sub(1) } else { None };
            for i in 0..l.steps.len() {
                if Some(i) == just_done {
                    done_card(cx, p, l, i);
                } else if i < step && p.reviewing.get() == Some(i) {
                    reread_card(cx, p, l, i);
                } else if i < step {
                    step_line(cx, i, &l.steps[i], true).on_press(move |cx| cx.emit(LessonEvent::Review(Some(i))));
                } else if i == step && just_done.is_none() {
                    current_card(cx, p, lesson, i);
                } else {
                    step_line(cx, i, &l.steps[i], false);
                }
            }
        })
        .gap(Pixels(tokens::SPACE_2))
        .padding(Pixels(tokens::SPACE_2))
        .width(Stretch(1.0))
        .height(Auto);
    });
}

/// A step as one line: a done one (✓, click to read it again) or one to come.
fn step_line<'a>(cx: &'a mut Context, i: usize, s: &'static Step, done: bool) -> Handle<'a, HStack> {
    HStack::new(cx, move |cx| {
        Label::new(cx, if done { "\u{2713}".to_string() } else { format!("{}", i + 1) })
            .class("value")
            .width(Pixels(16.0))
            .hoverable(false);
        Label::new(cx, s.text)
            .class("value")
            .text_wrap(false)
            .text_overflow(TextOverflow::Ellipsis)
            .width(Stretch(1.0))
            .hoverable(false);
    })
    .class("lesson-step")
    .toggle_class("is-done", done)
    .toggle_class("is-future", !done)
    .cursor(if done { CursorIcon::Hand } else { CursorIcon::Default })
    .gap(Pixels(tokens::SPACE_1))
    .alignment(Alignment::Left)
    .padding_left(Pixels(tokens::SPACE_1))
    .padding_right(Pixels(tokens::SPACE_1))
    .width(Stretch(1.0))
    .height(Pixels(22.0))
}

/// The card around a step shown in full.
fn card<'a>(cx: &'a mut Context, content: impl FnOnce(&mut Context)) -> Handle<'a, VStack> {
    VStack::new(cx, content)
        .class("lesson-card")
        .gap(Pixels(6.0))
        .padding(Pixels(tokens::SPACE_2))
        .width(Stretch(1.0))
        .height(Auto)
}

/// A wrapped line of text in a card.
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

/// The step to do now: what to do, new words, a hint once it's taken a
/// while, and its buttons.
fn current_card(cx: &mut Context, p: LessonBarProps, lesson: usize, i: usize) {
    let l = &LESSONS[lesson];
    let s = &l.steps[i];
    card(cx, move |cx| {
        Label::new(cx, format!("Step {} of {}", i + 1, l.steps.len())).class("label");
        text(cx, s.text.to_string(), "body").class("lesson-text");
        let words = super::course::new_words(l, i);
        if !words.is_empty() {
            let line = words.iter().map(|(term, meaning)| format!("{term}: {meaning}")).collect::<Vec<_>>().join("\n");
            text(cx, line, "value").class("lesson-words");
        }
        if !s.hint.is_empty() {
            text(cx, s.hint.to_string(), "value").toggle_class("hidden", p.hint_visible.map(|v| !*v));
        }
        if let Kind::Quiz { options, .. } = s.kind {
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
        // The step done, to hear before doing it, and yours to compare;
        // and how many of its notes are in.
        if matches!(s.kind, Kind::Action { .. }) {
            buttons(cx, move |cx| {
                preview_button(cx, p, Which::Example, "Hear it").class("is-on").toggle_class("hidden", p.has_example.map(|h| !*h));
                preview_button(cx, p, Which::Yours, "Hear yours").toggle_class("hidden", p.has_example.map(|h| !*h));
            });
            text(
                cx,
                p.ghost_progress.map(|g| match g {
                    Some((placed, all)) => format!("{placed} of {all} notes in \u{b7} the dashed outlines show where"),
                    None => String::new(),
                }),
                "value",
            )
            .toggle_class("hidden", p.ghost_progress.map(|g| g.is_none()));
        }
        let last = i + 1 == l.steps.len();
        match s.kind {
            Kind::Action { .. } => buttons(cx, move |cx| {
                Button::new(cx, |cx| Label::new(cx, "Show me"))
                    .class("btn")
                    .tooltip(|cx| {
                        Tooltip::new(cx, |cx| {
                            Label::new(cx, "Does this step for you - then \u{201c}Try it yourself\u{201d} takes it back.");
                        })
                        .arrow(false)
                    })
                    .on_press(|cx| cx.emit(LessonEvent::ShowMe));
                Button::new(cx, |cx| Label::new(cx, "Skip")).class("btn").class("quiet").on_press(|cx| cx.emit(LessonEvent::Skip));
                goal(cx, p, l);
            }),
            Kind::Info if last => buttons(cx, move |cx| {
                if lesson + 1 < LESSONS.len() {
                    Button::new(cx, |cx| Label::new(cx, "Next lesson"))
                        .class("btn")
                        .class("is-on")
                        .on_press(move |cx| cx.emit(ProjectEvent::StartLesson(lesson + 1)));
                }
                Button::new(cx, |cx| Label::new(cx, "Done")).class("btn").on_press(|cx| cx.emit(LessonEvent::Continue));
                // Back a lesson, within the same group (Carve, Theory...).
                if lesson > 0 && LESSONS[lesson - 1].group == l.group {
                    Button::new(cx, |cx| Label::new(cx, "Previous"))
                        .class("btn")
                        .class("quiet")
                        .on_press(move |cx| cx.emit(ProjectEvent::StartLesson(lesson - 1)));
                }
            }),
            Kind::Info => buttons(cx, move |cx| {
                Button::new(cx, |cx| Label::new(cx, "Continue")).class("btn").class("is-on").on_press(|cx| cx.emit(LessonEvent::Continue));
                goal(cx, p, l);
            }),
            Kind::Quiz { options, .. } => {
                buttons(cx, |cx| {
                    preview_button(cx, p, Which::Quiz, "Play").class("is-on");
                    Button::new(cx, |cx| Label::new(cx, "Skip")).class("btn").class("quiet").on_press(|cx| cx.emit(LessonEvent::Skip));
                });
                for (k, option) in options.iter().enumerate() {
                    Button::new(cx, move |cx| Label::new(cx, *option))
                        .class("btn")
                        .toggle_class("quiet", p.quiz_wrong.map(move |w| *w == Some(k)))
                        .width(Stretch(1.0))
                        .on_press(move |cx| cx.emit(LessonEvent::Answer(k)));
                }
            }
        }
    });
}

/// Where the lesson is going, to have in your ears first - beside a step's
/// buttons (a Sound match has its own Target / Yours instead).
fn goal(cx: &mut Context, p: LessonBarProps, l: &'static Lesson) {
    if super::sound_match::target(l.id).is_none() {
        preview_button(cx, p, Which::Goal, "Hear the goal").toggle_class("hidden", p.has_goal.map(|g| !*g));
    }
}

/// The step just done, held until Next step: what you did, why it sounds
/// that way, and Before / After to hear it.
fn done_card(cx: &mut Context, p: LessonBarProps, l: &'static Lesson, i: usize) {
    let s = &l.steps[i];
    card(cx, move |cx| {
        Label::new(cx, format!("\u{2713} Step {} done", i + 1)).class("label").class("lesson-dots");
        text(cx, s.text.to_string(), "value");
        text(cx, s.why.to_string(), "body").class("lesson-text");
        more(cx, p, format!("{} {}", s.text, s.why));
        buttons(cx, move |cx| {
            let unheard = p.change_step.map(move |c| *c != Some(i));
            for (which, label) in [(Which::Before, "Before"), (Which::After, "After")] {
                preview_button(cx, p, which, label).toggle_class("hidden", unheard);
            }
        });
        Button::new(cx, |cx| Label::new(cx, "\u{21b6} Try it yourself"))
            .class("btn")
            .toggle_class("hidden", p.shown_step.map(move |s| *s != Some(i)))
            .on_press(|cx| cx.emit(LessonEvent::TryYourself));
        Button::new(cx, |cx| Label::new(cx, "Next step \u{203a}"))
            .class("btn")
            .class("is-on")
            .on_press(|cx| cx.emit(LessonEvent::NextStep));
    });
}

/// A done step opened again: all of it, and why (click its title to fold it).
fn reread_card(cx: &mut Context, p: LessonBarProps, l: &'static Lesson, i: usize) {
    let s = &l.steps[i];
    card(cx, move |cx| {
        step_line(cx, i, s, true).on_press(|cx| cx.emit(LessonEvent::Review(None)));
        text(cx, s.text.to_string(), "body");
        let words = super::course::new_words(l, i);
        if !words.is_empty() {
            let line = words.iter().map(|(term, meaning)| format!("{term}: {meaning}")).collect::<Vec<_>>().join("\n");
            text(cx, line, "value").class("lesson-words");
        }
        if !s.why.is_empty() {
            text(cx, s.why.to_string(), "value").class("lesson-why");
        }
        more(cx, p, format!("{} {}", s.text, s.why));
    });
}
