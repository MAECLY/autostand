//! One shared HTTP client for every usage probe, and a response type that keeps
//! the headers.
//!
//! The existing `llm::helpers::http_get_json` throws the response headers away.
//! Codex fills percentages from `x-codex-*-used-percent` when the body omits
//! them, and Claude's 429 cooldown comes from `Retry-After`, so the usage path
//! needs the status, the headers *and* the body.
//!
//! Nothing here logs a header, a body or a URL. [`HttpResponse`]'s `Debug` is
//! written by hand for exactly that reason: the derived one would print every
//! `Authorization` echo and every token-bearing body the moment someone adds a
//! `{:?}` to a trace line.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::model::UsageError;
use super::parse;

/// Default identity for endpoints that do not require imitating a vendor client.
///
/// A probe that must mirror an official client (Claude's `claude-code/<v>`,
/// Codex's `originator`) overrides this per request; the version string then
/// lives in one constant inside that provider's module so it can be bumped in
/// one place.
pub const USER_AGENT: &str = concat!("autostand/", env!("CARGO_PKG_VERSION"));

/// Per-request ceiling. Usage is advisory: a probe that has not answered in ten
/// seconds has already failed the user.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// Total connection-pool idle timeout, so a probe every five minutes does not
/// hold a socket open between refreshes.
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// The process-wide client, built once.
///
/// One client means one connection pool and one TLS session cache across every
/// provider; building one per probe would renegotiate TLS on every refresh.
pub fn shared_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(DEFAULT_TIMEOUT)
            .pool_idle_timeout(IDLE_TIMEOUT)
            .build()
            .expect("reqwest client with rustls-tls should always build")
    })
}

/// A completed HTTP exchange: status, headers, body.
///
/// Construct one from a real request with [`get`] / [`post_json`], or from
/// captured parts with [`HttpResponse::from_parts`] — which is what makes a
/// mapper fixture-testable without a network.
#[derive(Clone, PartialEq, Eq)]
pub struct HttpResponse {
    status: u16,
    /// Lower-cased header names, so lookup never depends on the vendor's casing.
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

// Hand-written: a derived `Debug` would print every header value and the whole
// body. Only shapes are safe to show.
impl std::fmt::Debug for HttpResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpResponse")
            .field("status", &self.status)
            .field(
                "headers",
                &format_args!("<{} redacted>", self.headers.len()),
            )
            .field(
                "body",
                &format_args!("<{} bytes redacted>", self.body.len()),
            )
            .finish()
    }
}

impl HttpResponse {
    /// Build a response from captured parts. Header names are lower-cased here,
    /// so callers may pass them in any casing.
    #[must_use]
    pub fn from_parts(status: u16, headers: &[(&str, &str)], body: &[u8]) -> Self {
        Self {
            status,
            headers: headers
                .iter()
                .map(|(name, value)| ((*name).to_ascii_lowercase(), (*value).to_string()))
                .collect(),
            body: body.to_vec(),
        }
    }

    #[must_use]
    pub fn status(&self) -> u16 {
        self.status
    }

    #[must_use]
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// Case-insensitive header lookup.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }

    /// Header parsed as a number, for the `x-…-used-percent` family.
    #[must_use]
    pub fn header_f64(&self, name: &str) -> Option<f64> {
        self.header(name)
            .and_then(|raw| raw.trim().parse::<f64>().ok())
            .filter(|value| value.is_finite())
    }

    /// Raw body. Callers hand this to a pure mapper; nothing logs it.
    #[must_use]
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    /// Deserialize the body.
    ///
    /// A body that does not parse is [`UsageError::UnsupportedPayload`], which
    /// degrades the provider to "no data" — never a wrong number — and carries
    /// no fragment of the body with it.
    pub fn json<T: DeserializeOwned>(&self) -> Result<T, UsageError> {
        serde_json::from_slice(&self.body).map_err(|_| UsageError::UnsupportedPayload)
    }

    /// Deserialize the body as untyped JSON.
    pub fn json_value(&self) -> Result<Value, UsageError> {
        self.json()
    }

    /// Vendor cooldown from `Retry-After`, resolved against an injected `now`.
    #[must_use]
    pub fn retry_after_secs(&self, now: DateTime<Utc>) -> Option<u64> {
        self.header("retry-after")
            .and_then(|raw| parse::parse_retry_after(raw, now))
    }

    /// Classify a non-success status.
    ///
    /// The single place a status becomes a typed failure, so no provider invents
    /// its own 401-vs-403 handling. `Ok(())` for any 2xx.
    pub fn error_for_status(&self, now: DateTime<Utc>) -> Result<(), UsageError> {
        if self.is_success() {
            return Ok(());
        }
        match self.status {
            // Read-only decision: an expired token is reported, never refreshed.
            401 | 403 => Err(UsageError::SessionExpired),
            429 => Err(UsageError::RateLimited {
                retry_after_secs: self.retry_after_secs(now),
            }),
            status => Err(UsageError::UnexpectedStatus { status }),
        }
    }
}

/// GET a URL, keeping status, headers and body.
///
/// `headers` are applied verbatim so a probe can mirror the vendor's own client.
/// Errors are typed and content-free: a transport failure never carries the URL
/// or the response into an error string.
pub async fn get(
    url: &str,
    headers: &[(&str, &str)],
    timeout: Duration,
) -> Result<HttpResponse, UsageError> {
    let mut request = shared_client().get(url).timeout(timeout);
    for &(name, value) in headers {
        request = request.header(name, value);
    }
    send(request).await
}

