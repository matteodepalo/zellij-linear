#!/usr/bin/env bash
#
# One-shot installer for zellij-linear.
#
# Idempotent. Handles everything that doesn't require browser interaction:
#   - checks prerequisites (cargo, zellij)
#   - adds the wasm32-wasip1 Rust target
#   - builds + installs the wasm plugin into ~/.config/zellij/plugins
#   - installs the native CLI into ~/.cargo/bin via `cargo install`
#   - symlinks the sidebar layout into ~/.config/zellij/layouts
#
# What it can't do (browser- or workspace-specific):
#   - register your Linear OAuth application
#   - configure / login / init
# The script prints those next steps at the end.

set -euo pipefail

ZELLIJ_PLUGINS_DIR="${ZELLIJ_PLUGINS_DIR:-$HOME/.config/zellij/plugins}"
ZELLIJ_LAYOUTS_DIR="${ZELLIJ_LAYOUTS_DIR:-$HOME/.config/zellij/layouts}"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

require() {
    if ! command -v "$1" >/dev/null 2>&1; then
        printf 'error: required command `%s` not found.\n  %s\n' "$1" "$2" >&2
        exit 1
    fi
}

require cargo "Install Rust: https://rustup.rs/"
require zellij "Install Zellij: \`brew install zellij\` (macOS) or see https://zellij.dev/documentation/installation"
require rustup "Install rustup from https://rustup.rs/"

if ! rustup target list --installed 2>/dev/null | grep -q '^wasm32-wasip1$'; then
    echo "Adding wasm32-wasip1 Rust target..."
    rustup target add wasm32-wasip1
fi

echo "Building zellij-linear-plugin (release, wasm32-wasip1)..."
(cd "$REPO_ROOT" && cargo build --locked --release --target wasm32-wasip1 -p zellij-linear-plugin)

mkdir -p "$ZELLIJ_PLUGINS_DIR"
cp -f "$REPO_ROOT/target/wasm32-wasip1/release/zellij-linear-plugin.wasm" \
   "$ZELLIJ_PLUGINS_DIR/zellij-linear.wasm"
echo "Installed plugin -> $ZELLIJ_PLUGINS_DIR/zellij-linear.wasm"

echo "Installing zellij-linear CLI to ~/.cargo/bin..."
(cd "$REPO_ROOT" && cargo install --locked --path crates/zellij-linear)

mkdir -p "$ZELLIJ_LAYOUTS_DIR"
ln -sf "$REPO_ROOT/examples/layout.kdl" "$ZELLIJ_LAYOUTS_DIR/zellij-linear.kdl"
echo "Symlinked layout -> $ZELLIJ_LAYOUTS_DIR/zellij-linear.kdl"

cat <<'EOF'

Done. Next steps:

  1. Register a Linear OAuth application at
     https://linear.app/settings/api/applications
     with Redirect URI http://localhost:54173/cb

  2. Configure + log in:
       zellij-linear configure --client-id <YOUR_CLIENT_ID>
       zellij-linear login

  3. In any project folder, pick a Linear project and launch:
       zellij-linear init
       zellij --layout zellij-linear

If `zellij-linear` is reported as not found, add ~/.cargo/bin to your
PATH (one-liner for zsh):
       echo '. "$HOME/.cargo/env"' >> ~/.zshrc && exec zsh

EOF
