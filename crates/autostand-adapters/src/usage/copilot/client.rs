//! Copilot's two request shapes: the per-seat quota, and organization billing.
//!
//! The seat call mirrors the headers the official Copilot chat client sends,
//! including the `token` authorization scheme — `/copilot_internal/user` does
//! not accept `Bearer`. The version strings live in constants here so mirroring
//! a newer client is a one-line change.
//!
//! The org-billing calls use GitHub's public REST API and are only reached for
//! an org-managed seat, where the per-seat endpoint reports no quota of its own.

use std::time::Duration;

use crate::usage::http::{self, HttpResponse};
use crate::usage::model::UsageError;

/// Per-seat quota endpoint.
const USAGE_URL: &str = "https://api.github.com/copilot_internal/user";

/// The organizations the token's user belongs to. One page of 100 is plenty for
/// finding which org provides a seat.
const USER_ORGS_URL: &str = "https://api.github.com/user/orgs?per_page=100";

/// Editor identity the seat endpoint answers.
const EDITOR_VERSION: &str = "vscode/1.96.2";

/// Copilot chat plugin identity, used for both the plugin and agent headers.
const PLUGIN_VERSION: &str = "copilot-chat/0.26.7";

/// `User-Agent` the official client sends.
const CLIENT_USER_AGENT: &str = "GitHubCopilotChat/0.26.7";

/// API version pinned by the Copilot client for the seat endpoint.
const COPILOT_API_VERSION: &str = "2025-04-01";

/// API version pinned for the public REST billing endpoints.
const REST_API_VERSION: &str = "2022-11-28";

/// GitHub can be slow under load; beyond this a probe has already failed the user.
const TIMEOUT: Duration = Duration::from_secs(15);

/// Fetch the signed-in user's Copilot seat quota.
pub async fn fetch_usage(token: &str) -> Result<HttpResponse, UsageError> {
    let authorization = format!("token {token}");
    http::get(
        USAGE_URL,
        &[
            ("Authorization", authorization.as_str()),
            ("Accept", "application/json"),
            ("Editor-Version", EDITOR_VERSION),
            ("Editor-Plugin-Version", PLUGIN_VERSION),
            ("User-Agent", CLIENT_USER_AGENT),
            ("X-Github-Api-Version", COPILOT_API_VERSION),
        ],
        TIMEOUT,
    )
    .await
}

/// Fetch the organizations the token's user belongs to.
pub async fn fetch_user_orgs(token: &str) -> Result<HttpResponse, UsageError> {
    rest_get(USER_ORGS_URL, token).await
}

/// Fetch one organization's month-to-date billing usage summary.
///
/// Reading an org's billing requires owner or billing-manager rights; a plain
/// member gets 403, which the caller treats as an expected state rather than a
/// failure.
pub async fn fetch_org_usage_summary(org: &str, token: &str) -> Result<HttpResponse, UsageError> {
    let Some(slug) = encode_org_slug(org) else {
        return Err(UsageError::UnsupportedPayload);
    };
    let url = format!("https://api.github.com/orgs/{slug}/settings/billing/usage/summary");
    rest_get(&url, token).await
}

async fn rest_get(url: &str, token: &str) -> Result<HttpResponse, UsageError> {
    let authorization = format!("token {token}");
    http::get(
        url,
        &[
            ("Authorization", authorization.as_str()),
            ("Accept", "application/vnd.github+json"),
            ("User-Agent", http::USER_AGENT),
            ("X-GitHub-Api-Version", REST_API_VERSION),
        ],
        TIMEOUT,
    )
    .await
}

/// Accept an org slug only in GitHub's own alphabet.
///
/// Slugs are alphanumeric plus hyphen and underscore. Rejecting anything else
/// keeps a value read from a response body from being spliced into a URL path.
#[must_use]
pub fn encode_org_slug(org: &str) -> Option<String> {
    let trimmed = org.trim();
    if trimmed.is_empty()
        || !trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    Some(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::encode_org_slug;

    #[test]
    fn accepts_a_plain_org_slug() {
        assert_eq!(
            encode_org_slug("  acme-corp_1 ").as_deref(),
            Some("acme-corp_1")
        );
    }

    #[test]
    fn rejects_anything_that_could_escape_the_url_path() {
        // The slug comes from a response body, so it never gets spliced in raw.
        for hostile in [
            "../../users",
            "acme/repos",
            "acme?per_page=1",
            "acme#frag",
            "",
        ] {
            assert_eq!(encode_org_slug(hostile), None, "{hostile}");
        }
    }
}
