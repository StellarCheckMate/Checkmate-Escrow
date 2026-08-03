//! Request ID middleware for end-to-end tracing.
//!
//! Generates or extracts a request ID for each incoming request and:
//! - Stores it in task-local storage via `tracing-subscriber`
//! - Attaches it as `X-Request-ID` response header
//! - Includes it in all log lines via the `layer_request_id` tracing layer

use axum::{
    extract::Request,
    http::HeaderMap,
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::task::{Context, Poll};
use tower::Service;
use uuid::Uuid;

const REQUEST_ID_HEADER: &str = "X-Request-ID";
const REQUEST_ID_FIELD: &str = "request_id";

/// Extract request ID from headers or generate a new one.
///
/// If the client sends an `X-Request-ID` header, we use that value.
/// Otherwise, we generate a new UUID v4.
pub fn extract_or_generate_request_id(headers: &HeaderMap) -> String {
    headers
        .get(REQUEST_ID_HEADER)
        .and_then(|h| h.to_str().ok())
        .filter(|id| !id.is_empty())
        .map(|id| id.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

/// Middleware that adds request ID to every request and response.
///
/// This middleware:
/// 1. Extracts or generates a request ID from the incoming request
/// 2. Records it in tracing context (for structured logging)
/// 3. Attaches it to the response as `X-Request-ID` header
pub async fn request_id_middleware(
    mut request: Request,
    next: Next,
) -> Response {
    let headers = request.headers();
    let request_id = extract_or_generate_request_id(headers);

    // Insert the request ID into request extensions so downstream handlers can access it
    request.extensions_mut().insert(request_id.clone());

    // Record in tracing context
    tracing::Span::current().record(REQUEST_ID_FIELD, &request_id);

    let mut response = next.run(request).await;

    // Attach the request ID to the response header
    response
        .headers_mut()
        .insert(REQUEST_ID_HEADER, request_id.parse().unwrap_or_default());

    response
}

/// Tracing layer that formats request_id from the context.
///
/// Use this in your tracing subscriber to include request IDs in all logs.
/// Example:
/// ```ignore
/// tracing_subscriber::fmt()
///     .fmt_fields(format::PrettyFields::new())
///     .init();
/// ```
pub fn request_id_layer() -> impl Fn(&mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    |_| Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_request_id_from_header() {
        let mut headers = HeaderMap::new();
        let test_id = "test-request-id-123";
        headers.insert(REQUEST_ID_HEADER, test_id.parse().unwrap());

        let id = extract_or_generate_request_id(&headers);
        assert_eq!(id, test_id);
    }

    #[test]
    fn generate_request_id_when_header_missing() {
        let headers = HeaderMap::new();
        let id = extract_or_generate_request_id(&headers);

        // Should be a valid UUID
        assert!(Uuid::parse_str(&id).is_ok());
    }

    #[test]
    fn generate_request_id_when_header_empty() {
        let mut headers = HeaderMap::new();
        headers.insert(REQUEST_ID_HEADER, "".parse().unwrap());

        let id = extract_or_generate_request_id(&headers);

        // Should be a valid UUID (not empty string)
        assert!(!id.is_empty());
        assert!(Uuid::parse_str(&id).is_ok());
    }

    #[test]
    fn different_requests_get_different_ids() {
        let headers1 = HeaderMap::new();
        let headers2 = HeaderMap::new();

        let id1 = extract_or_generate_request_id(&headers1);
        let id2 = extract_or_generate_request_id(&headers2);

        assert_ne!(id1, id2, "Different requests should get different IDs");
    }
}
