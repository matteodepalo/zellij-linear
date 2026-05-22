# zellij-linear

A [Zellij](https://zellij.dev) plugin that mirrors a [Linear](https://linear.app)
project as a live sidebar and hands issues off to a [Claude Code](https://docs.claude.com/en/docs/claude-code/)
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
the `claude` pane; press Enter to submit (or `C` to auto-submit). The
plugin is read-mostly; for Claude-side mutations (creating issues,
posting comments, transitioning state) install the official Linear MCP
plugin from the Claude store — it composes well with this sidebar.

## What ships

| Artifact                  | Where it runs                 | Purpose                                    |
| ------------------------- | ----------------------------- | ------------------------------------------ |
| `zellij-linear.wasm`      | inside Zellij (wasm32-wasip1) | sidebar UI, polling, send-to-Claude bridge |
| `zellij-linear` (native)  | host shell                    | OAuth + PKCE login, token helper, `init`   |

## Prerequisites

- **Zellij** 0.44+ — `brew install zellij` on macOS, or see the
  [install guide](https://zellij.dev/documentation/installation).
- **Rust toolchain** with the `wasm32-wasip1` target:
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  rustup target add wasm32-wasip1
  ```
  Make sure `~/.cargo/bin` is on your `PATH` — the rustup installer
  offers to add it; if you skipped that step add this line to your
  shell rc (`~/.zshrc` or `~/.bashrc`):
  ```sh
  . "$HOME/.cargo/env"
  ```

## Install

```bash
git clone https://github.com/matteodepalo/zellij-linear
cd zellij-linear

# Build + install the native CLI to ~/.cargo/bin/zellij-linear
cargo install --locked --path crates/zellij-linear

# Build + install the wasm plugin into Zellij's plugins directory
cargo build --locked --release --target wasm32-wasip1 -p zellij-linear-plugin
mkdir -p ~/.config/zellij/plugins
cp target/wasm32-wasip1/release/zellij-linear-plugin.wasm \
   ~/.config/zellij/plugins/zellij-linear.wasm
```

Verify:

```bash
zellij-linear --help
ls ~/.config/zellij/plugins/zellij-linear.wasm
```

## First-time setup

### 1. Register a Linear OAuth application

zellij-linear doesn't ship a baked-in OAuth client — every user
registers their own. Linear's OAuth apps are workspace-scoped and free:

1. Open <https://linear.app/settings/api/applications> and create a
   new application. PKCE means no client secret is needed.
2. Set **Redirect URI** to `http://localhost:54173/cb`. (If 54173 is
   taken on your machine, pick another port and pass it via
   `--callback-port` in step 2 below.)
3. Copy the resulting **client ID**.

### 2. Configure + log in

```bash
zellij-linear configure --client-id <YOUR_CLIENT_ID>
# or, if you used a different callback port:
# zellij-linear configure --client-id <ID> --callback-port 12345

zellij-linear login
# → browser opens to Linear's consent screen
# → "Logged in as you@example.com"
zellij-linear status   # sanity check
```

Config lives at `~/.config/zellij-linear/config.toml`; OAuth tokens at
`~/.config/zellij-linear/auth.json` (mode `0600`, refreshed
automatically). You can also pass `ZELLIJ_LINEAR_CLIENT_ID` and
`ZELLIJ_LINEAR_CALLBACK_PORT` env vars in lieu of the config file.

## Per-project setup

In each project folder you want the sidebar in, run:

```bash
cd ~/code/my-project
zellij-linear init                            # interactive picker
# or non-interactive:
zellij-linear init --project "OpenClaw"       # case-insensitive name match
zellij-linear init --project <UUID> --force   # overwrite an existing .linear.toml
```

`init` lists the projects on your Linear workspace, lets you pick one,
and writes `./.linear.toml`. See `examples/.linear.toml` for the full
schema (state filters, Claude target command, prompt template).

## Launching

```bash
cd ~/code/my-project
zellij --layout /path/to/zellij-linear/examples/layout.kdl
```

The layout puts your work pane on the left (75 %) and the sidebar on the
right (25 %). On first run, Zellij prompts to grant the plugin's
permissions — approve once, the grant is cached.

If you'd rather load the plugin into an existing layout, drop this into
its KDL:

```kdl
pane size="25%" {
    plugin location="file:~/.config/zellij/plugins/zellij-linear.wasm"
}
```

## Daily use

### Keybindings (plugin pane focused)

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

### CLI reference

| Command                                              | What it does                                                 |
| ---------------------------------------------------- | ------------------------------------------------------------ |
| `zellij-linear configure --client-id <ID>`           | Save OAuth client ID to `~/.config/zellij-linear/config.toml`|
| `zellij-linear login`                                | Run the OAuth + PKCE flow, persist tokens                    |
| `zellij-linear status`                               | Show who you're logged in as + token expiry                  |
| `zellij-linear logout`                               | Delete `auth.json`                                           |
| `zellij-linear token`                                | Print current access token (auto-refresh); used by the plugin|
| `zellij-linear init [--project <NAME\|UUID>] [--force]` | Write `./.linear.toml` for the current folder            |

## How it works

**Polling.** Linear has no GraphQL subscriptions, so the plugin polls.
60 s idle, 5 s burst for 2 minutes after user actions. Every 5th poll
is a full refresh; the rest are `updatedAt > since` delta queries.
Idle traffic is ~60 req/hour — about 1 % of Linear's 5000/hour cap.

**Auth.** The plugin can't touch `~/.config` directly (Zellij would
need `FullHdAccess`). Instead it shells out to `zellij-linear token`,
which prints the current access token (refreshing first if it's
within 5 minutes of expiry). The plugin caches the token in memory and
re-runs the command on a 401, with a backoff cap before giving up.

**Send-to-Claude.** The plugin scans `PaneManifest` for a terminal
pane whose `terminal_command` contains `target_command` (default
`"claude"`) and writes the rendered prompt via
`write_chars_to_pane_id`. If no match is found, the prompt is copied
to the clipboard instead.

## Configuration files

| File                                  | What it carries                                              |
| ------------------------------------- | ------------------------------------------------------------ |
| `~/.config/zellij-linear/config.toml` | OAuth client ID + callback port (written by `configure`)     |
| `~/.config/zellij-linear/auth.json`   | OAuth access + refresh tokens (written by `login`, `0600`)   |
| `./.linear.toml`                      | Per-project config (written by `init`); see `examples/`      |

## Roadmap (out of v0.1)

- `s` keybind to transition state without leaving the sidebar
- Comments view in an issue-detail overlay
- Multi-project switcher
- Auto-resolve project from `git remote origin` URL
- Background polling variant shared across sessions

## License

MIT — see `LICENSE`.
