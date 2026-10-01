//! The guitar effects' DSP: a noise gate, an amp (drive and tone), a
//! speaker cabinet, a chorus, a tape echo and a spring reverb. The
//! settings live in `shared::guitar`; this is what runs them.
//!
//! Each unit is big (the echo's delay line, the reverbs' tanks), and a
//! chain swaps units on the audio thread - so the units are built once,
//! before the stream starts, into a `GuitarPool` and handed out and taken
//! back without allocating.

use shared::arrangement::{EqBand, EqBandKind};
use shared::eq::{coefficients, Biquad};
use shared::guitar::*;

use crate::dsp::{DcBlocker, OversampledDrive, Smoother};
use crate::eq::BiquadState;
use crate::fx::{DelayLine, Diffuser, Reverb};

fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// How many of each kind the pool holds: enough for every chain on a
/// busy project plus the live monitor. A chain asking for more gets a
/// pass-through in its place.
pub const POOL_PER_KIND: usize = 8;

/// The cookbook's 2nd-order low-pass.
fn low_pass(freq_hz: f32, q: f32, sample_rate: f32) -> Biquad {
    let w0 = std::f32::consts::TAU * freq_hz.clamp(20.0, sample_rate * 0.45) / sample_rate;
    let (sin_w0, cos_w0) = w0.sin_cos();
    let alpha = sin_w0 / (2.0 * q);
    let a0 = 1.0 + alpha;
    Biquad {
        b0: (1.0 - cos_w0) / 2.0 / a0,
        b1: (1.0 - cos_w0) / a0,
        b2: (1.0 - cos_w0) / 2.0 / a0,
        a1: -2.0 * cos_w0 / a0,
        a2: (1.0 - alpha) / a0,
    }
}

fn band(kind: EqBandKind, freq_hz: f32, gain_db: f32, q: f32, sample_rate: f32) -> Biquad {
    coefficients(&EqBand { kind, on: true, freq_hz, gain_db, q }, sample_rate)
}

/// One-pole smoothing coefficient for a time constant in milliseconds.
fn time_coeff(ms: f32, sample_rate: f32) -> f32 {
    1.0 - (-1.0 / (ms.max(0.01) * 0.001 * sample_rate)).exp()
}

/// A one-pole low-pass coefficient for a cutoff in Hz.
fn one_pole(freq_hz: f32, sample_rate: f32) -> f32 {
    1.0 - (-std::f32::consts::TAU * freq_hz / sample_rate).exp()
}

// ---------------------------------------------------------------- Gate

struct Gate {
    sample_rate: f32,
    open_level: f32,
    close_level: f32,
    release: f32,
    attack: f32,
    env_decay: f32,
    hold_samples: u32,
    env: f32,
    hold: u32,
    open: bool,
    gain: f32,
}

impl Gate {
    fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            open_level: 0.0,
            close_level: 0.0,
            release: 0.0,
            attack: time_coeff(0.5, sample_rate),
            env_decay: (-1.0 / (0.03 * sample_rate)).exp(),
            hold_samples: (0.03 * sample_rate) as u32,
            env: 0.0,
            hold: 0,
            open: false,
            gain: 0.0,
        }
    }

    fn reset(&mut self) {
        self.env = 0.0;
        self.hold = 0;
        self.open = false;
        self.gain = 0.0;
    }

    fn set(&mut self, v: &[f32; MAX_PARAMS]) {
        self.open_level = db_to_gain(v[GATE_THRESHOLD]);
        // Closes 4 dB lower than it opens, so a note's tail doesn't flutter the gate.
        self.close_level = self.open_level * 0.63;
        self.release = time_coeff(v[GATE_RELEASE], self.sample_rate);
    }

    #[inline]
    fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let level = l.abs().max(r.abs());
        self.env = level.max(self.env * self.env_decay);
        if self.open {
            if self.env > self.close_level {
                self.hold = self.hold_samples;
            } else if self.hold > 0 {
                self.hold -= 1;
            } else {
                self.open = false;
            }
        } else if self.env > self.open_level {
            self.open = true;
            self.hold = self.hold_samples;
        }
        let (target, coeff) = if self.open { (1.0, self.attack) } else { (0.0, self.release) };
        self.gain += (target - self.gain) * coeff;
        (l * self.gain, r * self.gain)
    }
}

// ----------------------------------------------------------------- Amp

/// Where the drive stage starts to bend: a little offset before the
/// saturator, so the clipping is lopsided like a tube's (even harmonics).
const AMP_BIAS: f32 = 0.08;
/// Level compensation: the saturator's output stops growing, so the level
/// is steered towards what a clean signal would be.
const AMP_REF: f32 = 0.14;
const AMP_MAKEUP: f32 = 1.5;

