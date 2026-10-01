//! What the audio thread can tell the UI about its own health, and small
//! helpers for logging without flooding the log.
//!
//! The audio callback must not log (formatting allocates, and writing a
//! file blocks), so it only bumps the atomics in `AudioDiag`; the UI thread
//! reads them about once a second and writes the log lines.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::{self, Debug, Display, Write};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::Relaxed};
use std::time::{Duration, Instant};

/// A block that started more than this many block-lengths after the last
/// one means the device ran dry in between: an audible glitch.
pub const LATE_GAP: f32 = 1.8;
/// A callback that used more than this much of its own block's time.
pub const BUSY_CPU: f32 = 0.8;

#[derive(Default)]
pub struct AudioDiag {
    callbacks: AtomicU64,
    block_frames: AtomicU32,
    late_blocks: AtomicU32,
    busy_blocks: AtomicU32,
    max_gap_x100: AtomicU32,
    max_cpu_x1000: AtomicU32,
    bad_samples: AtomicU32,
    first_bad_stage: AtomicU32,
    pool_dry: AtomicU32,
}

/// What happened since the last `take`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AudioSnapshot {
    pub callbacks: u64,
    pub block_frames: u32,
    pub late_blocks: u32,
    pub busy_blocks: u32,
    /// Worst start-to-start gap, in block lengths (1.0 is on time).
    pub max_gap: f32,
    /// Busiest callback, as a fraction of its block's time.
    pub max_cpu: f32,
    /// Samples that came out as NaN or infinity and were silenced.
    pub bad_samples: u32,
    /// Where the first one came from: see `stage_name`.
    pub first_bad_stage: u32,
    /// How many times a guitar effect was wanted and none was free.
    pub pool_dry: u32,
}

impl AudioDiag {
    /// Audio thread: one block done.
    pub fn block(&self, frames: u32, gap_ratio: f32, cpu: f32) {
        self.callbacks.fetch_add(1, Relaxed);
        self.block_frames.store(frames, Relaxed);
        if gap_ratio > LATE_GAP {
            self.late_blocks.fetch_add(1, Relaxed);
        }
        if cpu > BUSY_CPU {
            self.busy_blocks.fetch_add(1, Relaxed);
        }
        self.max_gap_x100.fetch_max((gap_ratio.clamp(0.0, 1000.0) * 100.0) as u32, Relaxed);
        self.max_cpu_x1000.fetch_max((cpu.clamp(0.0, 1000.0) * 1000.0) as u32, Relaxed);
    }

    /// Audio thread: `count` samples in this block were not finite, the
    /// first from `stage`.
    pub fn non_finite(&self, stage: u32, count: u32) {
        let _ = self.first_bad_stage.compare_exchange(0, stage, Relaxed, Relaxed);
        self.bad_samples.fetch_add(count, Relaxed);
    }

    pub fn set_pool_dry(&self, total: u32) {
        self.pool_dry.store(total, Relaxed);
    }

    /// UI thread: everything since the last call (the callback count is
    /// the running total).
    pub fn take(&self) -> AudioSnapshot {
        AudioSnapshot {
            callbacks: self.callbacks.load(Relaxed),
            block_frames: self.block_frames.load(Relaxed),
            late_blocks: self.late_blocks.swap(0, Relaxed),
            busy_blocks: self.busy_blocks.swap(0, Relaxed),
            max_gap: self.max_gap_x100.swap(0, Relaxed) as f32 / 100.0,
            max_cpu: self.max_cpu_x1000.swap(0, Relaxed) as f32 / 1000.0,
            bad_samples: self.bad_samples.swap(0, Relaxed),
            first_bad_stage: self.first_bad_stage.swap(0, Relaxed),
            pool_dry: self.pool_dry.load(Relaxed),
        }
    }
}

/// Stage codes for `AudioDiag::non_finite`: hundreds say which part of the
/// mix, the remainder which track slot.
pub const STAGE_INSTRUMENT: u32 = 100;
pub const STAGE_SLOT_EFFECTS: u32 = 200;
pub const STAGE_AUDIO_CLIPS: u32 = 300;
pub const STAGE_MONITOR_INPUT: u32 = 400;
pub const STAGE_MONITOR_CHAIN: u32 = 401;
pub const STAGE_MASTER_INPUT: u32 = 500;
pub const STAGE_MASTER_EFFECTS: u32 = 501;
pub const STAGE_LIMITER: u32 = 502;

