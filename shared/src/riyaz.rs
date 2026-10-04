//! Riyaz: what a singer needs to see while practising against the
//! tanpura - the pitch of their voice (YIN, de Cheveigné & Kawahara 2002),
//! where it sits against Sa and the raag's swars, and how steadily each
//! note was held.

use std::collections::VecDeque;

/// The voice's pitch over one frame, and how sure the detector is (0..1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pitch {
    pub hz: f32,
    pub clarity: f32,
}

/// A singing voice, low bass to high soprano.
pub const MIN_HZ: f32 = 65.0;
pub const MAX_HZ: f32 = 1100.0;
/// Below this level (RMS) it's breath or the room, not a note.
const SILENCE: f32 = 0.01;
/// YIN's dip threshold: lower is stricter about calling a frame voiced.
const THRESHOLD: f32 = 0.15;

/// The pitch of `frame` (at least `2 * sample_rate / MIN_HZ` samples is
/// best), or `None` when it's silence or not a clear note.
pub fn detect(frame: &[f32], sample_rate: f32) -> Option<Pitch> {
    let rms = (frame.iter().map(|x| x * x).sum::<f32>() / frame.len().max(1) as f32).sqrt();
    if rms < SILENCE {
        return None;
    }
    let tau_min = (sample_rate / MAX_HZ).floor().max(2.0) as usize;
    let tau_max = ((sample_rate / MIN_HZ).ceil() as usize).min(frame.len() / 2);
    if tau_max <= tau_min + 2 {
        return None;
    }
    let window = frame.len() - tau_max;
    // The difference function, then its cumulative mean normalised form.
    let mut d = vec![0.0f32; tau_max + 1];
    for (tau, slot) in d.iter_mut().enumerate().skip(1) {
        let mut sum = 0.0;
        for j in 0..window {
            let diff = frame[j] - frame[j + tau];
            sum += diff * diff;
        }
        *slot = sum;
    }
    let mut cmnd = vec![1.0f32; tau_max + 1];
    let mut running = 0.0;
    for tau in 1..=tau_max {
        running += d[tau];
        cmnd[tau] = if running > 0.0 { d[tau] * tau as f32 / running } else { 1.0 };
    }
    // The first dip under the threshold, followed down to its bottom.
    let mut tau = tau_min;
    while tau < tau_max {
        if cmnd[tau] < THRESHOLD {
            while tau + 1 < tau_max && cmnd[tau + 1] < cmnd[tau] {
                tau += 1;
            }
            break;
        }
        tau += 1;
    }
    if tau >= tau_max {
        return None;
    }
    // Between samples: a parabola through the dip and its neighbours.
    let (a, b, c) = (cmnd[tau - 1], cmnd[tau], cmnd[tau + 1]);
    let bend = a - 2.0 * b + c;
    let offset = if bend.abs() > 1.0e-9 { 0.5 * (a - c) / bend } else { 0.0 };
    let period = tau as f32 + offset.clamp(-1.0, 1.0);
    Some(Pitch { hz: sample_rate / period, clarity: (1.0 - b).clamp(0.0, 1.0) })
}

/// `hz` in cents above `sa_hz` (negative below).
pub fn cents(hz: f32, sa_hz: f32) -> f32 {
    1200.0 * (hz / sa_hz).log2()
}

/// Sa's frequency for a key (0 = C) in an octave (3 = C3, about 131 Hz).
pub fn sa_hz(key: u8, octave: i32) -> f32 {
    let midi = 12 * (octave + 1) + key as i32;
    440.0 * 2f32.powf((midi as f32 - 69.0) / 12.0)
}

/// The swar nearest `cents` among the raag's notes (`mask`: bit n = n
/// semitones above Sa), in semitones from Sa (negative below, 12 and up
/// above), and how far off it is in cents.
pub fn nearest_swar(cents: f32, mask: u16) -> (i32, f32) {
    let near = (cents / 100.0).round() as i32;
    let mut best = (near, f32::MAX);
    for semis in near - 6..=near + 6 {
        if mask & (1 << semis.rem_euclid(12)) == 0 {
            continue;
        }
        let off = cents - semis as f32 * 100.0;
        if off.abs() < best.1.abs() {
            best = (semis, off);
        }
    }
    best
}

