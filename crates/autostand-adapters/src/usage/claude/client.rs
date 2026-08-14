//! The single request the Claude usage probe makes.
//!
//! `GET https://api.anthropic.com/api/oauth/usage` is Anthropic's own,
//! undocumented endpoint, and it answers only for a request that looks like
//! Claude Code's. Mirroring that identity is a decision recorded in
//! `docs/specs/provider-usage.md` ("Risk accepted"), not an accident: without
//! the `anthropic-beta` header and the `claude-code/<v>` agent the endpoint does
//! not respond usefully.
//!
//! Note the absent `anthropic-version` header — Anthropic's own client omits it
//! here, and sending it changes the response.

use crate::usage::http::{self, HttpResponse};
use crate::usage::model::UsageError;

/// The usage endpoint. Production only: autostand never reads a staging or
/// local-OAuth credential, so it never calls a staging host.
pub const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";

/// The Claude Code release autostand identifies as.
///
/// **One constant, one place to bump.** Every request's `User-Agent` derives
/// from it, so keeping pace with the vendor is a one-line change.
pub const CLAUDE_CODE_VERSION: &str = "2.1.69";

/// The OAuth beta the usage endpoint is gated behind.
pub const ANTHROPIC_BETA: &str = "oauth-2025-04-20";

/// Fetch raw usage for one access token.
///
/// The token is placed in a header and dropped; it is never logged, never
/// returned, and never reaches an error. Transport failures come back as
/// [`UsageError::Network`] / [`UsageError::Timeout`], which carry no URL.
pub async fn fetch_usage(access_token: &str) -> Result<HttpResponse, UsageError> {
    let authorization = format!("Bearer {}", access_token.trim());
    let user_agent = format!("claude-code/{CLAUDE_CODE_VERSION}");
    http::get(
        USAGE_URL,
        &[
            ("Authorization", authorization.as_str()),
            ("Accept", "application/json"),
            ("Content-Type", "application/json"),
            ("anthropic-beta", ANTHROPIC_BETA),
            ("User-Agent", user_agent.as_str()),
        ],
        http::DEFAULT_TIMEOUT,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::{ANTHROPIC_BETA, CLAUDE_CODE_VERSION, USAGE_URL};

    #[test]
    fn the_endpoint_is_the_production_oauth_usage_path() {
        assert_eq!(USAGE_URL, "https://api.anthropic.com/api/oauth/usage");
        assert!(USAGE_URL.starts_with("https://"));
    }

    #[test]
    fn the_user_agent_version_lives_in_exactly_one_constant() {
        // The bump point promised by the spec: one string, one place.
        assert!(CLAUDE_CODE_VERSION
            .split('.')
            .all(|part| part.chars().all(|c| c.is_ascii_digit()) && !part.is_empty()));
        assert_eq!(
            format!("claude-code/{CLAUDE_CODE_VERSION}"),
            "claude-code/2.1.69"
        );
    }

    #[test]
    fn the_beta_header_is_the_oauth_one() {
        assert_eq!(ANTHROPIC_BETA, "oauth-2025-04-20");
    }
}
