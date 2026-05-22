# zellij-linear roadmap

Proposed improvements, grouped by theme. Nothing here is committed — the
list exists so contributors (and future Claude sessions) can pick up
ideas with enough context to decide whether they're worth doing.

When a section says "sketch:", it's a rough implementation plan, not a
spec. Treat it as a starting point.

## Shipped

- **`filter.assignee` knob** — `any` (default) | `me` | `<uuid>`.
  Switches the sidebar between "everything in the project" and
  "what's on my plate." Implemented in [`config.rs`](crates/zellij-linear-plugin/src/config.rs)
  and [`api.rs`](crates/zellij-linear-plugin/src/api.rs).

## Workflow features

### Issue-detail overlay with comments

**Goal.** Inspect an issue without leaving the terminal — current
description plus the comment thread, scrollable.

**Sketch.**
- Add `View::Detail(issue_id)` to `state::View`.
- New GraphQL query: `query IssueDetail($id: String!) { issue(id) { ...; comments(first: 50, orderBy: createdAt) { nodes { body createdAt user { name } } } } }`.
- Fetch lazily on `Enter` or `d` keybind from the list; cache per-issue
  in `State` so toggling back is instant.
- Reuse `ui::text::truncate` and the existing pane-width logic.

### State transition on send to Claude

**Goal.** Pressing `c` should optionally move the issue to a workflow
state (e.g. `Backlog → In Progress`) so the Linear UI reflects what
you're actually working on.

**Sketch.**
- Re-add `claude.transition_on_send` to `ClaudeConfig` (name of the
  target workflow state).
- Resolve the state ID once at startup via a `query WorkflowStates($teamId: ID!)`
  call and cache it.
- After `send_or_copy()` returns `Sent`, fire a `mutation IssueUpdate($id: String!, $stateId: String!) { issueUpdate(id: $id, input: { stateId: $stateId }) { success } }`.
- Surface failures in the transient status line.

### `s` keybind: transition without leaving the sidebar

**Goal.** Quick state changes (`Backlog → Todo`, `In Progress → Done`)
without context-switching to the browser.

**Sketch.**
- On `s`, show a modal listing the team's workflow states (cached from
  the transition-on-send work above).
- Number keys or `j`/`k` to select; `Enter` confirms.
- Reuse the same `issueUpdate` mutation.

### Multi-project switcher

**Goal.** Toggle the sidebar between several projects without editing
`.linear.toml` and restarting Zellij.

**Sketch.**
- Allow `.linear.toml` to list multiple projects under a `[[projects]]`
  array (each with a name, ID, and optional filter).
- `Tab` cycles between configured projects.
- Persist last-selected project in a small state file under
  `~/.config/zellij-linear/` so it sticks across sessions.

### Cycle / priority filters

**Goal.** Narrow the sidebar to the current cycle or a single priority.

**Sketch.** Extend `FilterConfig` with `cycle = "current" | "next" | "<uuid>"`
and `priority = [1, 2]` (Linear priority ints). Both clauses go into the
`IssueFilter` JSON we already build in `api.rs::build_issue_filter`.

### Auto-resolve project from git remote

**Goal.** Skip `zellij-linear init` when a sensible default can be
inferred from the working directory.

**Sketch.**
- On plugin load, if no `.linear.toml` is found, shell out to
  `git config --get remote.origin.url`.
- Hash-match the URL against project names / known mappings stored in
  `~/.config/zellij-linear/projects.toml`.
- Surface a one-line "Inferred project: X — press `i` to confirm" hint
  rather than silently picking.

## Architecture

### Background polling shared across sessions

**Goal.** Today every Zellij session that loads the plugin polls Linear
independently — opening five sessions means 5× the request load on the
free-tier rate limit. A background daemon could poll once and push
updates to all live plugins.

**Sketch.**
- New crate `zellij-linear-daemon` running as a `launchd`/`systemd`
  user service.
- Daemon writes the latest issue snapshot to
  `~/.cache/zellij-linear/snapshot.json` and a Unix socket for push.
- Plugin reads from `/host/.cache/...` (via the auto-mounted cache
  dir) and falls back to its own polling if the daemon isn't running.

### Multi-team filter

**Goal.** Pin queries to a specific team — useful when a project spans
teams and you only want the issues from yours.

**Sketch.**
- Re-add `team` to `ProjectConfig` (was removed when we deleted dead
  code).
- `team` is a Linear team UUID *or* team key (resolve key → UUID via
  `query Team($key: String!)` on first use).
- Add `team: { id: { eq: $teamId } }` to the `IssueFilter` JSON.

### Stable cursor for delta polls

**Goal.** The current delta logic uses `max(issue.updatedAt)` as the
next-poll cursor. If Linear emits two events in the same millisecond,
one can be missed.

**Sketch.**
- Track a `last_polled_at` (server-clock) cursor independently of the
  newest issue's `updatedAt`.
- Use the GraphQL `_meta.serverTime` field if Linear exposes one;
  otherwise accept the minor risk and document it.

## Polish

### Pane-resize re-layout

**Goal.** Selecting a narrow row should not break alignment when the
sidebar resizes mid-session.

**Sketch.** `ui::list::format_issue_row` already truncates per render,
but `priority_icon` is single-byte while title may be multi-byte.
Audit `truncate` against grapheme boundaries (today it splits on `char`
which is wrong for combined emoji).

### Faster help overlay

**Goal.** `?` currently rebuilds the help text every render. Build it
once and cache.

**Sketch.** `static HELP_TEXT: OnceLock<String>` initialized lazily on
first toggle. Negligible perf gain, but the helper makes follow-up
overlays (issue detail, state picker) easier.

### Tests for the bridge under headless conditions

**Goal.** `send_or_copy` is the riskiest user-visible code path
(misroute → wrong pane); cover it with property tests across many
`PaneManifest` shapes.

**Sketch.** Property test via `proptest`: generate random pane trees,
assert that exactly the pane whose `terminal_command` contains the
target substring (and is not a plugin) is returned.
