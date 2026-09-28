//! Thin hand-written wrapper around the generated palette
//! (`OUT_DIR/tokens.rs`, produced by `build.rs` from `design/tokens.json`).

// The generated module is the full token set (the single source of truth);
// this milestone's views only need a subset of it.
#![allow(dead_code)]

include!(concat!(env!("OUT_DIR"), "/tokens.rs"));

/// The colour themes, as `design/tokens.json` defines them (same ids).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeId {
    Studio,
    Daylight,
    Midnight,
    Contrast,
    Paper,
}

impl ThemeId {
    pub const ALL: [ThemeId; 5] = [ThemeId::Studio, ThemeId::Daylight, ThemeId::Midnight, ThemeId::Contrast, ThemeId::Paper];

    pub fn palette(self) -> Palette {
        match self {
            ThemeId::Studio => STUDIO,
            ThemeId::Daylight => DAYLIGHT,
            ThemeId::Midnight => MIDNIGHT,
            ThemeId::Contrast => CONTRAST,
            ThemeId::Paper => PAPER,
        }
    }

    /// Its id in tokens.json and settings.
    pub fn id(self) -> &'static str {
        match self {
            ThemeId::Studio => "studio",
            ThemeId::Daylight => "daylight",
            ThemeId::Midnight => "midnight",
            ThemeId::Contrast => "contrast",
            ThemeId::Paper => "paper",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.id() == id)
    }

    pub fn name(self) -> &'static str {
        match self {
            ThemeId::Studio => "Studio",
            ThemeId::Daylight => "Daylight",
            ThemeId::Midnight => "Midnight",
            ThemeId::Contrast => "High contrast",
            ThemeId::Paper => "Paper",
        }
    }

    /// The class on the root that switches `styles/themes.css`'s rules
    /// on (Studio, the default, has none).
    pub fn css_class(self) -> Option<String> {
        (self != ThemeId::Studio).then(|| format!("theme-{}", self.id()))
    }

    /// The next theme (Ctrl/Cmd+T cycles through them).
    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
}
