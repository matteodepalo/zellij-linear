mod config;
mod configure;
mod http_impl;
mod init;
mod login;
mod logout;
mod pkce;
mod status;
mod token;

use anyhow::Result;
use clap::{Parser, Subcommand};

/// Companion CLI for the zellij-linear plugin.
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Save the OAuth application client ID (and optional callback port).
    Configure {
        /// OAuth client ID from <https://linear.app/settings/api/applications>.
        #[arg(long)]
        client_id: String,
        /// Callback port. Must match a redirect URI registered on the
        /// Linear application (`http://localhost:<port>/cb`). Defaults
        /// to `DEFAULT_CALLBACK_PORT` (54173).
        #[arg(long)]
        callback_port: Option<u16>,
    },
    /// Pick a Linear project and write `./.linear.toml`.
    Init {
        /// Project name (case-insensitive substring) or UUID.
        /// Omit for an interactive picker.
        #[arg(long)]
        project: Option<String>,
        /// Overwrite an existing `.linear.toml`.
        #[arg(long)]
        force: bool,
    },
    /// Run the OAuth + PKCE authorization flow and persist tokens.
    Login,
    /// Delete the persisted auth file.
    Logout,
    /// Print human-readable auth state.
    Status,
    /// Print the current access token to stdout (refreshing if needed).
    Token,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Configure {
            client_id,
            callback_port,
        } => configure::run(client_id, callback_port),
        Cmd::Init { project, force } => init::run(project, force),
        Cmd::Login => login::run(),
        Cmd::Logout => logout::run(),
        Cmd::Status => status::run(),
        Cmd::Token => token::run(),
    }
}
