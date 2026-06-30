---
name: "🧪 Testing Work"
about: "Propose new tests, validation cases, or test suite enhancements."
title: "Add Test: [Short description of the test scenario]"
labels: ["testing"]
assignees: ""
---

# 🧪 Testing Work

> [!TIP]
> High test coverage is essential for contract security. Outline the target scenarios and assertions to help us maintain a robust test suite.

---

### 🎯 Testing Objective
*Describe the specific test coverage gaps, edge cases, or regression scenarios that need test coverage.*

---

### 🛡️ Targeted Test Cases

| Scenario | Inputs / Context | Expected Assertions |
| :--- | :--- | :--- |
| **1. Success Flow** | e.g. Normal deposit execution | State transitions to Active, event emitted |
| **2. Boundary / Error** | e.g. Negative stake amount | Contract rejects call with `InvalidAmount` |
| **3. Auth / Security** | e.g. Non-admin calls pause | Contract rejects call with `Unauthorized` |

---

### 📁 Target Modules
- [ ] Escrow Contract (`contracts/escrow/src/test.rs`)
- [ ] Oracle Contract (`contracts/oracle/src/test.rs`)
- [ ] E2E Integration Suite (`contracts/escrow/tests/`)

---

### 📋 Checklist & Tasks
- [ ] **Setup:** Configure the required mock environments, token balances, or admin registers in the test suite
- [ ] **Write:** Implement unit/integration tests with highly descriptive names
- [ ] **Verify State:** Assert exact ledger modifications and contract balances
- [ ] **Verify Errors:** Assert expected error codes or contract panics on negative paths
- [ ] **Verify Events:** Match topic and data structure for all emitted events
- [ ] **Execution:** Confirm the targeted test suite executes successfully
- [ ] **Assure:** Run the full workspace test suite to guarantee zero regressions
