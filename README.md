# zellij-linear

A [Zellij](https://zellij.dev) plugin that mirrors a Linear.app project as a
live sidebar and hands issues off to a [Claude Code](https://docs.claude.com/en/docs/claude-code/)
pane in the same session with a single keypress.

```
┌──────────────────────────────────────┬──────────────────────┐
│                                      │ Mobile app v2  (12)  │
│                                      │ ─────────────────────│
│ $ claude                             │ ! ENG-142  Fix login │
│ > ready                              │ ▲ ENG-141  Add ratel │
│                                      │ · ENG-138  Cache LRU │
│                                      │ · ENG-137  Polish UI │
│                                      │ …                    │
│                                      │ ─────────────────────│
│                                      │ [c] claude [r] [?]   │
└──────────────────────────────────────┴──────────────────────┘
```

Press `c` on a selected issue → its title + body + labels are pasted into
the `claude` pane; you press Enter to submit.

## What ships

| Artifact                    | Where it runs                | Purpose                                    |
| --------------------------- | ---------------------------- | ------------------------------------------ |
| `zellij-linear.wasm`        | inside Zellij (wasm32-wasip1)| sidebar UI + polling + claude bridge       |
| `zellij-linear` (native)    | host shell                   | OAuth + PKCE login, token helper           |

The plugin is read-mostly. For Claude-side **mutations** (creating issues,
posting comments, transitioning state), install the official Linear MCP
plugin from the Claude store — it composes well with this sidebar.

## Install

### 1. Build

```bash
git clone https://github.com/matteodepalo/zellij-linear
cd zellij-linear
rustup target add wasm32-wasip1
cargo build --release --target wasm32-wasip1 -p zellij-linear-plugin
cargo build --release -p zellij-linear
```

### 2. Install the artifacts

```bash
mkdir -p ~/.config/zellij/plugins
cp target/wasm32-wasip1/release/zellij-linear-plugin.wasm \
   ~/.config/zellij/plugins/zellij-linear.wasm
# Drop the binary anywhere on $PATH. `cargo install --path` works too.
cp target/release/zellij-linear /usr/local/bin/
```

### 3. Register a Linear OAuth application

zellij-linear uses OAuth 2.0 + PKCE — no API keys, no `client_secret`.

1. Open <https://linear.app/settings/api/applications> and create a new
   public application.
2. Add redirect URIs for the localhost ports tiny_http might bind to. The
   simplest is to set a single fixed redirect URI such as
   `http://localhost:54173/cb` and add `RFC8252` localhost loopback
   handling on the server side (Linear supports loopback per RFC8252).
3. Copy the resulting **client ID** into `crates/linear-client/src/lib.rs`
   (replace `REPLACE_ME_WITH_REGISTERED_CLIENT_ID`) and rebuild the CLI.

### 4. Log in

```bash
zellij-linear login
# browser opens → consent → "Logged in as you@example.com"
zellij-linear status  # sanity check
```

The auth file lives at `~/.config/zellij-linear/auth.json` and is
written `0600`.

### 5. Configure a project

```bash
cd ~/code/my-project
cp /path/to/zellij-linear/examples/.linear.toml .
$EDITOR .linear.toml      # set project_id (copy from Linear's URL)
```

### 6. Launch with the sidebar layout

```bash
zellij --layout /path/to/zellij-linear/examples/layout.kdl
```

The plugin pane will request permissions on first run. Approve them
once — Zellij caches the grant.

## Keybindings (plugin pane focused)

| Key      | Action                              |
| -------- | ----------------------------------- |
| `j` / ↓  | next issue                          |
| `k` / ↑  | previous issue                      |
| `g`      | jump to top                         |
| `G`      | jump to bottom                      |
| `r`      | refresh now                         |
| `c`      | send to Claude (paste only)         |
| `C`      | send to Claude + auto-submit        |
| `y`      | copy issue description              |
| `Y`      | copy formatted prompt               |
| `o`      | open in browser                     |
| `?`      | help overlay                        |
| `Esc`    | back / hide plugin                  |

## How it works

**Polling.** Linear has no GraphQL subscriptions, so the plugin polls.
60 s idle, 5 s burst for 2 minutes after user actions. Every 5th poll
is a full refresh; the rest are `updatedAt > since` delta queries.
Idle traffic is ~60 req/hour — about 1 % of Linear's 5000/hour cap.

**Auth.** The plugin can't touch `~/.config` directly (Zellij would need
`FullHdAccess`). Instead it shells out to `zellij-linear token`, which
prints the current access token (refreshing first if it's within 5
minutes of expiry). The plugin caches the token in memory and re-runs
the command on a 401.

**Send-to-Claude.** The plugin scans `PaneManifest` for a terminal pane
whose `terminal_command` contains `target_command` (default `"claude"`)
and writes the rendered prompt via `write_chars_to_pane_id`. If no
match is found, the prompt is copied to the clipboard instead.

## Configuration

Everything is in `examples/.linear.toml` with comments — copy it next to
any project. The only required field is `project_id`.

## Roadmap (out of v0.1)

- `s` keybind to transition state without leaving the sidebar
- Comments view in the issue detail overlay
- Multi-project switcher
- Auto-resolve project from `git remote origin` URL
- Background variant: one polling process shared across all sessions

## License

MIT — see `LICENSE`.
