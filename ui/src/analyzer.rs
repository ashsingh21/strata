//! The live spectrum analyzer: what's coming out of the speakers, pitch
//! by pitch, as it plays - a strip above the timeline, opened from the
//! header's Spectrum button. The engine copies its output into a ring
//! (`PreviewEnds::analyzer_tx`); this drains it every frame and, while the
//! strip is open, analyses the latest slice with the same maths as Sound
//! match (`shared::analysis`).

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

use vizia::prelude::*;

use shared::analysis::{spectrum, Analysis, FLOOR_DB};

use crate::lessons::match_view::{Graph, MatchGraph};
use crate::tokens::{self, ThemeId};

/// Samples analysed at once (about 85 ms at 48 kHz).
const WINDOW: usize = 4096;
/// How the display follows the sound, like a hardware analyzer's meters:
/// it eases up quickly (so hits still show) and falls back slowly (so it
/// can be read) - never jumping, which read as flicker. Time constants,
/// in seconds.
const RISE: f32 = 0.02;
const FALL: f32 = 0.3;

pub enum AnalyzerEvent {
    Toggle,
    Tick,
    /// Hold the picture still, or let it move again.
    ToggleFreeze,
    /// Start (or stop) building an average of everything that plays.
    ToggleAverage,
}

/// The analysis behind the strip, apart from the UI so it can be tested:
/// samples in, the spectrum to draw out, once a frame.
pub struct LiveSpectrum {
    recent: VecDeque<f32>,
    sample_rate: u32,
    shown: Vec<f32>,
    /// Power summed per band since averaging began, and how many frames.
    sum: Vec<f64>,
    frames: u32,
}

/// Below this (dB, in the loudest band) a frame is silence and isn't
/// averaged - stopping playback shouldn't drag the average down.
const AUDIBLE_DB: f32 = -70.0;

impl LiveSpectrum {
    pub fn new(sample_rate: u32) -> Self {
        Self { recent: VecDeque::with_capacity(WINDOW), sample_rate, shown: Vec::new(), sum: Vec::new(), frames: 0 }
    }

    /// Adds output samples, keeping the latest window.
    pub fn push(&mut self, samples: impl IntoIterator<Item = f32>) {
        self.recent.extend(samples);
        let excess = self.recent.len().saturating_sub(WINDOW);
        self.recent.drain(..excess);
    }

    pub fn clear(&mut self) {
        self.recent.clear();
        self.shown.clear();
        self.reset_average();
    }

    pub fn reset_average(&mut self) {
        self.sum.clear();
        self.frames = 0;
    }

    /// The average spectrum of every audible frame since the last reset.
    pub fn average(&self) -> Option<Vec<f32>> {
        (self.frames > 0).then(|| self.sum.iter().map(|&p| (10.0 * (p / self.frames as f64).max(1e-12).log10() as f32).max(FLOOR_DB)).collect())
    }

    /// Advances the display by `dt` seconds; the spectrum to show, once
    /// there's enough sound to analyse.
    pub fn frame(&mut self, dt: f32) -> Option<&[f32]> {
        if self.recent.len() < WINDOW {
            return None;
        }
        let samples: Vec<f32> = self.recent.iter().copied().collect();
        let seconds = WINDOW as f32 / self.sample_rate as f32;
        let now = blend_neighbours(&spectrum(&samples, self.sample_rate, 0.0, seconds));
        if now.iter().copied().fold(FLOOR_DB, f32::max) > AUDIBLE_DB {
            self.sum.resize(now.len(), 0.0);
            for (sum, db) in self.sum.iter_mut().zip(&now) {
                *sum += 10f64.powf(*db as f64 / 10.0);
            }
            self.frames += 1;
        }
        if self.shown.len() != now.len() {
            self.shown = now;
        } else {
            let (up, down) = (1.0 - (-dt / RISE).exp(), 1.0 - (-dt / FALL).exp());
            for (shown, &target) in self.shown.iter_mut().zip(&now) {
                let k = if target > *shown { up } else { down };
                *shown = (*shown + (target - *shown) * k).max(FLOOR_DB);
            }
        }
        Some(&self.shown)
    }
}

