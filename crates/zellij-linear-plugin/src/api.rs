//! Linear request dispatch from inside the wasm plugin.
//!
//! The plugin uses Zellij's async `web_request`; responses arrive via
//! `Event::WebRequestResult` and are routed back by inspecting the
//! `context` BTreeMap we attach here.

use std::collections::BTreeMap;

use linear_client::queries::{Q_PROJECT_ISSUES, Q_VIEWER_ISSUES};
use linear_client::types::{GraphQLResponse, Issue, IssuesPayload};
use linear_client::LINEAR_GRAPHQL;
use serde_json::{json, Value};
use zellij_tile::prelude::{web_request, HttpVerb};

use crate::config::AssigneeFilter;

/// Tag attached to Linear issue polling requests.
pub const KIND_FETCH_ISSUES: &str = "fetch_issues";
/// Tag attached to the initial token shellout.
pub const KIND_GET_TOKEN: &str = "get_token";
/// Tag attached to the token re-fetch after a 401.
pub const KIND_REFRESH_TOKEN: &str = "refresh_token";
/// Tag attached to `open` / `xdg-open` invocations.
pub const KIND_OPEN_URL: &str = "open_url";

/// Context keys we attach to requests so responses can be routed.
pub mod ctx {
    pub const KIND: &str = "kind";
    pub const REQ_ID: &str = "req_id";
    /// `"true"` if the request is a full refresh, `"false"` for a delta.
    pub const FULL: &str = "full";
}

pub struct FetchOptions<'a> {
    pub access_token: &'a str,
    pub project_id: Option<&'a str>,
    pub state_types: &'a [String],
    /// Drives query selection (viewer-scoped vs top-level) and whether
    /// an `assignee` clause is added to the filter.
    pub assignee: &'a AssigneeFilter,
    /// ISO-8601 timestamp; `None` for a full refresh.
    pub since: Option<&'a str>,
    /// Echoes back via the `WebRequestResult` event so the caller can
    /// correlate with its in-flight request.
    pub req_id: &'a str,
}

pub fn fetch_assigned_issues(opts: FetchOptions<'_>) {
    let full = opts.since.is_none();
    let query = match opts.assignee {
        AssigneeFilter::Me => Q_VIEWER_ISSUES,
        AssigneeFilter::Any | AssigneeFilter::User(_) => Q_PROJECT_ISSUES,
    };
    let filter = build_issue_filter(opts.project_id, opts.state_types, opts.assignee, opts.since);
    let body = json!({
        "query": query,
        "variables": { "filter": filter },
    });
    let body_bytes = serde_json::to_vec(&body).unwrap_or_default();

    let mut headers = BTreeMap::new();
    headers.insert(
        "Authorization".to_string(),
        format!("Bearer {}", opts.access_token),
    );
    headers.insert("Content-Type".to_string(), "application/json".to_string());

    let mut context = BTreeMap::new();
    context.insert(ctx::KIND.to_string(), KIND_FETCH_ISSUES.to_string());
    context.insert(ctx::REQ_ID.to_string(), opts.req_id.to_string());
    context.insert(ctx::FULL.to_string(), full.to_string());

    web_request(LINEAR_GRAPHQL, HttpVerb::Post, headers, body_bytes, context);
}

