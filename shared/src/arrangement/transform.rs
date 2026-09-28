//! Pure pixel math for the timeline: tick <-> x conversion, scroll, zoom and
//! grid snapping. No rendering or event-handling here, so it is unit
//! testable without pulling in Vizia.

use super::time::{Ticks, PPQ};

pub const MIN_PIXELS_PER_BEAT: f64 = 2.0;
pub const MAX_PIXELS_PER_BEAT: f64 = 400.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewTransform {
    pub pixels_per_beat: f64,
    /// Horizontal scroll, in pixels: `x = tick_to_unscrolled_x(tick) - scroll_x`.
    pub scroll_x: f64,
    /// Vertical scroll, in pixels, over the stacked lanes.
    pub scroll_y: f64,
}

impl Default for ViewTransform {
    fn default() -> Self {
        // 48px/bar at 4/4 (12px/beat), matching the design preview's default zoom.
        Self { pixels_per_beat: 12.0, scroll_x: 0.0, scroll_y: 0.0 }
    }
}

impl ViewTransform {
    pub fn tick_to_x(&self, tick: Ticks) -> f64 {
        (tick as f64 / PPQ as f64) * self.pixels_per_beat - self.scroll_x
    }

    pub fn x_to_tick(&self, x: f64) -> Ticks {
        (((x + self.scroll_x) / self.pixels_per_beat) * PPQ as f64).round() as Ticks
    }

    /// Pixel length of a tick span at the current zoom (no scroll offset).
    pub fn ticks_to_px(&self, ticks: Ticks) -> f64 {
        (ticks as f64 / PPQ as f64) * self.pixels_per_beat
    }

    pub fn px_to_ticks(&self, px: f64) -> Ticks {
        ((px / self.pixels_per_beat) * PPQ as f64).round() as Ticks
    }

    /// Zooms so the tick currently under `cursor_x` stays under `cursor_x`.
    /// `factor` > 1 zooms in, < 1 zooms out. Clamped to
    /// `[MIN_PIXELS_PER_BEAT, MAX_PIXELS_PER_BEAT]`.
    pub fn zoom_at(&mut self, cursor_x: f64, factor: f64) {
        let tick_under_cursor = self.x_to_tick(cursor_x);
        let new_ppb = (self.pixels_per_beat * factor).clamp(MIN_PIXELS_PER_BEAT, MAX_PIXELS_PER_BEAT);
        if new_ppb == self.pixels_per_beat {
            return;
        }
        self.pixels_per_beat = new_ppb;
        // Re-derive scroll so tick_under_cursor lands back at cursor_x:
        // cursor_x = tick_to_unscrolled_x(tick) - scroll_x.
        let unscrolled_x = (tick_under_cursor as f64 / PPQ as f64) * self.pixels_per_beat;
        self.scroll_x = unscrolled_x - cursor_x;
    }