/// Each band mixed with its neighbours (1-2-1, in power): at the low end
/// the bands are narrower than the FFT's resolution, so neighbours
/// flickered between catching a bin and missing it.
fn blend_neighbours(db: &[f32]) -> Vec<f32> {
    let power: Vec<f32> = db.iter().map(|d| 10f32.powf(d / 10.0)).collect();
    (0..power.len())
        .map(|i| {
            let left = power[i.saturating_sub(1)];
            let right = power[(i + 1).min(power.len() - 1)];
            let p = 0.25 * left + 0.5 * power[i] + 0.25 * right;
            (10.0 * p.max(1e-12).log10()).max(FLOOR_DB)
        })
        .collect()
}

pub struct AnalyzerModel {
    pub open: Signal<bool>,
    pub frozen: Signal<bool>,
    pub averaging: Signal<bool>,
    /// What the strip draws filled (the live spectrum, or while averaging
    /// the average), and as a line on top (the live one, while averaging).
    pub fill: Signal<Option<Arc<Analysis>>>,
    pub line: Signal<Option<Arc<Analysis>>>,
    tap: rtrb::Consumer<f32>,
    spectrum: LiveSpectrum,
    last: Instant,
}

impl AnalyzerModel {
    pub fn new(tap: rtrb::Consumer<f32>, sample_rate: u32) -> Self {
        Self {
            open: Signal::new(crate::settings::load_analyzer_open()),
            frozen: Signal::new(false),
            averaging: Signal::new(false),
            fill: Signal::new(None),
            line: Signal::new(None),
            tap,
            spectrum: LiveSpectrum::new(sample_rate),
            last: Instant::now(),
        }
    }

    /// Takes everything the engine has written (keeping it only while the
    /// strip is open).
    fn drain(&mut self) {
        let Ok(chunk) = self.tap.read_chunk(self.tap.slots()) else { return };
        if self.open.get() {
            let (a, b) = chunk.as_slices();
            self.spectrum.push(a.iter().chain(b).copied());
        }
        chunk.commit_all();
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
                    self.spectrum.clear();
                    self.fill.set(None);
                    self.line.set(None);
                    self.frozen.set(false);
                    self.averaging.set(false);
                }
            }
            AnalyzerEvent::ToggleFreeze => self.frozen.set(!self.frozen.get()),
            AnalyzerEvent::ToggleAverage => {
                let on = !self.averaging.get();
                self.averaging.set(on);
                self.spectrum.reset_average();
                if !on {
                    self.line.set(None);
                }
            }
            AnalyzerEvent::Tick => {
                let dt = self.last.elapsed().as_secs_f32().min(0.1);
                self.last = Instant::now();
                self.drain();
                if self.open.get() && !self.frozen.get() {
                    let wrap = |spectrum: Vec<f32>| Some(Arc::new(Analysis { spectrum, envelope: vec![] }));
                    if let Some(shown) = self.spectrum.frame(dt).map(<[f32]>::to_vec) {
                        if self.averaging.get() {
                            self.line.set(wrap(shown));
                            if let Some(average) = self.spectrum.average() {
                                self.fill.set(wrap(average));
                            }
                        } else {
                            self.fill.set(wrap(shown));
                        }
                    }
                }
            }
        });
    }
}

/// The strip: hidden while the analyzer is closed.
/// The signals the strip draws from.
#[derive(Clone, Copy)]
pub struct AnalyzerProps {
    open: Signal<bool>,
    frozen: Signal<bool>,
    averaging: Signal<bool>,
    fill: Signal<Option<Arc<Analysis>>>,
    line: Signal<Option<Arc<Analysis>>>,
}

impl AnalyzerProps {
    pub fn of(a: &AnalyzerModel) -> Self {
        Self { open: a.open, frozen: a.frozen, averaging: a.averaging, fill: a.fill, line: a.line }
    }
}

