//! The app's central `Model`: owns every `Signal` the views bind to, plus
//! the non-reactive engine-side resources (the params bridge, the telemetry
//! consumer and the live `EngineHandle`). Its `event()` is the only place
//! that mutates state in response to UI interaction or the 60fps timer tick.

use std::sync::Arc;
use std::time::Instant;

use vizia::prelude::*;

use engine::EngineHandle;
use shared::{Params, Position, Telemetry};

use crate::meter::HOT_THRESHOLD;
use crate::tokens::ThemeId;

/// -60 dBFS floor for the meter's dB-to-fraction mapping. `pub(crate)`
/// alongside the two helpers below: reused by `recorder`'s input meter
/// ballistics rather than duplicating this mapping.
pub(crate) const METER_FLOOR_DB: f32 = -60.0;
/// Release rate for meter ballistics.
pub(crate) const METER_DECAY_DB_PER_SEC: f32 = 20.0;
/// LFO rate for the Cutoff demo knob's modulation ring.
const LFO_RATE_HZ: f32 = 0.5;

pub struct AppData {
    pub theme: Signal<ThemeId>,

    // Transport.
    pub playing: Signal<bool>,
    pub loop_on: Signal<bool>,
    pub record_armed: Signal<bool>,
    pub click_on: Signal<bool>,
    pub position: Signal<Position>,
    /// The engine's exact running sample count as of the latest telemetry
    /// frame - sample-accurate, unlike `position` (quantized to the
    /// nearest 16th note). `main.rs`'s render timer converts this through
    /// `TempoMap::samples_to_ticks` to drive the playhead smoothly instead
    /// of visibly stepping once per 16th note.
    pub sample_counter: Signal<u64>,
    pub sidebar_open: Signal<bool>,

    // Engine status for the header and status bar.
    /// Audio-callback CPU load, 0..1 (the peak over the last ~half second,
    /// so short spikes stay visible).
    pub cpu_load: Signal<f32>,
    /// The device's buffer size in frames (0 until the first block).
    pub block_frames: Signal<u32>,
    pub sample_rate: u32,
    /// Master output peak in dBFS, with meter ballistics.
    pub output_db: Signal<f32>,
    cpu_peak: f32,
    cpu_peak_age: f32,
    taps: Vec<Instant>,
    meter_db_l: f32,
    meter_db_r: f32,

    // LFO demo.
    pub cutoff: Signal<f32>,
    pub cutoff_mod_center: Signal<f32>,
    pub cutoff_mod_depth: Signal<f32>,
    lfo_phase: f32,

    // Engine bridge (not reactive).
    params: Arc<Params>,
    telemetry: rtrb::Consumer<Telemetry>,
    last_tick: Instant,
    _engine: EngineHandle,
}

#[derive(Debug)]
pub enum AppEvent {
    ToggleTheme,
    ToggleSidebar,
    TogglePlay,
    Stop,
    /// Back to 1.1.1 without stopping.
    Rewind,
    /// One tap of tap-tempo.
    Tap,
    ToggleLoop,
    ToggleArm,
    ToggleClick,
    /// Mirrors TimelineEvent::SetTempo into the engine's own Params, so the
    /// click/position stay in sync with the arrangement's tempo map. Two
    /// separate events because AppData and TimelineState each own one half
    /// of what "the current tempo" means: the engine-facing atomic vs. the
    /// undoable arrangement data.
    SetBpm(f64),
    /// Only emitted by the LFO demo, which isn't currently mounted.
    #[allow(dead_code)]
    SetCutoff(f32),
    Tick,
}

impl AppData {
    pub fn new(
        params: Arc<Params>,
        telemetry: rtrb::Consumer<Telemetry>,
        engine: EngineHandle,
    ) -> Self {
        Self {
            theme: Signal::new(ThemeId::Studio),
            playing: Signal::new(false),
            loop_on: Signal::new(false),
            record_armed: Signal::new(false),
            click_on: Signal::new(false),
            position: Signal::new(Position::default()),
            sample_counter: Signal::new(0),
            sidebar_open: Signal::new(true),
            cpu_load: Signal::new(0.0),
            block_frames: Signal::new(0),
            sample_rate: engine.sample_rate,
            output_db: Signal::new(METER_FLOOR_DB),
            cpu_peak: 0.0,
            cpu_peak_age: 0.0,
            taps: Vec::with_capacity(8),
            meter_db_l: METER_FLOOR_DB,
            meter_db_r: METER_FLOOR_DB,
            cutoff: Signal::new(0.45),
            cutoff_mod_center: Signal::new(0.45),
            cutoff_mod_depth: Signal::new(0.15),
            lfo_phase: 0.0,
            params,
            telemetry,
            last_tick: Instant::now(),
            _engine: engine,
        }
    }
}

pub(crate) fn gain_to_db(gain: f32) -> f32 {
    if gain <= 0.0001 { -100.0 } else { 20.0 * gain.log10() }
}

