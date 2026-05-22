//! Plugin state — owned by the `ZellijPlugin` impl.

use std::collections::BTreeMap;

use linear_client::types::Issue;
use zellij_tile::prelude::PaneInfo;

use crate::config::ProjectConfig;
use crate::poll::PollState;
use crate::util::{now_millis, now_unix};

/// Transient status messages auto-clear this many seconds after being set.
pub const STATUS_TTL_SECS: u64 = 5;

/// Minimum time the "Loading…" UI stays visible after `load()` even if
/// Linear's API answers faster. Without this, Zellij's terminal repaint
/// pipeline coalesces the load/auth/fetch renders into one frame and
/// the user sees a blank pane that abruptly becomes the issue list.
pub const LOADING_HOLD_MS: u64 = 600;

#[derive(Default)]
pub struct State {
    /// Granted after the `PermissionRequestResult` event fires.
    pub permissions_granted: bool,
    /// Set once `zellij-linear token` returns successfully.
    pub access_token: Option<String>,
    /// Parsed `.linear.toml`. `None` until first load attempt; the
    /// `config_error` field surfaces parse / missing failures.
    pub config: Option<ProjectConfig>,
    pub config_error: Option<String>,
    /// Plugin-level configuration passed in via `load(configuration)`.
    pub plugin_config: BTreeMap<String, String>,

    /// Current issues for the configured project filter.
    pub issues: Vec<Issue>,
    pub selected_idx: usize,
    pub view: View,
    pub poll: PollState,
    pub panes: BTreeMap<usize, Vec<PaneInfo>>,

    /// Sticky error surfaced in the footer (auth missing, GraphQL error,
    /// rate-limited, etc.).
    pub last_error: Option<String>,
    /// Transient status (e.g. "Sent ENG-142 to Claude"). The `u64` is the
    /// unix-seconds expiry; access via [`State::current_status`] which
    /// hides expired entries.
    pub status_message: Option<(String, u64)>,

    /// Monotonic counter used to tag `web_request` and `run_command`
    /// invocations so the result can be matched.
    pub req_counter: u64,

    /// True once the initial Linear fetch has come back (even if empty).
    pub initial_load_done: bool,

    /// Unix-millis timestamp until which [`is_loading`] returns `true`
    /// regardless of `initial_load_done`. Lets the loading UI stay
    /// visible for at least [`LOADING_HOLD_MS`] after `load()`.
    pub loading_hold_until: u64,

    /// Number of consecutive 401s; once it grows past
    /// [`MAX_CONSECUTIVE_AUTH_FAILURES`] we stop refreshing automatically
    /// and surface a hard error instead. With the constant at 2 we allow
    /// two recoveries (3 failures total) before giving up.
    pub consecutive_auth_failures: u32,

    /// `true` while a `KIND_REFRESH_TOKEN` shellout is outstanding. We
    /// gate fetches on this so a timer tick during the refresh can't
    /// race in and dispatch with the still-stale token (which would
    /// 401 and bump `consecutive_auth_failures` a second time off a
    /// single underlying credential failure).
    pub auth_refresh_pending: bool,
}

pub const MAX_CONSECUTIVE_AUTH_FAILURES: u32 = 2;

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum View {
    #[default]
    List,
    Help,
}

impl State {
    pub fn next_req_id(&mut self) -> String {
        self.req_counter = self.req_counter.wrapping_add(1);
        self.req_counter.to_string()
    }

    pub fn selected_issue(&self) -> Option<&Issue> {
        self.issues.get(self.selected_idx)
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.issues.is_empty() {
            self.selected_idx = 0;
            return;
        }
        let len = self.issues.len() as isize;
        let new = (self.selected_idx as isize + delta).clamp(0, len - 1);
        self.selected_idx = new as usize;
    }

    /// Set a transient status that will auto-clear after
    /// [`STATUS_TTL_SECS`] seconds.
    pub fn set_status(&mut self, msg: &str) {
        let expiry = now_unix().saturating_add(STATUS_TTL_SECS);
        self.status_message = Some((msg.to_string(), expiry));
    }

    pub fn clear_status(&mut self) {
        self.status_message = None;
    }

    /// Drop the status message if it's past its expiry. Call at the top
    /// of `render` and `on_timer`.
    pub fn prune_expired_status(&mut self) {
        if let Some((_, expiry)) = &self.status_message {
            if *expiry <= now_unix() {
                self.status_message = None;
            }
        }
    }

    /// `Some(msg)` if a status is set and not yet expired.
    pub fn current_status(&self) -> Option<&str> {
        match &self.status_message {
            Some((msg, expiry)) if *expiry > now_unix() => Some(msg.as_str()),
            _ => None,
        }
    }

    /// True if the renderer should still be showing the "Loading…"
    /// state: either we haven't received data yet, or the post-load
    /// hold window hasn't elapsed.
    pub fn is_loading(&self) -> bool {
        !self.initial_load_done || now_millis() < self.loading_hold_until
    }

    pub fn loading_hold_remaining_ms(&self) -> u64 {
        self.loading_hold_until.saturating_sub(now_millis())
    }
}
