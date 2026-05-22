//! `zellij-linear token` — print the current access token to stdout,
//! refreshing first when within 5 minutes of expiry. The wasm plugin
//! shells out to this on bootstrap and on 401s.

use anyhow::{Context, Result};
use linear_client::auth::{auth_from_token, load, refresh, save};

use crate::http_impl::ReqwestClient;

const REFRESH_SKEW_SECS: u64 = 300;

pub fn run() -> Result<()> {
    let mut auth = load().context("not logged in (run `zellij-linear login`)")?;

    if auth.needs_refresh(REFRESH_SKEW_SECS) {
        let http = ReqwestClient::new().context("constructing HTTP client")?;
        let token = refresh(&http, &auth.refresh_token).context("refreshing OAuth tokens")?;
        auth = auth_from_token(token, auth.user_id, auth.user_email);
        save(&auth).context("persisting refreshed auth file")?;
    }

    println!("{}", auth.access_token);
    Ok(())
}