struct Amp {
    sample_rate: f32,
    pre_hp: DcBlocker,
    drive: OversampledDrive,
    dc: DcBlocker,
    fizz: f32,
    fizz_coeff: f32,
    tone: [Biquad; 3],
    tone_state: [BiquadState; 3],
    tone_dials: [f32; 3],
    gain: Smoother,
    volume: Smoother,
    pre_target: f32,
    volume_target: f32,
    reverb: Reverb,
    reverb_mix: f32,
    fresh: bool,
}

impl Amp {
    fn new(sample_rate: f32) -> Self {
        let mut amp = Self {
            sample_rate,
            pre_hp: DcBlocker::new(90.0, sample_rate),
            drive: OversampledDrive::default(),
            dc: DcBlocker::new(20.0, sample_rate),
            fizz: 0.0,
            fizz_coeff: one_pole(7000.0, sample_rate),
            tone: [Biquad::IDENTITY; 3],
            tone_state: [BiquadState::default(); 3],
            tone_dials: [f32::NAN; 3],
            gain: Smoother::new(1.0, 15.0, sample_rate),
            volume: Smoother::new(1.0, 15.0, sample_rate),
            pre_target: 1.0,
            volume_target: 1.0,
            reverb: Reverb::new(sample_rate),
            reverb_mix: 0.0,
            fresh: true,
        };
        amp.reset();
        amp
    }

    fn reset(&mut self) {
        self.pre_hp = DcBlocker::new(90.0, self.sample_rate);
        self.drive = OversampledDrive::default();
        self.dc = DcBlocker::new(20.0, self.sample_rate);
        self.fizz = 0.0;
        self.tone_state = [BiquadState::default(); 3];
        self.tone_dials = [f32::NAN; 3];
        self.reverb.clear();
        self.fresh = true;
    }

    fn set(&mut self, v: &[f32; MAX_PARAMS]) {
        self.pre_target = db_to_gain(v[AMP_GAIN] * 5.0);
        self.volume_target = db_to_gain(v[AMP_VOLUME]);
        self.reverb_mix = v[AMP_REVERB] * 0.5;
        let dials = [v[AMP_BASS], v[AMP_MID], v[AMP_TREBLE]];
        if dials != self.tone_dials {
            let db = |dial: f32| (dial - 5.0) * 2.4;
            let sr = self.sample_rate;
            self.tone = [
                band(EqBandKind::LowShelf, 100.0, db(dials[0]), 0.7, sr),
                band(EqBandKind::Bell, 800.0, db(dials[1]), 0.7, sr),
                band(EqBandKind::HighShelf, 3000.0, db(dials[2]), 0.7, sr),
            ];
            self.tone_dials = dials;
        }
        if self.fresh {
            self.gain.value = self.pre_target;
            self.volume.value = self.volume_target;
            self.fresh = false;
        }
    }

    #[inline]
    fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let pre = self.gain.next(self.pre_target);
        let volume = self.volume.next(self.volume_target);
        let mut y = self.pre_hp.process((l + r) * 0.5) * pre;
        y = self.dc.process(self.drive.process(y + AMP_BIAS));
        self.fizz += (y - self.fizz) * self.fizz_coeff;
        y = self.fizz;
        for (filter, state) in self.tone.iter().zip(self.tone_state.iter_mut()) {
            y = state.process(y, filter);
        }
        // Volume and the tone stack can add up to a lot; this bends the loudest peaks instead of clipping.
        y = (y * AMP_MAKEUP * AMP_REF / (AMP_REF * pre).tanh() * volume).tanh();
        self.reverb.process(y, y, 0.35, self.reverb_mix)
    }
}

// ------------------------------------------------------------- Cabinet

const CAB_TAPS: usize = 1024;

/// A guitar speaker's impulse response, built rather than recorded: the
/// cone's low resonance, a dip in the mids, the presence peak, then a
/// steep roll-off above about 5 kHz, and one early reflection off the
/// cabinet's back panel. Gain is set so the middle of the band passes at
/// unity.
fn cabinet_ir(sample_rate: f32) -> Vec<f32> {
    let stages = [
        band(EqBandKind::LowCut, 85.0, 0.0, 0.7, sample_rate),
        band(EqBandKind::Bell, 120.0, 5.0, 1.5, sample_rate),
        band(EqBandKind::Bell, 450.0, -3.0, 1.0, sample_rate),
        band(EqBandKind::Bell, 2700.0, 6.0, 1.6, sample_rate),
        low_pass(4800.0, 0.7, sample_rate),
        low_pass(5600.0, 0.6, sample_rate),
    ];
    let mut state = [BiquadState::default(); 6];
    let mut ir = vec![0.0f32; CAB_TAPS];
    for (n, out) in ir.iter_mut().enumerate() {
        let mut x = if n == 0 { 1.0 } else { 0.0 };
        for (filter, s) in stages.iter().zip(state.iter_mut()) {
            x = s.process(x, filter);
        }
        *out = x;
    }
    let reflection = (0.00077 * sample_rate) as usize;
    for n in (reflection..CAB_TAPS).rev() {
        ir[n] += 0.18 * ir[n - reflection];
    }
    let fade = CAB_TAPS / 8;
    for k in 0..fade {
        let w = 0.5 + 0.5 * (std::f32::consts::PI * k as f32 / fade as f32).cos();
        ir[CAB_TAPS - fade + k] *= w;
    }
    let mean_db: f32 = (0..24)
        .map(|i| {
            let f = 150.0 * (4000.0f32 / 150.0).powf(i as f32 / 23.0);
            stages.iter().map(|b| b.response_db(f, sample_rate)).sum::<f32>()
        })
        .sum::<f32>()
        / 24.0;
    let scale = db_to_gain(-mean_db);
    for v in &mut ir {
        *v *= scale;
    }
    ir
}

