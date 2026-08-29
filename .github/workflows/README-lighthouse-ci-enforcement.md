# Lighthouse CI enforcement (frontend-accessibility.yml)

## Problem

`frontend/lighthouserc.cjs` defines an accessibility score threshold
(`minScore: 0.9`), but the workflow ran `lhci autorun` without
`--failOnError`, so a broken collection/upload step could be misreported as
a pass instead of failing the PR check.

## What changed

- `.github/workflows/frontend-accessibility.yml`: added `--failOnError` to
  the `lhci autorun` invocation, so collection/upload errors — not just
  failed score assertions — fail the step.
- Added a new step, "Verify lhci fails when a threshold is lowered", that
  copies `lighthouserc.cjs`, sets `minScore` to an unattainable `1.01`, reruns
  `lhci autorun` against that copy, and asserts the command exits non-zero.
  If the gate ever silently stops blocking regressions, this step fails
  loudly instead.

## Why

Lighthouse regressions should never be able to land silently. This closes
the gap between "the config says 0.90" and "the workflow actually enforces
0.90."