    pub fn clamp_scroll(&mut self, max_scroll_x: f64, max_scroll_y: f64) {
        self.scroll_x = self.scroll_x.clamp(0.0, max_scroll_x.max(0.0));
        self.scroll_y = self.scroll_y.clamp(0.0, max_scroll_y.max(0.0));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapGrid {
    Quarter,
    Eighth,
    /// Three to a beat.
    EighthTriplet,
    Sixteenth,
    /// Six to a beat - triplet hi-hat rolls.
    SixteenthTriplet,
    ThirtySecond,
    Off,
}

impl SnapGrid {
    /// Every value, coarsest first - the Snap menu's order.
    pub const ALL: [SnapGrid; 7] = [
        SnapGrid::Quarter,
        SnapGrid::Eighth,
        SnapGrid::EighthTriplet,
        SnapGrid::Sixteenth,
        SnapGrid::SixteenthTriplet,
        SnapGrid::ThirtySecond,
        SnapGrid::Off,
    ];

    /// The next value in `ALL`, wrapping round.
    pub fn cycled(self) -> Self {
        let i = Self::ALL.iter().position(|&g| g == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }

    pub fn label(self) -> &'static str {
        match self {
            SnapGrid::Quarter => "1/4",
            SnapGrid::Eighth => "1/8",
            SnapGrid::EighthTriplet => "1/8T",
            SnapGrid::Sixteenth => "1/16",
            SnapGrid::SixteenthTriplet => "1/16T",
            SnapGrid::ThirtySecond => "1/32",
            SnapGrid::Off => "Off",
        }
    }

    /// Grid spacing in ticks, or `None` when snapping is off.
    pub fn ticks(self) -> Option<Ticks> {
        match self {
            SnapGrid::Quarter => Some(PPQ),
            SnapGrid::Eighth => Some(PPQ / 2),
            SnapGrid::EighthTriplet => Some(PPQ / 3),
            SnapGrid::Sixteenth => Some(PPQ / 4),
            SnapGrid::SixteenthTriplet => Some(PPQ / 6),
            SnapGrid::ThirtySecond => Some(PPQ / 8),
            SnapGrid::Off => None,
        }
    }

    /// The finest grid line an editor should draw for this snap: its own
    /// step when that doesn't fall on 16ths (triplets, 32nds), else 16ths.
    pub fn grid_step(self) -> Ticks {
        match self.ticks() {
            Some(step) if step < PPQ / 4 || (PPQ / 4) % step != 0 && step % (PPQ / 4) != 0 => step,
            _ => PPQ / 4,
        }
    }
}

/// Snaps `tick` to the nearest multiple of `grid`; a no-op when `grid` is
/// `Off` or when `bypass` (Alt/Option held) is set.
pub fn snap(tick: Ticks, grid: SnapGrid, bypass: bool) -> Ticks {
    if bypass {
        return tick;
    }
    match grid.ticks() {
        Some(step) if step > 0 => (tick as f64 / step as f64).round() as Ticks * step,
        _ => tick,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_x_round_trip() {
        let t = ViewTransform { pixels_per_beat: 12.0, scroll_x: 0.0, scroll_y: 0.0 };
        for tick in [0, PPQ, PPQ * 4, PPQ * 4 * 16] {
            let x = t.tick_to_x(tick);
            assert_eq!(t.x_to_tick(x), tick);
        }
    }

    #[test]
    fn default_matches_preview_48px_per_bar() {
        let t = ViewTransform::default();
        // 4 beats/bar * 12px/beat = 48px/bar, matching the Timeline preview.
        assert_eq!(t.ticks_to_px(PPQ * 4), 48.0);
    }

    #[test]
    fn scroll_shifts_x() {
        let mut t = ViewTransform::default();
        let x0 = t.tick_to_x(PPQ * 4);
        t.scroll_x = 48.0;
        assert_eq!(t.tick_to_x(PPQ * 4), x0 - 48.0);
    }

    #[test]
    fn zoom_clamped_to_bounds() {
        let mut t = ViewTransform::default();
        t.zoom_at(0.0, 1_000_000.0);
        assert_eq!(t.pixels_per_beat, MAX_PIXELS_PER_BEAT);
        t.zoom_at(0.0, 0.0000001);
        assert_eq!(t.pixels_per_beat, MIN_PIXELS_PER_BEAT);
    }

    #[test]
    fn zoom_keeps_tick_under_cursor() {
        let mut t = ViewTransform { scroll_x: 20.0, ..Default::default() };
        let cursor_x = 130.0;
        let tick_before = t.x_to_tick(cursor_x);

        t.zoom_at(cursor_x, 2.0);
        let tick_after = t.x_to_tick(cursor_x);

        assert_eq!(tick_before, tick_after);
    }

    #[test]
    fn snap_rounds_to_nearest_grid_line() {
        assert_eq!(snap(PPQ / 4 * 3 - 10, SnapGrid::Sixteenth, false), PPQ / 4 * 3);
        assert_eq!(snap(100, SnapGrid::Quarter, false), 0);
        assert_eq!(snap(PPQ + 100, SnapGrid::Quarter, false), PPQ);
    }

    #[test]
    fn snap_bypassed_is_identity() {
        assert_eq!(snap(1234567, SnapGrid::Sixteenth, true), 1234567);
    }

    #[test]
    fn snap_off_is_identity() {
        assert_eq!(snap(1234567, SnapGrid::Off, false), 1234567);
    }

    #[test]
    fn snap_grid_cycles_through_all_states() {
        let mut g = SnapGrid::Quarter;
        let mut seen = vec![g];
        for _ in 0..SnapGrid::ALL.len() - 1 {
            g = g.cycled();
            seen.push(g);
        }
        assert_eq!(seen, SnapGrid::ALL.to_vec());
        assert_eq!(g.cycled(), SnapGrid::Quarter);
    }

    /// Every step divides a beat evenly, so beats stay on the grid; the
    /// drawn grid is 16ths unless the step doesn't fit them.
    #[test]
    fn snap_steps_fit_the_beat() {
        for g in SnapGrid::ALL {
            if let Some(step) = g.ticks() {
                assert_eq!(PPQ % step, 0, "{}", g.label());
            }
        }
        assert_eq!(SnapGrid::Quarter.grid_step(), PPQ / 4);
        assert_eq!(SnapGrid::EighthTriplet.grid_step(), PPQ / 3);
        assert_eq!(SnapGrid::SixteenthTriplet.grid_step(), PPQ / 6);
        assert_eq!(SnapGrid::ThirtySecond.grid_step(), PPQ / 8);
    }
}
