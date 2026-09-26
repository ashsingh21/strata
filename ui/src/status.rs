//! The status bar's model: the control most recently touched, shown with
//! its live value on the right of the status bar ("Carve · Cutoff 1.20
//! kHz"). Controls report themselves with `StatusEvent::Touched`, handing
//! over their formatted-value `Memo`, so the readout keeps following the
//! value as it changes rather than freezing at the moment of the touch.

use vizia::prelude::*;

pub enum StatusEvent {
    Touched { name: String, value: Memo<String> },
}

pub struct StatusModel {
    pub touched: Signal<Option<(String, Memo<String>)>>,
}

impl StatusModel {
    pub fn new() -> Self {
        Self { touched: Signal::new(None) }
    }
}

impl Model for StatusModel {
    fn event(&mut self, _cx: &mut EventContext, event: &mut Event) {
        event.map(|event, _| match event {
            StatusEvent::Touched { name, value } => {
                let already = self.touched.get().is_some_and(|(n, _)| n == *name);
                if !already {
                    self.touched.set(Some((name.clone(), *value)));
                }
            }
        });
    }
}
