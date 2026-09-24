//! `skia_safe::Font::default()` carries a null typeface, which renders no
//! glyphs at all on some platforms. This resolves the system's default
//! typeface once via `FontMgr` so canvas-drawn text (the timeline ruler's
//! bar numbers, marker labels, clip names) actually shows up.

use vizia::vg;

pub fn canvas_font(size: f32) -> vg::Font {
    let typeface = vg::FontMgr::default().legacy_make_typeface(None, vg::FontStyle::default());
    let mut font = match typeface {
        Some(typeface) => vg::Font::new(typeface, size),
        None => vg::Font::default(),
    };
    font.set_size(size);
    font
}
