---
name: "🐛 Bug Report"
about: "Report a bug, security vulnerability, or unexpected contract behavior."
title: "Fix: [Short description of the bug]"
labels: ["bug"]
assignees: ""
---

# 🐛 Bug Report

> [!IMPORTANT]
> Please provide enough technical detail to reproduce the issue and evaluate the impact.

### Summary
*One-sentence summary of the bug.*

### Affected Area
- **Affected Contract:** `contracts/escrow` / `contracts/oracle` / `tooling` / `docs`
- **Target Function(s):** e.g. `deposit`, `submit_result`, `create_match`
- **Affected State/Mode:** `Pending` / `Active` / `Completed` / `Cancelled` / `Init`
- **Severity:** High / Medium / Low

### Expected Behavior
*What should happen when this code executes successfully?*

### Actual Behavior
*What happens instead? Include errors, state changes, or unexpected contract behavior.*

### Reproduction Steps
1. ...
2. ...
3. ...

### Environment
- **Rust Version:** `rustc --version`
- **Soroban / Stellar CLI Version:** `stellar --version`
- **Network:** Local Sandbox / Testnet / Futurenet / Other
- **OS / Platform:** Linux / macOS / Windows / Other

### Diagnostic Output
```text
# Paste compiler errors, panic output, or terminal logs here
```

### Impact
- [ ] Security
- [ ] Correctness
- [ ] Performance or gas
- [ ] UX or tooling
- [ ] Documentation

### Suggested Fix
*If you have an idea for a fix, describe it here.*

### Checklist
- [ ] Reproduced the bug locally
- [ ] Added a regression test covering this failure mode
- [ ] Verified authorization and state guards
- [ ] Confirmed event emission and storage behavior
- [ ] Ran `cargo fmt` and `cargo clippy`
- [ ] Ran `cargo test`
