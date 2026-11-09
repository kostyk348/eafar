//! viz — interactive visualizer for the EAFAR sparse event-driven engine.
//!
//! This crate is split exactly along the line the audit asked for:
//!
//! * **`lib.rs` (this file): GUI-independent logic.** The rewind ring
//!   buffer, the per-frame capture/restore into a [`SparseSession`], the
//!   events/s statistics tracker and the playback controller. All of it is
//!   plain Rust on top of the dependency-free `eafar` crate and covered by
//!   unit tests that run with plain `cargo test` — no egui, no window
//!   system, no GPU.
//! * **`gui.rs` (behind the `gui` feature): drawing only.** Converts the
//!   session state into an egui texture, draws the controls/HUD, and maps
//!   clicks to event injections. It contains no simulation logic.
//!
//! The `gui` feature needs cargo >= 1.85 (parts of the egui/eframe
//! dependency tree ship edition-2024 manifests); the logic in here builds
//! and tests on older stable toolchains too.

use std::collections::VecDeque;

use eafar::error::Result;
use eafar::prelude::*;

/// Drawing-only module (eframe/egui). Compiled behind the `gui` feature.
#[cfg(feature = "gui")]
pub mod gui;

// ---------------------------------------------------------------------------
// Rewind frames and the ring buffer
// ---------------------------------------------------------------------------

/// One rewindable snapshot of a session: the rule field's cells, the active
/// region set and the counters. Captured BEFORE every simulated step, so
/// "rewind" restores exactly the pre-step state and stepping again
/// reproduces the same trajectory (EAFAR is deterministic).
///
/// Memory: `cells` is one `f32` per cell (the other five SoA fields are not
/// used by the reference workloads), `active_ids` is at most one `u32` per
/// region — for the 192x192 demo world one frame is ~148 KiB.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// Generation the world was in BEFORE the next step.
    pub generation: u64,
    /// Rule-field values, row-major, `width * height` entries.
    pub cells: Vec<f32>,
    /// Active region ids at capture time (ascending — the scheduler keeps
    /// them ordered).
    pub active_ids: Vec<u32>,
    /// Cumulative events at capture time (for the HUD).
    pub events_total: u64,
}

impl Frame {
    /// Capture the current state of `session`.
    pub fn capture(session: &SparseSession) -> Frame {
        Frame {
            generation: session.generation(),
            cells: session.world().data(session.rule().field()).to_vec(),
            active_ids: session.sched().active_ids().iter().copied().collect(),
            events_total: session.events_total(),
        }
    }

    /// Restore `session` to this frame: fields bit-exactly, active set
    /// rebuilt, counters rewound (via a session checkpoint). Continuation
    /// from here is the deterministic continuation of the original
    /// trajectory.
    pub fn restore(&self, session: &mut SparseSession) -> Result<()> {
        let field = session.rule().field();
        {
            let slice = session.world_mut().data_mut(field);
            if slice.len() != self.cells.len() {
                return Err(eafar::error::Error::invalid_argument(
                    "eafar-viz: frame does not match the session world size",
                ));
            }
            slice.copy_from_slice(&self.cells);
        }
        let cp = SessionCheckpoint::from_parts(
            session.world(),
            &self.active_ids,
            self.generation,
            self.events_total,
        );
        session.restore(&cp)
    }
}

/// Fixed-capacity ring buffer of recent frames (the "recent states" the
/// audit asked the rewind control to walk back through). Pushing beyond
/// the capacity evicts the OLDEST frame.
#[derive(Debug)]
pub struct RewindBuffer {
    frames: VecDeque<Frame>,
    cap: usize,
}

impl RewindBuffer {
    /// Create a buffer holding at most `cap` frames.
    pub fn new(cap: usize) -> Self {
        assert!(cap > 0, "rewind capacity must be > 0");
        RewindBuffer {
            frames: VecDeque::with_capacity(cap),
            cap,
        }
    }

    /// Push a frame, evicting the oldest one on overflow.
    pub fn push(&mut self, frame: Frame) {
        if self.frames.len() == self.cap {
            self.frames.pop_front();
        }
        self.frames.push_back(frame);
    }

    /// Pop the newest frame (one step back). `None` when empty.
    pub fn undo(&mut self) -> Option<Frame> {
        self.frames.pop_back()
    }

    /// Number of buffered frames.
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// True when nothing can be rewound.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Buffer capacity.
    pub fn cap(&self) -> usize {
        self.cap
    }

    /// Drop all frames.
    pub fn clear(&mut self) {
        self.frames.clear();
    }
}

