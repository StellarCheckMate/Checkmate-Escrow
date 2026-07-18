/// Pluggable multi-provider architecture with automatic failover.
///
/// ## Design
///
/// [`GameProvider`] is the core trait.  Each chess platform (Chess.com,
/// Lichess, …) implements it behind a common interface.
///
/// [`ProviderRegistry`] holds an ordered list of providers.  When
/// [`ProviderRegistry::fetch_result`] is called it tries each provider in
/// precedence order:
///
/// 1. If the provider returns [`ProviderError::Unavailable`], move to the
///    next provider immediately (failover).
/// 2. If the provider returns [`ProviderError::RateLimited`], move to the
///    next provider immediately as well — another provider may have capacity.
/// 3. All other errors (terminal or logical) are returned to the caller as-is
///    without consulting further providers, because a different provider is
///    unlikely to give a different answer for the same game ID.
/// 4. If **all** providers fail with `Unavailable` or `RateLimited`, a
///    [`ProviderError::AllProvidersFailed`] is returned carrying each
///    individual error so callers can inspect them.
///
/// ## Tie-breaking / source disagreement
///
/// Because each provider returns a single authoritative result (from its own
/// API), there is no ambiguity in normal operation — the first provider that
/// returns a successful result wins.  If two providers were to disagree on the
/// game outcome (which should never happen for a legitimately finished game),
/// the registry trusts the **first provider in the precedence list**.  This
/// is intentional: operators should list the most-trusted source first.
///
/// ## Concurrency limit
///
/// Each implementation controls its own per-provider semaphore, ensuring that
/// the total number of in-flight HTTP requests to a single upstream never
/// exceeds the configured ceiling regardless of how many oracle tasks are
/// running in parallel.
use std::sync::Arc;

use contracts_oracle::types::Winner;

use crate::oracle::provider_error::ProviderError;

// ── Core trait ────────────────────────────────────────────────────────────────

/// The common interface every chess-platform client must implement.
///
/// Implementors are responsible for:
/// - Acquiring a rate-limiter token before every request.
/// - Acquiring a concurrency-semaphore permit before every request.
/// - Mapping platform-specific errors to [`ProviderError`].
#[async_trait::async_trait]
pub trait GameProvider: Send + Sync {
    /// A short, stable, human-readable name used in logs and error messages.
    fn name(&self) -> &'static str;

    /// Fetch the winner of the game identified by `game_id` on this platform.
    async fn fetch_result(&self, game_id: &str) -> Result<Winner, ProviderError>;
}

// ── Registry ──────────────────────────────────────────────────────────────────

/// An ordered list of [`GameProvider`]s with automatic failover.
///
/// Create one at startup and share it via `Arc<ProviderRegistry>` across tasks.
pub struct ProviderRegistry {
    providers: Vec<Arc<dyn GameProvider>>,
}

impl ProviderRegistry {
    /// Build a registry from an ordered list of providers (highest precedence
    /// first).
    ///
    /// # Panics
    ///
    /// Panics if `providers` is empty — a registry without providers cannot
    /// resolve any game.
    pub fn new(providers: Vec<Arc<dyn GameProvider>>) -> Self {
        assert!(!providers.is_empty(), "ProviderRegistry requires at least one provider");
        Self { providers }
    }

    /// Fetch the result for `game_id`, failing over across providers as
    /// described in the module documentation.
    pub async fn fetch_result(&self, game_id: &str) -> Result<Winner, ProviderError> {
        let mut failover_errors: Vec<ProviderError> = Vec::new();

        for provider in &self.providers {
            match provider.fetch_result(game_id).await {
                Ok(winner) => return Ok(winner),

                Err(e)
                    if e.should_failover() || e.is_rate_limited() || e.is_concurrency_limited() =>
                {
                    // Record and try the next provider. The caller can
                    // distinguish rate-limited retry cases from outage
                    // cases by inspecting the error variant in the final
                    // `AllProvidersFailed` bundle.
                    tracing_or_eprintln(provider.name(), game_id, &e);
                    failover_errors.push(e);
                }

                // Terminal or logical errors — return immediately without
                // failing over (another provider won't change the answer).
                Err(e) => return Err(e),
            }
        }

        // Every provider triggered a failover.
        Err(ProviderError::AllProvidersFailed {
            count: failover_errors.len(),
            errors: failover_errors,
        })
    }

    /// Number of providers registered.
    pub fn len(&self) -> usize {
        self.providers.len()
    }

    /// Returns `true` if no providers are registered (always false for a
    /// valid registry).
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}

