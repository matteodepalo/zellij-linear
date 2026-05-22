//! Plugin state — owned by the `ZellijPlugin` impl.

use std::collections::BTreeMap;

use linear_client::types::Issue;
use zellij_tile::prelude::PaneInfo;

use crate::config::ProjectConfig;
use crate::poll::PollState;

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
    /// Transient status (e.g. "Sent ENG-142 to Claude").
    pub status_message: Option<String>,

    /// Monotonic counter used to tag `web_request` and `run_command`
    /// invocations so the result can be matched.
    pub req_counter: u64,

    /// True once the initial Linear fetch has come back (even if empty).
    pub initial_load_done: bool,

    /// Number of consecutive 401s; once we hit
    /// [`MAX_CONSECUTIVE_AUTH_FAILURES`] we stop refreshing automatically
    /// and surface a hard error instead.
    pub consecutive_auth_failures: u32,
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
}
