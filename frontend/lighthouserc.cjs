/**
 * Lighthouse CI configuration for the Checkmate-Escrow frontend.
 *
 * Enforces a minimum accessibility score of 90 so that any PR that
 * regresses the frontend's WCAG compliance is caught before it merges.
 *
 * Deliberately CommonJS (`.cjs` + `module.exports`), not ESM: @lhci/cli
 * loads rc files via Node's `require()`, which — under this package's
 * "type": "module" — returns the ESM module-namespace wrapper object for
 * a plain `.js` file with `export default`, not the config itself. That
 * silently drops every setting here (collect/assert/upload all read as
 * undefined) and falls back to the unrelated `lighthouse:recommended`
 * preset. `.cjs` is parsed as CommonJS regardless of "type": "module",
 * so `require()` gets the real object.
 *
 * @see https://github.com/GoogleChrome/lighthouse-ci/blob/main/docs/configuration.md
 */

module.exports = {
  ci: {
    collect: {
      // Build the Vite app and serve the static output locally.
      staticDistDir: './dist',
      // Run three passes and take the median to reduce score variance.
      numberOfRuns: 3,
    },
    assert: {
      assertions: {
        // Fail the CI run if the accessibility score drops below 0.90 (90/100).
        'categories:accessibility': ['error', { minScore: 0.9 }],
      },
    },
    upload: {
      // Store results as GitHub Actions artifacts — no external LHCI server needed.
      target: 'temporary-public-storage',
    },
  },
};
