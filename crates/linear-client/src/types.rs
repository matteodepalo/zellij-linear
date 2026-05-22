//! Type definitions mirroring the subset of the Linear GraphQL schema
//! that we consume. Field names use `camelCase` to match Linear's API.

use serde::{Deserialize, Serialize};

/// A Linear issue surfaced in the sidebar.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub id: String,
    pub identifier: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Linear returns priority as 0..=4 (0 = no priority, 1 = urgent, 4 = low).
    #[serde(default)]
    pub priority: f64,
    pub state: IssueState,
    #[serde(default)]
    pub labels: LabelConnection,
    pub url: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueState {
    pub name: String,
    /// One of: `backlog`, `unstarted`, `started`, `completed`, `canceled`, `triage`.
    #[serde(rename = "type")]
    pub state_type: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LabelConnection {
    #[serde(default)]
    pub nodes: Vec<Label>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Label {
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IssueConnection {
    #[serde(default)]
    pub nodes: Vec<Issue>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub teams: TeamConnection,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct TeamConnection {
    #[serde(default)]
    pub nodes: Vec<Team>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Team {
    pub key: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ProjectConnection {
    #[serde(default)]
    pub nodes: Vec<Project>,
}

/// Shape returned by [`Q_PROJECTS`](crate::queries::Q_PROJECTS).
#[derive(Debug, Clone, Deserialize)]
pub struct ProjectsRoot {
    pub projects: ProjectConnection,
}

/// Wraps any Linear GraphQL response payload.
#[derive(Debug, Clone, Deserialize)]
pub struct GraphQLResponse<T> {
    pub data: Option<T>,
    #[serde(default)]
    pub errors: Vec<GraphQLError>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GraphQLError {
    pub message: String,
    #[serde(default)]
    pub path: Vec<serde_json::Value>,
    #[serde(default)]
    pub extensions: serde_json::Value,
}

/// `{ viewer: { ... } }`
#[derive(Debug, Clone, Deserialize)]
pub struct ViewerWrapper<T> {
    pub viewer: T,
}

/// The shape returned by `Q_VIEWER`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Viewer {
    pub id: String,
    pub name: String,
    pub email: String,
}

/// `viewer { assignedIssues { nodes [...] } }`
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssignedIssues {
    pub assigned_issues: IssueConnection,
}

/// Numeric Linear priorities.
pub mod priority {
    pub const NO_PRIORITY: f64 = 0.0;
    pub const URGENT: f64 = 1.0;
    pub const HIGH: f64 = 2.0;
    pub const NORMAL: f64 = 3.0;
    pub const LOW: f64 = 4.0;
}

/// `state.type` values from Linear's `WorkflowStateType` enum.
pub mod state_type {
    pub const BACKLOG: &str = "backlog";
    pub const UNSTARTED: &str = "unstarted";
    pub const STARTED: &str = "started";
    pub const COMPLETED: &str = "completed";
    pub const CANCELED: &str = "canceled";
    pub const TRIAGE: &str = "triage";

    /// Default states the sidebar shows: things still to do or in progress.
    pub const DEFAULT_OPEN: &[&str] = &[BACKLOG, UNSTARTED, STARTED];
}
