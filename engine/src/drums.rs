//! The Drum Kit instrument: plays `shared::drums::DRUM_KIT` samples as
//! one-shots, one voice per hit. Sample data comes from the same decoded
//! sources the arrangement's audio clips use, so nothing here allocates or
//! touches the disk. Note-offs are ignored: a drum hit always plays out.

use shared::drums::pad_for_note;
use shared::playback::DecodedSource;
use shared::synth::{NoteEvent, ALL_NOTES_OFF};

const MAX_VOICES: usize = 32;
/// How long a choked or stolen hit takes to fade out - short enough to
/// read as a cut, long enough not to click.
const FADE_MS: f32 = 5.0;

#[derive(Clone, Copy, Default)]
struct Voice {
    active: bool,
    note: u8,
    /// Index into the decoded-source list.
    source: usize,
    /// Position in source frames (fractional: sources are resampled on
    /// the fly to the output rate).
    pos: f64,
    /// Source frames per output frame.
    step: f64,
    gain: f32,
    /// 1.0 while playing; ramps to 0 once `fading`.
    fade: f32,
    fading: bool,
    /// Voice start order, for stealing the oldest when all are busy.
    started: u64,
}

pub struct DrumEngine {
    voices: [Voice; MAX_VOICES],
    sample_rate: f32,
    fade_step: f32,
    counter: u64,
}

impl DrumEngine {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            voices: [Voice::default(); MAX_VOICES],
            sample_rate,
            fade_step: 1.0 / (FADE_MS * 0.001 * sample_rate),
            counter: 0,
        }
    }

    pub fn handle_note_event(&mut self, event: NoteEvent, sources: &[DecodedSource]) {
        if event.note == ALL_NOTES_OFF {
            for v in &mut self.voices {
                v.fading = true;
            }
            return;
        }
        if !event.on {
            return;
        }
        let Some(pad) = pad_for_note(event.note) else { return };
        // Not decoded yet (or failed to decode): nothing to play.
        let Some(source) = sources.iter().position(|s| &*s.source == pad.sample) else { return };

        for v in self.voices.iter_mut().filter(|v| v.active && pad.chokes.contains(&v.note)) {
            v.fading = true;
        }
        // Retriggering the same pad cuts its previous hit too, the way a
        // real drum machine's voice does - keeps rolls from smearing.
        for v in self.voices.iter_mut().filter(|v| v.active && v.note == event.note) {
            v.fading = true;
        }

        let index = match self.voices.iter().position(|v| !v.active) {
            Some(i) => i,
            None => (0..MAX_VOICES).min_by_key(|&i| self.voices[i].started).unwrap_or(0),
        };
        self.counter += 1;
        let velocity = event.velocity.clamp(1, 127) as f32 / 127.0;
        self.voices[index] = Voice {
            active: true,
            note: event.note,
            source,
            pos: 0.0,
            step: sources[source].sample_rate as f64 / self.sample_rate as f64,
            // A gentle curve: soft hits are quieter, not inaudible.
            gain: velocity * velocity.sqrt(),
            fade: 1.0,
            fading: false,
            started: self.counter,
        };
    }

    /// One output frame: every sounding hit, summed.
    pub fn process(&mut self, sources: &[DecodedSource]) -> (f32, f32) {
        let mut out_l = 0.0;
        let mut out_r = 0.0;
        for v in self.voices.iter_mut().filter(|v| v.active) {
            let Some(src) = sources.get(v.source) else {
                v.active = false;
                continue;
            };
            let channels = src.channels.max(1) as usize;
            let frames = src.samples.len() / channels;
            let i = v.pos as usize;
            if i + 1 >= frames {
                v.active = false;
                continue;
            }
            let frac = (v.pos - i as f64) as f32;
            let at = |frame: usize, ch: usize| src.samples[frame * channels + ch.min(channels - 1)];
            let l = at(i, 0) + (at(i + 1, 0) - at(i, 0)) * frac;
            let r = at(i, 1) + (at(i + 1, 1) - at(i, 1)) * frac;
            if v.fading {
                v.fade -= self.fade_step;
                if v.fade <= 0.0 {
                    v.active = false;
                    continue;
                }
            }
            let g = v.gain * v.fade;
            out_l += l * g;
            out_r += r * g;
            v.pos += v.step;
        }
        (out_l, out_r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn source(name: &str, rate: u32, frames: usize) -> DecodedSource {
        DecodedSource { source: Arc::from(name), sample_rate: rate, channels: 1, samples: Arc::from(vec![0.5f32; frames]) }
    }

    fn hit(note: u8) -> NoteEvent {
        NoteEvent { slot: 0, note, on: true, velocity: 127 }
    }

    #[test]
    fn plays_a_pad_for_its_length_at_the_output_rate() {
        // 44.1 kHz source, 48 kHz output: 441 source frames last 480 output frames.
        let sources = [source("drums/kick.wav", 44_100, 441)];
        let mut d = DrumEngine::new(48_000.0);
        d.handle_note_event(hit(shared::drums::KICK), &sources);
        let sounding = (0..1000).filter(|_| d.process(&sources).0 != 0.0).count();
        assert!((478..=480).contains(&sounding), "{sounding}");
    }

    #[test]
    fn closed_hat_chokes_open_hat() {
        let sources = [source("drums/hihat_open.wav", 48_000, 48_000), source("drums/hihat_closed.wav", 48_000, 10)];
        let mut d = DrumEngine::new(48_000.0);
        d.handle_note_event(hit(shared::drums::OPEN_HAT), &sources);
        for _ in 0..100 {
            d.process(&sources);
        }
        d.handle_note_event(hit(shared::drums::CLOSED_HAT), &sources);
        for _ in 0..(0.01 * 48_000.0) as usize {
            d.process(&sources);
        }
        // Both done: the closed hat ran out, the open one was faded away.
        assert_eq!(d.process(&sources), (0.0, 0.0));
    }

    #[test]
    fn unknown_notes_and_undecoded_pads_are_silent() {
        let mut d = DrumEngine::new(48_000.0);
        d.handle_note_event(hit(0), &[]);
        d.handle_note_event(hit(shared::drums::KICK), &[]);
        assert_eq!(d.process(&[]), (0.0, 0.0));
    }
}
