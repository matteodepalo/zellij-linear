use std::collections::BTreeMap;

use zellij_tile::prelude::*;

mod api;
mod bridge;
mod config;
mod poll;
mod state;
mod ui;
mod util;

use crate::api::{
    ctx, fetch_assigned_issues, fetch_issue_detail, parse_detail_response, parse_issue_response,
    FetchOptions, ParsedDetail, ParsedIssues, KIND_FETCH_DETAIL, KIND_FETCH_ISSUES,
    KIND_GET_TOKEN, KIND_OPEN_URL, KIND_REFRESH_TOKEN,
};
use crate::bridge::{render_prompt, send_or_copy, SendOutcome, DEFAULT_PROMPT_TEMPLATE};
use linear_client::types::Issue;
use crate::state::{
    PluginMode, State, View, LIST_FIRST_ISSUE_LINE, LOADING_HOLD_MS,
    MAX_CONSECUTIVE_AUTH_FAILURES,
};
use crate::util::{debug_log, iso8601_now, set_debug};

register_plugin!(State);

const REQUIRED_PERMISSIONS: &[PermissionType] = &[
    PermissionType::WebAccess,
    PermissionType::ReadApplicationState,
    PermissionType::ChangeApplicationState,
    PermissionType::WriteToStdin,
    PermissionType::WriteToClipboard,
    PermissionType::RunCommands,
    // open_plugin_pane_floating dispatches under this gate; without it
    // the host drops the command silently and the shim panics on the
    // empty stdin response.
    PermissionType::OpenTerminalsOrPlugins,
];

const SUBSCRIBED_EVENTS: &[EventType] = &[
    EventType::Key,
    EventType::Mouse,
    EventType::PaneUpdate,
    EventType::WebRequestResult,
    EventType::RunCommandResult,
    EventType::PermissionRequestResult,
    EventType::Timer,
];

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        // Layout-level opt-in: `plugin { ... debug "true" }` flips
        // logging on even before .linear.toml has been read.
        if configuration.get("debug").map(|v| v == "true").unwrap_or(false) {
            set_debug(true);
        }
        debug_log("load: entered");

        // Detail mode is opt-in via plugin_config from the spawning
        // call to `open_plugin_pane_floating`. List mode is the default.
        if configuration.get("mode").map(String::as_str) == Some("detail") {
            self.mode = PluginMode::Detail;
            self.detail_issue_id = configuration.get("issue_id").cloned();
            debug_log(&format!(
                "load: detail mode, issue_id={:?}",
                self.detail_issue_id
            ));
        }

        self.plugin_config = configuration;
        self.loading_hold_until = crate::util::now_millis().saturating_add(LOADING_HOLD_MS);
        set_timeout((LOADING_HOLD_MS as f64 / 1000.0) + 0.05);

        subscribe(SUBSCRIBED_EVENTS);
        request_permission(REQUIRED_PERMISSIONS);

        match config::load_from_host() {
            Ok(Some(cfg)) => {
                if cfg.debug {
                    set_debug(true);
                }
                debug_log(&format!(
                    "load: cfg ok project_id={:?} assignee={:?} debug={}",
                    cfg.project_id, cfg.filter.assignee, cfg.debug
                ));
                self.config = Some(cfg);
            }
            Ok(None) => debug_log("load: no .linear.toml on /host"),
            Err(msg) => {
                debug_log(&format!("load: cfg error: {msg}"));
                self.config_error = Some(msg);
            }
        }
        debug_log("load: exiting");
    }

    fn update(&mut self, event: Event) -> bool {
        debug_log(&format!(
            "update: {:?}",
            std::mem::discriminant(&event)
        ));
        match event {
            Event::PermissionRequestResult(status) => self.on_permissions(status),
            Event::Key(key) => self.on_key(key),
            Event::Mouse(mouse) => self.on_mouse(mouse),
            Event::PaneUpdate(manifest) => self.on_panes(manifest),
            Event::WebRequestResult(status, _headers, body, context) => {
                self.on_web(status, body, context)
            }
            Event::RunCommandResult(exit, stdout, stderr, context) => {
                self.on_command(exit, stdout, stderr, context)
            }
            Event::Timer(_) => self.on_timer(),
            _ => false,
        }
    }

    fn pipe(&mut self, _pipe_message: PipeMessage) -> bool {
        false
    }

    fn render(&mut self, rows: usize, cols: usize) {
        debug_log(&format!(
            "render: r={rows} c={cols} perms={} token={} cfg={} loaded={} err={:?}",
            self.permissions_granted,
            self.access_token.is_some(),
            self.config.is_some(),
            self.initial_load_done,
            self.last_error
        ));
        self.prune_expired_status();
        ui::render(self, rows, cols);
    }
}

