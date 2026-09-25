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
/// Linear amplitude above which the clip LED latches (~-0.3 dBFS).
const CLIP_THRESHOLD: f32 = 0.965;
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

    // The one mixer strip.
    pub fader: Signal<f32>,
    pub pan: Signal<f32>,
    pub mute: Signal<bool>,
    pub solo: Signal<bool>,
    pub gain_db: Signal<f32>,
    pub meter_level_l: Signal<f32>,
    pub meter_level_r: Signal<f32>,
    pub meter_clip_l: Signal<bool>,
    pub meter_clip_r: Signal<bool>,
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
    TogglePlay,
    Stop,
    ToggleLoop,
    ToggleArm,
    ToggleClick,
    SetFader(f32),
    SetPan(f32),
    /// Mirrors TimelineEvent::SetTempo into the engine's own Params, so the
    /// click/position stay in sync with the arrangement's tempo map. Two
    /// separate events because AppData and TimelineState each own one half
    /// of what "the current tempo" means: the engine-facing atomic vs. the
    /// undoable arrangement data.
    SetBpm(f64),
    ToggleMute,
    ToggleSolo,
    ResetClip,
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
            fader: Signal::new(0.75),
            pan: Signal::new(0.5),
            mute: Signal::new(false),
            solo: Signal::new(false),
            gain_db: Signal::new(0.0),
            meter_level_l: Signal::new(0.0),
            meter_level_r: Signal::new(0.0),
            meter_clip_l: Signal::new(false),
            meter_clip_r: Signal::new(false),
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

/// Linear fader position (0..1) to gain, with unity (0 dB) at 0.75 and a
/// steep tail down to silence, matching typical DAW fader taper.
pub fn fader_to_gain(position: f32) -> f32 {
    let position = position.clamp(0.0, 1.0);
    if position <= 0.0 {
        return 0.0;
    }
    let db = if position >= 0.75 {
        // 0.75..1.0 maps to 0..+6 dB.
        (position - 0.75) / 0.25 * 6.0
    } else {
        // 0.0..0.75 maps to -inf..0 dB.
        (position / 0.75 - 1.0) * 60.0
    };
    10f32.powf(db / 20.0)
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
            AppEvent::TogglePlay => {
                let now_playing = !self.playing.get();
                self.playing.set(now_playing);
                self.params.set_playing(now_playing);
            }
            AppEvent::Stop => {
                self.playing.set(false);
                self.params.request_stop();
                self.position.set(Position::default());
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
            AppEvent::SetFader(value) => {
                self.fader.set(*value);
                let gain = fader_to_gain(*value);
                self.gain_db.set(gain_to_db(gain));
                self.params.set_gain(gain);
            }
            AppEvent::SetPan(value) => {
                self.pan.set(*value);
                self.params.set_pan(value * 2.0 - 1.0);
            }
            AppEvent::SetBpm(bpm) => {
                self.params.set_bpm(*bpm);
            }
            AppEvent::ToggleMute => {
                self.mute.update(|v| *v = !*v);
                let gain = if self.mute.get() { 0.0 } else { fader_to_gain(self.fader.get()) };
                self.params.set_gain(gain);
            }
            AppEvent::ToggleSolo => {
                self.solo.update(|v| *v = !*v);
            }
            AppEvent::ResetClip => {
                self.meter_clip_l.set(false);
                self.meter_clip_r.set(false);
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
        while let Ok(Telemetry { peak_l: l, peak_r: r, position }) = self.telemetry.pop() {
            peak_l = peak_l.max(l);
            peak_r = peak_r.max(r);
            latest_position = Some(position);
        }
        if let Some(position) = latest_position {
            self.position.set(position);
        }

        let decay = METER_DECAY_DB_PER_SEC * dt;
        let target_l = gain_to_db(peak_l).max(METER_FLOOR_DB);
        let target_r = gain_to_db(peak_r).max(METER_FLOOR_DB);
        self.meter_db_l = if target_l > self.meter_db_l { target_l } else { (self.meter_db_l - decay).max(target_l) };
        self.meter_db_r = if target_r > self.meter_db_r { target_r } else { (self.meter_db_r - decay).max(target_r) };
        self.meter_level_l.set(db_to_meter_fraction(self.meter_db_l));
        self.meter_level_r.set(db_to_meter_fraction(self.meter_db_r));
        let _ = HOT_THRESHOLD;

        if peak_l >= CLIP_THRESHOLD {
            self.meter_clip_l.set(true);
        }
        if peak_r >= CLIP_THRESHOLD {
            self.meter_clip_r.set(true);
        }

        // Animate the Cutoff demo knob's modulation ring centre.
        self.lfo_phase = (self.lfo_phase + LFO_RATE_HZ * std::f32::consts::TAU * dt) % std::f32::consts::TAU;
        let base = self.cutoff.get();
        let depth = self.cutoff_mod_depth.get();
        let center = (base + self.lfo_phase.sin() * depth).clamp(0.0, 1.0);
        self.cutoff_mod_center.set(center);
    }
}
