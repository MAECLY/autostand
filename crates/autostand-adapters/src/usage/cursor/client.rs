//! Cursor's usage requests: the dashboard RPCs, and the two `cursor.com` REST
//! endpoints that authenticate with a session cookie instead of a bearer token.
//!
//! Only reads. `OpenUsage` also calls `POST /oauth/token` to rotate the access
//! token; that endpoint is deliberately absent here, because autostand does not
//! refresh another application's credential.

use std::time::Duration;

use serde_json::json;

use crate::usage::http::{self, HttpResponse};
use crate::usage::model::UsageError;

use super::auth::CursorSession;

/// Current billing period usage — the primary payload.
const USAGE_URL: &str = "https://api2.cursor.sh/aiserver.v1.DashboardService/GetCurrentPeriodUsage";

/// Plan name, used to label the snapshot and to pick the mapping shape.
const PLAN_URL: &str = "https://api2.cursor.sh/aiserver.v1.DashboardService/GetPlanInfo";

/// Prepaid credit grants.
const CREDITS_URL: &str =
    "https://api2.cursor.sh/aiserver.v1.DashboardService/GetCreditGrantsBalance";

/// Prepaid balance held on the billing provider.
const STRIPE_URL: &str = "https://cursor.com/api/auth/stripe";

/// Request-based usage, for plans metered in requests rather than spend.
const REST_USAGE_URL: &str = "https://cursor.com/api/usage";

/// The dashboard RPCs are Connect-RPC endpoints and answer quickly.
const TIMEOUT: Duration = Duration::from_secs(10);

/// `GET` the current billing period's usage.
pub async fn fetch_usage(token: &str) -> Result<HttpResponse, UsageError> {
    dashboard_post(USAGE_URL, token).await
}

/// `GET` the plan metadata. Optional: a failure only costs the plan label.
pub async fn fetch_plan(token: &str) -> Result<HttpResponse, UsageError> {
    dashboard_post(PLAN_URL, token).await
}

/// `GET` the prepaid credit grants. Optional.
pub async fn fetch_credit_grants(token: &str) -> Result<HttpResponse, UsageError> {
    dashboard_post(CREDITS_URL, token).await
}

/// `GET` the prepaid balance. Optional; cookie-authenticated.
pub async fn fetch_stripe_balance(session: &CursorSession) -> Result<HttpResponse, UsageError> {
    cookie_get(STRIPE_URL, session).await
}

/// `GET` request-based usage for the session's account. Cookie-authenticated.
pub async fn fetch_request_based_usage(
    session: &CursorSession,
) -> Result<HttpResponse, UsageError> {
    // `user_id` was already restricted to a URL-safe alphabet when the session
    // was built, so nothing read from a token can reshape this query.
    let url = format!("{REST_USAGE_URL}?user={}", session.user_id);
    cookie_get(&url, session).await
}

/// A Connect-RPC call: bearer token, empty JSON body, protocol version header.
async fn dashboard_post(url: &str, token: &str) -> Result<HttpResponse, UsageError> {
    let authorization = format!("Bearer {token}");
    http::post_json(
        url,
        &[
            ("Authorization", authorization.as_str()),
            ("Content-Type", "application/json"),
            ("Connect-Protocol-Version", "1"),
        ],
        &json!({}),
        TIMEOUT,
    )
    .await
}

async fn cookie_get(url: &str, session: &CursorSession) -> Result<HttpResponse, UsageError> {
    let cookie = session.cookie_header();
    http::get(
        url,
        &[("Cookie", cookie.as_str()), ("Accept", "application/json")],
        TIMEOUT,
    )
    .await
}
