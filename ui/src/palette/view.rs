//! The palette's overlay: a dimmed backdrop, and a card with the search
//! box and the result rows. Mounted last at the root so it draws over
//! everything.

use vizia::prelude::*;

use super::{PaletteEvent, PaletteModel};
use crate::tokens;

#[derive(Clone, Copy)]
pub struct PaletteProps {
    open: Signal<bool>,
    query: Signal<String>,
    selected: Signal<usize>,
    rows: Memo<Vec<super::Row>>,
}

impl PaletteProps {
    pub fn of(model: &PaletteModel) -> Self {
        Self { open: model.open, query: model.query, selected: model.selected, rows: model.rows }
    }
}

pub fn palette_overlay(cx: &mut Context, p: PaletteProps) {
    Binding::new(cx, p.open, move |cx| {
        if !p.open.get() {
            return;
        }
        Element::new(cx)
            .class("palette-backdrop")
            .on_mouse_down(|cx, _| cx.emit(PaletteEvent::Close))
            .position_type(PositionType::Absolute)
            .top(Pixels(0.0))
            .left(Pixels(0.0))
            .width(Stretch(1.0))
            .height(Stretch(1.0));
        VStack::new(cx, move |cx| {
            let search = Textbox::new(cx, p.query)
                .placeholder("Type a command, sound or lesson")
                .on_edit(|cx, text| cx.emit(PaletteEvent::SetQuery(text)))
                .class("search")
                .font_size(15.0)
                .width(Stretch(1.0))
                .height(Pixels(36.0))
                .entity();
            cx.emit_to(search, TextEvent::StartEdit);

            Binding::new(cx, p.rows, move |cx| {
                let rows = p.rows.get();
                if rows.is_empty() {
                    Label::new(cx, "Nothing matches. Try fewer letters, or Ctrl+F to search the sidebar.")
                        .class("value")
                        .padding(Pixels(tokens::SPACE_3));
                }
                for (i, row) in rows.iter().enumerate() {
                    let (label, detail, shortcut) = (row.label.clone(), row.detail.clone(), crate::shortcut(row.shortcut));
                    HStack::new(cx, move |cx| {
                        Label::new(cx, label.clone())
                            .class("body")
                            .hoverable(false)
                            .text_wrap(false)
                            .text_overflow(TextOverflow::Ellipsis)
                            .width(Stretch(1.0));
                        Label::new(cx, detail.clone()).class("value").hoverable(false);
                        if !shortcut.is_empty() {
                            Label::new(cx, shortcut).class("palette-kbd").hoverable(false);
                        }
                    })
                    .class("menu-item")
                    .toggle_class("is-on", p.selected.map(move |s| *s == i))
                    .on_press(move |cx| cx.emit(PaletteEvent::RunRow(i)))
                    .cursor(CursorIcon::Hand)
                    .gap(Pixels(12.0))
                    .alignment(Alignment::Left)
                    .width(Stretch(1.0))
                    .height(Pixels(30.0));
                }
            });
            Label::new(cx, "Up / Down to choose, Enter to run, Esc to close").class("value").font_size(11.0);
        })
        .class("panel")
        .class("context-menu")
        .class("palette-card")
        .gap(Pixels(tokens::SPACE_1))
        .padding(Pixels(tokens::SPACE_2))
        .position_type(PositionType::Absolute)
        .top(Pixels(96.0))
        .left(Stretch(1.0))
        .right(Stretch(1.0))
        .width(Pixels(560.0))
        .height(Auto);
    });
}
