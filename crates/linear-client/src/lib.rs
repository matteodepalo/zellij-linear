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

/// OAuth client ID registered with Linear. Replace before shipping or
/// running `zellij-linear login`. Register an app at
/// <https://linear.app/settings/api/applications>.
pub const LINEAR_CLIENT_ID: &str = "REPLACE_ME_WITH_REGISTERED_CLIENT_ID";
