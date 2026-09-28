//! The one preview player, shared by the lessons ("Hear it") and the
//! browser: sends rendered audio to the engine's preview voice (see
//! `shared::playback::PreviewSound`), frees what it's done with, and
//! says whose preview is playing - starting one stops any other, so each
//! owner checks its token rather than assuming it's still the one heard.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use shared::playback::{PreviewBuffer, PreviewGain, PreviewSender, PreviewSound};

pub type SharedPlayer = Rc<RefCell<PreviewPlayer>>;

struct Playing {
    token: u64,
    started: Instant,
    seconds: f64,
    looping: bool,
}

pub struct PreviewPlayer {
    tx: rtrb::Producer<PreviewBuffer>,
    retired: rtrb::Consumer<PreviewBuffer>,
    gain: PreviewGain,
    sample_rate: u32,
    next_token: u64,
    playing: Option<Playing>,
}

impl PreviewPlayer {
    pub fn new(sender: PreviewSender, sample_rate: u32) -> SharedPlayer {
        Rc::new(RefCell::new(Self {
            tx: sender.play_tx,
            retired: sender.retired_rx,
            gain: sender.gain,
            sample_rate,
            next_token: 1,
            playing: None,
        }))
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Plays `audio` (interleaved stereo at the engine's rate), replacing
    /// whatever is playing. The token says, later, whether it still is.
    pub fn play(&mut self, audio: Vec<f32>, looping: bool) -> Option<u64> {
        let seconds = audio.len() as f64 / 2.0 / self.sample_rate as f64;
        if self.tx.push(Arc::new(PreviewSound { audio, looping })).is_err() {
            return None;
        }
        let token = self.next_token;
        self.next_token += 1;
        self.playing = Some(Playing { token, started: Instant::now(), seconds, looping });
        Some(token)
    }

    pub fn stop(&mut self) {
        if self.playing.take().is_some() {
            let _ = self.tx.push(Arc::new(PreviewSound { audio: Vec::new(), looping: false }));
        }
    }

    /// Stops the preview if it's still `token`'s.
    pub fn stop_if(&mut self, token: u64) {
        if self.current() == Some(token) {
            self.stop();
        }
    }

    /// The preview playing now, if any.
    pub fn current(&self) -> Option<u64> {
        self.playing.as_ref().filter(|p| p.looping || p.started.elapsed().as_secs_f64() < p.seconds).map(|p| p.token)
    }

    /// How far through the current preview (0..1; wraps while looping).
    pub fn progress(&self) -> Option<f32> {
        let p = self.playing.as_ref()?;
        let t = p.started.elapsed().as_secs_f64();
        let f = if p.looping { t % p.seconds.max(1e-6) } else { t.min(p.seconds) } / p.seconds.max(1e-6);
        Some(f as f32)
    }

    pub fn set_gain(&self, gain: f32) {
        self.gain.store(gain.to_bits(), Ordering::Relaxed);
    }

    /// Once a frame: frees buffers the engine has finished with.
    pub fn tick(&mut self) {
        while self.retired.pop().is_ok() {}
        if self.current().is_none() {
            self.playing = None;
        }
    }
}