struct Cabinet {
    sample_rate: f32,
    ir: Vec<f32>,
    history: Vec<f32>,
    pos: usize,
    tone: [BiquadState; 2],
    tone_filter: Biquad,
    tone_hz: f32,
    reverb: Reverb,
    room: Smoother,
    room_target: f32,
}

impl Cabinet {
    fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            ir: cabinet_ir(sample_rate),
            history: vec![0.0; CAB_TAPS * 2],
            pos: 0,
            tone: [BiquadState::default(); 2],
            tone_filter: Biquad::IDENTITY,
            tone_hz: 0.0,
            reverb: Reverb::new(sample_rate),
            room: Smoother::new(0.0, 30.0, sample_rate),
            room_target: 0.0,
        }
    }

    fn reset(&mut self) {
        self.history.fill(0.0);
        self.tone = [BiquadState::default(); 2];
        self.reverb.clear();
        self.tone_hz = 0.0;
    }

    fn set(&mut self, v: &[f32; MAX_PARAMS]) {
        if v[CAB_TONE] != self.tone_hz {
            self.tone_hz = v[CAB_TONE];
            self.tone_filter = low_pass(self.tone_hz, 0.7, self.sample_rate);
        }
        self.room_target = v[CAB_ROOM] * 0.5;
    }

    #[inline]
    fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let x = (l + r) * 0.5;
        self.pos = (self.pos + CAB_TAPS - 1) % CAB_TAPS;
        self.history[self.pos] = x;
        self.history[self.pos + CAB_TAPS] = x;
        let window = &self.history[self.pos..self.pos + CAB_TAPS];
        // Eight running sums, so the multiply-adds can overlap.
        let mut sums = [0.0f32; 8];
        for (h, w) in self.ir.chunks_exact(8).zip(window.chunks_exact(8)) {
            for k in 0..8 {
                sums[k] += h[k] * w[k];
            }
        }
        let mut y: f32 = sums.iter().sum();
        for s in &mut self.tone {
            y = s.process(y, &self.tone_filter);
        }
        let room = self.room.next(self.room_target);
        self.reverb.process(y, y, 0.15, room)
    }
}

// -------------------------------------------------------------- Chorus

const CHORUS_BASE_MS: f32 = 10.0;
const CHORUS_SWING_MS: f32 = 5.0;

struct Chorus {
    sample_rate: f32,
    left: DelayLine,
    right: DelayLine,
    phase: f32,
    step: f32,
    offset: f32,
    blend: Smoother,
    depth: Smoother,
    targets: (f32, f32),
    fresh: bool,
}

impl Chorus {
    fn new(sample_rate: f32) -> Self {
        let len = ((CHORUS_BASE_MS + CHORUS_SWING_MS + 3.0) * 0.001 * sample_rate) as usize + 4;
        Self {
            sample_rate,
            left: DelayLine::new(len),
            right: DelayLine::new(len),
            phase: 0.0,
            step: 0.0,
            offset: 0.0,
            blend: Smoother::new(0.0, 20.0, sample_rate),
            depth: Smoother::new(0.0, 20.0, sample_rate),
            targets: (0.0, 0.0),
            fresh: true,
        }
    }

    fn reset(&mut self) {
        self.left.clear();
        self.right.clear();
        self.phase = 0.0;
        self.fresh = true;
    }

    fn set(&mut self, v: &[f32; MAX_PARAMS]) {
        self.targets = (v[CHORUS_MIX], v[CHORUS_DEPTH]);
        self.step = v[CHORUS_RATE] / self.sample_rate;
        self.offset = v[CHORUS_SPREAD] * 0.5;
        if self.fresh {
            self.blend.value = self.targets.0;
            self.depth.value = self.targets.1;
            self.fresh = false;
        }
    }