impl State {
    fn on_permissions(&mut self, status: PermissionStatus) -> bool {
        match status {
            PermissionStatus::Granted => {
                self.permissions_granted = true;
                self.fetch_token(KIND_GET_TOKEN);
            }
            PermissionStatus::Denied => {
                self.last_error =
                    Some("Permissions denied. Run the plugin again and approve.".to_string());
            }
        }
        true
    }

    fn fetch_token(&mut self, kind: &str) {
        let mut ctx = BTreeMap::new();
        ctx.insert("kind".to_string(), kind.to_string());
        ctx.insert("req_id".to_string(), self.next_req_id());
        if kind == KIND_REFRESH_TOKEN {
            self.auth_refresh_pending = true;
        }
        run_command(&["zellij-linear", "token"], ctx);
    }

    fn on_command(
        &mut self,
        exit: Option<i32>,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        context: BTreeMap<String, String>,
    ) -> bool {
        let kind = context.get("kind").cloned().unwrap_or_default();
        match kind.as_str() {
            KIND_GET_TOKEN | KIND_REFRESH_TOKEN => {
                if kind == KIND_REFRESH_TOKEN {
                    self.auth_refresh_pending = false;
                }
                if exit == Some(0) {
                    let token = String::from_utf8_lossy(&stdout).trim().to_string();
                    if token.is_empty() {
                        self.last_error =
                            Some("zellij-linear token returned empty output".to_string());
                    } else {
                        self.access_token = Some(token);
                        self.last_error = None;
                        self.kick_off_initial_fetch_or_retry(kind.as_str());
                    }
                } else {
                    let stderr_str = String::from_utf8_lossy(&stderr);
                    let msg = stderr_str
                        .lines()
                        .find(|l| !l.trim().is_empty())
                        .unwrap_or("Run `zellij-linear login` to authenticate")
                        .to_string();
                    self.last_error = Some(msg);
                }
                true
            }
            _ => false,
        }
    }

    fn kick_off_initial_fetch_or_retry(&mut self, kind: &str) {
        match self.mode {
            PluginMode::Detail => {
                // Single shot — no polling cadence, no recurring timer.
                self.dispatch_detail_fetch();
            }
            PluginMode::List => {
                // Gate on !is_in_flight so a timer that raced through during the
                // refresh shellout doesn't get its in-flight request overwritten
                // (the response would then be dropped as stale).
                if self.can_fetch() && !self.poll.is_in_flight() {
                    self.dispatch_fetch();
                }
                if kind == KIND_GET_TOKEN {
                    self.schedule_next_poll();
                }
            }
        }
    }

    fn dispatch_detail_fetch(&mut self) {
        let Some(token) = self.access_token.clone() else {
            return;
        };
        let Some(issue_id) = self.detail_issue_id.clone() else {
            self.last_error =
                Some("Detail mode launched with no `issue_id` in plugin config.".to_string());
            return;
        };
        let req_id = self.next_req_id();
        debug_log(&format!("dispatch_detail_fetch: issue_id={issue_id}"));
        fetch_issue_detail(&token, &issue_id, &req_id);
    }

    fn can_fetch(&self) -> bool {
        self.access_token.is_some()
            && !self.auth_refresh_pending
            && self
                .config
                .as_ref()
                .and_then(|c| c.project_id.as_ref())
                .is_some()
    }

