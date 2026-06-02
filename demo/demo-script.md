# Checkmate-Escrow Demo: Testnet Walkthrough

End-to-end walkthrough of a match lifecycle on Stellar testnet — deploy, fund, play, payout.

## Prerequisites

- Rust with `wasm32-unknown-unknown` target: `rustup target add wasm32-unknown-unknown`
- [Stellar CLI](https://developers.stellar.org/docs/tools/developer-tools/cli/stellar-cli) (`stellar`)
- `curl` (for Friendbot funding)

---

## 1. Build the Contracts

```bash
./scripts/build.sh
```

Produces:
- `target/wasm32-unknown-unknown/release/escrow.wasm`
- `target/wasm32-unknown-unknown/release/oracle.wasm`

---

## 2. Create Testnet Identities

```bash
stellar keys generate admin   --network testnet
stellar keys generate player1 --network testnet
stellar keys generate player2 --network testnet
```

Fund all three via Friendbot:

```bash
for KEY in admin player1 player2; do
  curl -s "https://friendbot.stellar.org?addr=$(stellar keys address $KEY)" > /dev/null
  echo "Funded $KEY: $(stellar keys address $KEY)"
done
```

---

## 3. Deploy the Contracts

```bash
ESCROW_ID=$(stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/escrow.wasm \
  --source admin \
  --network testnet)

ORACLE_ID=$(stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/oracle.wasm \
  --source admin \
  --network testnet)

echo "Escrow contract:  $ESCROW_ID"
echo "Oracle contract:  $ORACLE_ID"
```

---

## 4. Initialize the Contracts

```bash
ADMIN_ADDR=$(stellar keys address admin)

# Initialize the oracle — admin is the trusted off-chain result submitter
stellar contract invoke --id $ORACLE_ID --source admin --network testnet \
  -- initialize \
    --admin $ADMIN_ADDR

# Initialize the escrow — oracle is the oracle *contract* address,
# admin is the escrow admin (pause/unpause/rotate oracle)
stellar contract invoke --id $ESCROW_ID --source admin --network testnet \
  -- initialize \
    --oracle $ORACLE_ID \
    --admin $ADMIN_ADDR
```

---

## 5. Get the Native XLM Token Address

```bash
XLM_TOKEN=$(stellar contract id asset --asset native --network testnet)
echo "XLM token contract: $XLM_TOKEN"
```

---

## 6. Create a Match

Player1 creates a match. Both players will stake 10 XLM each (1 XLM = 10,000,000 stroops).

```bash
P1_ADDR=$(stellar keys address player1)
P2_ADDR=$(stellar keys address player2)

MATCH_ID=$(stellar contract invoke --id $ESCROW_ID --source player1 --network testnet \
  -- create_match \
    --player1  $P1_ADDR \
    --player2  $P2_ADDR \
    --stake_amount 100000000 \
    --token    $XLM_TOKEN \
    --game_id  "abc123xyz" \
    --platform Lichess)

echo "Match ID: $MATCH_ID"
```

Verify:

```bash
stellar contract invoke --id $ESCROW_ID --source admin --network testnet \
  -- get_match --match_id $MATCH_ID
# state: Pending, player1_deposited: false, player2_deposited: false
```

---

## 7. Both Players Deposit

Each player transfers their stake into the escrow contract.

```bash
stellar contract invoke --id $ESCROW_ID --source player1 --network testnet \
  -- deposit --match_id $MATCH_ID --player $P1_ADDR

stellar contract invoke --id $ESCROW_ID --source player2 --network testnet \
  -- deposit --match_id $MATCH_ID --player $P2_ADDR
```

Confirm the escrow is fully funded:

```bash
stellar contract invoke --id $ESCROW_ID --source admin --network testnet \
  -- is_funded --match_id $MATCH_ID
# true

stellar contract invoke --id $ESCROW_ID --source admin --network testnet \
  -- get_escrow_balance --match_id $MATCH_ID
# 200000000  (2 × 10 XLM in stroops)
```

Match state is now `Active`.

Quick state/event checks (optional but recommended):

```bash
# Depositor flags imply funding
stellar contract invoke --id $ESCROW_ID --source admin --network testnet \
  -- is_funded --match_id $MATCH_ID
# true

# Check match state + timestamp/ledger fields
stellar contract invoke --id $ESCROW_ID --source admin --network testnet \
  -- get_match --match_id $MATCH_ID
# state: Active, player1_deposited: true, player2_deposited: true
```

---

## 8. Submit the Result

After the Lichess game completes, two calls are needed:

**Step 8a — Oracle contract records the result** (oracle contract admin submits):

```bash
# Note: the escrow payout requires `caller` auth equal to the `oracle` address stored in Escrow.
# In this demo, `oracle` is the oracle *contract id address* ($ORACLE_ID).

stellar contract invoke --id $ORACLE_ID --source admin --network testnet \
  -- submit_result \
    --match_id $MATCH_ID \
    --game_id  "abc123xyz" \
    --platform Lichess \
    --result   Player1Wins
```

Valid `--result` values: `Player1Wins`, `Player2Wins`, `Draw`.

**Step 8b — Escrow contract executes the payout** (escrow is called by the configured oracle address):

```bash
stellar contract invoke --id $ESCROW_ID --source $ORACLE_ID --network testnet \
  -- submit_result \
    --match_id $MATCH_ID \
    --winner   Player1 \
    --caller   $ORACLE_ID
```

Valid `--winner` values: `Player1`, `Player2`, `Draw`.

This atomically transfers the full pot (20 XLM) to Player1. For a draw, both players receive their 10 XLM back.

> **What changed vs older demos?** The current escrow contract does **not** allow the escrow admin (`admin`) to submit payouts. It requires `caller` auth to be the oracle address configured during `initialize` (the oracle contract id address in this demo).


---

## 9. Verify the Payout (state + expected events)

```bash
stellar contract invoke --id $ESCROW_ID --source admin --network testnet \
  -- get_match --match_id $MATCH_ID
# state: Completed, completed_ledger: <ledger number>

stellar contract invoke --id $ESCROW_ID --source admin --network testnet \
  -- get_escrow_balance --match_id $MATCH_ID
# 0  (funds have been paid out)
```

Optional event spot-checks (topic names match contract code):

```bash
# Look for match/completed for this MATCH_ID
# (The stellar CLI/events output format may vary; ensure the match_id and winner are present.)
stellar contract events --id $ESCROW_ID --network testnet --ledger 0
```

---

## 10. Governance + timeout behavior (keep this demo in sync with contract logic)

### 10a. Read and update the match timeout

`set_match_timeout` is admin-only.

```bash
stellar contract invoke --id $ESCROW_ID --source admin --network testnet \
  -- get_match_timeout
# e.g. 518400

# Example: double it
stellar contract invoke --id $ESCROW_ID --source admin --network testnet \
  -- set_match_timeout --timeout 1036800
```

Verify:

```bash
stellar contract invoke --id $ESCROW_ID --source admin --network testnet \
  -- get_match_timeout
# 1036800
```

### 10b. Pause blocks state-changing calls

```bash
# Pause (admin only)
stellar contract invoke --id $ESCROW_ID --source admin --network testnet \
  -- pause

# While paused, this should fail (ContractPaused)
stellar contract invoke --id $ESCROW_ID --source player1 --network testnet \
  -- deposit --match_id $MATCH_ID --player $P1_ADDR

# Unpause
stellar contract invoke --id $ESCROW_ID --source admin --network testnet \
  -- unpause
```


---

## Cancellation (Optional)

Either player can cancel a `Pending` match (before both deposits are made). Any deposited funds are refunded immediately.

```bash
stellar contract invoke --id $ESCROW_ID --source player1 --network testnet \
  -- cancel_match \
    --match_id $MATCH_ID \
    --caller   $P1_ADDR
```

---

## Match Lifecycle Summary

| Step | Contract | Function | Resulting State |
|------|----------|----------|-----------------|
| Create match | Escrow | `create_match` | `Pending` |
| Player1 deposits | Escrow | `deposit` | `Pending` |
| Player2 deposits | Escrow | `deposit` | `Active` |
| Record result | Oracle | `submit_result` | — |
| Execute payout | Escrow | `submit_result` | `Completed` |
