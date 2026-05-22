//! `zellij-linear login` — Authorization Code + PKCE flow.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use linear_client::auth::{auth_from_token, exchange_code, save, AuthFile};
use linear_client::http::{HttpClient, HttpResponse, HttpVerb};
use linear_client::types::{GraphQLResponse, Viewer, ViewerWrapper};
use linear_client::{
    queries::Q_VIEWER, DEFAULT_SCOPES, LINEAR_CLIENT_ID, LINEAR_GRAPHQL, LINEAR_OAUTH_AUTHORIZE,
    LINEAR_OAUTH_CALLBACK_PORT,
};
use url::Url;

use crate::http_impl::ReqwestClient;
use crate::pkce::{challenge_from_verifier, generate_state, generate_verifier};

const CALLBACK_TIMEOUT: Duration = Duration::from_secs(300);

pub fn run() -> Result<()> {
    let verifier = generate_verifier();
    let challenge = challenge_from_verifier(&verifier);
    let csrf_state = generate_state();

    let bind_addr = format!("127.0.0.1:{LINEAR_OAUTH_CALLBACK_PORT}");
    let listener = tiny_http::Server::http(bind_addr.as_str()).map_err(|e| {
        anyhow!(
            "could not bind {bind_addr} for the OAuth callback ({e}). \
             Another process is likely using that port — close it and retry."
        )
    })?;
    let redirect_uri = format!("http://localhost:{LINEAR_OAUTH_CALLBACK_PORT}/cb");

    let mut authorize = Url::parse(LINEAR_OAUTH_AUTHORIZE)?;
    authorize
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", LINEAR_CLIENT_ID)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("scope", DEFAULT_SCOPES)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &csrf_state)
        .append_pair("prompt", "consent");
    let authorize_url = authorize.to_string();

    eprintln!("Opening browser to authorize zellij-linear…");
    if webbrowser::open(&authorize_url).is_err() {
        eprintln!(
            "Couldn't open a browser automatically. Visit this URL manually:\n{authorize_url}"
        );
    } else {
        eprintln!("If nothing opened, visit:\n{authorize_url}");
    }

    let (code, returned_state) = wait_for_callback(&listener, CALLBACK_TIMEOUT)?;
    if returned_state != csrf_state {
        bail!("OAuth state mismatch — aborting (possible CSRF)");
    }

    let http = ReqwestClient::new().context("constructing HTTP client")?;
    let token = exchange_code(&http, &code, &verifier, &redirect_uri)
        .context("exchanging authorization code for tokens")?;
    let viewer = fetch_viewer(&http, &token.access_token).context("fetching authenticated user")?;
    let auth: AuthFile = auth_from_token(token, viewer.id, viewer.email);
    save(&auth).context("persisting auth file")?;

    eprintln!(
        "Logged in as {} ({}). Tokens written to {}",
        auth.user_email,
        auth.user_id,
        linear_client::auth::auth_file_path().display()
    );
    Ok(())
}

fn wait_for_callback(listener: &tiny_http::Server, timeout: Duration) -> Result<(String, String)> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            bail!("timed out waiting for the OAuth callback after {timeout:?}");
        }
        let req = match listener.recv_timeout(remaining)? {
            Some(r) => r,
            None => continue,
        };

        let url = req.url().to_string();
        // tiny_http hands us a request-target (`/cb?code=...`); join it onto a
        // dummy base so we can pull query pairs out via url::Url.
        let parsed = Url::parse(&format!("http://localhost{url}"))
            .map_err(|e| anyhow!("malformed callback URL {url}: {e}"))?;
        let params: HashMap<String, String> = parsed.query_pairs().into_owned().collect();

        if let Some(err) = params.get("error") {
            let desc = params.get("error_description").cloned().unwrap_or_default();
            let body = format!(
                "<html><body><h2>Authorization failed</h2><p>{err}: {desc}</p></body></html>"
            );
            let _ = req.respond(html_response(&body));
            bail!("authorization server returned error `{err}`: {desc}");
        }

        let code = params
            .get("code")
            .cloned()
            .ok_or_else(|| anyhow!("callback missing `code` parameter"))?;
        let state = params
            .get("state")
            .cloned()
            .ok_or_else(|| anyhow!("callback missing `state` parameter"))?;

        let body = "<html><body style=\"font-family:system-ui;text-align:center;padding-top:4em\">\
            <h2>Authorized.</h2><p>You can close this tab.</p></body></html>";
        let _ = req.respond(html_response(body));
        return Ok((code, state));
    }
}

fn html_response(body: &str) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    tiny_http::Response::from_data(body.as_bytes().to_vec()).with_header(
        "Content-Type: text/html; charset=utf-8"
            .parse::<tiny_http::Header>()
            .unwrap(),
    )
}

fn fetch_viewer(http: &dyn HttpClient, access_token: &str) -> Result<Viewer> {
    let body = serde_json::to_vec(&serde_json::json!({ "query": Q_VIEWER }))?;
    let auth_header = format!("Bearer {access_token}");
    let resp: HttpResponse = http.request(
        LINEAR_GRAPHQL,
        HttpVerb::Post,
        &[
            ("Authorization", auth_header.as_str()),
            ("Content-Type", "application/json"),
        ],
        &body,
    )?;
    if !resp.is_success() {
        bail!(
            "fetching viewer failed: status {}: {}",
            resp.status,
            resp.body_as_str()
        );
    }
    let parsed: GraphQLResponse<ViewerWrapper<Viewer>> = serde_json::from_slice(&resp.body)?;
    if !parsed.errors.is_empty() {
        bail!(
            "Linear GraphQL errors: {}",
            parsed
                .errors
                .iter()
                .map(|e| e.message.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
    parsed
        .data
        .map(|d| d.viewer)
        .ok_or_else(|| anyhow!("viewer query returned no data"))
}
