//! Prometheus metrics for the event indexer.
//!
//! ## Exposed metrics
//!
//! | Metric name                    | Type    | Description                                                |
//! |--------------------------------|---------|--------------------------------------------------------------|
//! | `indexer_matches_total`        | Counter | Total number of contract events indexed since process start. |
//! | `indexer_rpc_lag_seconds`      | Gauge   | Estimated seconds between the latest network ledger and the last ledger this instance has polled. |
//! | `indexer_rpc_lag_ledgers`      | Gauge   | Ledger count between the latest network ledger and the last ledger this instance has polled. Fires `IndexerLaggingAlert` when > 100. |
//! | `indexer_cache_hit_ratio`      | Gauge   | Hit rate (0.0-1.0) of the API response cache ([`crate::api_cache`]). |
//!
//! All metrics are registered on the default Prometheus registry and served at
//! `GET /metrics` in the text/plain exposition format understood by Prometheus
//! scrapers.
//!
//! ## Usage
//!
//! Call [`inc_matches_indexed`] once per event successfully written by the
//! poller ([`crate::rpc::poll_events`]), [`set_rpc_lag_from_ledger_gap`] once
//! per poll iteration (it updates both `indexer_rpc_lag_seconds` and
//! `indexer_rpc_lag_ledgers`), and [`set_cache_hit_ratio`] whenever `/metrics`
//! is scraped (it reads the live counters off
//! [`crate::api_cache::ApiCache::stats`], so there is no need for a background
//! updater).

use once_cell::sync::Lazy;
use prometheus::{register_gauge, register_int_counter, Gauge, IntCounter, TextEncoder};

/// Counter: total number of contract events indexed since process start.
///
/// Monotonic by design — Prometheus counters are meant to only go up; use
/// `rate()`/`increase()` in queries to get a per-interval indexed rate.
pub static INDEXER_MATCHES_TOTAL: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!(
        "indexer_matches_total",
        "Total number of contract events indexed since process start"
    )
    .expect("failed to register indexer_matches_total counter")
});

/// Gauge: estimated lag, in seconds, between the latest ledger on the Soroban
/// RPC network and the last ledger this instance has finished polling.
pub static INDEXER_RPC_LAG_SECONDS: Lazy<Gauge> = Lazy::new(|| {
    register_gauge!(
        "indexer_rpc_lag_seconds",
        "Estimated seconds between the latest network ledger and the last polled ledger"
    )
    .expect("failed to register indexer_rpc_lag_seconds gauge")
});

/// Gauge: raw ledger count between the latest network ledger and the last
/// ledger this instance has finished polling.
///
/// Alerting rule `IndexerLaggingAlert` fires when this value exceeds 100,
/// indicating that the indexer has fallen more than ~8 minutes behind the
/// chain tip and the UI may be showing stale match state.
pub static INDEXER_RPC_LAG_LEDGERS: Lazy<Gauge> = Lazy::new(|| {
    register_gauge!(
        "indexer_rpc_lag_ledgers",
        "Ledger count between the latest network ledger and the last polled ledger"
    )
    .expect("failed to register indexer_rpc_lag_ledgers gauge")
});

/// Gauge: hit rate (0.0-1.0) of the shared API response cache.
pub static INDEXER_CACHE_HIT_RATIO: Lazy<Gauge> = Lazy::new(|| {
    register_gauge!(
        "indexer_cache_hit_ratio",
        "Hit rate of the API response cache, in the range 0.0-1.0"
    )
    .expect("failed to register indexer_cache_hit_ratio gauge")
});

/// Approximate time between two consecutive Stellar ledgers closing.
/// Used to convert a ledger-count gap into an approximate second count for
/// [`INDEXER_RPC_LAG_SECONDS`] without an extra RPC round trip.
pub const STELLAR_LEDGER_CLOSE_SECS: f64 = 5.0;