/// From the mic's samples to pitches: brings the input down to about
/// 24 kHz (a voice needs no more, and the detector gets cheaper), keeps
/// the latest frame, and reads its pitch when asked.
pub struct Listener {
    window: Vec<f32>,
    pending: (f32, u32),
    factor: u32,
    rate: f32,
}

/// The rate the detector runs at, roughly.
const LISTEN_RATE: u32 = 24_000;
/// Samples per reading at that rate: about 43 ms, two periods of a low Sa.
pub const FRAME: usize = 1024;

impl Listener {
    pub fn new(input_rate: u32) -> Self {
        let factor = (input_rate / LISTEN_RATE).max(1);
        Self { window: Vec::with_capacity(FRAME * 2), pending: (0.0, 0), factor, rate: input_rate as f32 / factor as f32 }
    }

    /// Takes the input's samples (averaging each `factor` of them).
    pub fn feed(&mut self, samples: impl IntoIterator<Item = f32>) {
        for s in samples {
            self.pending.0 += s;
            self.pending.1 += 1;
            if self.pending.1 == self.factor {
                self.window.push(self.pending.0 / self.factor as f32);
                self.pending = (0.0, 0);
            }
        }
        if self.window.len() > FRAME * 2 {
            let excess = self.window.len() - FRAME;
            self.window.drain(..excess);
        }
    }

    /// The latest frame's pitch: `None` until a frame's worth has come in,
    /// `Some(None)` for silence.
    pub fn read(&self) -> Option<Option<Pitch>> {
        (self.window.len() >= FRAME).then(|| detect(&self.window[self.window.len() - FRAME..], self.rate))
    }
}

/// A note held long enough to count: which swar, how far off it sat on
/// average, how much it wavered, and for how long.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Held {
    pub swar: i32,
    pub mean: f32,
    pub spread: f32,
    pub secs: f32,
}

impl Held {
    /// Steady enough to call it held still (a wobble under this, in cents).
    pub const STEADY: f32 = 15.0;
}

/// A point of the pitch trace: when, and where against Sa (`None`: no note).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub t: f32,
    pub cents: Option<f32>,
}

/// Follows the voice: keeps the recent trace, and the notes it held.
pub struct Tracker {
    pub trace: VecDeque<Point>,
    pub held: Vec<Held>,
    /// The note being sung now: its swar, when it began, its readings.
    current: Option<(i32, f32, Vec<f32>)>,
}

/// How long a note must stay on one swar to count as held.
const HOLD_SECS: f32 = 0.3;
/// How far from the swar it may stray and still be "on" it.
const ON_SWAR: f32 = 50.0;
/// How much trace to keep, in seconds.
pub const TRACE_SECS: f32 = 10.0;
const MAX_HELD: usize = 8;

impl Default for Tracker {
    fn default() -> Self {
        Self::new()
    }
}

impl Tracker {
    pub fn new() -> Self {
        Self { trace: VecDeque::new(), held: Vec::new(), current: None }
    }

    pub fn clear(&mut self) {
        *self = Self::new();
    }

    /// One reading at time `t` (seconds): the voice's cents against Sa,
    /// or `None` for silence.
    pub fn push(&mut self, t: f32, cents: Option<f32>, mask: u16) {
        self.trace.push_back(Point { t, cents });
        while self.trace.front().is_some_and(|p| p.t < t - TRACE_SECS) {
            self.trace.pop_front();
        }
        let reading = cents.map(|c| (c, nearest_swar(c, mask)));
        match (&mut self.current, reading) {
            (Some((swar, _, readings)), Some((c, (s, off)))) if *swar == s && off.abs() <= ON_SWAR => readings.push(c),
            (_, reading) => {
                self.finish(t);
                self.current = reading.filter(|(_, (_, off))| off.abs() <= ON_SWAR).map(|(c, (s, _))| (s, t, vec![c]));
            }
        }
    }

    /// The note now being sung, if it's on a swar: (swar, cents off, held
    /// for how long).
    pub fn now(&self, t: f32) -> Option<(i32, f32, f32)> {
        let (swar, start, readings) = self.current.as_ref()?;
        let last = *readings.last()?;
        Some((*swar, last - *swar as f32 * 100.0, t - start))
    }

