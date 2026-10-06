//! The mic, for the tools that listen to you (Riyaz, the ear trainer):
//! the input's samples - sent only while someone is listening - brought
//! down to a pitch per reading. One at a time: the tools open one at a
//! time, and opening one sets listening for itself.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use shared::recorder::RecordParams;
use shared::riyaz::{Listener, Pitch};

pub struct Mic {
    rx: rtrb::Consumer<f32>,
    params: Arc<RecordParams>,
    listener: Listener,
}

pub type SharedMic = Rc<RefCell<Mic>>;

impl Mic {
    pub fn new(rx: rtrb::Consumer<f32>, params: Arc<RecordParams>) -> SharedMic {
        Rc::new(RefCell::new(Self { rx, params, listener: Listener::new(48_000) }))
    }

    /// Whether there's an input to listen to.
    pub fn available(&self) -> bool {
        self.params.input_rate() > 0
    }

    /// Starts or stops the samples coming; returns whether it's listening
    /// (not without an input). Starts fresh either way.
    pub fn set_listening(&mut self, on: bool) -> bool {
        let on = on && self.available();
        self.params.set_listening(on);
        while self.rx.pop().is_ok() {}
        self.listener = Listener::new(self.params.input_rate().max(1));
        on
    }

    /// Takes what came in since last time.
    pub fn feed(&mut self) {
        let rx = &mut self.rx;
        self.listener.feed(std::iter::from_fn(|| rx.pop().ok()));
    }

    /// The latest pitch: `None` until enough came in, `Some(None)` for
    /// silence.
    pub fn read(&self) -> Option<Option<Pitch>> {
        self.listener.read()
    }
}
