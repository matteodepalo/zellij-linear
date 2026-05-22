//! Adaptive polling state machine.
//!
//! Two cadences:
//!   * idle (60 s) — the normal cadence
//!   * burst (5 s) — entered for 2 minutes after user actions (`c`, refresh,
//!     state change)
//!
//! Every 5th poll is a full refresh; the rest are `updatedAt > since`
//! delta queries.

use std::time::{SystemTime, UNIX_EPOCH};

pub const IDLE_CADENCE_SECS: f64 = 60.0;
pub const BURST_CADENCE_SECS: f64 = 5.0;
pub const BURST_WINDOW_SECS: u64 = 120;
pub const FULL_REFRESH_EVERY: u64 = 5;

#[derive(Default)]
pub struct PollState {
    pub poll_count: u64,
    /// Unix seconds at which burst mode ends (0 = idle).
    pub burst_until: u64,
    /// ISO-8601 cursor passed to delta queries; `None` before the first
    /// full refresh.
    pub last_updated_at: Option<String>,
    /// `true` while a poll request is in flight; suppresses overlapping
    /// fires.
    pub in_flight: bool,
}

impl PollState {
    pub fn enter_burst(&mut self) {
        self.burst_until = now_unix().saturating_add(BURST_WINDOW_SECS);
    }

    pub fn is_bursting(&self) -> bool {
        now_unix() < self.burst_until
    }

    /// Cadence to use for the *next* scheduled poll.
    pub fn next_cadence_secs(&self) -> f64 {
        if self.is_bursting() {
            BURST_CADENCE_SECS
        } else {
            IDLE_CADENCE_SECS
        }
    }

    /// `true` if the upcoming poll should be a full refresh (no `since`
    /// cursor).
    pub fn should_full_refresh(&self) -> bool {
        self.last_updated_at.is_none() || self.poll_count % FULL_REFRESH_EVERY == 0
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
