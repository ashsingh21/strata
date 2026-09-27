//! The modulator pill/chip (`.st-mod`): the one rounded shape in Strata,
//! `mod` text and border on a `mod-soft` ground, with a tiny sine glyph.

use vizia::prelude::*;
use crate::hidpi::Logical;
use vizia::vg;

use crate::tokens::ThemeId;

pub struct SineGlyph {
    theme: Signal<ThemeId>,
}

impl SineGlyph {
    pub fn new(cx: &mut Context, theme: Signal<ThemeId>) -> Handle<'_, Self> {
        Self { theme }.build(cx, |_| {}).bind(theme, |mut handle| handle.needs_redraw())
    }
}

impl View for SineGlyph {
    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let _hidpi = crate::hidpi::scale(cx, canvas);
        let bounds = cx.lbounds();
        let palette = self.theme.get().palette();

        let mut path = vg::PathBuilder::new();
        let steps = 24;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let x = bounds.x + t * bounds.w;
            let y = bounds.y + bounds.h * 0.5 - (t * std::f32::consts::TAU).sin() * bounds.h * 0.42;
            if i == 0 {
                path.move_to(vg::Point::new(x, y));
            } else {
                path.line_to(vg::Point::new(x, y));
            }
        }
        let path = path.detach();

        let mut paint = vg::Paint::default();
        paint.set_color(palette.md);
        paint.set_style(vg::PaintStyle::Stroke);
        paint.set_stroke_width(1.5);
        paint.set_stroke_cap(vg::PaintCap::Round);
        paint.set_anti_alias(true);
        canvas.draw_path(&path, &paint);
    }
}

/// Builds a pill chip: sine glyph, name label, target count.
pub fn modulator_pill(cx: &mut Context, theme: Signal<ThemeId>, name: &'static str, targets: u32) {
    HStack::new(cx, move |cx| {
        SineGlyph::new(cx, theme).width(Pixels(20.0)).height(Pixels(10.0));
        Label::new(cx, name).class("control");
        Label::new(cx, targets.to_string()).class("mono");
    })
    .class("pill")
    .padding_left(Pixels(6.0))
    .padding_right(Pixels(8.0))
    .gap(Pixels(8.0))
    .alignment(Alignment::Center)
    .size(Auto);
}
