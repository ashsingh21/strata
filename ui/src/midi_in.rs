//! MIDI controllers: every MIDI input is opened (and one plugged in later
//! is picked up within a couple of seconds); keys and pads play the
//! selected track, knobs turn Carve's knobs, the sustain pedal holds notes.
//! What each message does is `shared::midi`; this connects the devices and
//! feeds the synth, on the UI's render timer.

use std::collections::HashSet;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use midir::{MidiInput, MidiInputConnection};
use vizia::prelude::*;

use shared::arrangement::{Arrangement, Instrument, TrackId};
use shared::midi::{self, Action, Layout, Message};

use crate::synth::state::SynthEvent;

/// How often to look for controllers plugged in or out.
const SCAN_EVERY: Duration = Duration::from_secs(2);
/// Our own client and the system's loopback aren't controllers.
const SKIP: [&str; 3] = ["midi through", "shor", "pipewire"];

pub enum MidiEvent {
    /// Once a frame, from the render timer.
    Tick,
}

pub struct MidiModel {
    /// The controllers connected, for the status bar ("" for none).
    pub devices: Signal<String>,
    connections: Vec<(String, MidiInputConnection<()>)>,
    tx: mpsc::Sender<(Layout, Message)>,
    rx: mpsc::Receiver<(Layout, Message)>,
    last_scan: Option<Instant>,
    sustain: bool,
    /// Notes let go while the pedal was down: released when it comes up.
    sustained: HashSet<u8>,
    arrangement: Signal<Arrangement>,
    selected_track: Signal<Option<TrackId>>,
}

impl MidiModel {
    pub fn new(arrangement: Signal<Arrangement>, selected_track: Signal<Option<TrackId>>) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            devices: Signal::new(String::new()),
            connections: Vec::new(),
            tx,
            rx,
            last_scan: None,
            sustain: false,
            sustained: HashSet::new(),
            arrangement,
            selected_track,
        }
    }

    /// Opens inputs that appeared and forgets ones that went away.
    fn scan(&mut self) {
        let Ok(probe) = MidiInput::new("Shor") else { return };
        let names: Vec<String> = probe.ports().iter().filter_map(|p| probe.port_name(p).ok()).collect();
        let before = self.connections.len();
        self.connections.retain(|(name, _)| names.contains(name));
        let mut changed = before != self.connections.len();
        for name in names {
            let lower = name.to_lowercase();
            if SKIP.iter().any(|s| lower.contains(s)) || self.connections.iter().any(|(n, _)| *n == name) {
                continue;
            }
            let Ok(input) = MidiInput::new("Shor") else { continue };
            let Some(port) = input.ports().into_iter().find(|p| input.port_name(p).ok().as_deref() == Some(&name)) else { continue };
            let layout = Layout::for_port(&name);
            let tx = self.tx.clone();
            match input.connect(
                &port,
                "shor-in",
                move |_, bytes, _| {
                    if let Some(message) = midi::parse(bytes) {
                        let _ = tx.send((layout, message));
                    }
                },
                (),
            ) {
                Ok(connection) => {
                    tracing::info!(target: "action", "midi: connected {name} ({layout:?})");
                    self.connections.push((name, connection));
                    changed = true;
                }
                Err(e) => tracing::warn!("midi: couldn't open {name}: {e}"),
            }
        }
        if changed {
            let shown = self.connections.iter().map(|(n, _)| short_name(n)).collect::<Vec<_>>().join(", ");
            self.devices.set(shown);
        }
    }

    /// The selected Drum Kit's pad notes, in pad order (none for Carve).
    fn drum_pads(&self) -> Vec<u8> {
        let arr = self.arrangement.get();
        let drums = self.selected_track.get().and_then(|t| arr.track(t)).is_some_and(|t| t.instrument == Some(Instrument::Drums));
        if drums { shared::drums::DRUM_KIT.iter().map(|p| p.note).collect() } else { Vec::new() }
    }

    fn handle(&mut self, cx: &mut EventContext, action: Action) {
        match action {
            Action::Note { note, velocity } => {
                self.sustained.remove(&note);
                cx.emit(SynthEvent::PlayNote(note, velocity));
            }
            Action::NoteOff { note } => {
                if self.sustain {
                    self.sustained.insert(note);
                } else {
                    cx.emit(SynthEvent::KeyRelease(note));
                }
            }
            Action::Knob { param, value } => {
                cx.emit(SynthEvent::Update(Box::new(move |s| param.apply_norm(s, value))));
                cx.emit(SynthEvent::Touched(param));
            }
            Action::Sustain(down) => {
                self.sustain = down;
                if !down {
                    for note in std::mem::take(&mut self.sustained) {
                        cx.emit(SynthEvent::KeyRelease(note));
                    }
                }
            }
        }
    }
}

/// "MPK mini IV" from "MPK mini IV:MPK mini IV MIDI 1 24:0" (ALSA's long names).
fn short_name(port: &str) -> String {
    port.split(':').next().unwrap_or(port).trim().to_string()
}

impl Model for MidiModel {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|event, _| match event {
            MidiEvent::Tick => {
                if self.last_scan.is_none_or(|t| t.elapsed() >= SCAN_EVERY) {
                    self.last_scan = Some(Instant::now());
                    self.scan();
                }
                let mut pads = None;
                while let Ok((layout, message)) = self.rx.try_recv() {
                    let pads = pads.get_or_insert_with(|| self.drum_pads()).clone();
                    if let Some(action) = midi::map(message, layout, &pads) {
                        self.handle(cx, action);
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_names_shorten_to_the_device() {
        assert_eq!(short_name("MPK mini IV:MPK mini IV MIDI 1 24:0"), "MPK mini IV");
        assert_eq!(short_name("Keystation"), "Keystation");
    }
}
