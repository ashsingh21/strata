//! Drop-down menus. Vizia's `Dropdown` opens them next to their button
//! (moving them to stay inside the window) and closes them on a click
//! elsewhere or Escape; this gives them the app's look.
//!
//! A menu's button presses `toggle`; each item does its action, then
//! `close`.

use vizia::prelude::*;

use crate::tokens::{SPACE_1, SPACE_2};

/// Opens or closes the menu this button belongs to.
pub fn toggle(cx: &mut EventContext) {
    cx.emit(PopupEvent::Switch);
}

/// Closes the menu the pressed item is in.
pub fn close(cx: &mut EventContext) {
    cx.emit(PopupEvent::Close);
}

/// A menu that opens at `placement` beside the button `trigger` builds.
/// The trigger needs an explicit width (`Auto` is fine): Vizia stretches a
/// dropdown's children, which is nothing inside an Auto-sized dropdown,
/// and a zero-width button can't be clicked.
pub fn menu<'a>(
    cx: &'a mut Context,
    placement: Placement,
    trigger: impl Fn(&mut Context) + 'static,
    content: impl Fn(&mut Context) + 'static,
) -> Handle<'a, Dropdown> {
    Dropdown::new(cx, trigger, content).placement(placement).show_arrow(false).arrow_size(Pixels(4.0)).size(Auto)
}

/// The menu's surface: a column of items, `width` wide.
pub fn panel(cx: &mut Context, width: f32, items: impl FnOnce(&mut Context)) {
    VStack::new(cx, items)
        .class("panel")
        .class("context-menu")
        .gap(Pixels(2.0))
        .padding_top(Pixels(SPACE_2))
        .padding_bottom(Pixels(SPACE_2))
        .padding_left(Pixels(SPACE_1))
        .padding_right(Pixels(SPACE_1))
        .width(Pixels(width))
        .height(Auto);
}

/// One row: `label`, and a shortcut hint on the right if there is one.
/// Lit when `on` is true (the current choice).
pub fn item(
    cx: &mut Context,
    label: impl Res<String> + Clone + 'static,
    shortcut: &'static str,
    on: impl Res<bool> + 'static,
    action: impl Fn(&mut EventContext) + Send + Sync + 'static,
) {
    HStack::new(cx, move |cx| {
        // Not hit-testable, so a click on the text reaches the row.
        Label::new(cx, label.clone())
            .class("body")
            .hoverable(false)
            .text_wrap(false)
            .text_overflow(TextOverflow::Ellipsis)
            .width(Stretch(1.0));
        let shortcut = crate::shortcut(shortcut);
        if !shortcut.is_empty() {
            Label::new(cx, shortcut).class("value").hoverable(false);
        }
    })
    .class("menu-item")
    .toggle_class("is-on", on)
    .gap(Pixels(12.0))
    .on_press(move |cx| {
        action(cx);
        close(cx);
    })
    .cursor(CursorIcon::Hand)
    .alignment(Alignment::Left)
    .width(Stretch(1.0))
    .height(Pixels(28.0));
}

/// A thin line between groups of items.
pub fn separator(cx: &mut Context) {
    Element::new(cx).class("menu-sep").width(Stretch(1.0)).height(Pixels(1.0));
}

/// A menu mounted once at the window root, opened beside whichever button
/// was pressed - for buttons inside the lower panel, which scrolls and so
/// clips a popup of their own. The dropdown's anchor is zero wide (it can
/// never catch a click) and moves onto the pressed button's left edge.
#[derive(Clone, Copy)]
pub struct Anchored {
    host: Entity,
    anchor: Signal<(f32, f32, f32)>,
}

impl Anchored {
    /// Builds it; call last in the root view, so it draws over the rest.
    pub fn build(cx: &mut Context, placement: Placement, content: impl Fn(&mut Context) + 'static) -> Self {
        let anchor = Signal::new((0.0f32, 0.0f32, 0.0f32));
        let host = menu(
            cx,
            placement,
            |cx| {
                Element::new(cx).width(Stretch(1.0)).height(Stretch(1.0));
            },
            content,
        )
        .position_type(PositionType::Absolute)
        .left(anchor.map(|a| Pixels(a.0)))
        .top(anchor.map(|a| Pixels(a.1)))
        .width(Pixels(0.0))
        .height(anchor.map(|a| Pixels(a.2)))
        .entity();
        Self { host, anchor }
    }

    /// A button's press: the menu, opened on that button.
    pub fn open_from(self, cx: &mut EventContext) {
        use crate::hidpi::Logical;
        let b = cx.lbounds();
        self.anchor.set((b.x, b.y, b.h));
        cx.emit_to(self.host, PopupEvent::Open);
    }
}
