//! Shared client types and queries for the Linear.app API.
//!
//! HTTP transport is abstracted behind [`http::HttpClient`] so the
//! wasm plugin (which uses Zellij's async `web_request`) and the native
//! CLI (which uses `reqwest`) can share types, queries, and auth logic.

pub mod auth;
pub mod http;
pub mod queries;
pub mod types;

pub const LINEAR_OAUTH_AUTHORIZE: &str = "https://linear.app/oauth/authorize";
pub const LINEAR_OAUTH_TOKEN: &str = "https://api.linear.app/oauth/token";
pub const LINEAR_GRAPHQL: &str = "https://api.linear.app/graphql";

pub const DEFAULT_SCOPES: &str = "read,write,issues:create,comments:create";

/// OAuth client ID for the `zellij-plugin` application registered at
/// <https://linear.app/settings/api/applications>. The client ID is a
/// public identifier (it appears in browser-visible authorize URLs);
/// PKCE means no client secret is needed.
pub const LINEAR_CLIENT_ID: &str = "00850fe032cfc1101fe7371595551593";

/// Loopback port the OAuth callback listener binds to. Must match a
/// `Redirect URI` registered on the Linear application above. The
/// listener is bound only during `zellij-linear login`.
pub const LINEAR_OAUTH_CALLBACK_PORT: u16 = 54173;
