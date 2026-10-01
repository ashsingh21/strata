//! The recording coordinator: watches `record_armed` + `playing` + an
//! armed audio track every render tick, drives the engine's writer
//! thread via `RecordCommand`, and - once the take stops - turns it into
//! a real timeline clip via `TimelineEvent::InsertRecordedClip`, which
//! goes through the same undo-aware `Command::InsertClip` as any other
//! clip insertion. While a take is in progress it's a transient preview
//! only (see `RecordingPreview`), never a real `Arrangement` clip, so
//! the arrangement/undo model stays exactly as pure as its own doc
//! comment claims ("mutated only through Command").

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use vizia::prelude::*;

use shared::arrangement::{Arrangement, Ticks, TrackId, TrackKind};
use shared::recorder::{InputTelemetry, RecordCommand, RecordParams};

use crate::app::{db_to_meter_fraction, gain_to_db, METER_DECAY_DB_PER_SEC, METER_FLOOR_DB};
use crate::timeline::state::TimelineEvent;

/// Input gain knob's normalized 0..1 position maps linearly to
/// -24..+24 dB, with 0.5 -> unity.
const INPUT_GAIN_RANGE_DB: f32 = 24.0;

pub fn input_gain_pos_to_db(pos: f32) -> f32 {
    (pos.clamp(0.0, 1.0) - 0.5) * 2.0 * INPUT_GAIN_RANGE_DB
}

/// What's currently being recorded, for the timeline to draw as a
/// growing clip in progress.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RecordingPreview {
    pub track: TrackId,
    pub start: Ticks,
    pub length: Ticks,
}

pub struct RecorderModel {
    pub preview: Signal<Option<RecordingPreview>>,
    /// 0..1 meter fill fraction, for the input level meter.
    pub input_level: Signal<f32>,
    /// 0..1 knob position; see `input_gain_pos_to_db`.
    pub input_gain_pos: Signal<f32>,
    /// Whether the input is heard live through the armed track's effects.
    /// Off at launch: with speakers, a live mic feeds back.
    pub monitoring: Signal<bool>,
    /// Per-block abs-peak of the current take so far, in capture order -
    /// the same numbers the input meter already gets from `InputTelemetry`,
    /// just kept instead of discarded, so the in-progress clip can draw a
    /// live envelope instead of sitting flat until the take is decoded.
    /// Empty whenever nothing's being recorded.
    pub live_peaks: Signal<Arc<[f32]>>,
    /// The chosen input device name, or `None` for the OS default. Saved
    /// (`settings::save_input_device`) and switched to immediately.
    pub selected_input_device: Signal<Option<Arc<str>>>,
    /// Every input device the host can see, for the picker - listed at
    /// startup and again each time the picker opens (so an interface
    /// plugged in later shows up).
    pub available_input_devices: Signal<Arc<[Arc<str>]>>,
    record_params: Arc<RecordParams>,
}

#[allow(clippy::enum_variant_names)]
pub enum RecorderModelEvent {
    SetPreview(Option<RecordingPreview>),
    SetInputLevel(f32),
    SetInputGain(f32),
    SetMonitoring(bool),
    SetLivePeaks(Arc<[f32]>),
    /// `None` reverts to "whatever the OS calls default".
    SetInputDevice(Option<Arc<str>>),
    /// Re-list the input devices (the picker is opening).
    RefreshInputDevices,
}

impl RecorderModel {
    pub fn new(record_params: Arc<RecordParams>) -> Self {
        Self {
            preview: Signal::new(None),
            input_level: Signal::new(0.0),
            input_gain_pos: Signal::new(0.5),
            monitoring: Signal::new(false),
            live_peaks: Signal::new(Arc::from([])),
            selected_input_device: Signal::new(crate::settings::load_input_device().map(Into::into)),
            available_input_devices: Signal::new(
                engine::input::available_input_devices().into_iter().map(Arc::<str>::from).collect(),
            ),
            record_params,
        }
    }
}

impl Model for RecorderModel {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|event, _| match event {
            RecorderModelEvent::SetPreview(preview) => self.preview.set(*preview),
            RecorderModelEvent::SetInputLevel(level) => self.input_level.set(*level),
            RecorderModelEvent::SetInputGain(pos) => {
                if let Some(skipped) = shared::diag::throttle("input-gain", std::time::Duration::from_millis(300)) {
                    tracing::debug!(target: "action", skipped, "input gain: {:+.1} dB", input_gain_pos_to_db(*pos));
                }
                self.input_gain_pos.set(*pos);
                self.record_params.set_input_gain_db(input_gain_pos_to_db(*pos));
            }
            RecorderModelEvent::SetMonitoring(on) => {
                tracing::debug!(target: "action", "monitor: {}", if *on { "on" } else { "off" });
                self.monitoring.set(*on);
                self.record_params.set_monitoring(*on);
            }
            RecorderModelEvent::SetLivePeaks(peaks) => self.live_peaks.set(peaks.clone()),
            RecorderModelEvent::SetInputDevice(device) => {
                tracing::debug!(target: "action", "input device: {}", device.as_deref().unwrap_or("system default"));
                crate::settings::save_input_device(device.as_deref());
                self.selected_input_device.set(device.clone());
                cx.emit(crate::app::AppEvent::SwitchInputDevice(device.clone()));
            }
            RecorderModelEvent::RefreshInputDevices => {
                self.available_input_devices
                    .set(engine::input::available_input_devices().into_iter().map(Arc::<str>::from).collect());
            }
        });
    }
}