/// POST a JSON body, keeping status, headers and body.
pub async fn post_json(
    url: &str,
    headers: &[(&str, &str)],
    body: &Value,
    timeout: Duration,
) -> Result<HttpResponse, UsageError> {
    let mut request = shared_client().post(url).json(body).timeout(timeout);
    for &(name, value) in headers {
        request = request.header(name, value);
    }
    send(request).await
}

async fn send(request: reqwest::RequestBuilder) -> Result<HttpResponse, UsageError> {
    let response = request
        .send()
        .await
        .map_err(|err| map_transport_error(&err))?;
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|text| (name.as_str().to_ascii_lowercase(), text.to_string()))
        })
        .collect();
    let body = response
        .bytes()
        .await
        .map_err(|err| map_transport_error(&err))?
        .to_vec();
    Ok(HttpResponse {
        status,
        headers,
        body,
    })
}

/// Transport failures carry no detail on purpose: `reqwest`'s `Display` includes
/// the full URL, which for some providers embeds an account id.
fn map_transport_error(error: &reqwest::Error) -> UsageError {
    if error.is_timeout() {
        UsageError::Timeout
    } else {
        UsageError::Network
    }
}

#[cfg(test)]
mod tests {
    use super::{shared_client, HttpResponse, DEFAULT_TIMEOUT, USER_AGENT};
    use crate::usage::model::UsageError;
    use chrono::{DateTime, TimeZone, Utc};

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 13, 12, 0, 0).unwrap()
    }

    #[test]
    fn the_client_is_built_once() {
        // Same pointer, so every provider shares one connection pool.
        assert!(std::ptr::eq(shared_client(), shared_client()));
    }

    #[test]
    fn the_default_identity_names_the_app_and_its_version() {
        assert!(USER_AGENT.starts_with("autostand/"));
        assert!(USER_AGENT.len() > "autostand/".len());
        assert_eq!(DEFAULT_TIMEOUT.as_secs(), 10);
    }

    #[test]
    fn headers_are_looked_up_case_insensitively() {
        let response =
            HttpResponse::from_parts(200, &[("X-Codex-Primary-Used-Percent", "42.5")], b"{}");
        assert_eq!(
            response.header("x-codex-primary-used-percent"),
            Some("42.5")
        );
        assert_eq!(
            response.header("X-CODEX-PRIMARY-USED-PERCENT"),
            Some("42.5")
        );
        assert_eq!(
            response.header_f64("x-codex-primary-used-percent"),
            Some(42.5)
        );
        assert_eq!(response.header("x-missing"), None);
        assert_eq!(response.header_f64("x-missing"), None);
    }

    #[test]
    fn a_non_numeric_header_is_none_not_zero() {
        let response = HttpResponse::from_parts(200, &[("x-credits", "n/a")], b"{}");
        assert_eq!(response.header_f64("x-credits"), None);
    }

    #[test]
    fn debug_redacts_headers_and_body() {
        // A stray `{:?}` in a trace line must not print an Authorization echo.
        let response = HttpResponse::from_parts(
            200,
            &[("authorization", "Bearer super-secret-token")],
            br#"{"access_token":"super-secret-token"}"#,
        );
        let rendered = format!("{response:?}");
        assert!(!rendered.contains("super-secret-token"), "{rendered}");
        assert!(!rendered.contains("authorization"), "{rendered}");
        assert!(rendered.contains("status: 200"), "{rendered}");
    }

    #[test]
    fn a_valid_body_deserializes() {
        #[derive(serde::Deserialize)]
        struct Payload {
            used: f64,
        }
        let response = HttpResponse::from_parts(200, &[], br#"{"used":42.0}"#);
        let payload: Payload = response.json().unwrap();
        assert!((payload.used - 42.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_broken_body_degrades_instead_of_leaking() {
        let response = HttpResponse::from_parts(200, &[], b"<html>error</html>");
        let error = response.json_value().unwrap_err();
        assert_eq!(error, UsageError::UnsupportedPayload);
        assert!(!error.to_string().contains("html"));
    }

    #[test]
    fn success_statuses_pass_the_status_check() {
        for status in [200, 201, 204, 299] {
            let response = HttpResponse::from_parts(status, &[], b"");
            assert!(response.is_success());
            assert!(response.error_for_status(now()).is_ok());
        }
    }

    #[test]
    fn auth_statuses_report_an_expiry_rather_than_refreshing() {
        for status in [401, 403] {
            let response = HttpResponse::from_parts(status, &[], b"");
            assert_eq!(
                response.error_for_status(now()).unwrap_err(),
                UsageError::SessionExpired
            );
        }
    }

    #[test]
    fn a_429_carries_the_vendor_cooldown() {
        let response = HttpResponse::from_parts(429, &[("Retry-After", "240")], b"");
        assert_eq!(
            response.error_for_status(now()).unwrap_err(),
            UsageError::RateLimited {
                retry_after_secs: Some(240)
            }
        );
        assert_eq!(response.retry_after_secs(now()), Some(240));
    }

    #[test]
    fn a_429_without_retry_after_leaves_the_cooldown_unstated() {
        // The caller applies its own default; the transport does not invent one.
        let response = HttpResponse::from_parts(429, &[], b"");
        assert_eq!(
            response.error_for_status(now()).unwrap_err(),
            UsageError::RateLimited {
                retry_after_secs: None
            }
        );
    }

    #[test]
    fn any_other_failure_keeps_only_the_status_number() {
        let response = HttpResponse::from_parts(503, &[], b"upstream said no");
        let error = response.error_for_status(now()).unwrap_err();
        assert_eq!(error, UsageError::UnexpectedStatus { status: 503 });
        assert_eq!(error.to_string(), "unexpected status");
    }
}
