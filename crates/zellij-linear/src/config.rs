//! OAuth application config (client ID + callback port) used by
//! `login` and `token`. Lives at `~/.config/zellij-linear/config.toml`
//! on Unix and `%APPDATA%\zellij-linear\config.toml` on Windows.
//!
//! Two ways to set it:
//!   * Env vars: `ZELLIJ_LINEAR_CLIENT_ID` (and optionally
//!     `ZELLIJ_LINEAR_CALLBACK_PORT`) override anything in the file.
//!   * `zellij-linear configure --client-id=…` writes the file.

use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, bail, Context, Result};
use linear_client::DEFAULT_CALLBACK_PORT;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    pub client_id: String,
    #[serde(default = "default_callback_port")]
    pub callback_port: u16,
}

fn default_callback_port() -> u16 {
    DEFAULT_CALLBACK_PORT
}

/// `$HOME/.config/zellij-linear/config.toml` on Unix,
/// `%APPDATA%\zellij-linear\config.toml` on Windows.
pub fn config_file_path() -> PathBuf {
    config_dir().join("config.toml")
}

#[cfg(unix)]
fn config_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("zellij-linear")
}

#[cfg(windows)]
fn config_dir() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("zellij-linear")
}

#[cfg(not(any(unix, windows)))]
fn config_dir() -> PathBuf {
    PathBuf::from(".zellij-linear")
}

/// Resolve OAuth config. Env vars win over the config file.
pub fn load() -> Result<ClientConfig> {
    if let Ok(client_id) = std::env::var("ZELLIJ_LINEAR_CLIENT_ID") {
        let callback_port = std::env::var("ZELLIJ_LINEAR_CALLBACK_PORT")
            .ok()
            .map(|s| s.parse::<u16>())
            .transpose()
            .map_err(|e| anyhow!("ZELLIJ_LINEAR_CALLBACK_PORT must be a u16: {e}"))?
            .unwrap_or(DEFAULT_CALLBACK_PORT);
        return Ok(ClientConfig {
            client_id,
            callback_port,
        });
    }

    let path = config_file_path();
    if !path.exists() {
        bail!(
            "no OAuth config found.\n\n\
             Register an application at https://linear.app/settings/api/applications \
             with a redirect URI of http://localhost:{DEFAULT_CALLBACK_PORT}/cb, then run:\n\n\
             \tzellij-linear configure --client-id <YOUR_CLIENT_ID>\n\n\
             Or set the ZELLIJ_LINEAR_CLIENT_ID environment variable."
        );
    }
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let parsed: ClientConfig =
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    if parsed.client_id.trim().is_empty() {
        bail!("client_id in {} is empty", path.display());
    }
    Ok(parsed)
}

pub fn save(cfg: &ClientConfig) -> Result<()> {
    let path = config_file_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let text = toml::to_string_pretty(cfg).context("serializing client config")?;
    fs::write(&path, &text).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}
