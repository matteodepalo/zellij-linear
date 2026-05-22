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

/// Maximum time we'll wait on an outstanding request before assuming
/// the response is lost and allowing a retry.
pub const REQUEST_TIMEOUT_SECS: u64 = 30;

#[derive(Default)]
pub struct PollState {
    pub poll_count: u64,
    /// Unix seconds at which burst mode ends (0 = idle).
    pub burst_until: u64,
    /// ISO-8601 cursor passed to delta queries; `None` before the first
    /// full refresh.
    pub last_updated_at: Option<String>,
    /// `Some(req_id)` while a poll request is in flight. Responses
    /// whose context's `req_id` doesn't match this are stale and dropped.
    pub in_flight_req_id: Option<String>,
    /// Unix seconds at which the currently in-flight request was dispatched.
    /// Used to time out lost responses.
    pub in_flight_since: u64,
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
    /// cursor): either we have no cursor yet, or the poll counter has
    /// reached the next multiple of [`FULL_REFRESH_EVERY`].
    pub fn should_full_refresh(&self) -> bool {
        if self.last_updated_at.is_none() {
            return true;
        }
        self.poll_count > 0 && self.poll_count.is_multiple_of(FULL_REFRESH_EVERY)
    }

    pub fn is_in_flight(&self) -> bool {
        self.in_flight_req_id.is_some()
    }

    /// `true` if the in-flight request has been outstanding longer than
    /// [`REQUEST_TIMEOUT_SECS`]. Caller should clear the slot and retry.
    pub fn in_flight_timed_out(&self) -> bool {
        self.is_in_flight()
            && now_unix().saturating_sub(self.in_flight_since) > REQUEST_TIMEOUT_SECS
    }

    pub fn mark_dispatched(&mut self, req_id: String) {
        self.in_flight_req_id = Some(req_id);
        self.in_flight_since = now_unix();
    }

    pub fn clear_in_flight(&mut self) {
        self.in_flight_req_id = None;
        self.in_flight_since = 0;
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_refresh_required_until_cursor_set() {
        let mut p = PollState::default();
        assert!(p.should_full_refresh(), "no cursor → full refresh");
        p.last_updated_at = Some("2026-05-22T12:00:00Z".to_string());
        assert!(!p.should_full_refresh(), "cursor set, poll_count=0 → delta");
        p.poll_count = 1;
        assert!(!p.should_full_refresh());
        p.poll_count = 4;
        assert!(!p.should_full_refresh());
        p.poll_count = 5;
        assert!(p.should_full_refresh(), "every 5th → full");
        p.poll_count = 10;
        assert!(p.should_full_refresh());
    }

    #[test]
    fn cadence_switches_on_burst() {
        let mut p = PollState::default();
        assert_eq!(p.next_cadence_secs(), IDLE_CADENCE_SECS);
        p.enter_burst();
        assert_eq!(p.next_cadence_secs(), BURST_CADENCE_SECS);
    }

    #[test]
    fn in_flight_tracks_req_id() {
        let mut p = PollState::default();
        assert!(!p.is_in_flight());
        p.mark_dispatched("42".to_string());
        assert!(p.is_in_flight());
        assert_eq!(p.in_flight_req_id.as_deref(), Some("42"));
        p.clear_in_flight();
        assert!(!p.is_in_flight());
    }
}
