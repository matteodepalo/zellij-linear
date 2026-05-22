//! `zellij-linear status` — human-readable auth state.

use anyhow::Result;
use linear_client::auth::{auth_file_path, load, AuthError};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn run() -> Result<()> {
    match load() {
        Ok(auth) => {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let remaining = auth.expires_at.saturating_sub(now);
            println!("Logged in as {} ({})", auth.user_email, auth.user_id);
            println!("Scope: {}", auth.scope);
            println!("Auth file: {}", auth_file_path().display());
            if remaining == 0 {
                println!("Access token expired — will refresh on next use");
            } else if remaining < 300 {
                println!("Access token expires in {remaining}s (will refresh on next use)");
            } else {
                println!("Access token expires in {remaining}s");
            }
        }
        Err(AuthError::NotLoggedIn(path)) => {
            println!("Not logged in (no auth file at {})", path.display());
            println!("Run `zellij-linear login` to authenticate.");
            std::process::exit(1);
        }
        Err(e) => return Err(e.into()),
    }
    Ok(())
}