#[derive(Clone, Copy)]
struct ActiveRecording {
    track: TrackId,
    start: Ticks,
    /// The playhead as of the last tick recording was confirmed still
    /// active. Used instead of the current tick to compute the finished
    /// length on the falling edge, since by then `Stop` (a separate,
    /// synchronous button-press event, not this timer) has typically
    /// already reset the playhead to 0 - using the live tick there would
    /// silently produce a ~zero-length clip.
    last_tick: Ticks,
    /// Index into a session-local counter rather than the source name
    /// itself, so the coordinator doesn't need to carry an `Arc<str>`
    /// around just to compare it against itself.
    source: usize,
}

/// Drives recording from the render timer, the same shape as
/// `timeline::scheduler::MidiScheduler`.
pub struct RecordingCoordinator {
    active: Cell<Option<ActiveRecording>>,
    take_counter: Cell<usize>,
    input_meter_db: Cell<f32>,
    /// Mirrors `RecorderModel::live_peaks`; kept here too since a `Signal`
    /// can only be replaced wholesale, not appended to; rebuilt into an
    /// `Arc` and pushed out each frame a take is active.
    live_peaks: RefCell<Vec<f32>>,
}

impl RecordingCoordinator {
    pub fn new() -> Self {
        Self {
            active: Cell::new(None),
            take_counter: Cell::new(0),
            input_meter_db: Cell::new(METER_FLOOR_DB),
            live_peaks: RefCell::new(Vec::new()),
        }
    }

    /// Call once per frame, independent of `advance`: drains the input
    /// device's peak telemetry (reported continuously, whether or not
    /// anything's armed, so gain can be staged first) and updates the
    /// input meter's fill fraction, with the same decay ballistics as
    /// the master meter in `app.rs`. While a take is active, also
    /// accumulates each block's peak as a rough live waveform for the
    /// in-progress clip - see `RecorderModel::live_peaks`.
    pub fn drain_input_meter(
        &self,
        cx: &mut EventContext,
        telemetry_rx: &RefCell<rtrb::Consumer<InputTelemetry>>,
        dt: f32,
    ) {
        let recording = self.active.get().is_some();
        let mut peak = 0.0f32;
        let mut got_any = false;
        while let Ok(InputTelemetry { peak: p }) = telemetry_rx.borrow_mut().pop() {
            peak = peak.max(p);
            if recording {
                got_any = true;
                self.live_peaks.borrow_mut().push(p);
            }
        }
        let target_db = gain_to_db(peak).max(METER_FLOOR_DB);
        let decay = METER_DECAY_DB_PER_SEC * dt;
        let db = self.input_meter_db.get();
        let db = if target_db > db { target_db } else { (db - decay).max(target_db) };
        self.input_meter_db.set(db);
        cx.emit(RecorderModelEvent::SetInputLevel(db_to_meter_fraction(db)));
        if got_any {
            cx.emit(RecorderModelEvent::SetLivePeaks(Arc::from(self.live_peaks.borrow().as_slice())));
        }
    }

    /// Call once per frame. `command_tx` sends Start/Stop to the engine's
    /// writer thread; `decode_request_tx` asks the persistent decode
    /// worker (see `timeline::peaks_loader::spawn_audio_decoder_worker`)
    /// to decode a just-finished take so it's audible without a restart.
    #[allow(clippy::too_many_arguments)]
    pub fn advance(
        &self,
        cx: &mut EventContext,
        arrangement: &Arrangement,
        armed: bool,
        playing: bool,
        tick: Ticks,
        command_tx: &RefCell<rtrb::Producer<RecordCommand>>,
        decode_request_tx: &std::sync::mpsc::Sender<Arc<str>>,
    ) {
        let armed_track = arrangement.tracks.iter().find(|t| t.arm && t.kind == TrackKind::Audio).map(|t| t.id);
        let should_record = armed && playing && armed_track.is_some();

        match (self.active.get(), should_record) {
            (None, true) => {
                let track = armed_track.expect("should_record implies armed_track.is_some()");
                // The counter starts at 0 each launch: skip names an
                // earlier session's takes already use, or they'd be
                // written over.
                let mut take = self.take_counter.get();
                while crate::paths::recordings_dir().join(format!("rec_{track}_{take}.wav")).exists() {
                    take += 1;
                }
                self.take_counter.set(take + 1);
                let source: Arc<str> = format!("rec_{track}_{take}.wav").into();
                let path = crate::paths::recordings_dir().join(&*source);
                tracing::debug!(target: "action", "recording: start");
                let _ = command_tx.borrow_mut().push(RecordCommand::Start { path });
                self.active.set(Some(ActiveRecording { track, start: tick, last_tick: tick, source: take }));
                self.live_peaks.borrow_mut().clear();
                cx.emit(RecorderModelEvent::SetLivePeaks(Arc::from([])));
                cx.emit(RecorderModelEvent::SetPreview(Some(RecordingPreview { track, start: tick, length: 1 })));
            }
            (Some(rec), true) => {
                self.active.set(Some(ActiveRecording { last_tick: tick, ..rec }));
                cx.emit(RecorderModelEvent::SetPreview(Some(RecordingPreview {
                    track: rec.track,
                    start: rec.start,
                    length: (tick - rec.start).max(1),
                })));
            }
            (Some(rec), false) => {
                tracing::debug!(target: "action", "recording: stop");
                let _ = command_tx.borrow_mut().push(RecordCommand::Stop);
                self.active.set(None);
                cx.emit(RecorderModelEvent::SetPreview(None));

                let length = (rec.last_tick - rec.start).max(1);
                let source: Arc<str> = format!("rec_{}_{}.wav", rec.track, rec.source).into();
                cx.emit(TimelineEvent::InsertRecordedClip {
                    track: rec.track,
                    start: rec.start,
                    length,
                    source: source.clone(),
                });
                let _ = decode_request_tx.send(source);
            }
            (None, false) => {}
        }
    }
}

impl Default for RecordingCoordinator {
    fn default() -> Self {
        Self::new()
    }
}
