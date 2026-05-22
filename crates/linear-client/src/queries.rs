//! GraphQL query strings. Kept as string constants — Linear's schema is
//! stable enough that hand-written queries are simpler than codegen.

/// Returns the authenticated user. Used after OAuth exchange to record
/// who logged in.
pub const Q_VIEWER: &str = r#"
query Viewer {
  viewer { id name email }
}
"#;

/// Full refresh: issues assigned to the viewer in the given project,
/// filtered by `state.type`. No `updatedAt` clause.
pub const Q_ASSIGNED_ISSUES_FULL: &str = r#"
query AssignedIssuesFull(
  $projectId: ID
  $stateTypes: [String!]
  $first: Int = 50
) {
  viewer {
    assignedIssues(
      filter: {
        project: { id: { eq: $projectId } }
        state: { type: { in: $stateTypes } }
      }
      orderBy: updatedAt
      first: $first
    ) {
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

/// Delta poll: same as full refresh but with `updatedAt: { gt: $since }`.
/// `$since` is required — for full refreshes use [`Q_ASSIGNED_ISSUES_FULL`].
pub const Q_ASSIGNED_ISSUES_DELTA: &str = r#"
query AssignedIssuesDelta(
  $projectId: ID
  $stateTypes: [String!]
  $since: DateTimeOrDuration!
  $first: Int = 50
) {
  viewer {
    assignedIssues(
      filter: {
        project: { id: { eq: $projectId } }
        state: { type: { in: $stateTypes } }
        updatedAt: { gt: $since }
      }
      orderBy: updatedAt
      first: $first
    ) {
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

/// Single issue with comments — reserved for the v0.2 detail view; the
/// plugin doesn't invoke this yet.
#[allow(dead_code)]
pub const Q_ISSUE_DETAIL: &str = r#"
query IssueDetail($id: String!) {
  issue(id: $id) {
    id
    identifier
    title
    description
    url
    updatedAt
    state { name type color }
    labels { nodes { name color } }
    comments(first: 50, orderBy: createdAt) {
      nodes {
        body
        createdAt
        user { name email }
      }
    }
  }
}
"#;