    fn dispatch_fetch(&mut self) {
        let Some(token) = self.access_token.clone() else {
            return;
        };
        let Some(cfg) = self.config.as_ref() else {
            return;
        };
        let project_id = cfg.project_id.clone();
        let state_types = cfg.state_types();
        let assignee = cfg.assignee_filter();
        debug_log(&format!(
            "dispatch_fetch: assignee={assignee:?} project_id={project_id:?} states={state_types:?}"
        ));
        let full = self.poll.should_full_refresh();
        let since = if full {
            None
        } else {
            self.poll.last_updated_at.clone()
        };
        let req_id = self.next_req_id();
        self.poll.mark_dispatched(req_id.clone());
        fetch_assigned_issues(FetchOptions {
            access_token: &token,
            project_id: project_id.as_deref(),
            state_types: &state_types,
            assignee: &assignee,
            since: since.as_deref(),
            req_id: &req_id,
        });
    }

    fn on_web(&mut self, status: u16, body: Vec<u8>, context: BTreeMap<String, String>) -> bool {
        let kind = context.get(ctx::KIND).map(String::as_str).unwrap_or("");
        let snippet = std::str::from_utf8(&body)
            .unwrap_or("")
            .chars()
            .take(300)
            .collect::<String>();
        debug_log(&format!(
            "on_web: kind={kind} status={status} body[0..300]={snippet:?}"
        ));
        if kind == KIND_FETCH_DETAIL {
            return self.on_detail_response(status, &body);
        }
        if kind != KIND_FETCH_ISSUES {
            return false;
        }
        let req_id = context.get(ctx::REQ_ID).cloned().unwrap_or_default();
        if self.poll.in_flight_req_id.as_ref() != Some(&req_id) {
            // Stale response (likely from a request we already timed out
            // and retried). Drop it.
            return false;
        }
        let full = context.get(ctx::FULL).map(String::as_str) == Some("true");
        self.poll.clear_in_flight();
        match parse_issue_response(status, &body) {
            ParsedIssues::Ok(new_issues) => {
                let was_empty_full = full && new_issues.is_empty();
                self.merge_issues(new_issues, full);
                self.initial_load_done = true;
                self.poll.poll_count = self.poll.poll_count.saturating_add(1);
                self.last_error = None;
                self.consecutive_auth_failures = 0;
                // Empty full refresh: anchor the cursor to "now" so
                // subsequent polls run as deltas instead of perpetual
                // full refreshes.
                if was_empty_full && self.poll.last_updated_at.is_none() {
                    self.poll.last_updated_at = Some(iso8601_now());
                }
                // If the loading hold hasn't elapsed yet, wake up at
                // its end so the issue list actually replaces the
                // "Loading…" frame on screen.
                let remaining = self.loading_hold_remaining_ms();
                if remaining > 0 {
                    set_timeout((remaining as f64 / 1000.0).max(0.05));
                }
            }
            ParsedIssues::Unauthorized => {
                self.consecutive_auth_failures = self.consecutive_auth_failures.saturating_add(1);
                if self.consecutive_auth_failures > MAX_CONSECUTIVE_AUTH_FAILURES {
                    self.last_error = Some(
                        "Authentication failed repeatedly. Run `zellij-linear login` again."
                            .to_string(),
                    );
                } else {
                    self.last_error = Some("401 — refreshing token…".to_string());
                    self.fetch_token(KIND_REFRESH_TOKEN);
                }
            }
            ParsedIssues::Error(msg) => {
                self.last_error = Some(msg);
            }
        }
        true
    }

    fn on_detail_response(&mut self, status: u16, body: &[u8]) -> bool {
        match parse_detail_response(status, body) {
            ParsedDetail::Ok(issue) => {
                self.detail_issue = Some(issue);
                self.initial_load_done = true;
                self.last_error = None;
                self.consecutive_auth_failures = 0;
            }
            ParsedDetail::NotFound => {
                self.last_error = Some("Issue not found.".to_string());
                self.initial_load_done = true;
            }
            ParsedDetail::Unauthorized => {
                self.consecutive_auth_failures = self.consecutive_auth_failures.saturating_add(1);
                if self.consecutive_auth_failures > MAX_CONSECUTIVE_AUTH_FAILURES {
                    self.last_error = Some(
                        "Authentication failed. Run `zellij-linear login` again.".to_string(),
                    );
                } else {
                    self.last_error = Some("401 — refreshing token…".to_string());
                    self.fetch_token(KIND_REFRESH_TOKEN);
                }
            }
            ParsedDetail::Error(msg) => {
                self.last_error = Some(msg);
                self.initial_load_done = true;
            }
        }
        true
    }

