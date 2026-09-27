//! The divider between the timeline and the lower panel (Carve, the clip
//! editor, the Effects Board): drag it to trade height between them, the
//! way Logic's and Ableton's editor dividers work. Double-click resets it.
//! On a laptop screen the panel at its natural height left the timeline a
//! sliver; now the panel is as tall as you make it and scrolls inside.

use vizia::prelude::*;
use crate::hidpi::Logical;
use vizia::vg;

use crate::tokens::ThemeId;

/// The panel's height when nothing's been chosen yet, or after a reset.
pub const DEFAULT_PANEL_HEIGHT: f32 = 440.0;
const MIN_PANEL_HEIGHT: f32 = 120.0;
/// Room always left above the divider (header, lesson bar, ruler and a
/// few tracks), so the panel can't squeeze the timeline away entirely.
const MIN_ABOVE: f32 = 280.0;
/// How tall the grab strip is.
pub const SPLITTER_HEIGHT: f32 = 7.0;

pub struct PanelSplitter {
    height: Signal<f32>,
    theme: Signal<ThemeId>,
    dragging: bool,
    hovered: bool,
    last_y: f32,
}

impl PanelSplitter {
    /// `height` is the lower panel's height in logical pixels; it's saved
    /// to settings when a drag ends.
    pub fn new(cx: &mut Context, height: Signal<f32>, theme: Signal<ThemeId>) -> Handle<'_, Self> {
        Self { height, theme, dragging: false, hovered: false, last_y: 0.0 }
            .build(cx, |_| {})
            .bind(theme, |mut h| h.needs_redraw())
            .cursor(CursorIcon::RowResize)
            .width(Stretch(1.0))
            .height(Pixels(SPLITTER_HEIGHT))
    }

    /// The tallest the panel may be for the window's current height.
    fn max_height(cx: &mut EventContext) -> f32 {
        let window_h = cx.with_current(Entity::root(), |cx| cx.lbounds().h);
        (window_h - MIN_ABOVE).max(MIN_PANEL_HEIGHT)
    }
}

impl View for PanelSplitter {
    fn element(&self) -> Option<&'static str> {
        Some("panel-splitter")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, _| match window_event {
            WindowEvent::MouseDown(MouseButton::Left) => {
                self.dragging = true;
                self.last_y = cx.lmouse().1;
                cx.capture();
                cx.needs_redraw();
            }
            WindowEvent::MouseMove(_, y) if self.dragging => {
                let y = crate::hidpi::l(cx, *y);
                let delta = y - self.last_y;
                self.last_y = y;
                let max = Self::max_height(cx);
                // Dragging up makes the panel taller.
                let next = (self.height.get() - delta).clamp(MIN_PANEL_HEIGHT, max);
                if next != self.height.get() {
                    self.height.set(next);
                }
            }
            WindowEvent::MouseUp(MouseButton::Left) if self.dragging => {
                self.dragging = false;
                cx.release();
                cx.needs_redraw();
                crate::settings::save_lower_panel_height(self.height.get());
            }
            WindowEvent::MouseDoubleClick(MouseButton::Left) => {
                self.dragging = false;
                let height = DEFAULT_PANEL_HEIGHT.min(Self::max_height(cx));
                self.height.set(height);
                crate::settings::save_lower_panel_height(height);
            }
            WindowEvent::MouseOver => {
                self.hovered = true;
                cx.needs_redraw();
            }
            WindowEvent::MouseOut => {
                self.hovered = false;
                cx.needs_redraw();
            }
            _ => {}
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let _hidpi = crate::hidpi::scale(cx, canvas);
        let b = cx.lbounds();
        let p = self.theme.get().palette();
        // A hairline across, and a short grip in the middle that brightens
        // on hover so the drag is discoverable.
        let mut line = vg::Paint::default();
        line.set_color(p.line);
        let mid = (b.y + b.h * 0.5).round();
        canvas.draw_path(&vg::Path::rect(vg::Rect::new(b.x, mid, b.x + b.w, mid + 1.0), None), &line);
        let mut grip = vg::Paint::default();
        grip.set_anti_alias(true);
        grip.set_color(if self.hovered || self.dragging { p.ink_muted } else { p.ink_faint });
        let cx_mid = b.x + b.w * 0.5;
        let grip_rect = vg::RRect::new_rect_xy(vg::Rect::new(cx_mid - 18.0, mid - 1.5, cx_mid + 18.0, mid + 2.5), 2.0, 2.0);
        canvas.draw_rrect(grip_rect, &grip);
    }
}
