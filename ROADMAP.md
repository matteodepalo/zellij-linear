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
- **Floating detail pane** — `Enter` on a selected issue spawns a
  second plugin instance in a floating Zellij pane that fetches the
  issue with its comment thread and renders it scrollable. The
  sidebar keeps polling the list; the detail instance is one-shot.
  Implemented in [`ui/detail.rs`](crates/zellij-linear-plugin/src/ui/detail.rs).
- **State transition on send** — `claude.transition_on_send = "In
  Progress"` (or any state name) in `.linear.toml` moves the issue to
  that workflow state when its prompt is written to the Claude pane.
  Case-insensitive, team-scoped, skipped on clipboard fallback.
  Workflow-state IDs are cached at startup via
  [`Q_TEAMS_WITH_STATES`](crates/linear-client/src/queries.rs) so the
  send→transition path is a single mutation round-trip.

## Workflow features

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

### Webhook delivery via user-owned tunnel (optional)

**Goal.** Today the plugin polls Linear's GraphQL endpoint on a 60 s
idle / 5 s burst cadence. For users who already run a tailnet, an
opt-in webhook path could replace polling with push delivery — the
sidebar reflects state changes within a second instead of up to a
minute.

**Sketch.**

- Add a companion daemon (`zellij-linear webhook serve --port 8080`)
  that binds a tiny HTTP listener on `127.0.0.1:8080` with a `/linear`
  route. The plugin reads the latest issue snapshot from the daemon
  over Unix-socket IPC (or by tailing a file the daemon writes).
- User exposes the port publicly with **Tailscale Funnel**:
  ```bash
  tailscale funnel --bg 8080
  # → https://<machine>.<tailnet>.ts.net/ now routes to localhost:8080
  ```
  Funnel is included on the free Personal plan, terminates TLS at
  Tailscale's edge with an auto-issued cert, and only exposes ports
  443 / 8443 / 10000 publicly. ([Tailscale Funnel docs](https://tailscale.com/kb/1223/funnel))
- Configure the webhook directly on the **OAuth application** the
  user already created during `zellij-linear configure`. Linear's
  application settings (linear.app/settings/api/applications →
  pick the app → Webhooks toggle) expose:
  - a single **Webhook URL** field,
  - a **Webhook signing secret** (revealed/copied from that page),
  - per-event checkboxes (Issues, Comments, Issue Labels, Projects,
    Cycles, Documents, Users, etc.).
  This avoids the GraphQL `webhookCreate` mutation entirely for the
  common case — no admin-scope token, no extra CLI subcommand
  required. User pastes the Tailscale Funnel URL, ticks `Issues` +
  `Comments` + `Issue Labels`, and copies the signing secret into
  `~/.config/zellij-linear/webhook.toml` (or the OS keyring).
- Daemon validates every POST: HMAC-SHA256 of the **raw** body against
  the stored secret, constant-time compared to the `Linear-Signature`
  header. Dedup on the `Linear-Delivery` UUID. Reject events whose
  `webhookTimestamp` is older than ~5 minutes (replay defense).

**Why this isn't trivial — flag in the design before implementing.**

- *Tailscale dependency.* Requires the user to install Tailscale, sign
  in, and have Funnel allowed in the tailnet ACL (`nodeAttrs` with
  `funnel`). Corporate tailnets often forbid this.
- *Re-authorization required.* Linear shows a warning on the OAuth
  Webhooks pane: *"Anyone who has already authorized your application,
  including installations in this workspace, will need to re-authorize
  before webhooks will be received."* The setup flow has to instruct
  the user to re-run `zellij-linear login` after toggling webhooks on,
  otherwise nothing ever arrives.
- *Liveness gap.* Webhooks are only delivered while the daemon's port
  is reachable. Linear retries 3× over ~7 hours, then disables the
  webhook. If the user's machine sleeps or the daemon dies, events
  are lost. A low-frequency reconciliation poll (e.g., every 5 min,
  plus on plugin open) is still required to plug gaps.
- *Public hostname disclosure.* The webhook URL puts the user's
  `<machine>.<tailnet>.ts.net` name on Linear's webhook config page,
  which is visible to anyone with workspace-settings access.
- *Single URL per OAuth app.* Linear's OAuth-app webhook config holds
  one URL — using two machines (laptop + desktop) with the same OAuth
  app means only one of them receives events. Multi-machine users
  either run separate OAuth apps or fall back to polling on the
  non-primary host.

**Fallback.** Tunnel mode is opt-in via `.linear.toml`:

```toml
[webhook]
enabled = true
url = "https://my-laptop.tailnet-name.ts.net/linear"
# signing secret stored in OS keyring under "zellij-linear/webhook/<id>"
```

When the section is absent the plugin uses today's polling cadence —
no Tailscale, no admin scope, no extra deps. Mode selection happens
once at startup; the plugin UI doesn't change because both paths feed
the same in-memory issue store.

**Setup guide (write later).** When this lands, add a README section
walking the user through: (1) enable webhooks on their OAuth app at
`linear.app/settings/api/applications/<id>`, paste the Funnel URL,
tick `Issues`/`Comments`/`Issue Labels`, copy the signing secret;
(2) `tailscale funnel --bg 8080`; (3) re-run `zellij-linear login`
(re-auth required); (4) drop the `[webhook]` block in `.linear.toml`.
Do NOT add this guide to the README before implementation — it
references CLI flags and config keys that don't exist yet.

**Linear docs.** [Webhooks reference](https://linear.app/developers/webhooks)
covers payload shape, retry behavior, the `Linear-Signature` /
`Linear-Delivery` / `Linear-Event` headers, and the optional
`webhookCreate` GraphQL mutation (only needed if we ever need to
register a second webhook independent of the OAuth app).

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