    fn merge_issues(&mut self, incoming: Vec<linear_client::types::Issue>, full: bool) {
        if full || self.issues.is_empty() {
            self.issues = incoming;
        } else {
            // Delta: replace matching ids, prepend new ones. Note: this
            // leaves issues that transitioned *out* of the filter set
            // (e.g. moved to `completed`) in place until the next full
            // refresh (every 5th poll).
            for inc in incoming {
                if let Some(slot) = self.issues.iter_mut().find(|i| i.id == inc.id) {
                    *slot = inc;
                } else {
                    self.issues.insert(0, inc);
                }
            }
        }
        // Keep the cursor in range; record the newest updatedAt.
        if self.selected_idx >= self.issues.len() {
            self.selected_idx = self.issues.len().saturating_sub(1);
        }
        if let Some(newest) = self.issues.iter().map(|i| i.updated_at.as_str()).max() {
            self.poll.last_updated_at = Some(newest.to_string());
        }
    }

    fn on_timer(&mut self) -> bool {
        self.prune_expired_status();
        // Detail mode is a one-shot fetch; no recurring polls.
        if matches!(self.mode, PluginMode::Detail) {
            return true;
        }
        if self.poll.in_flight_timed_out() {
            self.poll.clear_in_flight();
        }
        if self.can_fetch() && !self.poll.is_in_flight() {
            self.dispatch_fetch();
        }
        self.schedule_next_poll();
        // Always render on timer ticks so the loading-hold elapse swaps
        // the "Loading…" frame for the issue list even if no other
        // event fires.
        true
    }

    fn schedule_next_poll(&self) {
        set_timeout(self.poll.next_cadence_secs());
    }

    fn on_panes(&mut self, manifest: PaneManifest) -> bool {
        self.panes = manifest.panes.into_iter().collect();
        false
    }

    fn on_key(&mut self, key: KeyWithModifier) -> bool {
        // Ignore key chords with Ctrl/Alt/Super so the plugin doesn't
        // accidentally trigger on `Ctrl+C`, `Alt+R`, etc.
        if key
            .key_modifiers
            .iter()
            .any(|m| matches!(m, KeyModifier::Ctrl | KeyModifier::Alt | KeyModifier::Super))
        {
            return false;
        }

        // Any keypress invalidates a stale transient status message.
        self.clear_status();

        // Detail mode has its own (smaller) keymap — just scroll + close.
        if matches!(self.mode, PluginMode::Detail) {
            return self.on_detail_key(key.bare_key);
        }

        match key.bare_key {
            BareKey::Enter => {
                self.open_selected_in_detail_pane();
                true
            }
            BareKey::Char('j') | BareKey::Down => {
                self.move_selection(1);
                true
            }
            BareKey::Char('k') | BareKey::Up => {
                self.move_selection(-1);
                true
            }
            BareKey::Char('g') => {
                self.selected_idx = 0;
                true
            }
            BareKey::Char('G') => {
                self.selected_idx = self.issues.len().saturating_sub(1);
                true
            }
            BareKey::Char('r') => {
                self.poll.enter_burst();
                if self.can_fetch() && !self.poll.is_in_flight() {
                    self.dispatch_fetch();
                }
                self.set_status("Refreshing…");
                true
            }
            // Zellij delivers capital letters as Char('C')/Char('Y')
            // directly — we don't see a Char('c') + Shift modifier.
            BareKey::Char('c') => {
                self.send_selected(false);
                true
            }
            BareKey::Char('C') => {
                self.send_selected(true);
                true
            }
            BareKey::Char('y') => {
                if let Some(issue) = self.selected_issue().cloned() {
                    self.copy_issue_body(&issue);
                }
                true
            }
            BareKey::Char('Y') => {
                if let Some(issue) = self.selected_issue().cloned() {
                    self.copy_issue_prompt(&issue);
                }
                true
            }
            BareKey::Char('o') => {
                if let Some(issue) = self.selected_issue() {
                    let mut cmd_ctx = BTreeMap::new();
                    cmd_ctx.insert(ctx::KIND.to_string(), KIND_OPEN_URL.to_string());
                    let opener = if cfg!(target_os = "macos") {
                        "open"
                    } else {
                        "xdg-open"
                    };
                    let url = issue.url.clone();
                    let identifier = issue.identifier.clone();
                    run_command(&[opener, &url], cmd_ctx);
                    self.set_status(&format!("Opening {identifier}"));
                }
                true
            }
            BareKey::Char('?') => {
                self.view = if self.view == View::Help {
                    View::List
                } else {
                    View::Help
                };
                true
            }
            BareKey::Esc => {
                if self.view == View::Help {
                    self.view = View::List;
                    true
                } else {
                    hide_self();
                    false
                }
            }
            _ => false,
        }
    }

