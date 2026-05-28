# Repository Health Checklist

This checklist helps maintainers keep the repository consistent and complete.

## Required top-level files

- `README.md`
- `CONTRIBUTING.md`
- `CODE_OF_CONDUCT.md`
- `LICENSE`
- `Cargo.toml`
- `environments.toml`
- `.env.example`
- `.github/pull_request_template.md`

## Required documentation files

- `docs/architecture.md`
- `docs/oracle.md`
- `docs/security.md`
- `docs/roadmap.md`
- `docs/wave-guide.md`

## Verification steps

1. Ensure required top-level files exist.
2. Ensure `CODE_OF_CONDUCT.md` contains meaningful content.
3. Ensure README and CONTRIBUTING doc links point to existing local files.
4. Run the repository health check script:

```bash
./scripts/repository_health_check.sh
```

## Why this matters

A healthy repository prevents contributor confusion, broken documentation paths, and missing legal or community files.
