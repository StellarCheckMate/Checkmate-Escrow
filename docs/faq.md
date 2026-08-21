# Frequently Asked Questions (FAQ)

## Match & Gameplay

### 1. What happens if a match ends in a draw?

Both players get their stakes back. The escrow holds `2 × stake_amount`, and when the oracle submits a draw result, each player receives `1 × stake_amount` back. No one wins the full pot.

### 2. What's the difference between `is_funded` and `get_escrow_balance`?

- **`is_funded(match_id)`** — returns `true` only when **both** players have deposited. It gates whether the game can legally start. Use this to check if the match is ready to play.
- **`get_escrow_balance(match_id)`** — returns the total amount currently held in escrow: `0`, `1 × stake`, or `2 × stake`. Once a match is completed or cancelled (payouts done), this returns `0`.

| Scenario | `is_funded` | `get_escrow_balance` |
|----------|------------|----------------------|
| Only player1 deposited | `false` | `1 × stake` |
| Both deposited (Active) | `true` | `2 × stake` |
| Completed (payout done) | `true` | `0` |
| Cancelled (refunds done) | `false` | `0` |

### 3. How do I recover a stuck pending match?

If a match is stuck in `Pending` (one player deposited but the other didn't), either:

1. **Wait for timeout** — call `expire_match` after the configured timeout elapses (default: 30 days). This refunds the depositor.
2. **Cancel before timeout** — if you're a player in the match, call `cancel_match` to cancel it immediately (only works if the match is still `Pending`, i.e., not yet `Active`).

Check `get_match_timeout()` to see the current timeout, and `get_match(match_id)` to see when it was created.

### 4. Can a player cancel a match after both have deposited?

No. Once a match transitions to `Active` (both players deposited), only the oracle submitting a result or the timeout expiring can end it. This prevents one player from backing out mid-game. See [error code #19](error-codes.md#code-19-matchalreadyactive).

### 5. How long does a payout take?

Payouts settle in seconds once the oracle submits the result on-chain. Stellar's fast finality means the winner's account receives the funds within the block confirmation time (~5–6 seconds).

## Oracle & Results

### 6. Why does the oracle trust model matter?

The oracle is a trusted intermediary that bridges off-chain game data (Lichess, Chess.com) to the on-chain contract. It verifies the game result and submits it. If the oracle is compromised or malicious, it could submit false results. This is unavoidable in the current design — the contract has no way to independently verify that a Lichess game happened or who won.

**Mitigation**: The oracle service is run by the maintainers and operated transparently. Future versions may use decentralized oracle networks (e.g., Chainlink) or cryptographic game proofs to reduce trust.

### 7. What if the oracle service is down when my game finishes?

Resubmit the result once the oracle is back online. As long as the result is submitted within the Soroban TTL window (typically a few ledger slots, but kept generous via ledger snapshot retention), the payout will process. Avoid submitting results extremely late (weeks later) — the ledger snapshot needed for verification may be purged. See [error code #21](error-codes.md#code-21-snapshotnotfound).

### 8. Can a player submit a result themselves?

No. Only the configured oracle address can call `submit_result` (or `submit_result_with_oracle_record`). Players don't have authorization. This prevents disputes over who won — the oracle is the single source of truth.

### 9. What happens if the oracle goes offline?

If the oracle service is unavailable when a match ends, players are protected by multiple recovery mechanisms depending on when the oracle fails:

#### Before both players deposit (match in `Pending` state)

- **Anyone can call `expire_match(match_id)`** after the timeout elapses (default: 30 days from match creation).
- The contract refunds each depositor their stake in full (no penalty).
- The match transitions to `Cancelled` state.

#### After both players deposit but before result submission (match in `Active` state)

If the oracle fails to submit a result after a match has started (both players deposited), there are **two** recovery paths:

1. **Player-initiated recovery (within 24 hours of last activity)**:
   - Either player can call `dispute_and_rollback_match(match_id, reason)` within 24 hours of the last heartbeat.
   - Both players receive full refunds (no cancellation fee).
   - The match transitions to `Cancelled` state.
   - Players can extend this 24-hour window by calling `heartbeat_match` to signal the game is still in progress.

2. **Admin-initiated recovery (after 7 days of stall)**:
   - If more than **7 days** have elapsed since the last heartbeat with no result submitted, the admin can call `admin_resolve_stalled_match(match_id, resolution)`.
   - The admin specifies the resolution: `Winner::Player1`, `Winner::Player2`, or `Winner::Draw` (for full refunds).
   - This provides a bounded recovery path so funds are never permanently locked.
   - The match transitions to `Completed` state.
   - Emits an auditable `("match", "adm_stall")` event to distinguish this operator-initiated resolution from normal oracle settlement.

**Example timeline for an Active match:**
```
Day 0:  Players deposit stakes (match becomes Active)
Day 1:  Game finishes, but oracle is offline
Day 2:  Player calls dispute_and_rollback_match → both refunded ✓
        (or players wait for oracle)
Day 7:  Oracle still offline, 24h window expired
Day 8+: Admin calls admin_resolve_stalled_match → stakes distributed per admin decision
```

#### Short-term outage (oracle comes back online)

- The oracle service can submit the result as long as the Soroban ledger snapshot containing the match is still available (typically a few hours to days, depending on network settings).
- Once the oracle resubmits the result, the payout executes automatically.
- **No funds are at risk** — the escrow holds both stakes until a verified result is submitted.

#### How to check timeout status

```bash
# Get the configured timeout (in seconds; default 30 days)
stellar contract invoke --id $ESCROW_CONTRACT_ID -- get_match_timeout

# Get a specific match's creation time and last heartbeat
stellar contract invoke --id $ESCROW_CONTRACT_ID -- get_match --match_id <ID>
# Look for created_ledger and last_heartbeat in the response

# Check if dispute_and_rollback_match is available (within 24h window):
# current_timestamp - last_heartbeat < 86400 (24 hours in seconds)

# Check if admin_resolve_stalled_match is available (after 7-day stall):
# current_timestamp - last_heartbeat > 604800 (7 days in seconds)
```

#### For developers running an oracle service

- **Implement exponential backoff** when retrying result submissions.
- **Monitor your oracle process** for crashes or network connectivity issues.
- **Test failover scenarios** — the contract's timeout mechanism is your safety net, but users expect results within minutes/hours, not days.
- See [docs/oracle.md](oracle.md) for detailed rate limiting, retry strategy, and error handling guidance.

#### For match participants

- **If a match stalls before deposits:** Wait for the timeout, then call `expire_match` to recover your stake.
- **If a match stalls after deposits (Active):** 
  - Within 24 hours: Call `dispute_and_rollback_match` to get an immediate refund.
  - After 24 hours but before 7 days: Contact the oracle operator and wait.
  - After 7 days: Contact the admin to call `admin_resolve_stalled_match`.
- **If the oracle is permanently down:** The 7-day admin resolution window ensures your funds are never permanently locked.
- The combination of player rollback (24h) and admin resolution (7d) provides multiple circuit breakers — your funds cannot remain locked indefinitely.

## Testnet vs. Mainnet

### 10. How do I test on testnet without real money?

1. Use `stellar keys generate` to create a testnet account.
2. Fund it with free testnet tokens via the [Stellar Testnet Faucet](https://laboratory.stellar.org/#account-creator?network=testnet).
3. Create matches with small stake amounts on the testnet contract (configured in `.env` as `STELLAR_NETWORK=testnet`).
4. Follow the [Interactive Tutorial](tutorial-step-by-step.md) for a step-by-step walkthrough.

Testnet tokens have zero real value — no financial risk.

### 11. What's the risk difference between testnet and mainnet?

- **Testnet**: Tokens are test-only. Contracts are frequently updated. Use for learning and integration testing.
- **Mainnet**: Real tokens with real value. Contract code is audited and stable. Players risk actual funds. Always verify the contract address before sending real money.

Check `STELLAR_NETWORK` in `.env` to confirm which network you're on. Testnet RPC: `https://soroban-testnet.stellar.org`. Mainnet RPC: `https://soroban-mainnet.stellar.org`.

## Tokens & Allowlisting

### 12. Which tokens can I use?

By default, any Stellar token address is accepted. However, once the admin calls `add_allowed_token` with at least one token, the contract **only** accepts tokens on the allowlist. Call `get_allowed_tokens()` to see which tokens are currently allowed.

### 13. What if I try to create a match with a non-allowed token?

You'll get [error code #17](error-codes.md#code-17-tokennotallowed). Ask the admin to either:
- Add your token via `add_allowed_token`, or
- Use a token already on the allowlist.

## Administration

### 14. What can the admin do?

The admin can:
- Add/remove allowed tokens
- Pause/unpause the contract (blocks new matches, deposits, and result submissions)
- Update the oracle address
- Set the match timeout
- Transfer admin rights to another account

The admin cannot directly cancel matches or refund stuck stakes — only `expire_match`, `cancel_match`, or player actions do that.

### 15. How do I transfer admin rights safely?

Use a two-step process:
1. Current admin calls `propose_admin(new_admin_address)`.
2. New admin calls `accept_admin()` to confirm.

This prevents mistakes like typos in the new admin address. If the new admin rejects or doesn't accept, rights stay with the current admin.

## Errors & Troubleshooting

### 16. I got error #4 (Unauthorized). What now?

Either:
1. You're signing with the wrong keypair (not the admin, oracle, or depositing player).
2. The contract hasn't been `initialize`d yet.

Check `is_initialized()` and verify your signer. If initializing, ensure you're passing the correct admin and oracle addresses.

For detailed error reference, see [Error Codes Reference](error-codes.md).
