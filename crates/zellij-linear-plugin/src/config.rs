//! `.linear.toml` schema — describes a single project folder's
//! relationship to a Linear project.

use serde::Deserialize;

#[derive(Deserialize, Debug, Clone, Default)]
#[allow(dead_code)] // `team` is parsed for forward-compat with multi-team filters.
pub struct ProjectConfig {
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub team: Option<String>,
    #[serde(default)]
    pub filter: FilterConfig,
    #[serde(default)]
    pub claude: ClaudeConfig,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[allow(dead_code)] // `assignee` is reserved for v0.2 (multi-assignee filter).
pub struct FilterConfig {
    /// Linear `state.type` values: backlog | unstarted | started | completed | canceled | triage.
    pub states: Option<Vec<String>>,
    /// `"me"` (default), `"any"`, or a specific user id.
    pub assignee: Option<String>,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[allow(dead_code)] // `transition_on_send` is wired in v0.2.
pub struct ClaudeConfig {
    /// Substring matched against `PaneInfo.terminal_command` to locate the
    /// Claude pane.
    pub target_command: Option<String>,
    /// `false` (default): paste prompt only; the user submits with Enter.
    /// `true`: append a trailing newline so the prompt auto-submits.
    pub auto_submit: Option<bool>,
    /// v0.2 hook — name of the workflow state to transition to on send.
    /// Currently parsed but ignored.
    pub transition_on_send: Option<String>,
    /// Override the default prompt template. Available `{placeholders}`:
    /// `{identifier}`, `{title}`, `{description}`, `{labels_as_bullets}`,
    /// `{url}`.
    pub prompt_template: Option<String>,
}

impl ProjectConfig {
    pub fn target_command(&self) -> &str {
        self.claude
            .target_command
            .as_deref()
            .unwrap_or("claude")
    }

    pub fn state_types(&self) -> Vec<String> {
        match &self.filter.states {
            Some(s) if !s.is_empty() => s.clone(),
            _ => linear_client::types::state_type::DEFAULT_OPEN
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }
}

/// Load `/host/.linear.toml` (Zellij auto-mounts the session cwd at
/// `/host`). Returns `Ok(None)` if the file does not exist.
pub fn load_from_host() -> Result<Option<ProjectConfig>, String> {
    let path = "/host/.linear.toml";
    match std::fs::read_to_string(path) {
        Ok(text) => toml::from_str::<ProjectConfig>(&text)
            .map(Some)
            .map_err(|e| format!("parsing {path}: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("reading {path}: {e}")),
    }
}