pub fn analyzer_strip(cx: &mut Context, p: AnalyzerProps, theme: Signal<ThemeId>) {
    let AnalyzerProps { open, frozen, averaging, fill, line } = p;
    HStack::new(cx, move |cx| {
        MatchGraph::new(cx, Graph::Live, fill, line, theme).width(Stretch(1.0)).height(Stretch(1.0));
        VStack::new(cx, move |cx| {
            // A still picture to study: hold this moment, or the average
            // of everything played since pressing Average.
            Button::new(cx, |cx| Label::new(cx, "Freeze"))
                .class("btn")
                .toggle_class("is-on", frozen)
                .width(Stretch(1.0))
                .on_press(|cx| cx.emit(AnalyzerEvent::ToggleFreeze));
            Button::new(cx, |cx| Label::new(cx, "Average"))
                .class("btn")
                .toggle_class("is-on", averaging)
                .width(Stretch(1.0))
                .on_press(|cx| cx.emit(AnalyzerEvent::ToggleAverage));
        })
        .gap(Pixels(4.0))
        .width(Pixels(84.0))
        .height(Auto);
    })
    .gap(Pixels(tokens::SPACE_3))
    .class("lesson-bar")
    .toggle_class("hidden", open.map(|o| !*o))
    .padding(Pixels(tokens::SPACE_2))
    .padding_left(Pixels(tokens::SPACE_3))
    .padding_right(Pixels(tokens::SPACE_3))
    .width(Stretch(1.0))
    .height(Pixels(120.0));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A held chord (three saws, A2 E3 A3) - a sound whose spectrum isn't
    /// changing - as 60 fps worth of samples at a time.
    fn steady_chord(seconds: f32) -> Vec<f32> {
        let sr = 48_000.0;
        (0..(sr * seconds) as usize)
            .map(|i| {
                let t = i as f32 / sr;
                [110.0f32, 164.8, 220.0].iter().map(|hz| 0.2 * (2.0 * (t * hz).fract() - 1.0)).sum::<f32>()
            })
            .collect()
    }

    /// How the display moves from one frame to the next for a sound that
    /// isn't changing - pure jitter: (average move, dB; the 95th-percentile
    /// move of a band in a frame - the jumps the eye catches).
    fn jitter(spectrum: &mut LiveSpectrum, samples: &[f32]) -> (f32, f32) {
        let per_frame = 800;
        let mut prev: Option<Vec<f32>> = None;
        let mut moves = Vec::new();
        for (i, chunk) in samples.chunks(per_frame).enumerate() {
            spectrum.push(chunk.iter().copied());
            let Some(shown) = spectrum.frame(1.0 / 60.0).map(|s| s.to_vec()) else { continue };
            // Past the first second: settled.
            if let (Some(p), true) = (&prev, i >= 60) {
                moves.extend(shown.iter().zip(p).filter(|(a, b)| a.max(**b) > -60.0).map(|(a, b)| (a - b).abs()));
            }
            prev = Some(shown);
        }
        moves.sort_by(f32::total_cmp);
        let mean = moves.iter().sum::<f32>() / moves.len().max(1) as f32;
        (mean, moves.get(moves.len() * 95 / 100).copied().unwrap_or(0.0))
    }

    #[test]
    fn a_steady_sound_holds_still() {
        // Measured before smoothing: 0.37 dB on average, jumps of 2 dB.
        let (mean, p95) = jitter(&mut LiveSpectrum::new(48_000), &steady_chord(4.0));
        assert!(mean < 0.15 && p95 < 0.5, "jitter: mean {mean:.2} dB, p95 {p95:.2} dB per frame");
    }

    #[test]
    fn it_still_answers_quickly() {
        // Silence, then the chord: within a tenth of a second the display
        // is near where it settles.
        let mut live = LiveSpectrum::new(48_000);
        let feed = |live: &mut LiveSpectrum, samples: &[f32]| {
            let mut last = Vec::new();
            for chunk in samples.chunks(800) {
                live.push(chunk.iter().copied());
                if let Some(shown) = live.frame(1.0 / 60.0) {
                    last = shown.to_vec();
                }
            }
            last
        };
        feed(&mut live, &vec![0.0; 48_000]);
        let chord = steady_chord(2.0);
        let early = feed(&mut live, &chord[..4_800]);
        let settled = feed(&mut live, &chord[4_800..]);
        let loudest = (0..settled.len()).max_by(|&a, &b| settled[a].total_cmp(&settled[b])).unwrap();
        assert!(settled[loudest] - early[loudest] < 3.0, "{} dB behind after 0.1 s", settled[loudest] - early[loudest]);
    }

    #[test]
    fn the_average_ignores_silence() {
        let mut live = LiveSpectrum::new(48_000);
        let chord = steady_chord(2.0);
        for chunk in chord.chunks(800) {
            live.push(chunk.iter().copied());
            live.frame(1.0 / 60.0);
        }
        let before = live.average().unwrap();
        // Two seconds of silence (playback stopped) changes nothing.
        for chunk in vec![0.0; 96_000].chunks(800) {
            live.push(chunk.iter().copied());
            live.frame(1.0 / 60.0);
        }
        let after = live.average().unwrap();
        let loudest = (0..after.len()).max_by(|&a, &b| after[a].total_cmp(&after[b])).unwrap();
        assert!((after[loudest] - before[loudest]).abs() < 0.5);
        assert!(after[loudest] > -40.0);
    }
}