/// Assemble an `IssueFilter` JSON object from the configured scope.
/// For viewer-scoped queries the `assignee` clause is omitted (it's
/// implied by `viewer.assignedIssues`); for top-level queries we add
/// `assignee.id.eq = <uuid>` only when targeting a specific user.
fn build_issue_filter(
    project_id: Option<&str>,
    state_types: &[String],
    assignee: &AssigneeFilter,
    since: Option<&str>,
) -> Value {
    let mut filter = serde_json::Map::new();
    if let Some(pid) = project_id {
        filter.insert("project".into(), json!({ "id": { "eq": pid } }));
    }
    filter.insert("state".into(), json!({ "type": { "in": state_types } }));
    if let AssigneeFilter::User(uuid) = assignee {
        filter.insert("assignee".into(), json!({ "id": { "eq": uuid } }));
    }
    if let Some(s) = since {
        filter.insert("updatedAt".into(), json!({ "gt": s }));
    }
    Value::Object(filter)
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
        let snippet = std::str::from_utf8(body)
            .unwrap_or("")
            .chars()
            .take(200)
            .collect::<String>();
        return ParsedIssues::Error(format!("HTTP {status}: {snippet}"));
    }
    let parsed: GraphQLResponse<IssuesPayload> = match serde_json::from_slice(body) {
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
        .map(IssuesPayload::into_nodes)
        .unwrap_or_default();
    ParsedIssues::Ok(issues)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_401_as_unauthorized() {
        match parse_issue_response(401, b"{}") {
            ParsedIssues::Unauthorized => {}
            other => panic!(
                "expected Unauthorized, got {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn surfaces_non_success_with_snippet() {
        match parse_issue_response(500, b"server boom") {
            ParsedIssues::Error(msg) => assert!(msg.contains("500"), "msg = {msg}"),
            _ => panic!("expected Error"),
        }
    }

    #[test]
    fn surfaces_graphql_errors() {
        let body = br#"{"data":null,"errors":[{"message":"invalid filter"}]}"#;
        match parse_issue_response(200, body) {
            ParsedIssues::Error(msg) => assert!(msg.contains("invalid filter")),
            _ => panic!("expected Error"),
        }
    }

    #[test]
    fn parses_empty_node_list() {
        let body = br#"{"data":{"viewer":{"assignedIssues":{"nodes":[]}}}}"#;
        match parse_issue_response(200, body) {
            ParsedIssues::Ok(issues) => assert!(issues.is_empty()),
            _ => panic!("expected Ok"),
        }
    }

    #[test]
    fn parses_one_issue() {
        // Double `##` delimiters so the embedded `#` in `#f00` doesn't
        // close the raw string.
        let body = br##"{
            "data":{"viewer":{"assignedIssues":{"nodes":[{
                "id":"abc","identifier":"ENG-1","title":"Hi","description":null,
                "priority":2.0,"url":"https://linear.app/x","updatedAt":"2026-05-22T00:00:00Z",
                "state":{"name":"In Progress","type":"started","color":"#f00"},
                "labels":{"nodes":[]}
            }]}}}
        }"##;
        match parse_issue_response(200, body) {
            ParsedIssues::Ok(issues) => {
                assert_eq!(issues.len(), 1);
                assert_eq!(issues[0].identifier, "ENG-1");
                assert_eq!(issues[0].state.state_type, "started");
            }
            _ => panic!("expected Ok"),
        }
    }

    #[test]
    fn parses_top_level_issues_shape() {
        // Response shape from `Q_PROJECT_ISSUES` — `data.issues.nodes`,
        // no `viewer` wrapper.
        let body = br##"{
            "data":{"issues":{"nodes":[{
                "id":"xyz","identifier":"MAT-30","title":"Consolidate","description":null,
                "priority":0.0,"url":"https://linear.app/x","updatedAt":"2026-03-31T08:42:08Z",
                "state":{"name":"Backlog","type":"backlog","color":"#aaa"},
                "labels":{"nodes":[]}
            }]}}
        }"##;
        match parse_issue_response(200, body) {
            ParsedIssues::Ok(issues) => {
                assert_eq!(issues.len(), 1);
                assert_eq!(issues[0].identifier, "MAT-30");
                assert_eq!(issues[0].state.state_type, "backlog");
            }
            _ => panic!("expected Ok"),
        }
    }

    fn opts<'a>(
        token: &'a str,
        project: Option<&'a str>,
        states: &'a [String],
        assignee: &'a AssigneeFilter,
        since: Option<&'a str>,
    ) -> FetchOptions<'a> {
        FetchOptions {
            access_token: token,
            project_id: project,
            state_types: states,
            assignee,
            since,
            req_id: "1",
        }
    }

    #[test]
    fn filter_for_any_omits_assignee_clause() {
        let states = vec!["backlog".to_string()];
        let f = build_issue_filter(Some("pid"), &states, &AssigneeFilter::Any, None);
        assert!(
            f.get("assignee").is_none(),
            "no assignee clause for Any: {f}"
        );
        assert!(f.get("project").is_some(), "project clause kept");
    }

    #[test]
    fn filter_for_user_includes_uuid_clause() {
        let states = vec!["backlog".to_string()];
        let f = build_issue_filter(
            Some("pid"),
            &states,
            &AssigneeFilter::User("u-1".into()),
            None,
        );
        assert_eq!(f["assignee"]["id"]["eq"], "u-1");
    }

    #[test]
    fn filter_for_me_omits_assignee_clause() {
        // Viewer-scoped query — the `viewer.assignedIssues` field on the
        // query side enforces the assignee implicitly.
        let states = vec!["backlog".to_string()];
        let f = build_issue_filter(Some("pid"), &states, &AssigneeFilter::Me, None);
        assert!(f.get("assignee").is_none());
    }

    #[test]
    fn filter_with_since_adds_updated_at() {
        let states = vec!["backlog".to_string()];
        let f = build_issue_filter(
            Some("pid"),
            &states,
            &AssigneeFilter::Any,
            Some("2026-05-22T00:00:00Z"),
        );
        assert_eq!(f["updatedAt"]["gt"], "2026-05-22T00:00:00Z");
    }

    #[test]
    fn fetch_options_compile() {
        // Smoke check — exists purely to keep the constructor exercised
        // by the test binary so cargo flags an unused-fields drift.
        let states: Vec<String> = vec![];
        let assignee = AssigneeFilter::Any;
        let _ = opts("tok", None, &states, &assignee, None);
    }
}