fn tracing_or_eprintln(provider: &str, game_id: &str, err: &ProviderError) {
    // Use eprintln as a lightweight stand-in; replace with `tracing::warn!`
    // once the oracle service grows a tracing subscriber.
    eprintln!(
        "[oracle] provider={provider} game_id={game_id} failover triggered: {err}"
    );
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    /// A provider that always returns the supplied result.
    struct ConstProvider {
        name: &'static str,
        result: Result<Winner, &'static str>,
    }

    #[async_trait::async_trait]
    impl GameProvider for ConstProvider {
        fn name(&self) -> &'static str {
            self.name
        }
        async fn fetch_result(&self, _game_id: &str) -> Result<Winner, ProviderError> {
            match &self.result {
                Ok(w) => Ok(w.clone()),
                Err("unavailable") => Err(ProviderError::Unavailable {
                    provider: self.name,
                    reason: "down".to_string(),
                }),
                Err("rate_limited") => Err(ProviderError::RateLimited {
                    provider: self.name,
                    retry_after: Duration::from_secs(1),
                }),
                Err("not_found") => Err(ProviderError::GameNotFound),
                Err(other) => Err(ProviderError::InvalidGameId((*other).to_string())),
            }
        }
    }

    /// A provider that counts how many times it was called.
    struct CountingProvider {
        name: &'static str,
        calls: Arc<AtomicUsize>,
        result: Result<Winner, &'static str>,
    }

    #[async_trait::async_trait]
    impl GameProvider for CountingProvider {
        fn name(&self) -> &'static str {
            self.name
        }
        async fn fetch_result(&self, game_id: &str) -> Result<Winner, ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            ConstProvider {
                name: self.name,
                result: self.result.clone(),
            }
            .fetch_result(game_id)
            .await
        }
    }

    fn const_p(name: &'static str, r: Result<Winner, &'static str>) -> Arc<dyn GameProvider> {
        Arc::new(ConstProvider { name, result: r })
    }

    #[tokio::test]
    async fn primary_success_no_failover() {
        let reg = ProviderRegistry::new(vec![
            const_p("primary", Ok(Winner::Player1)),
            const_p("secondary", Ok(Winner::Player2)), // should not be called
        ]);
        let winner = reg.fetch_result("abc12345").await.unwrap();
        assert_eq!(winner, Winner::Player1);
    }

    #[tokio::test]
    async fn failover_on_unavailable() {
        let reg = ProviderRegistry::new(vec![
            const_p("primary", Err("unavailable")),
            const_p("secondary", Ok(Winner::Player2)),
        ]);
        let winner = reg.fetch_result("abc12345").await.unwrap();
        assert_eq!(winner, Winner::Player2);
    }

    #[tokio::test]
    async fn failover_on_rate_limited() {
        let reg = ProviderRegistry::new(vec![
            const_p("primary", Err("rate_limited")),
            const_p("secondary", Ok(Winner::Draw)),
        ]);
        let winner = reg.fetch_result("abc12345").await.unwrap();
        assert_eq!(winner, Winner::Draw);
    }

    #[tokio::test]
    async fn no_failover_on_terminal_error() {
        let secondary_calls = Arc::new(AtomicUsize::new(0));
        let reg = ProviderRegistry::new(vec![
            const_p("primary", Err("not_found")),
            Arc::new(CountingProvider {
                name: "secondary",
                calls: secondary_calls.clone(),
                result: Ok(Winner::Player1),
            }),
        ]);
        let err = reg.fetch_result("abc12345").await.unwrap_err();
        assert!(matches!(err, ProviderError::GameNotFound));
        assert_eq!(
            secondary_calls.load(Ordering::SeqCst),
            0,
            "secondary should not be called on terminal error"
        );
    }

    #[tokio::test]
    async fn all_providers_failed_returned() {
        let reg = ProviderRegistry::new(vec![
            const_p("p1", Err("unavailable")),
            const_p("p2", Err("rate_limited")),
            const_p("p3", Err("unavailable")),
        ]);
        let err = reg.fetch_result("abc12345").await.unwrap_err();
        assert!(matches!(
            err,
            ProviderError::AllProvidersFailed { count: 3, .. }
        ));
    }

    #[tokio::test]
    async fn first_provider_wins_on_success() {
        // Both succeed — primary's answer must win.
        let reg = ProviderRegistry::new(vec![
            const_p("primary", Ok(Winner::Player1)),
            const_p("secondary", Ok(Winner::Player2)),
        ]);
        assert_eq!(
            reg.fetch_result("abc12345").await.unwrap(),
            Winner::Player1
        );
    }
}
