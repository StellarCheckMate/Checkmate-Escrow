# Runbook: Admin Rotation & Oracle Rotation

This runbook covers the step-by-step procedures for rotating the admin key on either contract, or rotating the oracle address trusted by the escrow contract. Follow these steps exactly; deviating from the order or skipping verification can leave the contract in an unrecoverable state.

---

## Table of Contents

- [Prerequisites](#prerequisites)
- [Rotate the Oracle Address (Escrow)](#rotate-the-oracle-address-escrow)
- [Rotate the Escrow Admin (Two-Step)](#rotate-the-escrow-admin-two-step)
- [Rotate the Oracle Contract Admin (Direct)](#rotate-the-oracle-contract-admin-direct)
- [Verification Steps](#verification-steps)
- [Rollback Procedures](#rollback-procedures)
- [Event Reference](#event-reference)

---

## Prerequisites

Before starting any rotation:

- Confirm the current admin keypair is available and operational.
- Confirm the new keypair has been generated, backed up, and tested (e.g., can sign a testnet transaction).
- If using a multi-sig wallet, ensure all required signers are reachable.
- Note current contract state — check that no rotation is already in progress:

```bash
# Escrow: confirm current admin and oracle
stellar contract invoke --id $ESCROW_CONTRACT_ID --network <network> -- get_admin
stellar contract invoke --id $ESCROW_CONTRACT_ID --network <network> -- get_oracle

# Oracle: confirm current admin
stellar contract invoke --id $ORACLE_CONTRACT_ID --network <network> -- get_admin
```

- Recommended: pause the relevant contract during rotation to prevent new activity from racing with the key change (see [runbook-pause.md](runbook-pause.md)).

---

## Rotate the Oracle Address (Escrow)

Use this procedure when the oracle keypair is compromised or the oracle service is being migrated to a new contract address.

**Who can execute:** Escrow admin only.

### Step 1 — Pause the escrow contract (recommended)

```bash
stellar contract invoke \
  --id $ESCROW_CONTRACT_ID \
  --source <ESCROW_ADMIN_KEYPAIR> \
  --network <network> \
  -- pause
```

### Step 2 — Submit the oracle rotation

```bash
stellar contract invoke \
  --id $ESCROW_CONTRACT_ID \
  --source <ESCROW_ADMIN_KEYPAIR> \
  --network <network> \
  -- update_oracle \
  --new_oracle <NEW_ORACLE_CONTRACT_ADDRESS>
```

`new_oracle` must be a valid contract address and must not be the escrow contract's own address. The call fails with `InvalidAddress` if either condition is violated.

### Step 3 — Verify the rotation

```bash
stellar contract invoke \
  --id $ESCROW_CONTRACT_ID \
  --network <network> \
  -- get_oracle
# Expected output: <NEW_ORACLE_CONTRACT_ADDRESS>
```

Confirm the `admin.oracle_up` event was emitted (see [Event Reference](#event-reference)).

### Step 4 — Test the new oracle

Submit a test result via the new oracle on testnet (or a staging environment) to confirm end-to-end connectivity before unpausing production.

### Step 5 — Unpause the escrow contract

```bash
stellar contract invoke \
  --id $ESCROW_CONTRACT_ID \
  --source <ESCROW_ADMIN_KEYPAIR> \
  --network <network> \
  -- unpause
```

---

## Rotate the Escrow Admin (Two-Step)

The escrow contract uses a two-step handoff to prevent accidentally setting an admin address nobody controls. The current admin proposes a nominee; the nominee must accept before control transfers.

**Who can execute step 1:** Current escrow admin.  
**Who can execute step 2:** The nominated new admin.

### Step 1 — Propose the new admin

```bash
stellar contract invoke \
  --id $ESCROW_CONTRACT_ID \
  --source <CURRENT_ESCROW_ADMIN_KEYPAIR> \
  --network <network> \
  -- propose_admin \
  --new_admin <NEW_ESCROW_ADMIN_ADDRESS>
```

> Until the nominee calls `accept_admin`, the current admin retains full control. The proposal can be superseded by calling `propose_admin` again with a different address if needed.

### Step 2 — Nominee accepts

```bash
stellar contract invoke \
  --id $ESCROW_CONTRACT_ID \
  --source <NEW_ESCROW_ADMIN_KEYPAIR> \
  --network <network> \
  -- accept_admin
```

Control transfers atomically at this point.

### Step 3 — Verify the rotation

```bash
stellar contract invoke \
  --id $ESCROW_CONTRACT_ID \
  --network <network> \
  -- get_admin
# Expected output: <NEW_ESCROW_ADMIN_ADDRESS>
```

Confirm the `admin.xfer` event was emitted (see [Event Reference](#event-reference)).

### Step 4 — Revoke the old keypair's access

- Retire the old admin keypair from all secrets managers and CI/CD systems.
- Confirm the old admin address can no longer call admin-gated functions (e.g., attempt a `pause` with the old key; it should fail with `Unauthorized`).

---

## Rotate the Oracle Contract Admin (Direct)

The oracle contract uses a single-step admin rotation. The current admin directly sets the new admin address — there is no nomination/acceptance step. Take extra care to verify the new address before submitting.

**Who can execute:** Current oracle admin only.

### Step 1 — Pause the oracle contract (recommended)

```bash
stellar contract invoke \
  --id $ORACLE_CONTRACT_ID \
  --source <ORACLE_ADMIN_KEYPAIR> \
  --network <network> \
  -- pause
```

### Step 2 — Submit the rotation

```bash
stellar contract invoke \
  --id $ORACLE_CONTRACT_ID \
  --source <ORACLE_ADMIN_KEYPAIR> \
  --network <network> \
  -- update_admin \
  --new_admin <NEW_ORACLE_ADMIN_ADDRESS>
```

Control transfers immediately.

### Step 3 — Verify the rotation

```bash
stellar contract invoke \
  --id $ORACLE_CONTRACT_ID \
  --network <network> \
  -- get_admin
# Expected output: <NEW_ORACLE_ADMIN_ADDRESS>
```

Confirm the `admin.admin_rot` event was emitted (see [Event Reference](#event-reference)).

### Step 4 — Unpause the oracle contract

```bash
stellar contract invoke \
  --id $ORACLE_CONTRACT_ID \
  --source <NEW_ORACLE_ADMIN_KEYPAIR> \
  --network <network> \
  -- unpause
```

### Step 5 — Revoke the old keypair

Retire the old oracle admin keypair from all secrets managers and automated services.

---

## Verification Steps

After any rotation, run this full verification pass:

```bash
# Confirm escrow admin
stellar contract invoke --id $ESCROW_CONTRACT_ID --network <network> -- get_admin

# Confirm oracle address known to escrow
stellar contract invoke --id $ESCROW_CONTRACT_ID --network <network> -- get_oracle

# Confirm oracle contract admin
stellar contract invoke --id $ORACLE_CONTRACT_ID --network <network> -- get_admin

# Confirm contracts are not paused (unless deliberately left paused)
stellar contract invoke --id $ESCROW_CONTRACT_ID --network <network> -- is_paused
stellar contract invoke --id $ORACLE_CONTRACT_ID --network <network> -- is_paused
```

Spot-check by running a low-stakes match end-to-end on testnet with the new keys, from `create_match` through `submit_result`, and verify the payout is processed.

---

## Rollback Procedures

### Oracle rotation rollback

If the new oracle address is wrong or unreachable, call `update_oracle` again with the old oracle address — provided the escrow admin key is still available:

```bash
stellar contract invoke \
  --id $ESCROW_CONTRACT_ID \
  --source <ESCROW_ADMIN_KEYPAIR> \
  --network <network> \
  -- update_oracle \
  --new_oracle <OLD_ORACLE_CONTRACT_ADDRESS>
```

### Escrow admin rotation rollback (during proposal phase)

If the proposal has not yet been accepted, overwrite it with a corrected nominee:

```bash
stellar contract invoke \
  --id $ESCROW_CONTRACT_ID \
  --source <CURRENT_ESCROW_ADMIN_KEYPAIR> \
  --network <network> \
  -- propose_admin \
  --new_admin <CORRECT_NEW_ADMIN_ADDRESS>
```

### Escrow admin rotation rollback (after acceptance)

Once `accept_admin` has been called, the old admin has no authority. The new admin must use `propose_admin` + `accept_admin` to transfer control back:

```bash
# New admin proposes the old (or another) admin
stellar contract invoke \
  --id $ESCROW_CONTRACT_ID \
  --source <NEW_ESCROW_ADMIN_KEYPAIR> \
  --network <network> \
  -- propose_admin \
  --new_admin <PREVIOUS_OR_RECOVERY_ADMIN_ADDRESS>
```

### Oracle admin rotation rollback

Because `update_admin` is a direct one-step transfer, rollback requires the new admin to call `update_admin` again:

```bash
stellar contract invoke \
  --id $ORACLE_CONTRACT_ID \
  --source <NEW_ORACLE_ADMIN_KEYPAIR> \
  --network <network> \
  -- update_admin \
  --new_admin <PREVIOUS_OR_RECOVERY_ADMIN_ADDRESS>
```

If the new admin keypair is lost or compromised, there is no on-chain recovery path. Prevent this by verifying the new keypair can sign transactions before completing any rotation.

---

## Event Reference

Monitor for these events to confirm operations were recorded on-chain:

| Event topic | Contract | Data | Meaning |
|---|---|---|---|
| `(admin, oracle_up)` | Escrow | `(old_oracle, new_oracle)` | Oracle address updated |
| `(admin, xfer)` | Escrow | `(old_admin, new_admin)` | Admin transfer completed |
| `(admin, admin_rot)` | Oracle | `(old_admin, new_admin)` | Oracle admin rotated |
| `(admin, paused)` | Either | — | Contract paused |
| `(admin, unpaused)` | Either | — | Contract unpaused |

Use a Stellar event streaming endpoint or `stellar events` CLI to confirm these events appear in the ledger after each operation.
