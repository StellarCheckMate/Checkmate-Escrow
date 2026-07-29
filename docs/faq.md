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

If the oracle service is unavailable when a match ends, players are protected by the **match timeout** mechanism. Here's how it works:

#### Short-term outage (oracle comes back online)

- The oracle service can submit the result as long as the Soroban ledger snapshot containing the match is still available (typically a few hours to days, depending on network settings).
- Once the oracle resubmits the result, the payout executes automatically.
- **No funds are at risk** — the escrow holds both stakes until a verified result is submitted.

#### Prolonged outage or oracle never comes back

If the oracle remains offline past the match creation timeout (default: 30 days), players can recover their funds:

1. **Anyone can call `expire_match(match_id)`** after the timeout elapses.
2. The contract refunds each depositor their stake in full (no penalty).
3. The match transitions to `Cancelled` state.

**Example timeline:**
```
Day 0:  Players create match and deposit stakes
Day 1:  Game finishes, but oracle is offline
Day 2:  Oracle comes back — submits result, payout executes ✓
        (or oracle remains offline)
Day 30: Timeout elapses (default)
Day 31: Any player (or anyone else) calls expire_match → stakes refunded
```

#### How to check timeout status

```bash
# Get the configured timeout (in ledgers; default 17,280 ≈ 30 days)
stellar contract invoke --id $ESCROW_CONTRACT_ID -- get_match_timeout

# Get a specific match's creation time
stellar contract invoke --id $ESCROW_CONTRACT_ID -- get_match --match_id <ID>
# Look for created_ledger in the response

# Calculate if expire_match is eligible:
# current_ledger - created_ledger >= timeout
```

#### For developers running an oracle service

- **Implement exponential backoff** when retrying result submissions.
- **Monitor your oracle process** for crashes or network connectivity issues.
- **Test failover scenarios** — the contract's timeout mechanism is your safety net, but users expect results within minutes/hours, not days.
- See [docs/oracle.md](oracle.md) for detailed rate limiting, retry strategy, and error handling guidance.

#### For match participants

- **If a match stalls:** Contact the oracle operator through the expected channels (GitHub, email, Discord, etc.) and ask for a status update.
- **If the oracle is permanently down:** After the timeout, call `expire_match` to recover your stake. You'll get your deposit back even if the match never completes.
- The timeout is a guaranteed **circuit breaker** — your funds cannot remain locked indefinitely.

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
