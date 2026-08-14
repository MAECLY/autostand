//! The two requests the Z.ai probe makes.
//!
//! Both are the internal APIs Z.ai's own subscription UI calls. The quota call
//! is required for a usable snapshot; the subscription call exists only to name
//! the plan, so it is best-effort and never fails a refresh.

use crate::usage::creds::Secret;
use crate::usage::http::{self, HttpResponse};
use crate::usage::model::UsageError;

/// Session token usage and web-search quotas. Required.
pub const QUOTA_URL: &str = "https://api.z.ai/api/monitor/usage/quota/limit";

/// The account's active subscription(s). Best-effort, plan name only.
pub const SUBSCRIPTION_URL: &str = "https://api.z.ai/api/biz/subscription/list";

/// Fetch the quota limits.
pub async fn fetch_quota(key: &Secret) -> Result<HttpResponse, UsageError> {
    get(QUOTA_URL, key).await
}

/// Fetch the subscription list.
pub async fn fetch_subscription(key: &Secret) -> Result<HttpResponse, UsageError> {
    get(SUBSCRIPTION_URL, key).await
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
    use super::{QUOTA_URL, SUBSCRIPTION_URL};

    #[test]
    fn the_endpoints_are_the_ones_the_subscription_ui_calls() {
        assert_eq!(QUOTA_URL, "https://api.z.ai/api/monitor/usage/quota/limit");
        assert_eq!(
            SUBSCRIPTION_URL,
            "https://api.z.ai/api/biz/subscription/list"
        );
    }
}