// ---------------------------------------------------------------------------
// Statistics tracker (events per second, EMA)
// ---------------------------------------------------------------------------

/// Tracks executed events per wall-clock second with an exponential moving
/// average (so the HUD number is stable but responsive).
#[derive(Debug, Clone)]
pub struct StatsTracker {
    ema: f64,
    alpha: f64,
    last_events_total: u64,
    initialized: bool,
}

impl StatsTracker {
    /// `alpha` is the EMA weight of a new sample (0 < alpha <= 1].
    pub fn new(alpha: f64) -> Self {
        assert!((0.0..=1.0).contains(&alpha) && alpha > 0.0, "alpha in (0,1]");
        StatsTracker {
            ema: 0.0,
            alpha,
            last_events_total: 0,
            initialized: false,
        }
    }

    /// Feed one sample: the session's cumulative event count and the wall
    /// time (seconds) since the previous sample.
    pub fn record(&mut self, events_total: u64, dt_secs: f64) {
        if dt_secs <= 0.0 {
            return; // ignore degenerate samples, keep the previous EMA
        }
        let delta = events_total.saturating_sub(self.last_events_total);
        self.last_events_total = events_total;
        let inst = delta as f64 / dt_secs;
        if self.initialized {
            self.ema += self.alpha * (inst - self.ema);
        } else {
            self.ema = inst;
            self.initialized = true;
        }
    }

    /// Reset the baseline (used when the session is rebuilt so the first
    /// sample after a reset does not count the whole old history).
    pub fn reset(&mut self, events_total: u64) {
        self.last_events_total = events_total;
        self.ema = 0.0;
        self.initialized = false;
    }

    /// Current smoothed events-per-second estimate.
    pub fn events_per_sec(&self) -> f64 {
        self.ema
    }
}

// ---------------------------------------------------------------------------
// Playback controller
// ---------------------------------------------------------------------------

/// Play/pause state and the wall-clock -> generation-steps accumulator.
#[derive(Debug)]
pub struct Playback {
    playing: bool,
    gens_per_sec: f64,
    accumulator: f64,
    /// Upper bound of steps executed per UI frame (prevents a death spiral
    /// when the simulation is slower than the requested rate).
    max_steps_per_frame: u64,
}

impl Playback {
    pub fn new(gens_per_sec: f64) -> Self {
        Playback {
            playing: false,
            gens_per_sec: gens_per_sec.max(0.0),
            accumulator: 0.0,
            max_steps_per_frame: 64,
        }
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn play(&mut self) {
        self.playing = true;
    }

    pub fn pause(&mut self) {
        self.playing = false;
        self.accumulator = 0.0;
    }

    pub fn toggle(&mut self) {
        if self.playing {
            self.pause();
        } else {
            self.play();
        }
    }

    /// Queue exactly one step even when paused (the Step button).
    pub fn step_once(&mut self) {
        self.accumulator += 1.0;
    }

    /// Change the requested rate.
    pub fn set_gens_per_sec(&mut self, rate: f64) {
        self.gens_per_sec = rate.max(0.0);
    }

    pub fn gens_per_sec(&self) -> f64 {
        self.gens_per_sec
    }

    /// Advance by `dt_secs` of wall time and return how many whole
    /// simulation steps to run now. Works while paused too (that is how
    /// the Step button queues exactly one step).
    pub fn advance(&mut self, dt_secs: f64) -> u64 {
        if self.playing && dt_secs > 0.0 {
            self.accumulator += dt_secs * self.gens_per_sec;
        }
        // Whole steps only; keep the fractional remainder for the next call.
        let steps = self.accumulator.floor() as u64;
        self.accumulator -= steps as f64;
        steps.min(self.max_steps_per_frame)
    }

    /// Drop queued steps (used after a rebuild/rewind so stale steps don't
    /// fire immediately).
    pub fn reset(&mut self) {
        self.accumulator = 0.0;
    }
}

// ---------------------------------------------------------------------------
// Tests — plain cargo tests, no egui involved
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const W: u32 = 128;
    const RS: u32 = 32;

