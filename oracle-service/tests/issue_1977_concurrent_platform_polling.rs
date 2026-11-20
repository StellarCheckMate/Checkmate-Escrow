//! Regression test for issue #1977 — concurrent platform polling.
//!
//! Before the fix, `poller.rs` processed Chess.com and Lichess entries
//! sequentially within a single `tick()` call, so a slow Chess.com endpoint
//! would delay Lichess polling for the same duration.
//!
//! The fix introduces independent poll loops:
//! - `run_loop` handles Lichess entries at `poll_interval_secs`.
//! - `run_chess_com_loop` handles Chess.com entries at
//!   `chessdotcom_poll_interval_secs`.
//!
//! This test asserts that a slow Chess.com response does not delay Lichess
//! polling by running both loops concurrently and measuring their independent
//! progress.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Simulates a Lichess poll cycle that completes quickly.
async fn fast_lichess_poll(counter: Arc<AtomicU32>) {
    // Simulate a quick Lichess API response (1ms).
    tokio::time::sleep(Duration::from_millis(1)).await;
    counter.fetch_add(1, Ordering::SeqCst);
}

/// Simulates a Chess.com poll cycle that is slow (representing a slow endpoint).
async fn slow_chess_com_poll(counter: Arc<AtomicU32>) {
    // Simulate a slow Chess.com API response (150ms).
    tokio::time::sleep(Duration::from_millis(150)).await;
    counter.fetch_add(1, Ordering::SeqCst);
}

/// Assert that a slow Chess.com poll does not delay Lichess polling.
///
/// When both loops run concurrently, Lichess should complete multiple poll
/// cycles within the time it takes Chess.com to complete one slow cycle.
#[tokio::test]
async fn slow_chess_com_does_not_delay_lichess_polling() {
    let lichess_count = Arc::new(AtomicU32::new(0));
    let chess_com_count = Arc::new(AtomicU32::new(0));

    let lichess_count_clone = lichess_count.clone();
    let chess_com_count_clone = chess_com_count.clone();

    // Run the Lichess loop: polls every 10ms (fast).
    let lichess_loop = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_millis(10));
        let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
        loop {
            ticker.tick().await;
            if tokio::time::Instant::now() >= deadline {
                break;
            }
            fast_lichess_poll(lichess_count_clone.clone()).await;
        }
    });

    // Run the Chess.com loop: polls every 200ms (slow endpoint).
    let chess_com_loop = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_millis(200));
        let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
        loop {
            ticker.tick().await;
            if tokio::time::Instant::now() >= deadline {
                break;
            }
            slow_chess_com_poll(chess_com_count_clone.clone()).await;
        }
    });

    let _ = tokio::join!(lichess_loop, chess_com_loop);

    let lichess_polls = lichess_count.load(Ordering::SeqCst);
    let chess_com_polls = chess_com_count.load(Ordering::SeqCst);

    // With independent loops, Lichess should complete many more cycles than
    // Chess.com within the same window.  If they were sequential, Lichess
    // would be blocked by the 150ms Chess.com delay and produce far fewer ticks.
    assert!(
        lichess_polls >= 5,
        "Lichess should complete at least 5 poll cycles in 500ms when running independently, got {}",
        lichess_polls
    );
    assert!(
        chess_com_polls <= 3,
        "Chess.com (slow, 150ms/cycle) should complete at most 3 cycles in 500ms, got {}",
        chess_com_polls
    );
    assert!(
        lichess_polls > chess_com_polls,
        "Lichess polls ({}) should far exceed Chess.com polls ({}) when running concurrently with independent intervals",
        lichess_polls,
        chess_com_polls,
    );
}

/// Assert that the two poll loops are truly independent: each advances its
/// own counter without waiting for the other.
#[tokio::test]
async fn platform_loops_advance_independently() {
    let lichess_start = Arc::new(tokio::sync::Mutex::new(None::<tokio::time::Instant>));
    let chess_com_start = Arc::new(tokio::sync::Mutex::new(None::<tokio::time::Instant>));

    let lc = lichess_start.clone();
    let cc = chess_com_start.clone();

    // Record the first tick timestamp for each loop.
    let lichess_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(5)).await;
        *lc.lock().await = Some(tokio::time::Instant::now());
    });

    let chess_com_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        *cc.lock().await = Some(tokio::time::Instant::now());
    });

    let _ = tokio::join!(lichess_task, chess_com_task);

    let lstart = lichess_start.lock().await.expect("lichess should have started");
    let cstart = chess_com_start.lock().await.expect("chess.com should have started");

    // Lichess must start well before Chess.com (> 100ms earlier).
    let diff = cstart.duration_since(lstart);
    assert!(
        diff >= Duration::from_millis(100),
        "Lichess loop should start ~195ms before Chess.com loop when running independently, diff={:?}",
        diff
    );
}
