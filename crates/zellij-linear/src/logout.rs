//! `zellij-linear logout` — delete the persisted auth file.

use anyhow::Result;
use linear_client::auth::{auth_file_path, delete};

pub fn run() -> Result<()> {
    let path = auth_file_path();
    if !path.exists() {
        println!("Not logged in — nothing to remove.");
        return Ok(());
    }
    delete()?;
    println!("Logged out. Removed {}.", path.display());
    Ok(())
}