    fn on_mouse(&mut self, mouse: Mouse) -> bool {
        // Same hygiene as keys: any pointer activity invalidates a
        // transient status message.
        self.clear_status();

        if matches!(self.mode, PluginMode::Detail) {
            return match mouse {
                Mouse::ScrollUp(_) => {
                    self.scroll_detail_by(-1);
                    true
                }
                Mouse::ScrollDown(_) => {
                    self.scroll_detail_by(1);
                    true
                }
                _ => false,
            };
        }

        match mouse {
            Mouse::ScrollUp(_) => {
                self.move_selection(-1);
                true
            }
            Mouse::ScrollDown(_) => {
                self.move_selection(1);
                true
            }
            Mouse::LeftClick(line, _col) => {
                // `line` is `isize` — Zellij can report negatives for
                // clicks above the pane origin, so guard before casting.
                if line < 0 {
                    return false;
                }
                let l = line as usize;
                if l < LIST_FIRST_ISSUE_LINE {
                    return false;
                }
                let row = l - LIST_FIRST_ISSUE_LINE;
                if row >= self.list_body_rows {
                    return false;
                }
                let idx = self.list_viewport_offset + row;
                if idx >= self.issues.len() {
                    return false;
                }
                self.selected_idx = idx;
                self.open_selected_in_detail_pane();
                true
            }
            _ => false,
        }
    }

    fn scroll_detail_by(&mut self, delta: isize) {
        if delta >= 0 {
            let by = delta as usize;
            self.detail_scroll = self
                .detail_scroll
                .saturating_add(by)
                .min(self.detail_max_scroll);
        } else {
            let by = delta.unsigned_abs();
            self.detail_scroll = self.detail_scroll.saturating_sub(by);
        }
    }

    fn on_detail_key(&mut self, key: BareKey) -> bool {
        match key {
            BareKey::Char('j') | BareKey::Down => {
                self.scroll_detail_by(1);
                true
            }
            BareKey::Char('k') | BareKey::Up => {
                self.scroll_detail_by(-1);
                true
            }
            BareKey::Char('g') => {
                self.detail_scroll = 0;
                true
            }
            BareKey::Char('G') => {
                self.detail_scroll = self.detail_max_scroll;
                true
            }
            BareKey::PageDown | BareKey::Char(' ') => {
                self.scroll_detail_by(10);
                true
            }
            BareKey::PageUp => {
                self.scroll_detail_by(-10);
                true
            }
            BareKey::Char('o') => {
                if let Some(issue) = self.detail_issue.as_ref() {
                    let mut cmd_ctx = BTreeMap::new();
                    cmd_ctx.insert(ctx::KIND.to_string(), KIND_OPEN_URL.to_string());
                    let opener = if cfg!(target_os = "macos") {
                        "open"
                    } else {
                        "xdg-open"
                    };
                    run_command(&[opener, &issue.url], cmd_ctx);
                }
                true
            }
            BareKey::Char('c') => {
                if let Some(detail) = self.detail_issue.as_ref() {
                    let issue = detail.as_summary();
                    self.send_issue(&issue, false);
                }
                true
            }
            BareKey::Char('C') => {
                if let Some(detail) = self.detail_issue.as_ref() {
                    let issue = detail.as_summary();
                    self.send_issue(&issue, true);
                }
                true
            }
            BareKey::Char('y') => {
                if let Some(detail) = self.detail_issue.as_ref() {
                    let issue = detail.as_summary();
                    self.copy_issue_body(&issue);
                }
                true
            }
            BareKey::Char('Y') => {
                if let Some(detail) = self.detail_issue.as_ref() {
                    let issue = detail.as_summary();
                    self.copy_issue_prompt(&issue);
                }
                true
            }
            BareKey::Char('q') | BareKey::Esc => {
                close_self();
                false
            }
            _ => false,
        }
    }

