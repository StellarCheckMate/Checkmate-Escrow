//! Prometheus metrics for the oracle service.
//!
//! ## Exposed metrics
//!
//! | Metric name                | Type    | Description                                             |
//! |----------------------------|---------|---------------------------------------------------------|
//! | `oracle_queue_depth`       | Gauge   | Number of entries currently in the pending-verification queue. |
//! | `oracle_dead_letter_count` | Gauge   | Total entries sitting in the dead-letter store.         |
//! | `oracle_slash_total`       | Counter | Cumulative oracle slash events processed by the relay.  |
//!
//! All metrics are registered on the default Prometheus registry and served at
//! `GET /metrics` in the text/plain exposition format understood by Prometheus
//! scrapers.
//!
//! ## Usage
//!
//! Call [`set_queue_depth`] on every poller tick after loading the queue,
//! call [`set_dead_letter_count`] after any write to the dead-letter store, and
//! call [`increment_slash_total`] each time the slash relay confirms a new slash.

use base64::Engine;
use once_cell::sync::Lazy;
use prometheus::{register_counter, register_gauge, Counter, Gauge, TextEncoder};
use subtle::ConstantTimeEq;

/// Gauge: number of entries in the pending-verification queue.
///
/// Updated on every poller tick (after loading due entries) and after every
/// enqueue / remove operation so Prometheus reflects the live state.
pub static ORACLE_QUEUE_DEPTH: Lazy<Gauge> = Lazy::new(|| {
    register_gauge!(
        "oracle_queue_depth",
        "Number of entries currently in the pending-verification queue"
    )
    .expect("failed to register oracle_queue_depth gauge")
});

/// Gauge: number of entries in the dead-letter store.
///
/// Incremented when an entry is dead-lettered, decremented when one is
/// removed (e.g. after a successful replay).
pub static ORACLE_DEAD_LETTER_COUNT: Lazy<Gauge> = Lazy::new(|| {
    register_gauge!(
        "oracle_dead_letter_count",
        "Number of entries currently in the dead-letter store"
    )
    .expect("failed to register oracle_dead_letter_count gauge")
});

/// Counter: total number of oracle slash events processed by this service.
///
/// Incremented every time the relay processes an `oracle/slash` contract event
/// (i.e. a `finalize_slash` confirmation from the oracle contract). This counter
/// provides visibility into how often oracles are being penalised and is the
/// data source for the **Slash Events** Grafana panel.
pub static ORACLE_SLASH_TOTAL: Lazy<Counter> = Lazy::new(|| {
    register_counter!(
        "oracle_slash_total",
        "Total number of oracle slash events processed by the relay"
    )
    .expect("failed to register oracle_slash_total counter")
});

/// Set the `oracle_queue_depth` gauge to `n`.
///
/// Called by the poller on every tick and after queue mutations.
#[inline]
pub fn set_queue_depth(n: usize) {
    ORACLE_QUEUE_DEPTH.set(n as f64);
}

/// Set the `oracle_dead_letter_count` gauge to `n`.
///
/// Called by the dead-letter store after any write.
#[inline]
pub fn set_dead_letter_count(n: usize) {
    ORACLE_DEAD_LETTER_COUNT.set(n as f64);
}

/// Increment the `oracle_slash_total` counter by 1.
///
/// Called by the slash relay whenever it confirms a completed `oracle/slash`
/// event from the oracle contract (i.e. after `finalize_slash` succeeds).
#[inline]
pub fn increment_slash_total() {
    ORACLE_SLASH_TOTAL.inc();
}

/// Render all registered metrics in the Prometheus text exposition format.
///
/// Returns the rendered string, or an error message if encoding fails.
pub fn render() -> String {
    let encoder = TextEncoder::new();
    let families = prometheus::gather();
    encoder
        .encode_to_string(&families)
        .unwrap_or_else(|e| format!("# ERROR encoding metrics: {}\n", e))
}

/// Compare two byte slices in constant time.
///
/// Returns `true` only when the lengths match and every byte is equal. The
/// comparison time depends only on the length of the inputs, not on where the
/// first difference occurs.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.ct_eq(b).into()
}

/// Validate an HTTP Basic Auth `Authorization` header against the expected
/// credentials.
///
/// Parses `Authorization: Basic <base64>`, base64-decodes the payload, splits
/// it on the first `:` into username and password, and compares both parts in
/// constant time. Returns `false` for a missing header, a non-`Basic` scheme,
/// malformed base64, a payload without a `:`, or mismatched credentials.
pub fn check_basic_auth(
    headers: &http::HeaderMap,
    expected_user: &str,
    expected_password: &str,
) -> bool {
    let header = match headers.get(http::header::AUTHORIZATION) {
        Some(value) => value,
        None => return false,
    };
    let header = match header.to_str() {
        Ok(value) => value,
        Err(_) => return false,
    };

    let encoded = match header
        .strip_prefix("Basic ")
        .or_else(|| header.strip_prefix("basic "))
    {
        Some(value) => value.trim(),
        None => return false,
    };

    let decoded = match base64::engine::general_purpose::STANDARD.decode(encoded) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };

    let sep = match decoded.iter().position(|&b| b == b':') {
        Some(pos) => pos,
        None => return false,
    };
    let (user, rest) = decoded.split_at(sep);
    let password = &rest[1..];

    let user_ok = constant_time_eq(user, expected_user.as_bytes());
    let password_ok = constant_time_eq(password, expected_password.as_bytes());
    user_ok & password_ok
}

