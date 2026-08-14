//! Percent, timestamp and credential-blob hygiene, applied at the single point
//! of construction rather than scattered through the mappers.
//!
//! Everything here is a pure function. Where a function needs the current time
//! it takes `now` as a parameter — a mapper never reads a clock, which is what
//! makes a relative reset (`reset_after_seconds`) testable without freezing time
//! globally.

use chrono::{DateTime, Duration as ChronoDuration, TimeZone, Utc};
use serde::de::DeserializeOwned;
use serde_json::Value;

/// Below this magnitude an epoch number is read as seconds, above it as
/// milliseconds. `1e10` seconds is the year 2286 and `1e10` milliseconds is
/// 1970 — no real reset lands near the boundary.
const EPOCH_SECONDS_CUTOFF: f64 = 1e10;

/// Clamp a percentage into `0..=100`.
///
/// A non-finite reading becomes `None`, never `0`. This is the golden rule in
/// one line: the UI must be able to say "No data" instead of claiming a full
/// quota.
#[must_use]
pub fn clamp_percent(value: f64) -> Option<f64> {
    if value.is_finite() {
        Some(value.clamp(0.0, 100.0))
    } else {
        None
    }
}

/// Permissive numeric read: JSON numbers and numeric strings.
///
/// Booleans are rejected — several providers use `1`/`true` interchangeably in
/// *flag* fields, and silently reading `true` as `1.0` would turn a flag into a
/// quantity.
#[must_use]
pub fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64().filter(|v| v.is_finite()),
        Value::String(s) => s.trim().parse::<f64>().ok().filter(|v| v.is_finite()),
        _ => None,
    }
}

