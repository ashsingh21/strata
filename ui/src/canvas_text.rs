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

/// The few symbols the UI uses that Plex doesn't have (menu chevrons, ▸ ■,
/// ♭ ♯, ⌘ ⇧...): a 14-glyph subset of DejaVu Sans, next in the stylesheet's
/// font list. Without it those came out as empty boxes wherever the system
/// has no font with them (seen under Wine).
pub const SHOR_SYMBOLS: &[u8] = include_bytes!("../../design/fonts/ShorSymbols.ttf");

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

/// `text` shortened with a trailing "…" so it fits in `max_width` px at
/// `font` - for canvas labels that must stay inside a box (clip names on
/// narrow clips). Returns `None` when not even "…" fits.
pub fn fit_text(text: &str, font: &vg::Font, max_width: f32) -> Option<String> {
    let width = |s: &str| font.measure_str(s, None).0;
    if width(text) <= max_width {
        return Some(text.to_string());
    }
    let mut end = text.len();
    while end > 0 {
        end = text[..end].char_indices().next_back().map(|(i, _)| i).unwrap_or(0);
        let candidate = format!("{}\u{2026}", text[..end].trim_end());
        if width(&candidate) <= max_width {
            return Some(candidate);
        }
    }
    (width("\u{2026}") <= max_width).then(|| "\u{2026}".to_string())
}
