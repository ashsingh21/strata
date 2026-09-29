//! The lesson bar: one strip above the timeline while a lesson runs -
//! the lesson and the step it's on - and the props the lesson panel
//! (`panel.rs`, in the sidebar) shares. The steps themselves, with their
//! buttons, are in the panel.

use vizia::prelude::*;

use super::course::LESSONS;
use super::preview::Which;
use super::{LessonEvent, LessonModel};
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
    pub completed: Signal<bool>,
    pub explaining: Signal<Option<usize>>,
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
            completed: model.completed,
            explaining: model.explaining,
            theme,
        }
    }
}

/// The lesson's title and progress: a filled square per step done, the
/// current one outlined (or, rereading, the one being read).
pub(super) fn title_and_dots(cx: &mut Context, lesson: usize, marked: usize, current: usize) {
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

/// One line above the timeline while a lesson runs but its panel (the
/// sidebar's Learn) isn't on screen: the step now (or that one's just
/// done), Show lesson and Exit. With the panel showing it would only
/// repeat it, so it steps aside and gives the timeline its height back.
pub fn lesson_bar(cx: &mut Context, p: LessonBarProps, panel_shown: Memo<bool>) {
    let shown = Memo::new(move |_| (p.active.get(), p.completed.get(), panel_shown.get()));
    Binding::new(cx, shown, move |cx| {
        let Some((lesson, step)) = p.active.get() else { return };
        let l = &LESSONS[lesson];
        let line = match step.checked_sub(1).filter(|_| p.completed.get()) {
            Some(done) => format!("\u{2713} Step {} done - why it sounds that way is in the lesson panel", done + 1),
            None => format!("Step {} of {}: {}", step + 1, l.steps.len(), l.steps[step].text),
        };
        // Rebuilt rather than toggled hidden: shown again that way, views
        // can come back blank.
        if !panel_shown.get() {
        HStack::new(cx, move |cx| {
            title_and_dots(cx, lesson, step, step);
            Element::new(cx).class("hairline").width(Pixels(1.0)).height(Pixels(24.0));
            Label::new(cx, line.clone())
                .class("body")
                .text_wrap(false)
                .text_overflow(TextOverflow::Ellipsis)
                .width(Stretch(1.0));
            Button::new(cx, |cx| Label::new(cx, "Show lesson"))
                .class("btn")
                .class("is-on")
                .on_press(|cx| cx.emit(crate::browser::BrowserEvent::ShowLearn));
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
        .height(Pixels(44.0));
        }

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
pub(super) fn preview_button<'a>(cx: &'a mut Context, p: LessonBarProps, which: Which, label: &'static str) -> Handle<'a, Button> {
    Button::new(cx, move |cx| {
        Label::new(cx, p.previewing.map(move |now| {
            if *now == Some(which) { "\u{25a0} Stop".to_string() } else { format!("\u{25b8} {label}") }
        }))
    })
    .class("btn")
    .class("quiet")
    .on_press(move |cx| cx.emit(LessonEvent::Hear(which)))
}
