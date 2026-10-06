//! Makes the Tabla kit's samples (`assets/drums/tabla/*.wav`) - synthesised,
//! so they're Shor's own to ship. Run: `cargo run -p engine --example gen_tabla`.
//!
//! The dayan (right hand) rings at near-harmonic overtones thanks to its
//! black spot, tuned here to C#4; where it's struck decides which ring:
//! the rim (Na) bright, between rim and spot (Tin) softer, the open centre
//! (Tun) mostly the fundamental, a closed stroke (Te) barely rings. The
//! bayan (left hand) is a low boom that slides up as the wrist presses
//! (Ge), or a flat slap (Ke). Dha and Dhin are Na and Tin played with Ge.

use std::f32::consts::TAU;

const SR: u32 = 48_000;
const SA: f32 = 277.18; // C#4

/// A partial: (frequency ratio, level, decay time in seconds).
type Partial = (f32, f32, f32);

fn ring(fundamental: f32, partials: &[Partial], seconds: f32) -> Vec<f32> {
    let n = (seconds * SR as f32) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / SR as f32;
            let attack = (t / 0.001).min(1.0);
            partials.iter().map(|&(r, a, d)| a * (TAU * fundamental * r * t).sin() * (-t / d).exp()).sum::<f32>() * attack
        })
        .collect()
}

/// A strike: noise, filtered (high-pass above `hp` Hz, low-pass below
/// `lp`), dying away in `decay` seconds.
fn strike(level: f32, hp: f32, lp: f32, decay: f32, seconds: f32, seed: u32) -> Vec<f32> {
    let n = (seconds * SR as f32) as usize;
    let mut rng = seed | 1;
    let (mut low, mut prev_in, mut high) = (0.0f32, 0.0f32, 0.0f32);
    let a_lp = 1.0 - (-TAU * lp / SR as f32).exp();
    let a_hp = (-TAU * hp / SR as f32).exp();
    (0..n)
        .map(|i| {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            let x = rng as f32 / u32::MAX as f32 * 2.0 - 1.0;
            low += (x - low) * a_lp;
            high = a_hp * (high + low - prev_in);
            prev_in = low;
            high * level * (-(i as f32 / SR as f32) / decay).exp()
        })
        .collect()
}

/// The bayan's boom: its pitch rising from `from` to `to` Hz as the wrist presses.
fn boom(from: f32, to: f32, level: f32, decay: f32, seconds: f32) -> Vec<f32> {
    let n = (seconds * SR as f32) as usize;
    let mut phase = [0.0f32; 3];
    (0..n)
        .map(|i| {
            let t = i as f32 / SR as f32;
            let f = to + (from - to) * (-t / 0.12).exp();
            let mut out = 0.0;
            for (k, (ratio, amp, d)) in [(1.0, 1.0, decay), (2.0, 0.22, decay * 0.45), (3.0, 0.07, decay * 0.25)].iter().enumerate() {
                phase[k] += TAU * f * ratio / SR as f32;
                out += amp * phase[k].sin() * (-t / d).exp();
            }
            out * level * (t / 0.002).min(1.0)
        })
        .collect()
}

fn mix(parts: &[&[f32]]) -> Vec<f32> {
    let n = parts.iter().map(|p| p.len()).max().unwrap_or(0);
    (0..n).map(|i| parts.iter().map(|p| p.get(i).copied().unwrap_or(0.0)).sum()).collect()
}

/// Peak at -2 dBFS, the last 10 ms faded out.
fn finish(mut x: Vec<f32>) -> Vec<f32> {
    let peak = x.iter().fold(0f32, |m, v| m.max(v.abs())).max(1e-9);
    let fade = (SR as usize / 100).min(x.len());
    let len = x.len();
    for (i, v) in x.iter_mut().enumerate() {
        *v *= 0.79 / peak;
        if i >= len - fade {
            *v *= (len - i) as f32 / fade as f32;
        }
    }
    x
}

fn write(name: &str, samples: &[f32]) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/drums/tabla");
    std::fs::create_dir_all(&dir).unwrap();
    let spec = hound::WavSpec { channels: 1, sample_rate: SR, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut w = hound::WavWriter::create(dir.join(format!("{name}.wav")), spec).unwrap();
    for &s in samples {
        w.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).unwrap();
    }
    w.finalize().unwrap();
    println!("{name}.wav: {:.2} s", samples.len() as f32 / SR as f32);
}

fn main() {
    let na = mix(&[
        &ring(SA, &[(1.0, 0.22, 0.45), (2.0, 0.6, 0.42), (3.0, 0.5, 0.33), (4.0, 0.33, 0.24), (5.0, 0.22, 0.16), (7.1, 0.08, 0.06)], 1.2),
        &strike(0.35, 1500.0, 9000.0, 0.004, 0.05, 11),
    ]);
    let tin = mix(&[
        &ring(SA, &[(1.0, 0.5, 0.7), (2.0, 0.45, 0.5), (3.0, 0.22, 0.32), (4.0, 0.1, 0.22)], 1.4),
        &strike(0.18, 1000.0, 6000.0, 0.004, 0.05, 23),
    ]);
    let tun = mix(&[&ring(SA, &[(1.0, 0.9, 1.0), (2.0, 0.28, 0.55), (3.0, 0.1, 0.3)], 1.8), &strike(0.12, 400.0, 3000.0, 0.005, 0.05, 37)]);
    let te = mix(&[&ring(SA, &[(1.0, 0.4, 0.035), (2.0, 0.3, 0.025), (3.0, 0.15, 0.02)], 0.25), &strike(0.5, 600.0, 5000.0, 0.015, 0.2, 41)]);
    let ge = mix(&[&boom(92.0, 112.0, 1.0, 0.9, 1.8), &strike(0.25, 40.0, 600.0, 0.01, 0.1, 53)]);
    let ke = mix(&[&boom(80.0, 80.0, 0.7, 0.05, 0.3), &strike(0.6, 80.0, 1800.0, 0.02, 0.3, 67)]);
    let scaled = |x: &[f32], k: f32| x.iter().map(|v| v * k).collect::<Vec<f32>>();
    // Together, each hand a little softer.
    let dha = mix(&[&scaled(&finish(na.clone()), 0.75), &scaled(&finish(ge.clone()), 0.7)]);
    let dhin = mix(&[&scaled(&finish(tin.clone()), 0.75), &scaled(&finish(ge.clone()), 0.7)]);
    for (name, x) in [("na", na), ("tin", tin), ("tun", tun), ("te", te), ("ge", ge), ("ke", ke), ("dha", dha), ("dhin", dhin)] {
        write(name, &finish(x));
    }
}
