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
use shared::recorder::RecordCommand;

use crate::timeline::state::TimelineEvent;

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
}

pub enum RecorderModelEvent {
    SetPreview(Option<RecordingPreview>),
}

impl RecorderModel {
    pub fn new() -> Self {
        Self { preview: Signal::new(None) }
    }
}

impl Default for RecorderModel {
    fn default() -> Self {
        Self::new()
    }
}

impl Model for RecorderModel {
    fn event(&mut self, _cx: &mut EventContext, event: &mut Event) {
        event.map(|event, _| match event {
            RecorderModelEvent::SetPreview(preview) => self.preview.set(*preview),
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
}

impl RecordingCoordinator {
    pub fn new() -> Self {
        Self { active: Cell::new(None), take_counter: Cell::new(0) }
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
                let take = self.take_counter.get();
                self.take_counter.set(take + 1);
                let source: Arc<str> = format!("rec_{track}_{take}.wav").into();
                let path = crate::timeline::assets_dir().join(&*source);
                let _ = command_tx.borrow_mut().push(RecordCommand::Start { path });
                self.active.set(Some(ActiveRecording { track, start: tick, last_tick: tick, source: take }));
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