    fn open_selected_in_detail_pane(&mut self) {
        let Some(issue) = self.selected_issue() else {
            return;
        };
        let identifier = issue.identifier.clone();
        let mut config = BTreeMap::new();
        config.insert("mode".to_string(), "detail".to_string());
        config.insert("issue_id".to_string(), identifier.clone());
        // Carry the debug flag through so the spawned instance logs to
        // the same file if the user has it enabled.
        if crate::util::debug_enabled() {
            config.insert("debug".to_string(), "true".to_string());
        }
        let plugin_url = self
            .plugin_config
            .get("file_path")
            .cloned()
            .unwrap_or_else(|| {
                // The sidebar instance was loaded via this URL form; we
                // re-use the same one for the floating instance.
                "file:~/.config/zellij/plugins/zellij-linear.wasm".to_string()
            });
        debug_log(&format!(
            "open_selected_in_detail_pane: identifier={identifier} url={plugin_url}"
        ));
        // 80% × 80% centered — Zellij's default size for floating plugin
        // panes is ~half the screen, which is cramped for a Linear issue
        // with a real description and a comment thread. `PercentOrFixed`
        // isn't re-exported through `zellij-tile::prelude`, so build the
        // coords via the string-based constructor.
        let coords = FloatingPaneCoordinates::new(
            None,
            None,
            Some("80%".to_string()),
            Some("80%".to_string()),
            None,
            None,
        );
        open_plugin_pane_floating(&plugin_url, config, coords, BTreeMap::new());
        self.set_status(&format!("Opening {identifier}…"));
    }

    #[cfg(test)]
    pub(crate) fn merge_issues_for_test(
        &mut self,
        incoming: Vec<linear_client::types::Issue>,
        full: bool,
    ) {
        self.merge_issues(incoming, full);
    }

    fn send_selected(&mut self, auto_submit_override: bool) {
        let Some(issue) = self.selected_issue().cloned() else {
            return;
        };
        self.send_issue(&issue, auto_submit_override);
    }

    fn send_issue(&mut self, issue: &Issue, auto_submit_override: bool) {
        let cfg = self.config.clone();
        let template = cfg
            .as_ref()
            .and_then(|c| c.claude.prompt_template.clone())
            .unwrap_or_else(|| DEFAULT_PROMPT_TEMPLATE.to_string());
        let auto_submit = auto_submit_override
            || cfg
                .as_ref()
                .and_then(|c| c.claude.auto_submit)
                .unwrap_or(false);
        let target = cfg
            .as_ref()
            .map(|c| c.target_command().to_string())
            .unwrap_or_else(|| "claude".to_string());
        let prompt = render_prompt(issue, &template);
        match send_or_copy(&self.panes, &target, prompt, auto_submit) {
            SendOutcome::Sent => {
                self.set_status(&format!("Sent {} to Claude", issue.identifier));
                self.poll.enter_burst();
            }
            SendOutcome::Copied => {
                self.set_status(&format!(
                    "No `{target}` pane — copied prompt for {}",
                    issue.identifier
                ));
            }
        }
    }

    fn copy_issue_body(&mut self, issue: &Issue) {
        let body = issue.description.clone().unwrap_or_default();
        copy_to_clipboard(body);
        self.set_status("Copied issue body");
    }