    #[inline]
    fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let blend = self.blend.next(self.targets.0);
        let depth = self.depth.next(self.targets.1);
        self.left.push(l);
        self.right.push(r);
        self.phase = (self.phase + self.step).fract();
        let to_samples = 0.001 * self.sample_rate;
        let swing = CHORUS_SWING_MS * depth;
        let lfo_l = (self.phase * std::f32::consts::TAU).sin();
        let lfo_r = ((self.phase + self.offset).fract() * std::f32::consts::TAU).sin();
        let wet_l = self.left.read((CHORUS_BASE_MS + swing * lfo_l) * to_samples);
        let wet_r = self.right.read((CHORUS_BASE_MS + swing * lfo_r) * to_samples);
        (l + (wet_l - l) * blend, r + (wet_r - r) * blend)
    }
}

// ----------------------------------------------------------- Tape echo

const ECHO_MAX_MS: f32 = 1200.0;
/// How far the tape's speed wanders, in ms of delay at Tape = 100%: a slow
/// wow and a fast flutter.
const WOW_MS: f32 = 0.5;
const FLUTTER_MS: f32 = 0.08;

struct Echo {
    sample_rate: f32,
    line: DelayLine,
    time: Smoother,
    time_target: f32,
    feedback: f32,
    mix: Smoother,
    mix_target: f32,
    tone_coeff: f32,
    tone: f32,
    hp_coeff: f32,
    hp: f32,
    tape: f32,
    wow: f32,
    flutter: f32,
    fresh: bool,
}

impl Echo {
    fn new(sample_rate: f32) -> Self {
        let len = (ECHO_MAX_MS * 0.001 * sample_rate) as usize + (0.004 * sample_rate) as usize + 8;
        Self {
            sample_rate,
            line: DelayLine::new(len),
            // Slow on purpose: turning Time bends the pitch of the repeats, like a tape machine.
            time: Smoother::new(0.0, 90.0, sample_rate),
            time_target: 0.0,
            feedback: 0.0,
            mix: Smoother::new(0.0, 20.0, sample_rate),
            mix_target: 0.0,
            tone_coeff: 1.0,
            tone: 0.0,
            hp_coeff: one_pole(100.0, sample_rate),
            hp: 0.0,
            tape: 0.0,
            wow: 0.0,
            flutter: 0.0,
            fresh: true,
        }
    }

    fn reset(&mut self) {
        self.line.clear();
        self.tone = 0.0;
        self.hp = 0.0;
        self.fresh = true;
    }

    fn set(&mut self, v: &[f32; MAX_PARAMS]) {
        self.time_target = v[ECHO_TIME] * 0.001 * self.sample_rate;
        self.feedback = v[ECHO_FEEDBACK];
        self.mix_target = v[ECHO_MIX];
        self.tone_coeff = one_pole(v[ECHO_TONE], self.sample_rate);
        self.tape = v[ECHO_TAPE];
        if self.fresh {
            self.time.value = self.time_target;
            self.mix.value = self.mix_target;
            self.fresh = false;
        }
    }

    #[inline]
    fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let x = (l + r) * 0.5;
        let time = self.time.next(self.time_target);
        let mix = self.mix.next(self.mix_target);
        self.wow = (self.wow + 0.55 / self.sample_rate).fract();
        self.flutter = (self.flutter + 6.3 / self.sample_rate).fract();
        let wander = (WOW_MS * (self.wow * std::f32::consts::TAU).sin() + FLUTTER_MS * (self.flutter * std::f32::consts::TAU).sin())
            * self.tape
            * 0.001
            * self.sample_rate;
        let repeat = self.line.read(time + wander);
        // Each pass round the loop gets darker, loses its lows, and is squashed a little.
        self.tone += (repeat - self.tone) * self.tone_coeff;
        self.hp += (self.tone - self.hp) * self.hp_coeff;
        let heard = self.tone - self.hp;
        let drive = 1.0 + 2.0 * self.tape;
        let looped = (heard * drive).tanh() / drive;
        self.line.push(x + looped * self.feedback);
        (l + heard * mix, r + heard * mix)
    }
}

// -------------------------------------------------------------- Spring

/// The dispersion chain's allpass lengths, in ms: what turns a click into
/// a springy "boing".
const DRIP_MS: [f32; 6] = [2.1, 2.9, 3.7, 4.7, 6.1, 7.9];

struct Spring {
    sample_rate: f32,
    low_cut: f32,
    low_cut_coeff: f32,
    tone: [f32; 2],
    tone_coeff: f32,
    drip: [Diffuser; 6],
    lens: [f32; 6],
    drip_gain: f32,
    reverb: Reverb,
    size: f32,
    mix: Smoother,
    mix_target: f32,
    fresh: bool,
}

