//! `.linear.toml` schema — describes a single project folder's
//! relationship to a Linear project.

use serde::Deserialize;

#[derive(Deserialize, Debug, Clone, Default)]
pub struct ProjectConfig {
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    /// `debug = true` writes diagnostics to `/tmp/zellij-linear.log`.
    /// No-op by default.
    #[serde(default)]
    pub debug: bool,
    #[serde(default)]
    pub filter: FilterConfig,
    #[serde(default)]
    pub claude: ClaudeConfig,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct FilterConfig {
    /// Linear `state.type` values: backlog | unstarted | started | completed | canceled | triage.
    pub states: Option<Vec<String>>,
    /// `"any"` (default — every issue in the project), `"me"` (issues
    /// assigned to the authenticated user), or a Linear user UUID.
    pub assignee: Option<String>,
}

/// Resolved assignee filter. Drives both query selection (viewer-scoped
/// vs top-level) and which clauses end up in the GraphQL `IssueFilter`
/// object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssigneeFilter {
    /// `viewer.assignedIssues` — no explicit assignee clause needed.
    Me,
    /// Top-level `issues` with no assignee clause — all issues in the
    /// project the user can see.
    Any,
    /// Top-level `issues` with `assignee.id.eq = <uuid>`.
    User(String),
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct ClaudeConfig {
    /// Substring matched against `PaneInfo.terminal_command` to locate the
    /// Claude pane.
    pub target_command: Option<String>,
    /// `false` (default): paste prompt only; the user submits with Enter.
    /// `true`: append a trailing newline so the prompt auto-submits.
    pub auto_submit: Option<bool>,
    /// Override the default prompt template. Available `{placeholders}`:
    /// `{identifier}`, `{title}`, `{description}`, `{labels_as_bullets}`,
    /// `{url}`.
    pub prompt_template: Option<String>,
}

impl ProjectConfig {
    pub fn target_command(&self) -> &str {
        self.claude.target_command.as_deref().unwrap_or("claude")
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

    /// Parse `filter.assignee`. Unset or empty defaults to [`Any`].
    /// `"me"`/`"any"` are case-insensitive; anything else is treated as
    /// a Linear user UUID and passed through unchanged.
    pub fn assignee_filter(&self) -> AssigneeFilter {
        match self.filter.assignee.as_deref().map(str::trim) {
            None | Some("") => AssigneeFilter::Any,
            Some(v) => match v.to_ascii_lowercase().as_str() {
                "me" => AssigneeFilter::Me,
                "any" => AssigneeFilter::Any,
                _ => AssigneeFilter::User(v.to_string()),
            },
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

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_with_assignee(value: Option<&str>) -> ProjectConfig {
        ProjectConfig {
            filter: FilterConfig {
                states: None,
                assignee: value.map(str::to_string),
            },
            ..ProjectConfig::default()
        }
    }

    #[test]
    fn assignee_default_is_any() {
        assert_eq!(
            cfg_with_assignee(None).assignee_filter(),
            AssigneeFilter::Any
        );
        assert_eq!(
            cfg_with_assignee(Some("")).assignee_filter(),
            AssigneeFilter::Any
        );
    }

    #[test]
    fn assignee_me_and_any_are_case_insensitive() {
        assert_eq!(
            cfg_with_assignee(Some("me")).assignee_filter(),
            AssigneeFilter::Me
        );
        assert_eq!(
            cfg_with_assignee(Some("ME")).assignee_filter(),
            AssigneeFilter::Me
        );
        assert_eq!(
            cfg_with_assignee(Some("Any")).assignee_filter(),
            AssigneeFilter::Any
        );
    }

    #[test]
    fn assignee_uuid_passes_through() {
        let uuid = "57f37e8e-21f0-49f2-b944-6b718eb4bd1c";
        assert_eq!(
            cfg_with_assignee(Some(uuid)).assignee_filter(),
            AssigneeFilter::User(uuid.to_string())
        );
    }
}
