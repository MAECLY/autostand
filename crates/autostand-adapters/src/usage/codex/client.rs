//! One request: `GET /backend-api/wham/usage`.
//!
//! This replaces the `codex app-server --stdio` spawn the app used to run — no
//! child process, no eight-second protocol handshake, and it works whether or
//! not the `codex` CLI is on `PATH`.
//!
//! The request carries autostand's own `User-Agent`: unlike Claude's usage
//! endpoint, this one does not require imitating the vendor's client.
//!
//! Deliberately absent: the rate-limit **reset-credit claim**. Consuming a reset
//! credit is an irreversible account mutation and belongs nowhere near a usage
//! panel, so no code path here can perform one.

use crate::usage::http::{self, HttpResponse};
use crate::usage::model::UsageError;

use super::auth::CodexCredential;

/// The account usage endpoint the Codex clients read.
pub const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

/// Scopes the request to one account when the credential names one.
const ACCOUNT_HEADER: &str = "ChatGPT-Account-Id";

/// Fetch the raw usage response, keeping status, headers and body.
///
/// Errors are typed and content-free: no URL, header or body ever reaches an
/// error string, and the response itself is handed straight to the pure mapper.
pub async fn fetch_usage(credential: &CodexCredential) -> Result<HttpResponse, UsageError> {
    let owned = usage_headers(&credential.access_token, credential.account_id.as_deref());
    let headers: Vec<(&str, &str)> = owned
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    http::get(USAGE_URL, &headers, http::DEFAULT_TIMEOUT).await
}

/// The exact header set, built purely so it can be asserted without a network.
fn usage_headers(access_token: &str, account_id: Option<&str>) -> Vec<(String, String)> {
    let mut headers = vec![
        (
            "Authorization".to_string(),
            format!("Bearer {access_token}"),
        ),
        ("Accept".to_string(), "application/json".to_string()),
        ("User-Agent".to_string(), http::USER_AGENT.to_string()),
    ];
    if let Some(account_id) = account_id.map(str::trim).filter(|id| !id.is_empty()) {
        headers.push((ACCOUNT_HEADER.to_string(), account_id.to_string()));
    }
    headers
}

#[cfg(test)]
mod tests {
    use super::{usage_headers, ACCOUNT_HEADER, USAGE_URL};
    use crate::usage::http::USER_AGENT;

    fn value<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
        headers
            .iter()
            .find(|(header, _)| header == name)
            .map(|(_, value)| value.as_str())
    }

    #[test]
    fn the_request_mirrors_the_documented_contract() {
        let headers = usage_headers("token-abc", Some("acct-1"));
        assert_eq!(value(&headers, "Authorization"), Some("Bearer token-abc"));
        assert_eq!(value(&headers, "Accept"), Some("application/json"));
        assert_eq!(value(&headers, "User-Agent"), Some(USER_AGENT));
        assert_eq!(value(&headers, ACCOUNT_HEADER), Some("acct-1"));
        assert!(USAGE_URL.starts_with("https://"));
    }

    #[test]
    fn the_account_header_is_omitted_when_there_is_no_account() {
        // An empty id is "absent", not a header with an empty value: the endpoint
        // rejects a blank account scope.
        for account in [None, Some(""), Some("   ")] {
            let headers = usage_headers("token-abc", account);
            assert_eq!(value(&headers, ACCOUNT_HEADER), None, "{account:?}");
            assert_eq!(headers.len(), 3);
        }
    }

    #[test]
    fn autostand_identifies_itself_rather_than_imitating_the_vendor_client() {
        assert!(USER_AGENT.starts_with("autostand/"));
    }
}
