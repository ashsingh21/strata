//! IBM Plex Sans, Strata's one typeface, bundled from `design/fonts/`:
//! registered with Vizia at startup for stylesheet text, and used
//! directly for canvas-drawn text (the timeline ruler's bar numbers,
//! marker labels, clip names, pad labels) - `skia_safe::Font::default()`
//! carries a null typeface that renders no glyphs on some platforms.

use vizia::vg;

pub const PLEX_SANS_REGULAR: &[u8] = include_bytes!("../../design/fonts/IBMPlexSans-Regular.ttf");
pub const PLEX_SANS_MEDIUM: &[u8] = include_bytes!("../../design/fonts/IBMPlexSans-Medium.ttf");
pub const PLEX_SANS_SEMIBOLD: &[u8] = include_bytes!("../../design/fonts/IBMPlexSans-SemiBold.ttf");

pub const PLEX_SANS: [&[u8]; 3] = [PLEX_SANS_REGULAR, PLEX_SANS_MEDIUM, PLEX_SANS_SEMIBOLD];

thread_local! {
    /// Built once per (UI) thread: parsing the font data on every draw
    /// call would be wasted work at 60 fps.
    static CANVAS_TYPEFACE: Option<vg::Typeface> = vg::FontMgr::default()
        .new_from_data(PLEX_SANS_MEDIUM, None)
        .or_else(|| vg::FontMgr::default().legacy_make_typeface(None, vg::FontStyle::default()));
}

pub fn canvas_font(size: f32) -> vg::Font {
    let mut font = CANVAS_TYPEFACE.with(|typeface| match typeface {
        Some(typeface) => vg::Font::new(typeface.clone(), size),
        None => vg::Font::default(),
    });
    font.set_size(size);
    font
}
