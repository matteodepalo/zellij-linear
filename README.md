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

The quick path is the bundled `install.sh`. It builds the wasm plugin,
installs the CLI, copies the wasm into Zellij's plugins directory, and
symlinks the sidebar layout — all idempotent.

```bash
git clone https://github.com/matteodepalo/zellij-linear
cd zellij-linear
./install.sh
```

If you'd rather drive each step yourself:

```bash
# Native CLI → ~/.cargo/bin/zellij-linear
cargo install --locked --path crates/zellij-linear

# Wasm plugin → ~/.config/zellij/plugins/zellij-linear.wasm
cargo build --locked --release --target wasm32-wasip1 -p zellij-linear-plugin
mkdir -p ~/.config/zellij/plugins
cp target/wasm32-wasip1/release/zellij-linear-plugin.wasm \
   ~/.config/zellij/plugins/zellij-linear.wasm

# Layout → ~/.config/zellij/layouts/zellij-linear.kdl
mkdir -p ~/.config/zellij/layouts
ln -sf "$(pwd)/examples/layout.kdl" ~/.config/zellij/layouts/zellij-linear.kdl
```

Verify:

```bash
zellij-linear --help
ls ~/.config/zellij/plugins/zellij-linear.wasm
ls ~/.config/zellij/layouts/zellij-linear.kdl
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
schema (state filters, assignee filter, Claude target command, prompt
template).

By default the sidebar shows **every issue in the project** the viewer
can see. To narrow it to "what's on my plate", set
`filter.assignee = "me"` in `.linear.toml`. You can also pin to a
specific user by passing their UUID.

## Launching

`install.sh` already symlinked the layout into Zellij's layout directory,
so you can refer to it by name from anywhere:

```bash
cd ~/code/my-project
zellij --layout zellij-linear
# or, to name the session at the same time:
zellij -s my-project -n zellij-linear
```

(`-n / --new-session-with-layout` always creates a fresh session; `-l`
combined with `-s` tries to *attach* to that session, which fails if it
doesn't exist.)

The layout puts your work pane on the left (75 %) and the sidebar on the
right (25 %). If you skipped `install.sh`, point at the file directly:

```bash
zellij --layout /path/to/zellij-linear/examples/layout.kdl
```

Or load the plugin into your own KDL — note the `focus=true` so the
first-run permission dialog receives your `y` keypress:

```kdl
pane size="25%" focus=true {
    plugin location="file:~/.config/zellij/plugins/zellij-linear.wasm"
}
```

### First-run permission prompt

The first time a fresh wasm is loaded, Zellij shows a permission dialog
in the plugin pane. Approve with `y`; Zellij caches the grant in
`~/Library/Caches/org.Zellij-Contributors.Zellij/permissions.kdl`
(macOS) so subsequent launches go straight to the sidebar.

**If the pane looks blank on first launch**, that's
[Zellij #4749](https://github.com/zellij-org/zellij/issues/4749) —
the host's permission prompt overflows in narrow tiled panes and
appears invisible. Two workarounds:

1. The bundled layout has `focus=true` on the plugin pane, so you can
   simply press `y` once — even if the prompt text isn't visible, the
   keypress is routed to Zellij's dialog and the grant goes through.
   Verify by re-launching: the sidebar should now render normally.
2. Pre-grant by manually writing the cache file (one-time setup):
   ```bash
   cat >> ~/Library/Caches/org.Zellij-Contributors.Zellij/permissions.kdl <<'EOF'
   "$HOME/.config/zellij/plugins/zellij-linear.wasm" {
       ReadApplicationState
       RunCommands
       WebAccess
       ChangeApplicationState
       WriteToStdin
       WriteToClipboard
       OpenTerminalsOrPlugins
   }
   EOF
   ```
   (Linux equivalent path: `~/.cache/zellij/permissions.kdl`.)

zellij-linear requests these capabilities. None of them is optional —
without each one, the listed feature stops working:

| Permission                | What it enables                                                                  |
| ------------------------- | -------------------------------------------------------------------------------- |
| `WebAccess`               | HTTP(S) calls to `api.linear.app/graphql` for issue polling                      |
| `ReadApplicationState`    | discover the Claude pane via `PaneManifest.terminal_command`                     |
| `ChangeApplicationState`  | focus the Claude pane before pasting, hide the plugin on `Esc`                   |
| `WriteToStdin`            | the actual paste — `write_chars_to_pane_id` into the Claude pane                 |
| `WriteToClipboard`        | clipboard fallback when no Claude pane is found, plus `y` / `Y` keybinds         |
| `RunCommands`             | `zellij-linear token` (auth refresh) and `open` / `xdg-open` for `o` (open URL)  |
| `OpenTerminalsOrPlugins`  | spawn the floating issue-detail pane (`Enter` on a list item)                    |

## Daily use

### Keybindings (plugin pane focused)

| Key      | Action                              |
| -------- | ----------------------------------- |
| `Enter`  | open issue in floating detail pane  |
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

Inside the floating detail pane:

| Key                    | Action                              |
| ---------------------- | ----------------------------------- |
| `j` / ↓                | scroll down one line                |
| `k` / ↑                | scroll up one line                  |
| `Space` / `PgDn`       | scroll down a page                  |
| `PgUp`                 | scroll up a page                    |
| `g`                    | jump to top                         |
| `G`                    | jump to bottom                      |
| `c`                    | send to Claude (paste only)         |
| `C`                    | send to Claude + auto-submit        |
| `y`                    | copy issue description              |
| `Y`                    | copy formatted prompt               |
| `o`                    | open issue in browser               |
| `q` / `Esc`            | close the detail pane               |

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

## Troubleshooting

Set `debug = true` at the top of `.linear.toml` to append diagnostics
to `/tmp/zellij-linear.log` (Zellij auto-mounts the host's TMPDIR
there — on macOS that's `/var/folders/.../T/zellij-501/zellij-linear.log`).
The log shows `load()` config parsing, every `update()` event,
`render()` calls with state snapshot, web request bodies, and the
resolved assignee filter. No-op when unset.

## Roadmap

See [`ROADMAP.md`](./ROADMAP.md) for proposed improvements (issue-detail
overlay, state transitions on send, multi-project switcher, etc.).

## License

MIT — see `LICENSE`.
