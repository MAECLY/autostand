//! The two requests the Grok probe makes.
//!
//! Both hit `cli-chat-proxy.grok.com`, the same proxy the Grok CLI itself talks
//! to — its `billing.rs` appends `/billing?format=credits` to this base URL — so
//! the endpoint carries the CLI's own stability guarantees rather than a scraped
//! web route. The `X-XAI-Token-Auth` header is what makes the proxy accept a CLI
//! token at all, which is why it is mirrored verbatim.
//!
//! Nothing here interprets a response: status, headers and body go straight to
//! the pure mapper.

use std::time::Duration;

use crate::usage::creds::Secret;
use crate::usage::http::{self, HttpResponse};
use crate::usage::model::UsageError;

/// The weekly shared-pool config, in the `credits` proto-JSON format the CLI uses.
pub const CREDITS_URL: &str = "https://cli-chat-proxy.grok.com/v1/billing?format=credits";

/// Account settings, read only for the subscription tier's display name.
pub const SETTINGS_URL: &str = "https://cli-chat-proxy.grok.com/v1/settings";

/// Marks the bearer as a Grok CLI token. Without it the proxy rejects the call.
pub const TOKEN_AUTH_HEADER: &str = "X-XAI-Token-Auth";

/// Value of [`TOKEN_AUTH_HEADER`], as the CLI sends it.
pub const TOKEN_AUTH_VALUE: &str = "xai-grok-cli";

/// Fetch the credits config. Required for a usable snapshot.
pub async fn fetch_credits(token: &Secret) -> Result<HttpResponse, UsageError> {
    get(CREDITS_URL, token).await
}

/// Fetch account settings. Best-effort: a failure costs the plan label, never a meter.
pub async fn fetch_settings(token: &Secret) -> Result<HttpResponse, UsageError> {
    get(SETTINGS_URL, token).await
}

async fn get(url: &str, token: &Secret) -> Result<HttpResponse, UsageError> {
    http::get(
        url,
        &[
            ("Authorization", token.bearer().as_str()),
            (TOKEN_AUTH_HEADER, TOKEN_AUTH_VALUE),
            ("Accept", "application/json"),
        ],
        http::DEFAULT_TIMEOUT,
    )
    .await
}

/// Per-request ceiling, restated so the provider page documents one number.
pub const TIMEOUT: Duration = http::DEFAULT_TIMEOUT;

#[cfg(test)]
mod tests {
    use super::{CREDITS_URL, SETTINGS_URL, TIMEOUT, TOKEN_AUTH_HEADER, TOKEN_AUTH_VALUE};

    #[test]
    fn the_endpoints_are_the_ones_the_cli_itself_calls() {
        assert!(CREDITS_URL.starts_with("https://cli-chat-proxy.grok.com/"));
        assert!(CREDITS_URL.ends_with("/v1/billing?format=credits"));
        assert_eq!(SETTINGS_URL, "https://cli-chat-proxy.grok.com/v1/settings");
    }

    #[test]
    fn the_cli_token_header_is_mirrored_verbatim() {
        assert_eq!(TOKEN_AUTH_HEADER, "X-XAI-Token-Auth");
        assert_eq!(TOKEN_AUTH_VALUE, "xai-grok-cli");
    }

    #[test]
    fn usage_never_blocks_a_refresh_for_more_than_ten_seconds() {
        assert_eq!(TIMEOUT.as_secs(), 10);
    }
}
