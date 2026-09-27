//! HiDPI for hand-drawn views. Vizia lays views out in logical points but
//! gives a canvas view physical pixels: its `bounds()` and mouse positions
//! are scaled by the display's factor (2 on a Retina Mac). Our canvases
//! draw with logical sizes (row heights, font sizes, pixels per beat), so
//! on a Retina screen they came out half size while the Vizia-laid-out
//! views around them (track headers) didn't - rows stopped lining up.
//!
//! The fix, per view: at the top of `draw`, `let _hidpi = hidpi::scale(cx,
//! canvas);` (scales the canvas until it's dropped), and read positions
//! through `Logical` (`cx.lbounds()`, `cx.lmouse()`, `hidpi::l(cx, x)`)
//! everywhere a view measures itself or the pointer. At a factor of 1
//! all of this is a no-op.

use vizia::prelude::*;

/// Scales `canvas` so logical coordinates land on the right physical
/// pixels; undone when the guard drops (early returns included).
pub fn scale<'a>(cx: &DrawContext, canvas: &'a Canvas) -> ScaledCanvas<'a> {
    let s = cx.scale_factor();
    canvas.save();
    canvas.scale((s, s));
    ScaledCanvas(canvas)
}

/// Clips drawing to `bounds` (logical, from `lbounds`) until the scale
/// guard drops. For canvases whose content scrolls past their edges (the
/// ruler, the timeline, the piano roll): Skia doesn't clip to a view's
/// layout bounds, so off-screen markers, clips and lines painted over
/// the panels beside them.
pub fn clip(canvas: &Canvas, bounds: BoundingBox) {
    canvas.clip_rect(vizia::vg::Rect::new(bounds.x, bounds.y, bounds.x + bounds.w, bounds.y + bounds.h), None, true);
}

pub struct ScaledCanvas<'a>(&'a Canvas);

impl Drop for ScaledCanvas<'_> {
    fn drop(&mut self) {
        self.0.restore();
    }
}

fn divide(b: BoundingBox, s: f32) -> BoundingBox {
    BoundingBox { x: b.x / s, y: b.y / s, w: b.w / s, h: b.h / s }
}

/// This view's bounds and the pointer, in logical points.
pub trait Logical {
    fn lbounds(&self) -> BoundingBox;
    /// The pointer position.
    fn lmouse(&self) -> (f32, f32);
}

impl Logical for DrawContext<'_> {
    fn lbounds(&self) -> BoundingBox {
        divide(self.bounds(), self.scale_factor())
    }
    fn lmouse(&self) -> (f32, f32) {
        let s = self.scale_factor();
        (self.mouse().cursor_x / s, self.mouse().cursor_y / s)
    }
}

impl Logical for EventContext<'_> {
    fn lbounds(&self) -> BoundingBox {
        divide(self.bounds(), self.scale_factor())
    }
    fn lmouse(&self) -> (f32, f32) {
        let s = self.scale_factor();
        (self.mouse().cursor_x / s, self.mouse().cursor_y / s)
    }
}

/// A physical coordinate from an event (e.g. `MouseMove`'s x/y) in points.
pub fn l(cx: &EventContext, physical: f32) -> f32 {
    physical / cx.scale_factor()
}