/// Permissive boolean read: real booleans, `0`/`1`, and `"true"`/`"false"`.
///
/// `None` for anything else, so "absent" stays distinguishable from "false".
#[must_use]
pub fn boolean(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(b) => Some(*b),
        Value::Number(n) => n.as_f64().map(|v| v != 0.0),
        Value::String(s) => match s.trim().to_ascii_lowercase().as_str() {
            "true" | "1" => Some(true),
            "false" | "0" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

/// Trimmed, non-empty string. An empty string is "missing", not a value.
#[must_use]
pub fn text(value: &Value) -> Option<&str> {
    value.as_str().map(str::trim).filter(|s| !s.is_empty())
}

/// Parse an RFC 3339 timestamp, normalising the shapes providers actually send.
///
/// Handled: a space instead of `T`, a trailing `" UTC"`, a missing zone
/// (assumed UTC), and fractional seconds of any length — truncated or padded to
/// exactly three digits, which is the one shape every parser agrees on.
#[must_use]
pub fn parse_rfc3339(raw: &str) -> Option<DateTime<Utc>> {
    let normalized = normalize_timestamp(raw)?;
    DateTime::parse_from_rfc3339(&normalized)
        .ok()
        .map(|parsed| parsed.with_timezone(&Utc))
}

/// Interpret an epoch number, choosing seconds or milliseconds by magnitude.
#[must_use]
pub fn parse_epoch(value: f64) -> Option<DateTime<Utc>> {
    if !value.is_finite() {
        return None;
    }
    let millis = if value.abs() < EPOCH_SECONDS_CUTOFF {
        value * 1000.0
    } else {
        value
    };
    #[allow(clippy::cast_possible_truncation)]
    Utc.timestamp_millis_opt(millis.trunc() as i64).single()
}

/// Resolve a relative reset (`reset_after_seconds`) against an injected `now`.
#[must_use]
pub fn parse_reset_after_seconds(seconds: f64, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    if !seconds.is_finite() {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)]
    let offset = ChronoDuration::try_seconds(seconds.trunc() as i64)?;
    now.checked_add_signed(offset)
}

/// Keys a provider may use for an absolute reset, in probe order.
const ABSOLUTE_RESET_KEYS: &[&str] = &["resets_at", "reset_at", "resetsAt", "resetAt"];

/// Keys a provider may use for a relative reset, in probe order.
const RELATIVE_RESET_KEYS: &[&str] = &[
    "reset_after_seconds",
    "resetAfterSeconds",
    "resets_in_seconds",
    "seconds_until_reset",
];

/// Resolve a reset time from whatever shape the provider used.
///
/// - A string is RFC 3339 (see [`parse_rfc3339`]).
/// - A number is an epoch (see [`parse_epoch`]).
/// - An object is searched for an absolute key first, then a relative one
///   resolved against `now` — absolute wins because it survives clock skew.
///
/// `now` is a parameter, never `Utc::now()`, so a relative reset is testable.
#[must_use]
pub fn parse_reset_at(value: &Value, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    match value {
        Value::String(_) => text(value).and_then(parse_rfc3339),
        Value::Number(_) => number(value).and_then(parse_epoch),
        Value::Object(_) => {
            for key in ABSOLUTE_RESET_KEYS {
                if let Some(found) = value.get(key).and_then(|inner| match inner {
                    Value::String(_) => text(inner).and_then(parse_rfc3339),
                    Value::Number(_) => number(inner).and_then(parse_epoch),
                    _ => None,
                }) {
                    return Some(found);
                }
            }
            for key in RELATIVE_RESET_KEYS {
                if let Some(found) = value
                    .get(key)
                    .and_then(number)
                    .and_then(|seconds| parse_reset_after_seconds(seconds, now))
                {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

/// Parse a `Retry-After` header value: integer seconds or an HTTP date.
///
/// Returns the cooldown in seconds relative to `now`; a date already in the past
/// yields `0` rather than a negative wait.
#[must_use]
pub fn parse_retry_after(raw: &str, now: DateTime<Utc>) -> Option<u64> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if let Ok(seconds) = raw.parse::<i64>() {
        return Some(seconds.max(0).unsigned_abs());
    }
    let at = DateTime::parse_from_rfc2822(raw)
        .ok()
        .map(|parsed| parsed.with_timezone(&Utc))?;
    Some((at - now).num_seconds().max(0).unsigned_abs())
}

/// Deserialize JSON, falling back to a hex-encoded JSON blob.
///
/// Several vendors store their credential file as hex (optionally `0x`-prefixed)
/// rather than plain JSON, so every credential reader needs both paths.
#[must_use]
pub fn decode_json_with_hex_fallback<T: DeserializeOwned>(bytes: &[u8]) -> Option<T> {
    if let Ok(direct) = serde_json::from_slice::<T>(bytes) {
        return Some(direct);
    }
    let decoded = decode_hex(std::str::from_utf8(bytes).ok()?)?;
    serde_json::from_slice::<T>(&decoded).ok()
}

/// Decode an optionally `0x`-prefixed hex string.
#[must_use]
pub fn decode_hex(raw: &str) -> Option<Vec<u8>> {
    let trimmed = raw.trim();
    let body = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    if body.is_empty() || body.len() % 2 != 0 || !body.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let bytes = body.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let text = std::str::from_utf8(pair).ok()?;
        out.push(u8::from_str_radix(text, 16).ok()?);
    }
    Some(out)
}

/// Decode base64url (JWT flavour), tolerating missing padding.
///
/// Hand-rolled rather than pulling a crate: the whole alphabet is 64 entries and
/// the only consumer is a JWT `exp` claim.
#[must_use]
pub fn decode_base64url(raw: &str) -> Option<Vec<u8>> {
    let mut accumulator: u32 = 0;
    let mut bits: u32 = 0;
    let mut out = Vec::with_capacity(raw.len() * 3 / 4);
    for byte in raw.trim().bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            b'=' => break,
            _ => return None,
        };
        accumulator = (accumulator << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            #[allow(clippy::cast_possible_truncation)]
            out.push((accumulator >> bits) as u8);
        }
    }
    Some(out)
}

/// Decode a JWT's payload segment as JSON. No signature verification: the token
/// is the vendor's own and is only read to learn when it expires.
#[must_use]
pub fn jwt_payload(token: &str) -> Option<Value> {
    let segment = token.split('.').nth(1)?;
    let decoded = decode_base64url(segment)?;
    serde_json::from_slice(&decoded).ok()
}

/// Read a JWT's `exp` claim as an instant.
#[must_use]
pub fn jwt_expiry(token: &str) -> Option<DateTime<Utc>> {
    jwt_payload(token)
        .as_ref()
        .and_then(|payload| payload.get("exp"))
        .and_then(number)
        .and_then(parse_epoch)
}

/// Unwrap a `go-keyring-base64:` value — how Go tools (`gh`, `agy`) store a
/// secret in the macOS keychain. A value without the prefix comes back trimmed.
#[must_use]
pub fn unwrap_go_keyring(raw: &str) -> Option<String> {
    const PREFIX: &str = "go-keyring-base64:";
    let trimmed = raw.trim();
    let text = match trimmed.strip_prefix(PREFIX) {
        Some(encoded) => {
            let decoded = decode_base64url(encoded.trim())?;
            String::from_utf8(decoded).ok()?.trim().to_string()
        }
        None => trimmed.to_string(),
    };
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// Integer cents to dollars. Snap to whole cents first: the inputs are already
/// integer cents, and rounding guards against float drift before the divide.
#[must_use]
pub fn cents_to_dollars(cents: f64) -> f64 {
    cents.round() / 100.0
}

/// Title-case a plan name: `"pro_plus"` → `"Pro Plus"`, `"PRO PLAN"` → `"Pro Plan"`.
///
/// Splits on `_`, `-` and whitespace, then upper-cases each word's first
/// character and lower-cases the tail — providers are inconsistent about case.
#[must_use]
pub fn title_case(raw: &str) -> String {
    raw.split(|c: char| c == '_' || c == '-' || c.is_whitespace())
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => {
                    first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Normalise a timestamp to strict RFC 3339 with exactly three fractional digits.
fn normalize_timestamp(raw: &str) -> Option<String> {
    let mut value = raw.trim().to_string();
    if value.is_empty() {
        return None;
    }
    if let Some(stripped) = value.strip_suffix(" UTC") {
        value = format!("{stripped}Z");
    }
    // "2026-08-13 12:00:00" — a space where RFC 3339 wants a `T`.
    if value.as_bytes().get(10) == Some(&b' ') {
        value.replace_range(10..11, "T");
    }
    if !value.is_char_boundary(19) || value.len() < 19 {
        return None;
    }
    let (head, rest) = value.split_at(19);
    let (fraction, zone) = split_fraction(rest);
    // No zone at all means the provider meant UTC; assuming local time here
    // would silently shift every reset by the user's offset.
    let zone = if zone.is_empty() { "Z" } else { zone };
    Some(match fraction {
        Some(digits) => format!("{head}.{digits}{zone}"),
        None => format!("{head}{zone}"),
    })
}

/// Split `.<digits><zone>` into a three-digit fraction and the zone suffix.
fn split_fraction(rest: &str) -> (Option<String>, &str) {
    let Some(after_dot) = rest.strip_prefix('.') else {
        return (None, rest);
    };
    let digit_count = after_dot.chars().take_while(char::is_ascii_digit).count();
    let (digits, zone) = after_dot.split_at(digit_count);
    if digits.is_empty() {
        return (None, zone);
    }
    let mut normalized: String = digits.chars().take(3).collect();
    while normalized.len() < 3 {
        normalized.push('0');
    }
    (Some(normalized), zone)
}

#[cfg(test)]
mod tests {
    use super::{
        boolean, cents_to_dollars, clamp_percent, decode_base64url, decode_hex,
        decode_json_with_hex_fallback, jwt_expiry, number, parse_epoch, parse_reset_after_seconds,
        parse_reset_at, parse_retry_after, parse_rfc3339, text, title_case, unwrap_go_keyring,
    };
    use chrono::{DateTime, TimeZone, Utc};
    use serde_json::json;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 13, 12, 0, 0).unwrap()
    }

    // ---- percent ----------------------------------------------------------

    #[test]
    fn percent_clamps_into_range() {
        assert_eq!(clamp_percent(-5.0), Some(0.0));
        assert_eq!(clamp_percent(0.0), Some(0.0));
        assert_eq!(clamp_percent(42.5), Some(42.5));
        assert_eq!(clamp_percent(140.0), Some(100.0));
    }

    #[test]
    fn non_finite_percent_is_none_not_zero() {
        // The golden rule: a broken reading must never render as a full quota.
        assert_eq!(clamp_percent(f64::NAN), None);
        assert_eq!(clamp_percent(f64::INFINITY), None);
        assert_eq!(clamp_percent(f64::NEG_INFINITY), None);
    }

    // ---- scalars ----------------------------------------------------------

    #[test]
    fn number_accepts_json_numbers_and_numeric_strings() {
        assert_eq!(number(&json!(12)), Some(12.0));
        assert_eq!(number(&json!(12.5)), Some(12.5));
        assert_eq!(number(&json!("12.5")), Some(12.5));
        assert_eq!(number(&json!(" 12.5 ")), Some(12.5));
    }

    #[test]
    fn number_rejects_booleans_and_junk() {
        // `true` must not become `1.0`: a flag is not a quantity.
        assert_eq!(number(&json!(true)), None);
        assert_eq!(number(&json!(false)), None);
        assert_eq!(number(&json!("abc")), None);
        assert_eq!(number(&json!(null)), None);
        assert_eq!(number(&json!({})), None);
    }

    #[test]
    fn boolean_reads_the_three_encodings_providers_use() {
        assert_eq!(boolean(&json!(true)), Some(true));
        assert_eq!(boolean(&json!(1)), Some(true));
        assert_eq!(boolean(&json!("TRUE")), Some(true));
        assert_eq!(boolean(&json!(0)), Some(false));
        assert_eq!(boolean(&json!("false")), Some(false));
        assert_eq!(boolean(&json!("maybe")), None);
        assert_eq!(boolean(&json!(null)), None);
    }

    #[test]
    fn text_treats_blank_as_missing() {
        assert_eq!(text(&json!("  hello ")), Some("hello"));
        assert_eq!(text(&json!("   ")), None);
        assert_eq!(text(&json!("")), None);
        assert_eq!(text(&json!(3)), None);
    }

    // ---- timestamps -------------------------------------------------------

    #[test]
    fn rfc3339_with_zulu_parses() {
        let parsed = parse_rfc3339("2026-08-13T12:00:00Z").unwrap();
        assert_eq!(parsed, now());
    }

    #[test]
    fn rfc3339_fraction_is_normalised_to_three_digits() {
        // Nanosecond precision truncates; single-digit precision pads.
        let long = parse_rfc3339("2026-08-13T12:00:00.123456789Z").unwrap();
        assert_eq!(long.timestamp_subsec_millis(), 123);
        let short = parse_rfc3339("2026-08-13T12:00:00.5Z").unwrap();
        assert_eq!(short.timestamp_subsec_millis(), 500);
    }

    #[test]
    fn rfc3339_accepts_space_separator_and_utc_suffix() {
        assert_eq!(parse_rfc3339("2026-08-13 12:00:00 UTC").unwrap(), now());
        assert_eq!(parse_rfc3339("2026-08-13 12:00:00").unwrap(), now());
    }

    #[test]
    fn rfc3339_keeps_an_explicit_offset() {
        // 14:00+02:00 is the same instant as 12:00Z.
        assert_eq!(parse_rfc3339("2026-08-13T14:00:00+02:00").unwrap(), now());
    }

    #[test]
    fn rfc3339_rejects_junk_instead_of_guessing() {
        assert_eq!(parse_rfc3339(""), None);
        assert_eq!(parse_rfc3339("   "), None);
        assert_eq!(parse_rfc3339("tomorrow"), None);
        assert_eq!(parse_rfc3339("2026-13-45T99:99:99Z"), None);
    }

    #[test]
    fn epoch_seconds_and_milliseconds_split_at_the_cutoff() {
        let seconds = parse_epoch(1_786_000_000.0).unwrap();
        let millis = parse_epoch(1_786_000_000_000.0).unwrap();
        assert_eq!(seconds, millis);
        assert_eq!(seconds.timestamp(), 1_786_000_000);
    }

    #[test]
    fn epoch_rejects_non_finite() {
        assert_eq!(parse_epoch(f64::NAN), None);
        assert_eq!(parse_epoch(f64::INFINITY), None);
    }

    #[test]
    fn relative_reset_resolves_against_the_injected_now() {
        let at = parse_reset_after_seconds(3_600.0, now()).unwrap();
        assert_eq!(at, now() + chrono::Duration::hours(1));
    }

    #[test]
    fn reset_at_reads_a_bare_string_or_number() {
        assert_eq!(
            parse_reset_at(&json!("2026-08-13T12:00:00Z"), now()).unwrap(),
            now()
        );
        assert_eq!(
            parse_reset_at(&json!(1_786_000_000), now())
                .unwrap()
                .timestamp(),
            1_786_000_000
        );
    }

    #[test]
    fn reset_at_prefers_an_absolute_key_over_a_relative_one() {
        // Absolute survives clock skew; relative is the fallback.
        let window = json!({ "reset_at": 1_786_000_000, "reset_after_seconds": 60 });
        assert_eq!(
            parse_reset_at(&window, now()).unwrap().timestamp(),
            1_786_000_000
        );
    }

    #[test]
    fn reset_at_falls_back_to_the_relative_key() {
        let window = json!({ "reset_after_seconds": 900 });
        assert_eq!(
            parse_reset_at(&window, now()).unwrap(),
            now() + chrono::Duration::minutes(15)
        );
    }

    #[test]
    fn reset_at_is_none_when_the_window_says_nothing() {
        assert_eq!(parse_reset_at(&json!({ "other": 1 }), now()), None);
        assert_eq!(parse_reset_at(&json!(null), now()), None);
        assert_eq!(parse_reset_at(&json!([1, 2]), now()), None);
    }

    #[test]
    fn retry_after_reads_seconds_and_http_dates() {
        assert_eq!(parse_retry_after("300", now()), Some(300));
        assert_eq!(parse_retry_after(" 300 ", now()), Some(300));
        assert_eq!(
            parse_retry_after("Thu, 13 Aug 2026 12:05:00 GMT", now()),
            Some(300)
        );
    }

    #[test]
    fn retry_after_never_returns_a_negative_wait() {
        assert_eq!(parse_retry_after("-30", now()), Some(0));
        assert_eq!(
            parse_retry_after("Thu, 13 Aug 2026 11:00:00 GMT", now()),
            Some(0)
        );
        assert_eq!(parse_retry_after("soon", now()), None);
        assert_eq!(parse_retry_after("", now()), None);
    }

    // ---- credential blobs -------------------------------------------------

    #[derive(serde::Deserialize, Debug, PartialEq)]
    struct Creds {
        token: String,
    }

    #[test]
    fn json_decodes_directly_when_it_is_plain() {
        let decoded: Creds = decode_json_with_hex_fallback(br#"{"token":"abc"}"#).unwrap();
        assert_eq!(decoded.token, "abc");
    }

    #[test]
    fn json_decodes_from_a_hex_blob() {
        let hex = br"7b22746f6b656e223a22616263227d";
        let decoded: Creds = decode_json_with_hex_fallback(hex).unwrap();
        assert_eq!(decoded.token, "abc");
        let prefixed = format!("0x{}", std::str::from_utf8(hex).unwrap());
        let decoded: Creds = decode_json_with_hex_fallback(prefixed.as_bytes()).unwrap();
        assert_eq!(decoded.token, "abc");
    }

    #[test]
    fn json_decode_gives_up_rather_than_half_parsing() {
        assert!(decode_json_with_hex_fallback::<Creds>(b"not json").is_none());
        assert!(decode_json_with_hex_fallback::<Creds>(b"7b22").is_none());
        assert!(decode_json_with_hex_fallback::<Creds>(b"").is_none());
    }

    #[test]
    fn hex_rejects_odd_length_and_non_hex() {
        assert_eq!(decode_hex("abc"), None);
        assert_eq!(decode_hex("zz"), None);
        assert_eq!(decode_hex(""), None);
        assert_eq!(decode_hex("0a0B"), Some(vec![0x0a, 0x0b]));
    }

    #[test]
    fn base64url_decodes_without_padding() {
        assert_eq!(decode_base64url("aGVsbG8").unwrap(), b"hello");
        assert_eq!(decode_base64url("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(decode_base64url("").unwrap(), Vec::<u8>::new());
        assert_eq!(decode_base64url("!!!"), None);
    }

    #[test]
    fn jwt_expiry_reads_the_exp_claim() {
        // {"exp":1786000000} base64url-encoded, with a throwaway header/signature.
        let token = "aGVhZGVy.eyJleHAiOjE3ODYwMDAwMDB9.c2ln";
        assert_eq!(jwt_expiry(token).unwrap().timestamp(), 1_786_000_000);
    }

    #[test]
    fn jwt_expiry_is_none_for_a_malformed_token() {
        assert_eq!(jwt_expiry("not-a-jwt"), None);
        assert_eq!(jwt_expiry("a.!!!.c"), None);
        assert_eq!(jwt_expiry("aGVhZGVy.eyJzdWIiOiJ4In0.c2ln"), None);
    }

    #[test]
    fn go_keyring_prefix_is_unwrapped() {
        assert_eq!(
            unwrap_go_keyring("go-keyring-base64:aGVsbG8=").as_deref(),
            Some("hello")
        );
        assert_eq!(
            unwrap_go_keyring("  plain-token "),
            Some("plain-token".into())
        );
        assert_eq!(unwrap_go_keyring("   "), None);
    }

    // ---- formatting -------------------------------------------------------

    #[test]
    fn cents_convert_without_float_drift() {
        assert!((cents_to_dollars(1234.0) - 12.34).abs() < f64::EPSILON);
        assert!((cents_to_dollars(1_233.999_9) - 12.34).abs() < f64::EPSILON);
    }

    #[test]
    fn plan_names_title_case_across_separators() {
        assert_eq!(title_case("pro_plus"), "Pro Plus");
        assert_eq!(title_case("PRO PLAN"), "Pro Plan");
        assert_eq!(title_case("team-5x"), "Team 5x");
        assert_eq!(title_case(""), "");
    }
}
