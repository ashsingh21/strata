//! The Strata stereo meter: two 4px channels, `volt` up to -6 dBFS and `hot`
//! above, with a latching `record`-coloured clip LED. This view is a pure
//! renderer: the dB mapping, peak-hold decay and clip latching all happen in
//! the app's telemetry timer (`app.rs`), which feeds it ready-to-draw 0..1
//! fill fractions.
//!
use vizia::prelude::*;
use vizia::vg;

use crate::tokens::ThemeId;

const CHANNEL_WIDTH: f32 = 4.0;
const CHANNEL_GAP: f32 = 2.0;
const LED_HEIGHT: f32 = 4.0;
/// Fraction of full scale at which volt meter colour switches to hot
/// (corresponds to -6 dBFS on the meter's -60..0 dB scale).
pub const HOT_THRESHOLD: f32 = 0.9;

type ClipResetCallback = Box<dyn Fn(&mut EventContext)>;

/// Generic over the level source (a plain `Signal<f32>` or a derived
/// `Memo<f32>`).
pub struct Meter<L: SignalGet<f32> + Copy + 'static> {
    level_l: L,
    level_r: L,
    clip_l: Signal<bool>,
    clip_r: Signal<bool>,
    theme: Signal<ThemeId>,
    on_clip_reset: Option<ClipResetCallback>,
}

impl<L: SignalGet<f32> + Copy + 'static> Meter<L> {
    pub fn new(
        cx: &mut Context,
        level_l: L,
        level_r: L,
        clip_l: Signal<bool>,
        clip_r: Signal<bool>,
        theme: Signal<ThemeId>,
        on_clip_reset: impl 'static + Fn(&mut EventContext),
    ) -> Handle<'_, Self> {
        Self { level_l, level_r, clip_l, clip_r, theme, on_clip_reset: Some(Box::new(on_clip_reset)) }
            .build(cx, |_| {})
            .bind(level_l, |mut handle| handle.needs_redraw())
            .bind(level_r, |mut handle| handle.needs_redraw())
            .bind(clip_l, |mut handle| handle.needs_redraw())
            .bind(clip_r, |mut handle| handle.needs_redraw())
            .bind(theme, |mut handle| handle.needs_redraw())
    }
}

impl<L: SignalGet<f32> + Copy + 'static> View for Meter<L> {
    fn element(&self) -> Option<&'static str> {
        Some("strata-meter")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, _| {
            if let WindowEvent::MouseDown(button) = window_event {
                if *button == MouseButton::Left {
                    if let Some(callback) = &self.on_clip_reset {
                        (callback)(cx);
                    }
                }
            }
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let palette = self.theme.get().palette();

        let channels = [
            (self.level_l.get().clamp(0.0, 1.0), self.clip_l.get()),
            (self.level_r.get().clamp(0.0, 1.0), self.clip_r.get()),
        ];

        for (i, (level, clip)) in channels.iter().enumerate() {
            let x = bounds.x + i as f32 * (CHANNEL_WIDTH + CHANNEL_GAP);
            let channel_rect = vg::Rect::new(x, bounds.y, x + CHANNEL_WIDTH, bounds.y + bounds.h);

            let mut bg_paint = vg::Paint::default();
            bg_paint.set_color(palette.bg_000);
            bg_paint.set_anti_alias(true);
            canvas.draw_path(&vg::Path::rect(channel_rect, None), &bg_paint);

            let lo_frac = level.min(HOT_THRESHOLD);
            let hi_frac = (level - HOT_THRESHOLD).max(0.0);

            if lo_frac > 0.0 {
                let lo_rect = vg::Rect::new(
                    x,
                    bounds.y + bounds.h * (1.0 - lo_frac),
                    x + CHANNEL_WIDTH,
                    bounds.y + bounds.h,
                );
                let mut lo_paint = vg::Paint::default();
                lo_paint.set_color(palette.volt);
                lo_paint.set_anti_alias(true);
                canvas.draw_path(&vg::Path::rect(lo_rect, None), &lo_paint);
            }

            if hi_frac > 0.0 {
                let hi_rect = vg::Rect::new(
                    x,
                    bounds.y + bounds.h * (1.0 - HOT_THRESHOLD - hi_frac),
                    x + CHANNEL_WIDTH,
                    bounds.y + bounds.h * (1.0 - HOT_THRESHOLD),
                );
                let mut hi_paint = vg::Paint::default();
                hi_paint.set_color(palette.hot);
                hi_paint.set_anti_alias(true);
                canvas.draw_path(&vg::Path::rect(hi_rect, None), &hi_paint);
            }

            if *clip {
                let led_rect = vg::Rect::new(x, bounds.y, x + CHANNEL_WIDTH, bounds.y + LED_HEIGHT);
                let mut led_paint = vg::Paint::default();
                led_paint.set_color(palette.record);
                led_paint.set_anti_alias(true);
                canvas.draw_path(&vg::Path::rect(led_rect, None), &led_paint);
            }
        }
    }
}
