//! HTTP middleware for the oracle service.
//!
//! This module provides two layers of protection for the incoming HTTP server:
//!
//! - [`rate_limit`] — per-IP (100 req/min) and per-API-key (1000 req/min)
//!   token-bucket rate limiting, returning `429 Too Many Requests` with
//!   standard `X-RateLimit-*` headers when a bucket is exhausted.
//!
//! - [`waf`] — lightweight Web Application Firewall that blocks:
//!     - bursts > 20 req/s from a single IP (429)
//!     - request bodies larger than 1 MiB (413)
//!     - URIs longer than 2 048 characters (400)
//!     - URIs containing null bytes (400)
//!
//! - [`ip_allowlist`] — second-factor protection for admin endpoints
//!   (`/admin/*`). Even with a valid API key, requests from source IPs
//!   outside the `ORACLE_ADMIN_ALLOWED_IPS` CIDR allowlist are rejected
//!   with `403 Forbidden`, so a leaked admin API key alone is not enough
//!   to reach admin functionality.

pub mod ip_allowlist;
pub mod rate_limit;
pub mod waf;