/// Increment [`INDEXER_MATCHES_TOTAL`] by one.
///
/// Called by the poller after each contract event is durably written.
#[inline]
pub fn inc_matches_indexed() {
    INDEXER_MATCHES_TOTAL.inc();
}

/// Set [`INDEXER_RPC_LAG_SECONDS`] and [`INDEXER_RPC_LAG_LEDGERS`] from the
/// gap (in ledgers) between the latest network ledger and the last ledger this
/// instance polled.
#[inline]
pub fn set_rpc_lag_from_ledger_gap(latest_ledger: u32, last_polled_ledger: u32) {
    let ledger_gap = latest_ledger.saturating_sub(last_polled_ledger) as f64;
    INDEXER_RPC_LAG_SECONDS.set(ledger_gap * STELLAR_LEDGER_CLOSE_SECS);
    INDEXER_RPC_LAG_LEDGERS.set(ledger_gap);
}

/// Set [`INDEXER_CACHE_HIT_RATIO`] directly (already computed as hits / total).
#[inline]
pub fn set_cache_hit_ratio(ratio: f64) {
    INDEXER_CACHE_HIT_RATIO.set(ratio);
}

/// Render all registered metrics in the Prometheus text exposition format.
pub fn render() -> String {
    let encoder = TextEncoder::new();
    let families = prometheus::gather();
    encoder
        .encode_to_string(&families)
        .unwrap_or_else(|e| format!("# ERROR encoding metrics: {}\n", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_total_counter_increments() {
        let before = INDEXER_MATCHES_TOTAL.get();
        inc_matches_indexed();
        inc_matches_indexed();
        assert_eq!(INDEXER_MATCHES_TOTAL.get(), before + 2);
    }

    #[test]
    fn rpc_lag_reflects_the_ledger_gap() {
        set_rpc_lag_from_ledger_gap(1000, 995);
        assert_eq!(INDEXER_RPC_LAG_SECONDS.get(), 5.0 * STELLAR_LEDGER_CLOSE_SECS);
        assert_eq!(INDEXER_RPC_LAG_LEDGERS.get(), 5.0);
    }

    #[test]
    fn rpc_lag_is_zero_when_caught_up() {
        set_rpc_lag_from_ledger_gap(1000, 1000);
        assert_eq!(INDEXER_RPC_LAG_SECONDS.get(), 0.0);
        assert_eq!(INDEXER_RPC_LAG_LEDGERS.get(), 0.0);
    }

    #[test]
    fn rpc_lag_never_goes_negative_on_a_stale_latest_ledger() {
        // A momentarily stale "latest" read (e.g. hitting a lagging RPC replica)
        // must not produce a negative lag.
        set_rpc_lag_from_ledger_gap(500, 600);
        assert_eq!(INDEXER_RPC_LAG_SECONDS.get(), 0.0);
        assert_eq!(INDEXER_RPC_LAG_LEDGERS.get(), 0.0);
    }

    #[test]
    fn rpc_lag_ledgers_exceeds_alert_threshold() {
        // Verify that a gap > 100 ledgers is faithfully reported (the
        // Prometheus alert rule `IndexerLaggingAlert` fires at > 100).
        set_rpc_lag_from_ledger_gap(1200, 1000);
        assert!(INDEXER_RPC_LAG_LEDGERS.get() > 100.0);
    }

    #[test]
    fn cache_hit_ratio_is_settable() {
        set_cache_hit_ratio(0.75);
        assert_eq!(INDEXER_CACHE_HIT_RATIO.get(), 0.75);
    }

    #[test]
    fn render_includes_all_metric_names() {
        inc_matches_indexed();
        set_rpc_lag_from_ledger_gap(10, 5);
        set_cache_hit_ratio(0.5);
        let output = render();
        assert!(output.contains("indexer_matches_total"));
        assert!(output.contains("indexer_rpc_lag_seconds"));
        assert!(output.contains("indexer_rpc_lag_ledgers"));
        assert!(output.contains("indexer_cache_hit_ratio"));
    }
}
