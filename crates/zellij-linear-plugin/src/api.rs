//! Linear request dispatch from inside the wasm plugin.
//!
//! The plugin uses Zellij's async `web_request`; responses arrive via
//! `Event::WebRequestResult` and are routed back by inspecting the
//! `context` BTreeMap we attach here.

use std::collections::BTreeMap;

use linear_client::queries::{Q_ASSIGNED_ISSUES_DELTA, Q_ASSIGNED_ISSUES_FULL};
use linear_client::types::{AssignedIssues, GraphQLResponse, Issue, ViewerWrapper};
use linear_client::LINEAR_GRAPHQL;
use serde_json::json;
use zellij_tile::prelude::{web_request, HttpVerb};

/// Tag attached to Linear issue polling requests.
pub const KIND_FETCH_ISSUES: &str = "fetch_issues";
/// Tag attached to the initial token shellout.
pub const KIND_GET_TOKEN: &str = "get_token";
/// Tag attached to the token re-fetch after a 401.
pub const KIND_REFRESH_TOKEN: &str = "refresh_token";

pub struct FetchOptions<'a> {
    pub access_token: &'a str,
    pub project_id: Option<&'a str>,
    pub state_types: &'a [String],
    /// ISO-8601 timestamp; `None` for a full refresh.
    pub since: Option<&'a str>,
    /// Echoes back via the `WebRequestResult` event so the caller can
    /// correlate with its in-flight request.
    pub req_id: &'a str,
}

pub fn fetch_assigned_issues(opts: FetchOptions<'_>) {
    let (query, variables) = match opts.since {
        Some(since) => (
            Q_ASSIGNED_ISSUES_DELTA,
            json!({
                "projectId": opts.project_id,
                "stateTypes": opts.state_types,
                "since": since,
            }),
        ),
        None => (
            Q_ASSIGNED_ISSUES_FULL,
            json!({
                "projectId": opts.project_id,
                "stateTypes": opts.state_types,
            }),
        ),
    };
    let body = json!({ "query": query, "variables": variables });
    let body_bytes = serde_json::to_vec(&body).unwrap_or_default();

    let mut headers = BTreeMap::new();
    headers.insert(
        "Authorization".to_string(),
        format!("Bearer {}", opts.access_token),
    );
    headers.insert("Content-Type".to_string(), "application/json".to_string());

    let mut context = BTreeMap::new();
    context.insert("kind".to_string(), KIND_FETCH_ISSUES.to_string());
    context.insert("req_id".to_string(), opts.req_id.to_string());

    web_request(LINEAR_GRAPHQL, HttpVerb::Post, headers, body_bytes, context);
}

pub enum ParsedIssues {
    /// Full or delta response with the parsed issues.
    Ok(Vec<Issue>),
    /// 401 — caller should re-fetch the token and retry once.
    Unauthorized,
    /// Any other failure (parse error, GraphQL error, transport error,
    /// non-success status code).
    Error(String),
}

pub fn parse_issue_response(status: u16, body: &[u8]) -> ParsedIssues {
    if status == 401 {
        return ParsedIssues::Unauthorized;
    }
    if !(200..300).contains(&status) {
        let snippet = std::str::from_utf8(body).unwrap_or("").chars().take(200).collect::<String>();
        return ParsedIssues::Error(format!("HTTP {status}: {snippet}"));
    }
    let parsed: GraphQLResponse<ViewerWrapper<AssignedIssues>> = match serde_json::from_slice(body)
    {
        Ok(p) => p,
        Err(e) => return ParsedIssues::Error(format!("parse error: {e}")),
    };
    if !parsed.errors.is_empty() {
        return ParsedIssues::Error(
            parsed
                .errors
                .iter()
                .map(|e| e.message.clone())
                .collect::<Vec<_>>()
                .join("; "),
        );
    }
    let issues = parsed
        .data
        .map(|d| d.viewer.assigned_issues.nodes)
        .unwrap_or_default();
    ParsedIssues::Ok(issues)
}
