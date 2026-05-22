mod http_impl;
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
        Cmd::Login => login::run(),
        Cmd::Logout => logout::run(),
        Cmd::Status => status::run(),
        Cmd::Token => token::run(),
    }
}