pub(crate) fn db_to_meter_fraction(db: f32) -> f32 {
    ((db - METER_FLOOR_DB) / -METER_FLOOR_DB).clamp(0.0, 1.0)
}

impl Model for AppData {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|app_event, _| match app_event {
            AppEvent::ToggleTheme => {
                self.theme.update(|t| *t = t.toggled());
            }
            AppEvent::ToggleSidebar => self.sidebar_open.update(|o| *o = !*o),
            AppEvent::TogglePlay => {
                let now_playing = !self.playing.get();
                self.playing.set(now_playing);
                self.params.set_playing(now_playing);
            }
            AppEvent::Stop => {
                self.playing.set(false);
                self.params.request_stop();
                self.position.set(Position::default());
                self.sample_counter.set(0);
            }
            AppEvent::Rewind => {
                self.params.request_stop();
                self.position.set(Position::default());
                self.sample_counter.set(0);
            }
            AppEvent::Tap => {
                // Average the last few intervals; a pause over two seconds
                // starts a fresh count.
                let now = Instant::now();
                if self.taps.last().is_some_and(|t| now - *t > std::time::Duration::from_secs(2)) {
                    self.taps.clear();
                }
                self.taps.push(now);
                if self.taps.len() > 5 {
                    self.taps.remove(0);
                }
                if self.taps.len() >= 2 {
                    let span = (*self.taps.last().unwrap() - self.taps[0]).as_secs_f64();
                    let bpm = (60.0 * (self.taps.len() - 1) as f64 / span).clamp(40.0, 300.0);
                    let bpm = (bpm * 100.0).round() / 100.0;
                    self.params.set_bpm(bpm);
                    cx.emit(crate::timeline::state::TimelineEvent::SetTempo(bpm));
                }
            }
            AppEvent::ToggleLoop => {
                self.loop_on.update(|v| *v = !*v);
            }
            AppEvent::ToggleArm => {
                self.record_armed.update(|v| *v = !*v);
            }
            AppEvent::ToggleClick => {
                self.click_on.update(|v| *v = !*v);
                self.params.set_click_enabled(self.click_on.get());
            }
            AppEvent::SetBpm(bpm) => {
                self.params.set_bpm(*bpm);
            }
            AppEvent::SetCutoff(value) => {
                self.cutoff.set(*value);
            }
            AppEvent::Tick => self.tick(cx),
        });
    }
}

impl AppData {
    fn tick(&mut self, _cx: &mut EventContext) {
        let now = Instant::now();
        let dt = (now - self.last_tick).as_secs_f32().min(0.25);
        self.last_tick = now;

        // Drain telemetry, keeping the loudest peak seen since the last
        // tick and the most recent position.
        let mut peak_l = 0.0f32;
        let mut peak_r = 0.0f32;
        let mut latest_position = None;
        let mut latest_sample_counter = None;
        let mut cpu = 0.0f32;
        while let Ok(Telemetry { peak_l: l, peak_r: r, position, sample_counter, cpu_load, block_frames }) = self.telemetry.pop() {
            peak_l = peak_l.max(l);
            peak_r = peak_r.max(r);
            latest_position = Some(position);
            latest_sample_counter = Some(sample_counter);
            cpu = cpu.max(cpu_load);
            if block_frames != self.block_frames.get() {
                self.block_frames.set(block_frames);
            }
        }
        self.cpu_peak_age += dt;
        if cpu >= self.cpu_peak || self.cpu_peak_age > 0.5 {
            self.cpu_peak = cpu;
            self.cpu_peak_age = 0.0;
            self.cpu_load.set(cpu);
        }
        if let Some(position) = latest_position {
            self.position.set(position);
        }
        if let Some(sample_counter) = latest_sample_counter {
            self.sample_counter.set(sample_counter);
        }

        let decay = METER_DECAY_DB_PER_SEC * dt;
        let target_l = gain_to_db(peak_l).max(METER_FLOOR_DB);
        let target_r = gain_to_db(peak_r).max(METER_FLOOR_DB);
        self.meter_db_l = if target_l > self.meter_db_l { target_l } else { (self.meter_db_l - decay).max(target_l) };
        self.meter_db_r = if target_r > self.meter_db_r { target_r } else { (self.meter_db_r - decay).max(target_r) };
        self.output_db.set(self.meter_db_l.max(self.meter_db_r));
        let _ = HOT_THRESHOLD;

        // Animate the Cutoff demo knob's modulation ring centre.
        self.lfo_phase = (self.lfo_phase + LFO_RATE_HZ * std::f32::consts::TAU * dt) % std::f32::consts::TAU;
        let base = self.cutoff.get();
        let depth = self.cutoff_mod_depth.get();
        let center = (base + self.lfo_phase.sin() * depth).clamp(0.0, 1.0);
        self.cutoff_mod_center.set(center);
    }
}
