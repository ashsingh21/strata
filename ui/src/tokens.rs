//! Thin hand-written wrapper around the generated palette
//! (`OUT_DIR/tokens.rs`, produced by `build.rs` from `design/tokens.json`).

// The generated module is the full token set (the single source of truth);
// this milestone's views only need a subset of it.
#![allow(dead_code)]

include!(concat!(env!("OUT_DIR"), "/tokens.rs"));

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeId {
    Studio,
    Daylight,
}

impl ThemeId {
    pub fn palette(self) -> Palette {
        match self {
            ThemeId::Studio => STUDIO,
            ThemeId::Daylight => DAYLIGHT,
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            ThemeId::Studio => ThemeId::Daylight,
            ThemeId::Daylight => ThemeId::Studio,
        }
    }

    /// The CSS class that scopes `styles/daylight.css`'s overrides.
    pub fn is_daylight(self) -> bool {
        matches!(self, ThemeId::Daylight)
    }
}
