//! IP allowlist middleware for admin endpoints.
//!
//! Admin endpoints (`/admin/replay`, `/admin/queue`, etc.) are protected by
//! API-key authentication, but a leaked key alone should not be sufficient
//! to reach them. This middleware adds a second layer: the caller's source
//! IP must fall within one of the CIDR ranges configured via the
//! `ORACLE_ADMIN_ALLOWED_IPS` environment variable (comma-separated, e.g.
//! `"127.0.0.1/32,10.0.0.0/8"`). Requests from outside the allowlist are
//! rejected with `403 Forbidden` before the handler ever runs.
//!
//! If `ORACLE_ADMIN_ALLOWED_IPS` is unset or empty, the allowlist is empty
//! and *all* requests are rejected — fail closed rather than fail open.
//!
//! ## Usage
//!
//! ```rust,ignore
//! let allowlist = IpAllowlistState::from_env();
//! let admin_routes = Router::new()
//!     .route("/admin/replay", post(replay_handler))
//!     .route("/admin/queue", get(queue_handler))
//!     .layer(axum::middleware::from_fn_with_state(
//!         allowlist,
//!         oracle_service::middleware::ip_allowlist::admin_ip_allowlist_middleware,
//!     ));
//! ```

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::{
    body::Body,
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use tracing::warn;

/// A single parsed CIDR range, e.g. `10.0.0.0/8` or `::1/128`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CidrRange {
    network: IpAddr,
    prefix_len: u8,
}

impl CidrRange {
    /// Parse a CIDR range string. A bare IP address (no `/prefix`) is
    /// treated as a single-host route (`/32` for IPv4, `/128` for IPv6).
    pub fn parse(s: &str) -> Result<Self, String> {
        let s = s.trim();
        let (addr_part, prefix_part) = match s.split_once('/') {
            Some((a, p)) => (a, Some(p)),
            None => (s, None),
        };
        let network: IpAddr = addr_part
            .parse()
            .map_err(|_| format!("invalid IP address in CIDR range: {s}"))?;
        let max_prefix: u8 = if network.is_ipv4() { 32 } else { 128 };
        let prefix_len: u8 = match prefix_part {
            Some(p) => p
                .parse()
                .map_err(|_| format!("invalid prefix length in CIDR range: {s}"))?,
            None => max_prefix,
        };
        if prefix_len > max_prefix {
            return Err(format!("prefix length out of range in CIDR range: {s}"));
        }
        Ok(Self {
            network,
            prefix_len,
        })
    }

    /// Returns true if `ip` falls within this CIDR range.
    pub fn contains(&self, ip: &IpAddr) -> bool {
        match (self.network, ip) {
            (IpAddr::V4(net), IpAddr::V4(candidate)) => {
                let mask: u32 = if self.prefix_len == 0 {
                    0
                } else {
                    u32::MAX << (32 - self.prefix_len)
                };
                (u32::from(net) & mask) == (u32::from(*candidate) & mask)
            }
            (IpAddr::V6(net), IpAddr::V6(candidate)) => {
                let mask: u128 = if self.prefix_len == 0 {
                    0
                } else {
                    u128::MAX << (128 - self.prefix_len)
                };
                (u128::from(net) & mask) == (u128::from(*candidate) & mask)
            }
            // Mismatched address families never match (no IPv4-mapped-IPv6
            // coercion — an operator who wants both must list both).
            _ => false,
        }
    }
}

/// Shared state for the IP allowlist middleware: the parsed set of
/// admin-allowed CIDR ranges.
#[derive(Clone)]
pub struct IpAllowlistState {
    ranges: Arc<Vec<CidrRange>>,
}

impl IpAllowlistState {
    pub fn new(ranges: Vec<CidrRange>) -> Self {
        Self {
            ranges: Arc::new(ranges),
        }
    }

    /// Build the allowlist from the `ORACLE_ADMIN_ALLOWED_IPS` environment
    /// variable. Unset or empty results in a deny-all allowlist.
    pub fn from_env() -> Self {
        let raw = std::env::var("ORACLE_ADMIN_ALLOWED_IPS").unwrap_or_default();
        Self::from_str(&raw)
    }

