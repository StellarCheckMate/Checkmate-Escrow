//! API-key authentication for the admin-like endpoints.
//!
//! The event-indexer serves a few endpoints that expose sensitive financial
//! data or expensive whole-table aggregates — the analytics suite
//! (`/analytics/*`), player transaction history (`/transactions/*`) and
//! service stats (`/stats`).  Before this module existed, any client that
//! could reach the service could query balance data or trigger expensive
//! scans with no authentication at all.
//!
//! [`require_api_key`] gates exactly those paths.  Clients present the shared
//! secret configured via `EVENT_INDEXER_API_KEY` in an `X-Api-Key` header.
//! The comparison is constant-time so a timing side channel cannot be used to
//! guess the key.
//!
//! ## Fail-closed default
//! If no key is configured, protected endpoints **refuse every request with
//! `401`** instead of silently serving sensitive data.  A misconfigured
//! server is an outage, never a leak; operators are told to set
//! `EVENT_INDEXER_API_KEY` in the startup log.
//!
//! ## Layering
//! The middleware is installed *outside* [`crate::validation::validate_request`]
//! (see [`crate::api::build_router`]), so unauthenticated callers are rejected
//! before any input is even parsed — they learn nothing about whether an
//! address or parameter is well-formed.

use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};

use crate::api::{ApiResponse, AppState};

/// Header clients use to present the shared secret.
pub const API_KEY_HEADER: &str = "X-Api-Key";

/// The admin-like paths this middleware protects.
///
/// Everything else — match and event listing, per-player match history, health
/// and API docs — stays public so the frontend and websocket-server can
/// consume it without credentials.
pub fn is_protected_path(path: &str) -> bool {
    path == "/stats" || path.starts_with("/analytics/") || path.starts_with("/transactions/")
}

/// Compare two strings in time proportional to their length, not to how many
/// bytes match, so an attacker cannot learn the key byte-by-byte from response
/// timing.  Length is intentionally leaked (it is not secret-worthy here).
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

/// Standard `401` envelope for this service, matching the shape every other
/// rejection uses (`success: false`, `error` populated).
fn unauthorized(message: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "ApiKey")],
        Json(ApiResponse::<()> {
            success: false,
            data: None,
            error: Some(message.to_string()),
        }),
    )
        .into_response()
}

/// Axum middleware that gates [`is_protected_path`] with the configured
/// shared secret.
///
/// - Path not protected → pass through untouched.
/// - Protected and no key configured → `401` (fail closed).
/// - Protected, key configured, header missing or wrong → `401`.
/// - Protected, header matches → pass through.
pub async fn require_api_key(State(state): State<AppState>, req: Request, next: Next) -> Response {
    if !is_protected_path(req.uri().path()) {
        return next.run(req).await;
    }

    let Some(expected) = state.api_key.as_deref() else {
        return unauthorized(
            "this endpoint requires an API key, but EVENT_INDEXER_API_KEY is not \
             configured on this server",
        );
    };

    let provided = req
        .headers()
        .get(API_KEY_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !constant_time_eq(provided, expected) {
        return unauthorized("invalid or missing X-Api-Key header");
    }

    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_paths_are_recognised() {
        for path in [
            "/stats",
            "/analytics/overview",
            "/analytics/player/GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN",
            "/analytics/token/USDC",
            "/transactions/player/GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN",
        ] {
            assert!(is_protected_path(path), "{path} must be protected");
        }
    }

    #[test]
    fn public_paths_are_not_protected() {
        for path in [
            "/health",
            "/api/docs",
            "/api/openapi.yaml",
            "/events",
            "/events/42",
            "/matches",
            "/matches/active",
            "/matches/pending",
            "/match/42",
            "/matches/42",
            "/players/GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN/matches",
            // A path that merely starts with a protected prefix must not match.
            "/analytics",
            "/transactions",
            "/stats/everything",
        ] {
            assert!(!is_protected_path(path), "{path} must stay public");
        }
    }

    #[test]
    fn constant_time_eq_matches_identity() {
        assert!(constant_time_eq("", ""));
        assert!(constant_time_eq("secret-key", "secret-key"));
    }

    #[test]
    fn constant_time_eq_rejects_differences() {
        assert!(!constant_time_eq("secret-key", "secret-keY"));
        assert!(!constant_time_eq("secret-key", "secret"));
        assert!(!constant_time_eq("secret", "secret-key"));
        assert!(!constant_time_eq("a", "b"));
        assert!(!constant_time_eq("", "x"));
    }
}