    fn copy_issue_prompt(&mut self, issue: &Issue) {
        let template = self
            .config
            .as_ref()
            .and_then(|c| c.claude.prompt_template.clone())
            .unwrap_or_else(|| DEFAULT_PROMPT_TEMPLATE.to_string());
        let prompt = render_prompt(issue, &template);
        copy_to_clipboard(prompt);
        self.set_status("Copied formatted prompt");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use linear_client::types::{Issue, IssueState, LabelConnection};

    fn issue(id: &str, ident: &str, updated_at: &str) -> Issue {
        Issue {
            id: id.into(),
            identifier: ident.into(),
            title: format!("Title for {ident}"),
            description: None,
            priority: 3.0,
            state: IssueState {
                name: "Todo".into(),
                state_type: "unstarted".into(),
                color: "#000000".into(),
            },
            labels: LabelConnection::default(),
            url: format!("https://linear.app/x/{ident}"),
            updated_at: updated_at.into(),
        }
    }

    #[test]
    fn merge_full_replaces_existing() {
        let mut s = State {
            issues: vec![issue("old1", "ENG-1", "2026-05-22T00:00:00Z")],
            ..State::default()
        };
        s.merge_issues_for_test(vec![issue("new1", "ENG-9", "2026-05-22T01:00:00Z")], true);
        assert_eq!(s.issues.len(), 1);
        assert_eq!(s.issues[0].id, "new1");
    }

    #[test]
    fn merge_full_with_empty_clears() {
        let mut s = State {
            issues: vec![issue("a", "ENG-1", "2026-05-22T00:00:00Z")],
            ..State::default()
        };
        s.merge_issues_for_test(vec![], true);
        assert!(s.issues.is_empty());
    }

    #[test]
    fn merge_delta_prepends_new_ids() {
        let mut s = State {
            issues: vec![issue("a", "ENG-1", "2026-05-22T00:00:00Z")],
            ..State::default()
        };
        s.merge_issues_for_test(vec![issue("b", "ENG-2", "2026-05-22T02:00:00Z")], false);
        assert_eq!(s.issues.len(), 2);
        assert_eq!(s.issues[0].id, "b", "new id prepended");
        assert_eq!(s.issues[1].id, "a");
    }

    #[test]
    fn merge_delta_replaces_matching_id_in_place() {
        let mut s = State {
            issues: vec![
                issue("a", "ENG-1", "2026-05-22T00:00:00Z"),
                issue("b", "ENG-2", "2026-05-22T01:00:00Z"),
            ],
            ..State::default()
        };
        let mut updated = issue("a", "ENG-1", "2026-05-22T03:00:00Z");
        updated.title = "Updated title".to_string();
        s.merge_issues_for_test(vec![updated], false);
        assert_eq!(s.issues.len(), 2, "no new row");
        assert_eq!(s.issues[0].id, "a");
        assert_eq!(s.issues[0].title, "Updated title");
        assert_eq!(s.issues[1].id, "b", "untouched row keeps position");
    }

    #[test]
    fn merge_clamps_selected_idx_when_list_shrinks() {
        let mut s = State {
            issues: vec![
                issue("a", "ENG-1", "2026-05-22T00:00:00Z"),
                issue("b", "ENG-2", "2026-05-22T01:00:00Z"),
                issue("c", "ENG-3", "2026-05-22T02:00:00Z"),
            ],
            selected_idx: 2,
            ..State::default()
        };
        s.merge_issues_for_test(vec![issue("a", "ENG-1", "2026-05-22T00:00:00Z")], true);
        assert_eq!(s.selected_idx, 0, "clamped to last valid index");
    }

    #[test]
    fn merge_updates_cursor_to_newest_updated_at() {
        let mut s = State::default();
        s.merge_issues_for_test(
            vec![
                issue("a", "ENG-1", "2026-05-22T00:00:00Z"),
                issue("b", "ENG-2", "2026-05-22T05:00:00Z"),
                issue("c", "ENG-3", "2026-05-22T01:00:00Z"),
            ],
            true,
        );
        // CAUTION: lexicographic max equals chronological max only while
        // Linear emits a stable RFC 3339 format with `Z`.
        assert_eq!(
            s.poll.last_updated_at.as_deref(),
            Some("2026-05-22T05:00:00Z")
        );
    }

    #[test]
    fn merge_delta_into_empty_list_acts_like_full() {
        let mut s = State::default();
        s.merge_issues_for_test(vec![issue("a", "ENG-1", "2026-05-22T00:00:00Z")], false);
        assert_eq!(s.issues.len(), 1);
    }
}
