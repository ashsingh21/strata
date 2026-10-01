//! Reads the audio thread's health counters (`shared::diag`) once a second
//! and writes what matters to the log: late or overloaded blocks, samples
//! that came out as NaN, a stream that stopped calling back, and a short
//! "still fine" line every half minute so a quiet log can be trusted.

use std::sync::Arc;
use std::time::{Duration, Instant};

use shared::diag::{stage_name, AudioSnapshot};
use shared::Params;

const CHECK_EVERY: Duration = Duration::from_secs(1);
const STILL_FINE_EVERY: Duration = Duration::from_secs(30);
/// A glitchy stretch is reported at most this often, with the totals.
const WARN_EVERY: Duration = Duration::from_secs(3);

pub struct AudioWatch {
    params: Arc<Params>,
    last_check: Instant,
    last_report: Instant,
    last_callbacks: u64,
    stopped: bool,
    /// Worst of the half minute since the last "still fine" line.
    worst: AudioSnapshot,
    late_total: u32,
    /// Trouble seen since the last warning was written.
    pending: AudioSnapshot,
    last_warn: Instant,
}

impl AudioWatch {
    pub fn new(params: Arc<Params>) -> Self {
        let now = Instant::now();
        Self { params, last_check: now, last_report: now, last_callbacks: 0, stopped: false, worst: AudioSnapshot::default(), late_total: 0, pending: AudioSnapshot::default(), last_warn: now - WARN_EVERY }
    }

    /// Call every UI frame; does its work about once a second.
    pub fn tick(&mut self) {
        if self.last_check.elapsed() < CHECK_EVERY {
            return;
        }
        self.last_check = Instant::now();
        let snap = self.params.diag().take();
        self.worst.max_gap = self.worst.max_gap.max(snap.max_gap);
        self.worst.max_cpu = self.worst.max_cpu.max(snap.max_cpu);
        self.worst.busy_blocks += snap.busy_blocks;
        self.late_total += snap.late_blocks;

        if snap.callbacks == self.last_callbacks && self.last_callbacks > 0 {
            if !self.stopped {
                self.stopped = true;
                tracing::error!(target: "audio", "the output stream stopped calling back ({} callbacks so far): no sound can be playing", snap.callbacks);
            }
        } else if self.stopped {
            self.stopped = false;
            tracing::warn!(target: "audio", "the output stream is calling back again");
        }
        let blocks = snap.callbacks.saturating_sub(self.last_callbacks);
        self.last_callbacks = snap.callbacks;

        self.pending.late_blocks += snap.late_blocks;
        self.pending.busy_blocks += snap.busy_blocks;
        self.pending.max_gap = self.pending.max_gap.max(snap.max_gap);
        self.pending.max_cpu = self.pending.max_cpu.max(snap.max_cpu);
        self.pending.callbacks += blocks;
        let trouble = self.pending.late_blocks > 0 || self.pending.busy_blocks > 0;
        if trouble && self.last_warn.elapsed() >= WARN_EVERY {
            let p = std::mem::take(&mut self.pending);
            self.last_warn = Instant::now();
            tracing::warn!(
                target: "audio",
                "{} late block(s) (worst started {:.1}x its length after the last), {} over 80% of their time (busiest {:.0}%), {} frames per block, {} blocks in this stretch",
                p.late_blocks,
                p.max_gap,
                p.busy_blocks,
                p.max_cpu * 100.0,
                snap.block_frames,
                p.callbacks
            );
        }
        if snap.bad_samples > 0 {
            tracing::error!(
                target: "audio",
                "{} sample(s) were NaN or infinite and were silenced; the first came from {}",
                snap.bad_samples,
                stage_name(snap.first_bad_stage)
            );
        }
        if snap.pool_dry > 0 && snap.pool_dry != self.worst.pool_dry {
            self.worst.pool_dry = snap.pool_dry;
            tracing::warn!(target: "audio", "no free guitar effect of the kind wanted ({} times): it passes the signal through", snap.pool_dry);
        }

        if self.last_report.elapsed() >= STILL_FINE_EVERY {
            self.last_report = Instant::now();
            tracing::info!(
                target: "audio",
                "{} frames per block, busiest callback {:.0}% of its time, worst gap {:.1}x, {} late block(s) in the last 30 s, playing: {}",
                snap.block_frames,
                self.worst.max_cpu * 100.0,
                self.worst.max_gap,
                self.late_total,
                self.params.playing()
            );
            let pool_dry = self.worst.pool_dry;
            self.worst = AudioSnapshot { pool_dry, ..AudioSnapshot::default() };
            self.late_total = 0;
        }
    }
}