/// Build the `401 Unauthorized` response returned when `/metrics` auth fails.
///
/// Includes a `WWW-Authenticate: Basic realm="metrics"` challenge so that
/// clients know to supply credentials.
pub fn unauthorized_response() -> http::Response<String> {
    http::Response::builder()
        .status(http::StatusCode::UNAUTHORIZED)
        .header(
            http::header::WWW_AUTHENTICATE,
            "Basic realm=\"metrics\"",
        )
        .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body("Unauthorized\n".to_string())
        .expect("failed to build 401 response")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gauges_are_accessible_and_settable() {
        set_queue_depth(5);
        assert_eq!(ORACLE_QUEUE_DEPTH.get(), 5.0);

        set_dead_letter_count(2);
        assert_eq!(ORACLE_DEAD_LETTER_COUNT.get(), 2.0);
    }

    #[test]
    fn render_includes_metric_names() {
        set_queue_depth(3);
        set_dead_letter_count(1);
        let output = render();
        assert!(
            output.contains("oracle_queue_depth"),
            "rendered output missing oracle_queue_depth"
        );
        assert!(
            output.contains("oracle_dead_letter_count"),
            "rendered output missing oracle_dead_letter_count"
        );
    }

    fn auth_header(credentials: &str) -> http::HeaderMap {
        let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::AUTHORIZATION,
            format!("Basic {}", encoded).parse().unwrap(),
        );
        headers
    }

    #[test]
    fn basic_auth_accepts_valid_credentials() {
        let headers = auth_header("scraper:s3cret");
        assert!(check_basic_auth(&headers, "scraper", "s3cret"));
    }

    #[test]
    fn basic_auth_rejects_wrong_password() {
        let headers = auth_header("scraper:wrong");
        assert!(!check_basic_auth(&headers, "scraper", "s3cret"));
    }

    #[test]
    fn basic_auth_rejects_wrong_username() {
        let headers = auth_header("intruder:s3cret");
        assert!(!check_basic_auth(&headers, "scraper", "s3cret"));
    }

    #[test]
    fn basic_auth_rejects_missing_header() {
        let headers = http::HeaderMap::new();
        assert!(!check_basic_auth(&headers, "scraper", "s3cret"));
    }

    #[test]
    fn basic_auth_rejects_malformed_base64() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::AUTHORIZATION,
            "Basic !!!not-base64!!!".parse().unwrap(),
        );
        assert!(!check_basic_auth(&headers, "scraper", "s3cret"));
    }

    #[test]
    fn basic_auth_rejects_non_basic_scheme() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::AUTHORIZATION,
            "Bearer sometoken".parse().unwrap(),
        );
        assert!(!check_basic_auth(&headers, "scraper", "s3cret"));
    }

    #[test]
    fn basic_auth_rejects_payload_without_colon() {
        let headers = auth_header("nocolon");
        assert!(!check_basic_auth(&headers, "scraper", "s3cret"));
    }

    #[test]
    fn unauthorized_response_has_challenge() {
        let response = unauthorized_response();
        assert_eq!(response.status(), http::StatusCode::UNAUTHORIZED);
        assert_eq!(
            response
                .headers()
                .get(http::header::WWW_AUTHENTICATE)
                .unwrap(),
            "Basic realm=\"metrics\""
        );
    }

    /// When auth credentials are configured, an unauthenticated request
    /// (missing Authorization header) must be rejected with 401.
    #[test]
    fn metrics_endpoint_rejects_unauthenticated_when_auth_configured() {
        // Simulate what the metrics handler does: if credentials are set and
        // the request carries no Authorization header, check_basic_auth returns
        // false and the caller must return 401.
        let headers = http::HeaderMap::new(); // no Authorization header
        let authorized = check_basic_auth(&headers, "scraper", "s3cret");
        assert!(
            !authorized,
            "unauthenticated request must be rejected when auth is configured"
        );
        // Verify the proper challenge response is built.
        let resp = unauthorized_response();
        assert_eq!(resp.status(), http::StatusCode::UNAUTHORIZED);
        assert!(
            resp.headers()
                .get(http::header::WWW_AUTHENTICATE)
                .is_some(),
            "401 response must carry a WWW-Authenticate challenge"
        );
    }

    /// When auth credentials are configured, a request with valid credentials
    /// must be accepted.
    #[test]
    fn metrics_endpoint_accepts_valid_credentials_when_auth_configured() {
        let headers = auth_header("scraper:s3cret");
        assert!(
            check_basic_auth(&headers, "scraper", "s3cret"),
            "valid credentials must be accepted when auth is configured"
        );
    }
}
