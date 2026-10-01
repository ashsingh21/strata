//! Tips that appear once, in a corner, when they'd help - and never
//! again after "Got it". At most one shows at a time, none during a
//! lesson or while the palette is open. "Show tips again" in the palette
//! brings them back.

use vizia::prelude::*;

use crate::tokens;

pub struct Hint {
    pub id: &'static str,
    pub text: &'static str,
    /// Shown only when this holds.
    pub when: fn(&Situation) -> bool,
}

/// What the app is doing, as far as hints care.
#[derive(Clone, Copy, Default)]
pub struct Situation {
    pub piano_roll_open: bool,
}

/// In priority order: the first one that applies and isn't dismissed shows.
pub const HINTS: &[Hint] = &[
    Hint { id: "palette", text: "Looking for something? Press Ctrl+K to search every command, sound and lesson.", when: |_| true },
    Hint {
        id: "piano-roll",
        text: "Drag a note's right edge to change its length; new notes copy it. Click the ruler to move the playhead.",
        when: |c| c.piano_roll_open,
    },
];

/// The hint to show now, if any.
pub fn pick(dismissed: &[String], ctx: &Situation) -> Option<usize> {
    HINTS.iter().position(|h| (h.when)(ctx) && !dismissed.iter().any(|d| d == h.id))
}

pub enum HintEvent {
    Dismiss(&'static str),
    ResetAll,
}

pub struct HintModel {
    dismissed: Signal<Vec<String>>,
    pub shown: Memo<Option<usize>>,
}

impl HintModel {
    pub fn new(piano_roll_open: Memo<bool>, lesson_active: Memo<bool>, palette_open: Signal<bool>) -> Self {
        let dismissed = Signal::new(crate::settings::load_hints_dismissed());
        let shown = Memo::new(move |_| {
            if lesson_active.get() || palette_open.get() {
                return None;
            }
            pick(&dismissed.get(), &Situation { piano_roll_open: piano_roll_open.get() })
        });
        Self { dismissed, shown }
    }
}

impl Model for HintModel {
    fn event(&mut self, _cx: &mut EventContext, event: &mut Event) {
        event.map(|e, _| match e {
            HintEvent::Dismiss(id) => {
                let mut now = self.dismissed.get();
                if !now.iter().any(|d| d == id) {
                    now.push(id.to_string());
                    crate::settings::save_hints_dismissed(&now);
                    self.dismissed.set(now);
                }
            }
            HintEvent::ResetAll => {
                crate::settings::save_hints_dismissed(&[]);
                self.dismissed.set(Vec::new());
            }
        });
    }
}

/// The tip's card, bottom right above the status bar. Mount at the root.
pub fn hint_card(cx: &mut Context, shown: Memo<Option<usize>>) {
    Binding::new(cx, shown, move |cx| {
        let Some(i) = shown.get() else { return };
        let hint = &HINTS[i];
        let id = hint.id;
        HStack::new(cx, move |cx| {
            Label::new(cx, hint.text).class("body").text_wrap(true).width(Stretch(1.0));
            Button::new(cx, |cx| Label::new(cx, "Got it"))
                .class("btn")
                .on_press(move |cx| cx.emit(HintEvent::Dismiss(id)));
        })
        .class("panel")
        .class("context-menu")
        .class("hint-card")
        .gap(Pixels(tokens::SPACE_3))
        .padding(Pixels(tokens::SPACE_3))
        .alignment(Alignment::Left)
        .position_type(PositionType::Absolute)
        .right(Pixels(16.0))
        .bottom(Pixels(36.0))
        .width(Pixels(380.0))
        .height(Auto);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_palette_tip_comes_first_then_is_gone_for_good() {
        let ctx = Situation::default();
        assert_eq!(pick(&[], &ctx).map(|i| HINTS[i].id), Some("palette"));
        assert_eq!(pick(&["palette".to_string()], &ctx), None);
    }

    #[test]
    fn the_piano_roll_tip_waits_for_the_piano_roll() {
        let done = vec!["palette".to_string()];
        assert_eq!(pick(&done, &Situation { piano_roll_open: true }).map(|i| HINTS[i].id), Some("piano-roll"));
        let all = vec!["palette".to_string(), "piano-roll".to_string()];
        assert_eq!(pick(&all, &Situation { piano_roll_open: true }), None);
    }

    #[test]
    fn ids_are_unique() {
        for (i, a) in HINTS.iter().enumerate() {
            assert!(HINTS[i + 1..].iter().all(|b| b.id != a.id));
        }
    }
}
