//! One request: Devin's seat-management `GetUserStatus`.
//!
//! The endpoint is a Connect-RPC method, so the API key travels in the JSON
//! body's `metadata` rather than in an `Authorization` header, and the request
//! identifies itself as the Devin editor extension because that is the client
//! the endpoint answers.

use std::time::Duration;

use serde_json::json;

use crate::usage::http::{self, HttpResponse};
use crate::usage::model::UsageError;

/// Connect-RPC service that owns the user-status method.
const SERVICE: &str = "exa.seat_management_pb.SeatManagementService";

/// Extension version the request presents as.
///
/// One constant, so mirroring a newer Devin client is a one-line change.
pub const CLIENT_VERSION: &str = "1.108.2";

/// Devin's status call can be slower than a plain quota read, but a probe that
/// has not answered in this long has already failed the user.
const TIMEOUT: Duration = Duration::from_secs(15);

/// Fetch the signed-in user's status from `api_server_url`.
///
/// `api_server_url` comes from the local credentials file and has already been
/// restricted to `https` — the API key is in the body, so a plaintext host would
/// put it on the wire in the clear.
pub async fn fetch_user_status(
    api_key: &str,
    api_server_url: &str,
) -> Result<HttpResponse, UsageError> {
    let url = format!("{api_server_url}/{SERVICE}/GetUserStatus");
    let body = json!({
        "metadata": {
            "apiKey": api_key,
            "ideName": "devin",
            "ideVersion": CLIENT_VERSION,
            "extensionName": "devin",
            "extensionVersion": CLIENT_VERSION,
            "locale": "en"
        }
    });
    http::post_json(
        &url,
        &[
            ("Content-Type", "application/json"),
            ("Connect-Protocol-Version", "1"),
        ],
        &body,
        TIMEOUT,
    )
    .await
}