    fn finish(&mut self, t: f32) {
        let Some((swar, start, readings)) = self.current.take() else { return };
        if t - start < HOLD_SECS || readings.is_empty() {
            return;
        }
        let target = swar as f32 * 100.0;
        let mean = readings.iter().map(|c| c - target).sum::<f32>() / readings.len() as f32;
        let var = readings.iter().map(|c| (c - target - mean).powi(2)).sum::<f32>() / readings.len() as f32;
        self.held.push(Held { swar, mean, spread: var.sqrt(), secs: t - start });
        if self.held.len() > MAX_HELD {
            self.held.remove(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;
    const BHAIRAV: u16 = (1 << 0) | (1 << 1) | (1 << 4) | (1 << 5) | (1 << 7) | (1 << 8) | (1 << 11);

    /// A voice-like tone: a fundamental and a few falling harmonics.
    fn tone(hz: f32, len: usize) -> Vec<f32> {
        (0..len)
            .map(|i| {
                let t = i as f32 / SR;
                (1..=5).map(|h| (std::f32::consts::TAU * hz * h as f32 * t).sin() * 0.3 / h as f32).sum()
            })
            .collect()
    }

    #[test]
    fn it_finds_a_sung_pitch_within_a_few_cents() {
        for hz in [82.4, 130.8, 196.0, 261.6, 440.0, 880.0] {
            let p = detect(&tone(hz, 2048), SR).unwrap_or_else(|| panic!("nothing at {hz}"));
            assert!(cents(p.hz, hz).abs() < 5.0, "{hz} read as {}", p.hz);
            assert!(p.clarity > 0.8);
        }
    }

    #[test]
    fn silence_and_noise_are_not_notes() {
        assert!(detect(&vec![0.0; 2048], SR).is_none());
        let mut seed = 1u32;
        let noise: Vec<f32> = (0..2048)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                (seed as f32 / u32::MAX as f32 - 0.5) * 0.5
            })
            .collect();
        assert!(detect(&noise, SR).is_none());
    }

    #[test]
    fn swars_snap_to_the_raag() {
        // 130 cents above Sa: in Bhairav that's komal Re (100), 30 sharp.
        let (swar, off) = nearest_swar(130.0, BHAIRAV);
        assert_eq!(swar, 1);
        assert!((off - 30.0).abs() < 0.01);
        // 200 cents (shuddha Re) isn't in Bhairav: komal Re or Ga, whichever's nearer.
        assert_eq!(nearest_swar(260.0, BHAIRAV).0, 4);
        // Below Sa: Ni of the lower octave is -100.
        assert_eq!(nearest_swar(-95.0, BHAIRAV).0, -1);
        assert!((sa_hz(0, 3) - 130.81).abs() < 0.01);
    }

    #[test]
    fn the_listener_follows_a_voice_from_the_mic() {
        // A 48 kHz input, fed in blocks like the audio callback's.
        let mut listener = Listener::new(48_000);
        let voice = tone(220.0, 48_000 / 2);
        assert!(listener.read().is_none());
        for block in voice.chunks(512) {
            listener.feed(block.iter().copied());
        }
        let pitch = listener.read().unwrap().unwrap();
        assert!(cents(pitch.hz, 220.0).abs() < 5.0, "{}", pitch.hz);
        listener.feed(std::iter::repeat_n(0.0, FRAME * 2));
        assert_eq!(listener.read(), Some(None));
        // 44.1 kHz too.
        let mut listener = Listener::new(44_100);
        let voice: Vec<f32> = (0..22_050).map(|i| (std::f32::consts::TAU * 196.0 * i as f32 / 44_100.0).sin() * 0.3).collect();
        listener.feed(voice);
        assert!(cents(listener.read().unwrap().unwrap().hz, 196.0).abs() < 5.0);
    }

    #[test]
    fn a_steady_note_is_held_and_a_slide_is_not() {
        let mut t = Tracker::new();
        let mut time = 0.0;
        // Sa, 5 cents sharp, steady for a second.
        for _ in 0..30 {
            t.push(time, Some(5.0 + (time * 20.0).sin()), BHAIRAV);
            time += 1.0 / 30.0;
        }
        // A slide up to Pa, never resting.
        for i in 0..10 {
            t.push(time, Some(100.0 + i as f32 * 60.0), BHAIRAV);
            time += 1.0 / 30.0;
        }
        t.push(time, None, BHAIRAV);
        assert_eq!(t.held.len(), 1);
        let sa = t.held[0];
        assert_eq!(sa.swar, 0);
        assert!((sa.mean - 5.0).abs() < 1.0 && sa.spread < Held::STEADY && sa.secs > 0.9);
    }
}
