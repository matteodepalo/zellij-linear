//! `zellij-linear configure` — write the OAuth application config so
//! `login` and `token` can find it.

use anyhow::Result;

use crate::config::{config_file_path, save, ClientConfig};

pub fn run(client_id: String, callback_port: Option<u16>) -> Result<()> {
    let cfg = ClientConfig {
        client_id: client_id.trim().to_string(),
        callback_port: callback_port.unwrap_or(linear_client::DEFAULT_CALLBACK_PORT),
    };
    save(&cfg)?;
    eprintln!(
        "Wrote {}\n  client_id = {}\n  callback_port = {}\n\n\
         Make sure http://localhost:{}/cb is registered as a redirect URI on the Linear app.",
        config_file_path().display(),
        cfg.client_id,
        cfg.callback_port,
        cfg.callback_port
    );
    Ok(())
}
