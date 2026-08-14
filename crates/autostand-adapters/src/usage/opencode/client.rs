//! The one request the `OpenCode` probe makes.
//!
//! `GET https://opencode.ai/zen/go/v1/usage` is `OpenCode`'s own Go usage API —
//! the same numbers its dashboard shows — so no vendor client is imitated here
//! and the default autostand `User-Agent` applies.

use crate::usage::creds::Secret;
use crate::usage::http::{self, HttpResponse};
use crate::usage::model::UsageError;

/// Go plan usage: session, weekly and monthly windows as percentages.
pub const USAGE_URL: &str = "https://opencode.ai/zen/go/v1/usage";

/// Fetch the Go plan's usage windows.
pub async fn fetch_usage(key: &Secret) -> Result<HttpResponse, UsageError> {
    http::get(
        USAGE_URL,
        &[
            ("Authorization", key.bearer().as_str()),
            ("Accept", "application/json"),
        ],
        http::DEFAULT_TIMEOUT,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::USAGE_URL;

    #[test]
    fn the_endpoint_is_opencodes_own_go_usage_api() {
        assert_eq!(USAGE_URL, "https://opencode.ai/zen/go/v1/usage");
    }
}
