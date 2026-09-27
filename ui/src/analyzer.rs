//! The live spectrum analyzer: what's coming out of the speakers, pitch
//! by pitch, as it plays - a strip above the timeline, opened from the
//! header's Spectrum button. The engine copies its output into a ring
//! (`PreviewEnds::analyzer_tx`); this drains it every frame and, while the
//! strip is open, analyses the latest slice with the same maths as Sound
//! match (`shared::analysis`).

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use vizia::prelude::*;

use shared::analysis::{spectrum, Analysis, FLOOR_DB};

use crate::lessons::match_view::{Graph, MatchGraph};
use crate::tokens::{self, ThemeId};

/// Samples analysed at once (about 85 ms at 48 kHz).
const WINDOW: usize = 4096;
/// How often the display updates.
const EVERY: Duration = Duration::from_millis(33);
/// How fast a band falls back after a peak, in dB per update - quick to
/// rise, slow to fall, like a hardware analyzer, so it's readable.
const FALL_DB: f32 = 2.0;

pub enum AnalyzerEvent {
    Toggle,
    Tick,
}

pub struct AnalyzerModel {
    pub open: Signal<bool>,
    /// The latest spectrum (in `Analysis::spectrum`), smoothed.
    pub live: Signal<Option<Arc<Analysis>>>,
    tap: rtrb::Consumer<f32>,
    recent: VecDeque<f32>,
    sample_rate: u32,
    last: Instant,
}

impl AnalyzerModel {
    pub fn new(tap: rtrb::Consumer<f32>, sample_rate: u32) -> Self {
        Self {
            open: Signal::new(crate::settings::load_analyzer_open()),
            live: Signal::new(None),
            tap,
            recent: VecDeque::with_capacity(WINDOW),
            sample_rate,
            last: Instant::now(),
        }
    }

    /// Takes everything the engine has written, keeping the latest window.
    fn drain(&mut self) {
        let Ok(chunk) = self.tap.read_chunk(self.tap.slots()) else { return };
        let open = self.open.get();
        if open {
            let (a, b) = chunk.as_slices();
            self.recent.extend(a.iter().chain(b).copied());
            let excess = self.recent.len().saturating_sub(WINDOW);
            self.recent.drain(..excess);
        }
        chunk.commit_all();
    }

    fn analyse(&mut self) {
        if self.recent.len() < WINDOW {
            return;
        }
        let samples: Vec<f32> = self.recent.iter().copied().collect();
        let seconds = WINDOW as f32 / self.sample_rate as f32;
        let now = spectrum(&samples, self.sample_rate, 0.0, seconds);
        let smoothed = match self.live.get() {
            Some(prev) if prev.spectrum.len() == now.len() => {
                now.iter().zip(&prev.spectrum).map(|(&n, &p)| n.max(p - FALL_DB).max(FLOOR_DB)).collect()
            }
            _ => now,
        };
        self.live.set(Some(Arc::new(Analysis { spectrum: smoothed, envelope: vec![] })));
    }
}

impl Model for AnalyzerModel {
    fn event(&mut self, _cx: &mut EventContext, event: &mut Event) {
        event.map(|event, _| match event {
            AnalyzerEvent::Toggle => {
                let open = !self.open.get();
                self.open.set(open);
                crate::settings::save_analyzer_open(open);
                if !open {
                    self.recent.clear();
                    self.live.set(None);
                }
            }
            AnalyzerEvent::Tick => {
                self.drain();
                if self.open.get() && self.last.elapsed() >= EVERY {
                    self.last = Instant::now();
                    self.analyse();
                }
            }
        });
    }
}

/// The strip: hidden while the analyzer is closed.
pub fn analyzer_strip(cx: &mut Context, open: Signal<bool>, live: Signal<Option<Arc<Analysis>>>, theme: Signal<ThemeId>) {
    HStack::new(cx, move |cx| {
        MatchGraph::new(cx, Graph::Live, live, Signal::new(None), theme).width(Stretch(1.0)).height(Stretch(1.0));
    })
    .class("lesson-bar")
    .toggle_class("hidden", open.map(|o| !*o))
    .padding(Pixels(tokens::SPACE_2))
    .padding_left(Pixels(tokens::SPACE_3))
    .padding_right(Pixels(tokens::SPACE_3))
    .width(Stretch(1.0))
    .height(Pixels(120.0));
}
