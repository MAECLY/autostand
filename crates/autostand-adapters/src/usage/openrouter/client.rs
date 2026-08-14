//! The two requests the `OpenRouter` probe makes.
//!
//! They are fetched independently on purpose: `OpenRouter` gates either endpoint
//! for particular key types, so a 403 from one must not blank out what the other
//! returned.

use crate::usage::creds::Secret;
use crate::usage::http::{self, HttpResponse};
use crate::usage::model::UsageError;

/// Account-wide credit balance and lifetime spend.
pub const CREDITS_URL: &str = "https://openrouter.ai/api/v1/credits";

/// Key metadata: tier and the optional per-key spend cap.
pub const KEY_URL: &str = "https://openrouter.ai/api/v1/key";

/// Fetch the account's credit totals.
pub async fn fetch_credits(key: &Secret) -> Result<HttpResponse, UsageError> {
    get(CREDITS_URL, key).await
}

/// Fetch this key's metadata.
pub async fn fetch_key(key: &Secret) -> Result<HttpResponse, UsageError> {
    get(KEY_URL, key).await
}

async fn get(url: &str, key: &Secret) -> Result<HttpResponse, UsageError> {
    http::get(
        url,
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
    use super::{CREDITS_URL, KEY_URL};

    #[test]
    fn the_endpoints_are_the_documented_v1_routes() {
        assert_eq!(CREDITS_URL, "https://openrouter.ai/api/v1/credits");
        assert_eq!(KEY_URL, "https://openrouter.ai/api/v1/key");
    }
}