impl Spring {
    fn new(sample_rate: f32) -> Self {
        let lens = DRIP_MS.map(|ms| ms * 0.001 * sample_rate);
        Self {
            sample_rate,
            low_cut: 0.0,
            low_cut_coeff: one_pole(150.0, sample_rate),
            tone: [0.0; 2],
            tone_coeff: 1.0,
            drip: lens.map(|len| Diffuser::new(len, 0)),
            lens,
            drip_gain: 0.5,
            reverb: Reverb::new(sample_rate),
            size: 0.4,
            mix: Smoother::new(0.0, 20.0, sample_rate),
            mix_target: 0.0,
            fresh: true,
        }
    }

    fn reset(&mut self) {
        self.low_cut = 0.0;
        self.tone = [0.0; 2];
        for d in &mut self.drip {
            d.clear();
        }
        self.reverb.clear();
        self.fresh = true;
    }

    fn set(&mut self, v: &[f32; MAX_PARAMS]) {
        self.size = 0.1 + 0.7 * v[SPRING_LENGTH];
        self.tone_coeff = one_pole(v[SPRING_TONE], self.sample_rate);
        self.drip_gain = 0.3 + 0.45 * v[SPRING_DRIP];
        self.mix_target = v[SPRING_MIX] * 0.6;
        if self.fresh {
            self.mix.value = self.mix_target;
            self.fresh = false;
        }
    }

    #[inline]
    fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let mix = self.mix.next(self.mix_target);
        let mut x = (l + r) * 0.5;
        // A spring tank is band-limited at both ends.
        self.low_cut += (x - self.low_cut) * self.low_cut_coeff;
        x -= self.low_cut;
        for s in &mut self.tone {
            *s += (x - *s) * self.tone_coeff;
            x = *s;
        }
        for (d, len) in self.drip.iter_mut().zip(self.lens) {
            x = d.process(x, self.drip_gain, len);
        }
        let (tank_l, tank_r) = self.reverb.process(x, x, self.size, 1.0);
        // The reverb hands back its input plus the tail; the input half is the springy attack.
        (l + (tank_l + 0.4 * x) * mix, r + (tank_r + 0.4 * x) * mix)
    }
}

// ---------------------------------------------------------------- Unit

enum Dsp {
    Gate(Gate),
    Amp(Box<Amp>),
    Cabinet(Box<Cabinet>),
    Chorus(Chorus),
    Echo(Echo),
    Spring(Box<Spring>),
}

/// One guitar effect, ready to run.
pub struct GuitarUnit {
    kind: GuitarKind,
    dsp: Dsp,
    /// 0 = the effect is heard, 1 = it's bypassed; glides between, so
    /// switching it off doesn't click.
    bypass: Smoother,
    bypass_target: f32,
}

impl GuitarUnit {
    fn new(kind: GuitarKind, sample_rate: f32) -> Self {
        let dsp = match kind {
            GuitarKind::Gate => Dsp::Gate(Gate::new(sample_rate)),
            GuitarKind::Amp => Dsp::Amp(Box::new(Amp::new(sample_rate))),
            GuitarKind::Cabinet => Dsp::Cabinet(Box::new(Cabinet::new(sample_rate))),
            GuitarKind::Chorus => Dsp::Chorus(Chorus::new(sample_rate)),
            GuitarKind::Echo => Dsp::Echo(Echo::new(sample_rate)),
            GuitarKind::Spring => Dsp::Spring(Box::new(Spring::new(sample_rate))),
        };
        Self { kind, dsp, bypass: Smoother::new(0.0, 8.0, sample_rate), bypass_target: 0.0 }
    }

    pub fn kind(&self) -> GuitarKind {
        self.kind
    }

    /// Back to silence: no tail from whatever it last processed.
    fn reset(&mut self) {
        match &mut self.dsp {
            Dsp::Gate(u) => u.reset(),
            Dsp::Amp(u) => u.reset(),
            Dsp::Cabinet(u) => u.reset(),
            Dsp::Chorus(u) => u.reset(),
            Dsp::Echo(u) => u.reset(),
            Dsp::Spring(u) => u.reset(),
        }
        self.bypass.value = 0.0;
    }

    pub fn set(&mut self, fx: &GuitarFx) {
        if fx.kind != self.kind {
            return;
        }
        match &mut self.dsp {
            Dsp::Gate(u) => u.set(&fx.values),
            Dsp::Amp(u) => u.set(&fx.values),
            Dsp::Cabinet(u) => u.set(&fx.values),
            Dsp::Chorus(u) => u.set(&fx.values),
            Dsp::Echo(u) => u.set(&fx.values),
            Dsp::Spring(u) => u.set(&fx.values),
        }
        self.bypass_target = if fx.bypassed { 1.0 } else { 0.0 };
    }

    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let (wl, wr) = match &mut self.dsp {
            Dsp::Gate(u) => u.process(l, r),
            Dsp::Amp(u) => u.process(l, r),
            Dsp::Cabinet(u) => u.process(l, r),
            Dsp::Chorus(u) => u.process(l, r),
            Dsp::Echo(u) => u.process(l, r),
            Dsp::Spring(u) => u.process(l, r),
        };
        let b = self.bypass.next(self.bypass_target);
        if b < 1.0e-4 {
            (wl, wr)
        } else {
            (wl + (l - wl) * b, wr + (r - wr) * b)
        }
    }
}