pub fn stage_name(code: u32) -> String {
    match code {
        STAGE_MONITOR_INPUT => "the live input".into(),
        STAGE_MONITOR_CHAIN => "the monitor's effect chain".into(),
        STAGE_MASTER_INPUT => "the sum going into the master".into(),
        STAGE_MASTER_EFFECTS => "the master effects".into(),
        STAGE_LIMITER => "the limiter".into(),
        STAGE_AUDIO_CLIPS => "audio clips or their track effects".into(),
        c if c / 100 == 1 => format!("the instrument in slot {}", c % 100),
        c if c / 100 == 2 => format!("the effects on slot {}", c % 100),
        c => format!("stage {c}"),
    }
}

/// A value's `Debug` text cut off after `limit` characters - and the cut
/// stops the formatting itself, so a command carrying thousands of notes
/// costs nothing. Display, so it is only built if the log line is.
pub struct Brief<'a, T: Debug>(pub &'a T, pub usize);

struct Capped {
    text: String,
    limit: usize,
}

impl Write for Capped {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            if self.text.chars().count() >= self.limit {
                return Err(fmt::Error);
            }
            self.text.push(c);
        }
        Ok(())
    }
}

impl<T: Debug> Display for Brief<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut capped = Capped { text: String::new(), limit: self.1 };
        let complete = write!(capped, "{:?}", self.0).is_ok();
        f.write_str(&capped.text)?;
        if !complete {
            f.write_str("...")?;
        }
        Ok(())
    }
}

/// The leading word of a `Debug` text: an enum variant's name.
pub fn variant_name<T: Debug>(value: &T) -> String {
    let mut capped = Capped { text: String::new(), limit: 40 };
    let _ = write!(capped, "{value:?}");
    capped.text.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect()
}

thread_local! {
    static LAST_LOGGED: RefCell<HashMap<String, (Instant, u32)>> = RefCell::new(HashMap::new());
}

/// For things that repeat while a knob is dragged: `Some(n)` the first
/// time and then at most once per `gap`, with `n` how many were skipped in
/// between; `None` otherwise. Per thread.
pub fn throttle(key: &str, gap: Duration) -> Option<u32> {
    LAST_LOGGED.with(|map| {
        let mut map = map.borrow_mut();
        let now = Instant::now();
        let entry = map.entry(key.to_string()).or_insert((now - gap - Duration::from_millis(1), 0));
        if now.duration_since(entry.0) >= gap {
            let skipped = entry.1;
            *entry = (now, 0);
            Some(skipped)
        } else {
            entry.1 += 1;
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    #[allow(dead_code)]
    enum Thing {
        Batch(Vec<u32>),
        Move { to: u32 },
    }

    #[test]
    fn a_long_value_is_cut_and_says_so() {
        let big = Thing::Batch((0..100_000).collect());
        let text = Brief(&big, 30).to_string();
        assert!(text.starts_with("Batch([0, 1, 2"), "{text}");
        assert!(text.ends_with("..."));
        assert_eq!(Brief(&Thing::Move { to: 3 }, 30).to_string(), "Move { to: 3 }");
    }

    #[test]
    fn the_variant_name_is_the_first_word() {
        assert_eq!(variant_name(&Thing::Batch(vec![1])), "Batch");
        assert_eq!(variant_name(&Thing::Move { to: 1 }), "Move");
    }

    #[test]
    fn a_repeating_event_is_let_through_once_per_gap_with_a_count_of_the_rest() {
        let gap = Duration::from_secs(60);
        assert_eq!(throttle("test-key", gap), Some(0));
        assert_eq!(throttle("test-key", gap), None);
        assert_eq!(throttle("test-key", gap), None);
        assert_eq!(throttle("other-key", gap), Some(0));
        assert_eq!(throttle("test-key", Duration::ZERO), Some(2));
    }

    #[test]
    fn the_snapshot_reports_the_worst_block_and_then_starts_over() {
        let diag = AudioDiag::default();
        diag.block(256, 1.0, 0.1);
        diag.block(256, 3.5, 0.9);
        diag.non_finite(STAGE_INSTRUMENT + 3, 5);
        diag.non_finite(STAGE_LIMITER, 2);
        let snap = diag.take();
        assert_eq!((snap.callbacks, snap.late_blocks, snap.busy_blocks), (2, 1, 1));
        assert_eq!((snap.max_gap, snap.bad_samples, snap.first_bad_stage), (3.5, 7, 103));
        assert_eq!(stage_name(snap.first_bad_stage), "the instrument in slot 3");
        let again = diag.take();
        assert_eq!((again.callbacks, again.late_blocks, again.bad_samples, again.max_gap), (2, 0, 0, 0.0));
    }
}