    #[test]
    fn rewind_buffer_evicts_oldest_and_undoes_newest() {
        let mut buf = RewindBuffer::new(3);
        for gen in 0..5u64 {
            buf.push(Frame {
                generation: gen,
                cells: vec![gen as f32; 4],
                active_ids: vec![],
                events_total: gen * 10,
            });
        }
        assert_eq!(buf.len(), 3);
        // Capacity 3 -> frames 2,3,4 survive; frame 0 and 1 were evicted.
        let f = buf.undo().unwrap();
        assert_eq!(f.generation, 4);
        let f = buf.undo().unwrap();
        assert_eq!(f.generation, 3);
        let f = buf.undo().unwrap();
        assert_eq!(f.generation, 2);
        assert!(buf.undo().is_none());
        assert!(buf.is_empty());
        buf.clear();
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn frame_capture_restore_round_trip_is_bit_exact() {
        let mut session = SparseSession::new(Workload::Heat, W, W, RS, 7).unwrap();
        // Advance a bit so the state and the active set are non-trivial.
        for _ in 0..5 {
            session.step_generation().unwrap();
        }
        let frame = Frame::capture(&session);
        let cells_at_capture = session.world().data(Field::Temperature).to_vec();
        let active_at_capture: Vec<u32> =
            session.sched().active_ids().iter().copied().collect();

        // Advance far beyond (heat fully decays and every region sleeps).
        for _ in 0..300 {
            session.step_generation().unwrap();
        }
        assert_eq!(session.sched().active_count(), 0, "heat world must settle");

        // Restore the captured frame: same cells, same active set.
        frame.restore(&mut session).unwrap();
        assert!(session
            .world()
            .data(Field::Temperature)
            .iter()
            .zip(cells_at_capture.iter())
            .all(|(a, b)| a.to_bits() == b.to_bits()));
        let restored_active: Vec<u32> =
            session.sched().active_ids().iter().copied().collect();
        assert_eq!(restored_active, active_at_capture);

        // Stepping again continues the same trajectory (compare against a
        // reference run that never rewound but was itself captured at the
        // same point).
        let mut reference = SparseSession::new(Workload::Heat, W, W, RS, 7).unwrap();
        for _ in 0..5 {
            reference.step_generation().unwrap();
        }
        for _ in 0..3 {
            reference.step_generation().unwrap();
            session.step_generation().unwrap();
        }
        assert_eq!(session.state_hash(), reference.state_hash());
    }

    #[test]
    fn frame_size_mismatch_is_an_error() {
        let session = SparseSession::new(Workload::Conway, W, W, RS, 1).unwrap();
        let frame = Frame::capture(&session);
        let mut other = SparseSession::new(Workload::Conway, 64, 64, RS, 1).unwrap();
        assert!(frame.restore(&mut other).is_err());
    }

    #[test]
    fn stats_tracker_ema_math() {
        let mut t = StatsTracker::new(0.5);
        // First sample initializes the EMA directly: 100 events in 1 s.
        t.record(100, 1.0);
        assert!((t.events_per_sec() - 100.0).abs() < 1e-9);
        // Second sample: 300 more events in 1 s -> inst 300, ema 200.
        t.record(400, 1.0);
        assert!((t.events_per_sec() - 200.0).abs() < 1e-9);
        // Degenerate dt is ignored.
        t.record(500, 0.0);
        assert!((t.events_per_sec() - 200.0).abs() < 1e-9);
        // Reset rebaselines so old history is not counted.
        t.reset(1_000_000);
        t.record(1_000_050, 0.5); // 50 events in 0.5 s = 100/s
        assert!((t.events_per_sec() - 100.0).abs() < 1e-9);
    }

    #[test]
    fn playback_accumulates_whole_steps_and_caps_bursts() {
        let mut p = Playback::new(10.0); // 10 gens per second
        assert!(!p.is_playing());
        assert_eq!(p.advance(1.0), 0, "paused: no steps");
        p.play();
        assert_eq!(p.advance(0.35), 3, "0.35 s at 10 gps -> 3 whole steps");
        assert_eq!(p.advance(0.04), 0, "remainder 0.9 -> not a whole step yet");
        assert_eq!(p.advance(0.06), 1, "0.9 + 0.6 -> one more whole step");

        // Burst cap: at 1000 gps, 1 s would be 1000 steps but is capped.
        let mut fast = Playback::new(1000.0);
        fast.play();
        assert_eq!(fast.advance(1.0), 64, "steps per frame are capped");
        assert_eq!(fast.advance(0.0), 0, "no time passed -> nothing new");

        // step_once queues exactly one step even when paused.
        let mut paused = Playback::new(10.0);
        paused.step_once();
        assert_eq!(paused.advance(0.0), 1);
        assert_eq!(paused.advance(0.0), 0, "queued step is consumed once");

        // Pause clears the accumulator.
        let mut q = Playback::new(100.0);
        q.play();
        q.advance(0.5);
        q.pause();
        assert_eq!(q.advance(1.0), 0);
    }
}
