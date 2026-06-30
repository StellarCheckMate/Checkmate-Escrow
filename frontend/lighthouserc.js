/** @type {import('@lhci/cli').LighthouseRcConfig} */
export default {
  ci: {
    collect: {
      // Build the frontend and serve it locally for auditing
      staticDistDir: './dist',
      numberOfRuns: 1,
    },
    assert: {
      assertions: {
        // Fail CI if accessibility score drops below 90%
        'categories:accessibility': ['error', { minScore: 0.9 }],
      },
    },
    upload: {
      // Store results as a static file report (no external server needed)
      target: 'temporary-public-storage',
    },
  },
};