    /// Build the allowlist from a comma-separated CIDR list string.
    /// Entries that fail to parse are logged and skipped rather than
    /// panicking, so a single typo doesn't take down the whole allowlist
    /// (it just makes that one range ineffective).
    pub fn from_str(raw: &str) -> Self {
        let ranges = raw
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .filter_map(|s| match CidrRange::parse(s) {
                Ok(r) => Some(r),
                Err(e) => {
                    warn!("skipping invalid ORACLE_ADMIN_ALLOWED_IPS entry: {e}");
                    None
                }
            })
            .collect();
        Self::new(ranges)
    }

    pub fn is_allowed(&self, ip: &IpAddr) -> bool {
        self.ranges.iter().any(|r| r.contains(ip))
    }
}

/// Axum middleware that rejects requests whose source IP is not in the
/// admin allowlist with `403 Forbidden`.
pub async fn admin_ip_allowlist_middleware(
    State(state): State<IpAllowlistState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let ip = addr.ip();
    if !state.is_allowed(&ip) {
        warn!("rejected admin request from non-allowlisted IP: {ip}");
        return (
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "forbidden",
                "message": "source IP is not in the admin allowlist",
            })),
        )
            .into_response();
    }
    next.run(request).await.into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cidr_parses_ipv4_range() {
        let r = CidrRange::parse("10.0.0.0/8").unwrap();
        assert!(r.contains(&"10.1.2.3".parse().unwrap()));
        assert!(!r.contains(&"11.0.0.1".parse().unwrap()));
    }

    #[test]
    fn cidr_parses_bare_ip_as_host_route() {
        let r = CidrRange::parse("127.0.0.1").unwrap();
        assert!(r.contains(&"127.0.0.1".parse().unwrap()));
        assert!(!r.contains(&"127.0.0.2".parse().unwrap()));
    }

    #[test]
    fn cidr_rejects_garbage_input() {
        assert!(CidrRange::parse("not-an-ip").is_err());
        assert!(CidrRange::parse("10.0.0.0/99").is_err());
    }

    #[test]
    fn empty_allowlist_denies_everything() {
        let state = IpAllowlistState::from_str("");
        assert!(!state.is_allowed(&"127.0.0.1".parse().unwrap()));
    }

    #[test]
    fn allowlist_accepts_ip_inside_ranges_and_rejects_outside() {
        let state = IpAllowlistState::from_str("192.168.1.0/24,10.0.0.0/8");
        assert!(state.is_allowed(&"192.168.1.42".parse().unwrap()));
        assert!(state.is_allowed(&"10.5.5.5".parse().unwrap()));
        assert!(!state.is_allowed(&"8.8.8.8".parse().unwrap()));
    }

    #[test]
    fn allowlist_handles_ipv6_ranges() {
        let state = IpAllowlistState::from_str("::1/128");
        assert!(state.is_allowed(&"::1".parse().unwrap()));
        assert!(!state.is_allowed(&"::2".parse().unwrap()));
    }

    #[test]
    fn invalid_entries_are_skipped_not_fatal() {
        let state = IpAllowlistState::from_str("not-a-cidr,10.0.0.0/8");
        assert!(state.is_allowed(&"10.1.1.1".parse().unwrap()));
    }

    #[tokio::test]
    async fn middleware_rejects_disallowed_ip_with_403() {
        use axum::body::Body as AxumBody;
        use axum::http::Request as HttpRequest;
        use axum::routing::get;
        use axum::Router;
        use tower::ServiceExt;

        let state = IpAllowlistState::from_str("10.0.0.0/8");
        let app = Router::new()
            .route("/admin/queue", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(
                state,
                admin_ip_allowlist_middleware,
            ));

        let req = HttpRequest::builder()
            .uri("/admin/queue")
            .extension(ConnectInfo(SocketAddr::from(([203, 0, 113, 5], 12345))))
            .body(AxumBody::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn middleware_allows_allowlisted_ip() {
        use axum::body::Body as AxumBody;
        use axum::http::Request as HttpRequest;
        use axum::routing::get;
        use axum::Router;
        use tower::ServiceExt;

        let state = IpAllowlistState::from_str("203.0.113.0/24");
        let app = Router::new()
            .route("/admin/queue", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(
                state,
                admin_ip_allowlist_middleware,
            ));

        let req = HttpRequest::builder()
            .uri("/admin/queue")
            .extension(ConnectInfo(SocketAddr::from(([203, 0, 113, 5], 12345))))
            .body(AxumBody::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
