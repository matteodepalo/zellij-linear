//! GraphQL query strings. Kept as string constants — Linear's schema is
//! stable enough that hand-written queries are simpler than codegen.

/// Returns the authenticated user. Used after OAuth exchange to record
/// who logged in.
pub const Q_VIEWER: &str = r#"
query Viewer {
  viewer { id name email }
}
"#;

/// Viewer-scoped issues query — used when `filter.assignee = "me"`.
/// The `$filter` variable is built in Rust to optionally include an
/// `updatedAt: { gt: ... }` clause (delta) or omit it (full refresh).
pub const Q_VIEWER_ISSUES: &str = r#"
query ViewerIssues($filter: IssueFilter, $first: Int = 50) {
  viewer {
    assignedIssues(filter: $filter, orderBy: updatedAt, first: $first) {
      nodes {
        id
        identifier
        title
        description
        priority
        url
        updatedAt
        state { name type color }
        labels { nodes { name color } }
      }
    }
  }
}
"#;

/// Top-level issues query — used when `filter.assignee = "any"` (the
/// default) or a specific user UUID. The `$filter` variable carries
/// project/state/assignee/updatedAt clauses assembled in Rust.
pub const Q_PROJECT_ISSUES: &str = r#"
query ProjectIssues($filter: IssueFilter, $first: Int = 50) {
  issues(filter: $filter, orderBy: updatedAt, first: $first) {
    nodes {
      id
      identifier
      title
      description
      priority
      url
      updatedAt
      state { name type color }
      labels { nodes { name color } }
    }
  }
}
"#;

/// Projects the viewer can see. Used by `zellij-linear init` to let
/// the user pick a project without leaving the terminal.
pub const Q_PROJECTS: &str = r#"
query Projects($first: Int = 100) {
  projects(first: $first, orderBy: updatedAt) {
    nodes {
      id
      name
      teams(first: 5) { nodes { key name } }
    }
  }
}
"#;
