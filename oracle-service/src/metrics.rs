//! Prometheus metrics for the oracle service.
//!
//! ## Exposed metrics
//!
//! | Metric name                | Type  | Description                                             |
//! |----------------------------|-------|---------------------------------------------------------|
//! | `oracle_queue_depth`       | Gauge | Number of entries currently in the pending-verification queue. |
//! | `oracle_dead_letter_count` | Gauge | Total entries sitting in the dead-letter store.         |
//!
//! All metrics are registered on the default Prometheus registry and served at
//! `GET /metrics` in the text/plain exposition format understood by Prometheus
//! scrapers.
//!
//! ## Usage
//!
//! Call [`set_queue_depth`] on every poller tick after loading the queue, and
//! call [`set_dead_letter_count`] after any write to the dead-letter store.

use once_cell::sync::Lazy;
use prometheus::{register_gauge, Gauge, TextEncoder};

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
}