/// Every guitar unit there will ever be, built before the stream starts.
/// Chains take one with `take` and hand it back with `give`; neither
/// allocates.
pub struct GuitarPool {
    free: [Vec<Box<GuitarUnit>>; 6],
    /// Times an effect was wanted and none of its kind was free.
    pub dry: u32,
}

impl GuitarPool {
    pub fn new(sample_rate: f32) -> Self {
        let make = |kind: GuitarKind| (0..POOL_PER_KIND).map(|_| Box::new(GuitarUnit::new(kind, sample_rate))).collect::<Vec<_>>();
        Self { free: GuitarKind::ALL.map(make), dry: 0 }
    }

    /// A silent unit of `kind`, or `None` when they're all in use.
    pub fn take(&mut self, kind: GuitarKind) -> Option<Box<GuitarUnit>> {
        let Some(mut unit) = self.free[kind.rank() as usize].pop() else {
            self.dry = self.dry.saturating_add(1);
            return None;
        };
        unit.reset();
        Some(unit)
    }

    pub fn give(&mut self, unit: Box<GuitarUnit>) {
        self.free[unit.kind.rank() as usize].push(unit);
    }

    #[cfg(test)]
    fn available(&self, kind: GuitarKind) -> usize {
        self.free[kind.rank() as usize].len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn unit(fx: GuitarFx) -> GuitarUnit {
        let mut u = GuitarUnit::new(fx.kind, SR);
        u.set(&fx);
        u
    }

    fn sine(i: usize, hz: f32, amp: f32) -> f32 {
        amp * (std::f32::consts::TAU * hz * i as f32 / SR).sin()
    }

    fn rms(v: &[f32]) -> f32 {
        (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt()
    }

    fn run(u: &mut GuitarUnit, n: usize, input: impl Fn(usize) -> f32) -> Vec<f32> {
        (0..n).map(|i| u.process(input(i), input(i)).0).collect()
    }

    #[test]
    fn the_gate_silences_hiss_and_passes_playing() {
        let mut u = unit(GuitarFx::new(GuitarKind::Gate));
        let hiss = run(&mut u, 24_000, |i| sine(i, 300.0, 0.0005));
        assert!(rms(&hiss[12_000..]) < 1.0e-5, "{}", rms(&hiss[12_000..]));
        let played = run(&mut u, 24_000, |i| sine(i, 300.0, 0.2));
        let level = rms(&played[12_000..]);
        assert!((level - 0.2 / 2f32.sqrt()).abs() < 0.005, "{level}");
    }

    #[test]
    fn a_tail_dying_away_closes_the_gate_without_chattering() {
        let mut u = unit(GuitarFx::new(GuitarKind::Gate));
        let input = |i: usize| sine(i, 200.0, 0.3 * (-(i as f32) / 10_000.0).exp());
        let out = run(&mut u, 96_000, input);
        let heard: Vec<f32> = (90_000..96_000).map(input).collect();
        assert!(rms(&out[90_000..96_000]) < rms(&heard) * 0.05, "still open: out {} heard {}", rms(&out[90_000..96_000]), rms(&heard));
        assert!(out.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn the_amp_keeps_a_similar_loudness_from_clean_to_full_drive() {
        let level_at = |gain: f32| {
            let mut fx = GuitarFx::new(GuitarKind::Amp);
            fx.values[AMP_GAIN] = gain;
            fx.values[AMP_REVERB] = 0.0;
            let mut u = unit(fx);
            let out = run(&mut u, 48_000, |i| sine(i, 330.0, 0.1));
            rms(&out[24_000..])
        };
        let (clean, loud) = (level_at(0.0), level_at(10.0));
        let ratio_db = 20.0 * (loud / clean).log10();
        assert!(ratio_db.abs() < 9.0, "clean {clean}, full {loud}: {ratio_db} dB apart");
        assert!(clean > 0.03, "{clean}");
    }

    #[test]
    fn the_amp_stays_finite_and_bounded_at_full_scale_and_every_knob_up() {
        let mut fx = GuitarFx::new(GuitarKind::Amp);
        fx.values = [10.0, 10.0, 10.0, 10.0, 12.0, 1.0];
        let mut u = unit(fx);
        let out = run(&mut u, 48_000, |i| if i % 97 < 48 { 1.0 } else { -1.0 });
        assert!(out.iter().all(|x| x.is_finite() && x.abs() < 6.0), "peak {}", out.iter().fold(0.0f32, |m, x| m.max(x.abs())));
    }

    #[test]
    fn the_amp_adds_harmonics_when_driven() {
        let mut fx = GuitarFx::new(GuitarKind::Amp);
        fx.values[AMP_GAIN] = 8.0;
        fx.values[AMP_REVERB] = 0.0;
        let mut u = unit(fx);
        let out = run(&mut u, 48_000, |i| sine(i, 200.0, 0.2));
        let tail = &out[24_000..];
        let bin = |hz: f32| {
            let (mut re, mut im) = (0.0f64, 0.0f64);
            for (i, y) in tail.iter().enumerate() {
                let ph = std::f64::consts::TAU * hz as f64 * i as f64 / SR as f64;
                re += *y as f64 * ph.cos();
                im += *y as f64 * ph.sin();
            }
            (re * re + im * im).sqrt()
        };
        assert!(bin(600.0) > bin(200.0) * 0.02, "3rd harmonic {} vs fundamental {}", bin(600.0), bin(200.0));
    }

    #[test]
    fn the_cabinet_is_dark_and_keeps_the_middle() {
        let mut fx = GuitarFx::new(GuitarKind::Cabinet);
        fx.values[CAB_ROOM] = 0.0;
        let level_at = |hz: f32| {
            let mut u = unit(fx);
            let out = run(&mut u, 24_000, |i| sine(i, hz, 0.3));
            rms(&out[12_000..])
        };
        let mid = level_at(1000.0);
        assert!((mid / (0.3 / 2f32.sqrt())).log10().abs() < 0.25, "{mid}");
        assert!(level_at(12_000.0) < mid * 0.1);
        assert!(level_at(40.0) < mid * 0.5);
    }

    #[test]
    fn a_cabinet_impulse_comes_back_as_its_response() {
        let mut fx = GuitarFx::new(GuitarKind::Cabinet);
        fx.values[CAB_ROOM] = 0.0;
        fx.values[CAB_TONE] = 12_000.0;
        let mut u = unit(fx);
        let out = run(&mut u, 2 * CAB_TAPS, |i| if i == 0 { 1.0 } else { 0.0 });
        assert!(out[..64].iter().any(|x| x.abs() > 0.01));
        assert!(rms(&out[CAB_TAPS + 64..]) < 1.0e-6, "rings past its length");
    }

    #[test]
    fn the_echo_repeats_at_the_set_time() {
        let mut fx = GuitarFx::new(GuitarKind::Echo);
        fx.values[ECHO_TIME] = 100.0;
        fx.values[ECHO_FEEDBACK] = 0.0;
        fx.values[ECHO_MIX] = 1.0;
        fx.values[ECHO_TONE] = 10_000.0;
        fx.values[ECHO_TAPE] = 0.0;
        let mut u = unit(fx);
        // A short click, so the high-pass in the repeat's path doesn't flatten it.
        let out = run(&mut u, 12_000, |i| if i < 8 { 1.0 } else { 0.0 });
        let echo = &out[2000..];
        let (at, peak) = echo.iter().enumerate().fold((0, 0.0f32), |m, (i, x)| if x.abs() > m.1 { (i, x.abs()) } else { m });
        let expected = 4800 - 2000;
        assert!(peak > 0.1 && (at as i32 - expected).abs() < 40, "peak {peak} at {at}, wanted about {expected}");
    }

    #[test]
    fn echo_feedback_makes_repeats_that_die_away() {
        let mut fx = GuitarFx::new(GuitarKind::Echo);
        fx.values[ECHO_TIME] = 50.0;
        fx.values[ECHO_FEEDBACK] = 0.5;
        fx.values[ECHO_MIX] = 1.0;
        let mut u = unit(fx);
        let out = run(&mut u, 96_000, |i| sine(i, 800.0, 0.5) * if i < 2400 { 1.0 } else { 0.0 });
        assert!(rms(&out[4000..9600]) > 0.01, "no repeats");
        assert!(rms(&out[90_000..]) < 0.002, "doesn't die away");
    }

    #[test]
    fn the_chorus_leaves_a_dry_signal_alone_and_moves_a_wet_one() {
        let mut fx = GuitarFx::new(GuitarKind::Chorus);
        fx.values[CHORUS_MIX] = 0.0;
        let mut dry = unit(fx);
        let out = run(&mut dry, 4800, |i| sine(i, 440.0, 0.3));
        assert!((out[4000] - sine(4000, 440.0, 0.3)).abs() < 1.0e-5);
        fx.values[CHORUS_MIX] = 1.0;
        let mut wet = unit(fx);
        let out = run(&mut wet, 4800, |i| sine(i, 440.0, 0.3));
        assert!((out[4000] - sine(4000, 440.0, 0.3)).abs() > 1.0e-3);
    }

    #[test]
    fn the_spring_has_a_tail() {
        let mut fx = GuitarFx::new(GuitarKind::Spring);
        fx.values[SPRING_MIX] = 1.0;
        let mut u = unit(fx);
        let out = run(&mut u, 48_000, |i| if i < 16 { 0.8 } else { 0.0 });
        assert!(rms(&out[6000..24_000]) > 1.0e-3, "{}", rms(&out[6000..24_000]));
        assert!(out.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn a_bypassed_effect_passes_the_signal_untouched_once_it_has_settled() {
        let mut u = unit(GuitarFx::new(GuitarKind::Amp).bypass());
        let out = run(&mut u, 9600, |i| sine(i, 440.0, 0.3));
        assert!((out[9000] - sine(9000, 440.0, 0.3)).abs() < 1.0e-4);
    }

    #[test]
    fn every_effect_survives_noise_at_extreme_settings() {
        let mut seed = 12345u32;
        let mut noise = move || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) as f32 / (1 << 23) as f32 - 1.0
        };
        let samples: Vec<f32> = (0..48_000).map(|_| noise()).collect();
        for kind in GuitarKind::ALL {
            for norm in [0.0, 1.0] {
                let mut fx = GuitarFx::new(kind);
                for i in 0..fx.param_count() {
                    fx.set_norm(i, norm);
                }
                let mut u = unit(fx);
                for &x in &samples {
                    let (l, r) = u.process(x, -x);
                    assert!(l.is_finite() && r.is_finite() && l.abs() < 50.0, "{} at {norm}: {l}", kind.name());
                }
            }
        }
    }

    #[test]
    fn the_pool_hands_out_and_takes_back_without_running_dry() {
        let mut pool = GuitarPool::new(SR);
        let mut held = Vec::new();
        while let Some(u) = pool.take(GuitarKind::Spring) {
            held.push(u);
        }
        assert_eq!(held.len(), POOL_PER_KIND);
        assert_eq!(pool.available(GuitarKind::Amp), POOL_PER_KIND);
        pool.give(held.pop().unwrap());
        assert_eq!(pool.available(GuitarKind::Spring), 1);
    }

    #[test]
    fn a_unit_handed_back_and_taken_again_starts_silent() {
        let mut pool = GuitarPool::new(SR);
        let mut fx = GuitarFx::new(GuitarKind::Echo);
        fx.values[ECHO_MIX] = 1.0;
        let mut u = pool.take(GuitarKind::Echo).unwrap();
        u.set(&fx);
        for i in 0..24_000 {
            u.process(sine(i, 500.0, 0.5), 0.0);
        }
        pool.give(u);
        let mut again = pool.take(GuitarKind::Echo).unwrap();
        again.set(&fx);
        let out: Vec<f32> = (0..24_000).map(|_| again.process(0.0, 0.0).0).collect();
        assert!(rms(&out) < 1.0e-6);
    }
}

#[cfg(test)]
mod fuzz {
    use super::*;

    const SR: f32 = 48_000.0;

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 40) as f32 / (1u64 << 24) as f32
        }
    }

    #[test]
    fn knob_jumps_never_make_a_unit_non_finite_or_runaway() {
        let mut rng = Rng(0x9E3779B97F4A7C15);
        for kind in GuitarKind::ALL {
            let mut unit = GuitarUnit::new(kind, SR);
            let mut fx = GuitarFx::new(kind);
            let mut worst = 0.0f32;
            for block in 0..4000 {
                if block % 3 == 0 {
                    for i in 0..kind.specs().len() {
                        let r = rng.next();
                        let n = if r < 0.25 { 0.0 } else if r > 0.75 { 1.0 } else { rng.next() };
                        fx.set_norm(i, n);
                    }
                    fx.bypassed = rng.next() < 0.1;
                }
                unit.set(&fx);
                let loud = rng.next() < 0.5;
                for s in 0..128 {
                    let x = (if loud { 1.0 } else { 0.1 }) * (rng.next() * 2.0 - 1.0) + 0.5 * (s as f32 * 0.1).sin();
                    let (l, r) = unit.process(x, x);
                    assert!(l.is_finite() && r.is_finite(), "{} went non-finite at block {block}", kind.name());
                    worst = worst.max(l.abs()).max(r.abs());
                }
            }
            assert!(worst < 40.0, "{} peaked at {worst}", kind.name());
            // After the knobs settle, it must still pass sound.
            let fx = GuitarFx::new(kind);
            unit.set(&fx);
            let mut energy = 0.0;
            for i in 0..96_000 {
                let x = 0.3 * (std::f32::consts::TAU * 220.0 * i as f32 / SR).sin();
                let (l, _) = unit.process(x, x);
                if i > 48_000 {
                    energy += l * l;
                }
            }
            assert!(energy > 1.0e-3, "{} is silent after the knobs settle", kind.name());
        }
    }
}
