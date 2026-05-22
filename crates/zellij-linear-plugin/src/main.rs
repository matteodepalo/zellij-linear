use std::collections::BTreeMap;

use zellij_tile::prelude::*;

mod api;
mod bridge;
mod config;
mod poll;
mod state;
mod ui;

use crate::api::{
    fetch_assigned_issues, parse_issue_response, FetchOptions, ParsedIssues, KIND_FETCH_ISSUES,
    KIND_GET_TOKEN, KIND_REFRESH_TOKEN,
};
use crate::bridge::{render_prompt, send_or_copy, SendOutcome, DEFAULT_PROMPT_TEMPLATE};
use crate::state::{State, View, MAX_CONSECUTIVE_AUTH_FAILURES};

register_plugin!(State);

const REQUIRED_PERMISSIONS: &[PermissionType] = &[
    PermissionType::WebAccess,
    PermissionType::ReadApplicationState,
    PermissionType::ChangeApplicationState,
    PermissionType::WriteToStdin,
    PermissionType::WriteToClipboard,
    PermissionType::RunCommands,
];

const SUBSCRIBED_EVENTS: &[EventType] = &[
    EventType::Key,
    EventType::PaneUpdate,
    EventType::TabUpdate,
    EventType::WebRequestResult,
    EventType::RunCommandResult,
    EventType::PermissionRequestResult,
    EventType::Timer,
];

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        self.plugin_config = configuration;

        // Subscribe before requesting permissions so we don't miss the
        // PermissionRequestResult event that triggers token bootstrap.
        subscribe(SUBSCRIBED_EVENTS);
        request_permission(REQUIRED_PERMISSIONS);

        match config::load_from_host() {
            Ok(Some(cfg)) => self.config = Some(cfg),
            Ok(None) => {} // surfaced in the renderer
            Err(msg) => self.config_error = Some(msg),
        }
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::PermissionRequestResult(status) => self.on_permissions(status),
            Event::Key(key) => self.on_key(key),
            Event::PaneUpdate(manifest) => self.on_panes(manifest),
            Event::TabUpdate(_) => false,
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
        match kind {
            KIND_GET_TOKEN => {
                if self.can_fetch() {
                    self.dispatch_fetch();
                }
                self.schedule_next_poll();
            }
            KIND_REFRESH_TOKEN => {
                if self.can_fetch() {
                    self.dispatch_fetch();
                }
            }
            _ => {}
        }
    }

    fn can_fetch(&self) -> bool {
        self.access_token.is_some()
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
        let full = self.poll.should_full_refresh();
        let since = if full {
            None
        } else {
            self.poll.last_updated_at.clone()
        };
        let req_id = self.next_req_id();
        self.poll.in_flight = true;
        fetch_assigned_issues(FetchOptions {
            access_token: &token,
            project_id: project_id.as_deref(),
            state_types: &state_types,
            since: since.as_deref(),
            req_id: &req_id,
        });
    }

    fn on_web(&mut self, status: u16, body: Vec<u8>, context: BTreeMap<String, String>) -> bool {
        let kind = context.get("kind").cloned().unwrap_or_default();
        if kind != KIND_FETCH_ISSUES {
            return false;
        }
        self.poll.in_flight = false;
        match parse_issue_response(status, &body) {
            ParsedIssues::Ok(new_issues) => {
                self.merge_issues(new_issues);
                self.initial_load_done = true;
                self.poll.poll_count = self.poll.poll_count.saturating_add(1);
                self.last_error = None;
                self.consecutive_auth_failures = 0;
            }
            ParsedIssues::Unauthorized => {
                self.consecutive_auth_failures =
                    self.consecutive_auth_failures.saturating_add(1);
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

    fn merge_issues(&mut self, incoming: Vec<linear_client::types::Issue>) {
        if self.poll.should_full_refresh() || self.issues.is_empty() {
            self.issues = incoming;
        } else {
            // Delta: replace matching ids, prepend new ones.
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
        if self.can_fetch() && !self.poll.in_flight {
            self.dispatch_fetch();
        }
        self.schedule_next_poll();
        false
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
        if key.key_modifiers.iter().any(|m| {
            matches!(
                m,
                KeyModifier::Ctrl | KeyModifier::Alt | KeyModifier::Super
            )
        }) {
            return false;
        }

        // Any keypress invalidates a stale transient status message.
        self.status_message = None;

        match key.bare_key {
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
                if self.can_fetch() && !self.poll.in_flight {
                    self.dispatch_fetch();
                }
                self.status_message = Some("Refreshing…".to_string());
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
                if let Some(issue) = self.selected_issue() {
                    let body = issue.description.clone().unwrap_or_default();
                    copy_to_clipboard(body);
                    self.status_message = Some("Copied issue body".to_string());
                }
                true
            }
            BareKey::Char('Y') => {
                if let Some(issue) = self.selected_issue() {
                    let template = self
                        .config
                        .as_ref()
                        .and_then(|c| c.claude.prompt_template.clone())
                        .unwrap_or_else(|| DEFAULT_PROMPT_TEMPLATE.to_string());
                    let prompt = render_prompt(issue, &template);
                    copy_to_clipboard(prompt);
                    self.status_message = Some("Copied formatted prompt".to_string());
                }
                true
            }
            BareKey::Char('o') => {
                if let Some(issue) = self.selected_issue() {
                    let mut ctx = BTreeMap::new();
                    ctx.insert("kind".to_string(), "open_url".to_string());
                    let opener = if cfg!(target_os = "macos") {
                        "open"
                    } else {
                        "xdg-open"
                    };
                    run_command(&[opener, &issue.url], ctx);
                    self.status_message = Some(format!("Opening {}", issue.identifier));
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

    fn send_selected(&mut self, auto_submit_override: bool) {
        let Some(issue) = self.selected_issue().cloned() else {
            return;
        };
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
        let prompt = render_prompt(&issue, &template);
        match send_or_copy(&self.panes, &target, prompt, auto_submit) {
            SendOutcome::Sent => {
                self.status_message = Some(format!("Sent {} to Claude", issue.identifier));
                self.poll.enter_burst();
            }
            SendOutcome::Copied => {
                self.status_message = Some(format!(
                    "No `{target}` pane — copied prompt for {}",
                    issue.identifier
                ));
            }
        }
    }
}
