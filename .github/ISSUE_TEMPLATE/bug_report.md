---
name: "🐛 Bug Report"
about: "Report a bug, security vulnerability, or unexpected contract behavior."
title: "Fix: [Short description of the bug]"
labels: ["bug"]
assignees: ""
---

# 🐛 Bug Report

> [!IMPORTANT]
> Please provide as much technical context as possible to help us identify and resolve the issue.

---

### 📝 Problem Summary
*Provide a clear, one-sentence summary of the bug.*

---

### 🔍 Technical Breakdown

| Category | Details |
| :--- | :--- |
| **Affected Contract** | `contracts/escrow` / `contracts/oracle` / `tooling` |
| **Target Function(s)** | e.g., `deposit`, `submit_result`, `create_match` |
| **Severity / Priority** | High / Medium / Low |

#### 💥 Observed Behavior
> Describe what actually happens when this issue is triggered.

#### 🎯 Expected Behavior
> Describe what the correct contract execution or state transition should be.

---

### 💻 Environment & Tooling
*Please specify the versions of the development tools used:*

* **Rust Version:** `rustc --version`
* **Soroban / Stellar CLI Version:** `stellar --version`
* **Network Context:** Local Sandbox / Testnet / Futurenet

---

### 🧪 Reproduction Steps & Diagnostic Output
1. Step one to trigger the issue
2. Step two to trigger the issue
3. Observed command line output

<details>
<summary><b>📋 Diagnostic Logs & Stack Traces</b> (Click to expand)</summary>

```rust
// Paste terminal logs, compiler errors, or panic stack traces here
```
</details>

---

### 🛠️ Proposed Remediation
*Describe any thoughts or potential solutions you have for resolving this issue.*

---

### 📋 Checklist & Tasks
- [ ] **Analyze:** Investigate the source code logic in the affected file
- [ ] **Reproduce:** Add a targeted unit test verifying the failure path
- [ ] **Harden:** Enforce necessary safety guards, boundary validations, or authorization checks
- [ ] **Resolve:** Implement the fix within the contract logic
- [ ] **Verify Events:** Confirm any affected on-chain events are published correctly
- [ ] **Lint & Format:** Execute `cargo fmt` and `cargo clippy` to ensure code quality
- [ ] **Assure:** Execute the full test suite (`cargo test`) to guarantee zero regressions
