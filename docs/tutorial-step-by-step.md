# 🎓 Checkmate-Escrow Interactive Tutorial

> **Goal:** Take a brand-new user from zero to a completed, paid-out match on
> Stellar **testnet** in **under 15 minutes** — with no real money at risk.

This is the guided, hands-on companion to the end-to-end
[demo walkthrough](../demo/demo-script.md). Where the demo is a terse reference,
this tutorial is paced for someone seeing Checkmate-Escrow for the first time:
every step explains *what* you are doing, *why* it matters, and *how to know it
worked*.

By the end you will have:

- ✅ Created a match between two players
- ✅ Funded the escrow with both players' stakes
- ✅ Submitted a verified result and watched the winner get paid automatically
- ✅ Verified the on-chain payout transaction on Stellar Explorer
- ✅ Queried get_player_stats and confirmed the on-chain analytics reflect the match result

---

## 🧭 How to use this tutorial

Pick the format that suits you — the content is identical across all three:

| Format | Best for | Where |
|--------|----------|-------|
| 📝 **Text** | Following along at your own pace | This document |
| 🎬 **Video** | Watching the full flow first | [Video walkthroughs](#-video-walkthroughs) |
| 🧩 **Interactive** | Checking your understanding | [Tutorial quiz & checklist](tutorial-quiz.md) |

> 🧪 **Practice mode (testnet-only).** Everything below runs on Stellar
> **testnet** using free [Friendbot](https://friendbot.stellar.org) funds. No
> mainnet keys, no real XLM, nothing to lose. This is the safe sandbox for
> learning — see [Practice mode & local development](#-practice-mode--local-development)
> if you would rather run everything against a local node.

---

## 📋 Before you begin

### What you need installed

| Tool | Why | Check it works |
|------|-----|----------------|
| [Rust](https://www.rust-lang.org/tools/install) (1.70+) | Builds the contracts | `rustc --version` |
| `wasm32` target | Compiles to Wasm | `rustup target add wasm32-unknown-unknown` |
| [Stellar CLI](https://developers.stellar.org/docs/tools/developer-tools/cli/stellar-cli) | Talks to the network | `stellar --version` |
| `curl` | Funds testnet accounts | `curl --version` |

### Time budget

| Step | What happens | Approx. time |
|------|--------------|--------------|
| Setup | Build, keys, deploy | ~6 min |
| Step 1 | Create a match | ~2 min |
| Step 2 | Deposit funds | ~2 min |
| Step 3 | Check the result & payout | ~3 min |
| Step 4 | Verify the payout | ~2 min |
| Step 5 | Read player stats | ~1 min |
| **Total** | | **< 17 min** |

> 💡 **First time only:** the contract build (`./scripts/build.sh`) can take a
> few minutes as Rust compiles dependencies. Subsequent runs are fast.


---

## 🔑 Connecting a wallet (browser UI)

The CLI steps in this tutorial interact with the contracts directly via `stellar contract invoke`. If you are using the **browser frontend** instead of the CLI, you need a connected wallet. Checkmate-Escrow supports two options:

### Option A — Freighter (browser extension)

Freighter is a Chrome/Firefox browser extension that stores your Stellar keys locally.

1. Install from [freighter.app](https://freighter.app).
2. Create or import an account.
3. Switch the network to **Testnet** (top-right network selector in the extension).
4. Click **Connect wallet** in the app and select **Freighter**.
5. Approve the connection request in the extension popup.

The app reads your public key from Freighter. No private key leaves the extension.

### Option B — Albedo (web popup, no install)

Albedo is a web-based key manager that opens a secure `albedo.link` popup for signing. No browser extension is needed.

1. Click **Connect wallet** in the app and select **Albedo**.
2. The Albedo popup opens. Create a new account or log in with an existing one.
3. Approve the public key share request.

Albedo signs transactions in its own popup — your key never touches the app. Some browsers block popups by default; if the popup doesn't open, check your popup-blocker settings and try again from a button click.

### Switching wallets mid-session

Calling `connect('albedo')` or `connect('freighter')` while already connected resets the session cleanly — no page reload required.

> 🧪 **CLI tutorial:** The steps from [Setup](#️-setup--get-to-a-deployed-contract) onward use `stellar contract invoke` directly and do not require a browser wallet. The wallet section above only applies when running the frontend.

---

## 🏗️ Setup — get to a deployed contract

Run these once before Step 1. They mirror the
[demo walkthrough](../demo/demo-script.md), condensed here so the tutorial is
self-contained.

```bash
# 1. Build the contracts
./scripts/build.sh

# 2. Create three testnet identities
stellar keys generate admin   --network testnet
stellar keys generate player1 --network testnet
stellar keys generate player2 --network testnet

# 3. Fund them with free testnet XLM via Friendbot
for KEY in admin player1 player2; do
  curl -s "https://friendbot.stellar.org?addr=$(stellar keys address $KEY)" > /dev/null
  echo "Funded $KEY: $(stellar keys address $KEY)"
done

# 4. Deploy both contracts
ESCROW_ID=$(stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/escrow.wasm \
  --source admin --network testnet)
ORACLE_ID=$(stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/oracle.wasm \
  --source admin --network testnet)

# 5. Initialize both contracts
ADMIN_ADDR=$(stellar keys address admin)
stellar contract invoke --id $ORACLE_ID --source admin --network testnet \
  -- initialize --admin $ADMIN_ADDR
stellar contract invoke --id $ESCROW_ID --source admin --network testnet \
  -- initialize --oracle $ORACLE_ID --admin $ADMIN_ADDR

# 6. Grab the native XLM token + player addresses for later
XLM_TOKEN=$(stellar contract id asset --asset native --network testnet)
P1_ADDR=$(stellar keys address player1)
P2_ADDR=$(stellar keys address player2)

echo "Escrow: $ESCROW_ID"
echo "Oracle: $ORACLE_ID"
```

> 📸 _Screenshot:_ `assets/tutorial/00-setup-deployed.png` — terminal showing the
> two contract IDs printed after deployment.

✅ **Checkpoint:** you should see two long `C...` contract IDs. Keep this terminal
open — the `$ESCROW_ID`, `$ORACLE_ID`, `$XLM_TOKEN`, `$P1_ADDR`, and `$P2_ADDR`
variables are reused in every step below.

---

## ♟️ Step 1 — Create a match

**What you're doing:** registering a wager on-chain. Player1 proposes a match
against Player2, declaring the stake, the token, and which chess game settles it.

**Why it matters:** the match is the contract's unit of escrow. Nothing can be
deposited or paid out until a match exists, and its terms (stake, token, players)
are locked in at creation.

```bash
MATCH_ID=$(stellar contract invoke --id $ESCROW_ID --source player1 --network testnet \
  -- create_match \
    --player1      $P1_ADDR \
    --player2      $P2_ADDR \
    --stake_amount 100000000 \
    --token        $XLM_TOKEN \
    --game_id      "abc123xyz" \
    --platform     Lichess)

echo "Match ID: $MATCH_ID"
```

> 🔢 **Stroops, not XLM.** Stellar amounts are integers in *stroops*:
> `1 XLM = 10,000,000 stroops`. So `100000000` above is **10 XLM** per player.

**Confirm it worked:**

```bash
stellar contract invoke --id $ESCROW_ID --source admin --network testnet \
  -- get_match --match_id $MATCH_ID
```

```
state: Pending, player1_deposited: false, player2_deposited: false
```

> 📸 _Screenshot:_ `assets/tutorial/01-create-match.png` — the `get_match` output
> showing `state: Pending`.

✅ **Checkpoint:** the match is `Pending`. Neither player has deposited yet.

🧩 **Quick check:** *Why is the match `Pending` and not `Active`?*
<details><summary>Answer</summary>
A match only becomes <code>Active</code> once <em>both</em> players have
deposited their stake. Creating the match just records the terms; no funds have
moved yet.
</details>

---

## 💰 Step 2 — Deposit funds

**What you're doing:** each player transfers their stake into the escrow contract.

**Why it matters:** the escrow holds both stakes so neither player can back out
once the game starts. The match flips to `Active` the moment the **second**
deposit lands — that is the on-chain signal that the game may begin.

```bash
# Player1 deposits their 10 XLM stake
stellar contract invoke --id $ESCROW_ID --source player1 --network testnet \
  -- deposit --match_id $MATCH_ID --player $P1_ADDR

# Player2 deposits their 10 XLM stake
stellar contract invoke --id $ESCROW_ID --source player2 --network testnet \
  -- deposit --match_id $MATCH_ID --player $P2_ADDR
```

**Confirm the escrow is fully funded:**

```bash
stellar contract invoke --id $ESCROW_ID --source admin --network testnet \
  -- is_funded --match_id $MATCH_ID
# true

stellar contract invoke --id $ESCROW_ID --source admin --network testnet \
  -- get_escrow_balance --match_id $MATCH_ID
# 200000000   (2 × 10 XLM in stroops)
```

> 📸 _Screenshot:_ `assets/tutorial/02-deposit-funded.png` — `is_funded` returning
> `true` and the balance at `200000000`.

> 🧠 **`is_funded` vs `get_escrow_balance`** — these answer *different*
> questions and are a common source of confusion:
> - `is_funded` → `true` only when **both** players have deposited (match is
>   `Active`). It reflects deposit *flags*, not token balances.
> - `get_escrow_balance` → the *amount* currently held: `0`, `1×stake`, or
>   `2×stake`. After payout/refund it returns `0`.

✅ **Checkpoint:** `is_funded` is `true`, balance is `200000000`, and the match
state is now `Active`.

🧩 **Quick check:** *After only Player1 deposits, what does each function return?*
<details><summary>Answer</summary>
<code>is_funded</code> → <code>false</code> (both must deposit), but
<code>get_escrow_balance</code> → <code>100000000</code> (one stake is already
held). See the table in the <a href="../README.md">README</a>.
</details>

---

## 🏆 Step 3 — Check the match result (and get paid)

**What you're doing:** recording the game's outcome and triggering the payout.

**Why it matters:** this is the trustless core of Checkmate-Escrow. Once the
oracle reports a verified result, the escrow pays the winner the **entire pot**
in the same transaction — no platform can withhold or delay it.

The result is settled in two calls:

### 3a — Oracle records the verified result

```bash
stellar contract invoke --id $ORACLE_ID --source admin --network testnet \
  -- submit_result \
    --match_id $MATCH_ID \
    --game_id  "abc123xyz" \
    --platform Lichess \
    --result   Player1Wins
```

> Valid `--result` values: `Player1Wins`, `Player2Wins`, `Draw`.

### 3b — Escrow executes the payout

The escrow only accepts the result from its configured **oracle address** (the
oracle *contract id* in this tutorial), never from the admin directly.

```bash
stellar contract invoke --id $ESCROW_ID --source $ORACLE_ID --network testnet \
  -- submit_result \
    --match_id $MATCH_ID \
    --winner   Player1 \
    --caller   $ORACLE_ID
```

> Valid `--winner` values: `Player1`, `Player2`, `Draw`. On a `Draw`, both
> players get their 10 XLM back instead of one winner taking 20 XLM.

**Confirm the payout:**

```bash
stellar contract invoke --id $ESCROW_ID --source admin --network testnet \
  -- get_match --match_id $MATCH_ID
# state: Completed, completed_ledger: <ledger number>

stellar contract invoke --id $ESCROW_ID --source admin --network testnet \
  -- get_escrow_balance --match_id $MATCH_ID
# 0   (the pot has been paid out)
```

> 📸 _Screenshot:_ `assets/tutorial/03-result-completed.png` — `get_match` showing
> `state: Completed` and the balance back to `0`.

✅ **Checkpoint:** the match is `Completed` and the escrow balance is `0` — the
winner has been paid. 🎉 **You've completed a full match lifecycle!**

🧩 **Quick check:** *Why is the escrow balance `0` even though the winner was just
paid 20 XLM?*
<details><summary>Answer</summary>
<code>get_escrow_balance</code> reports funds <em>held in escrow for the match</em>.
Once the payout executes, the pot has left escrow (it is now in the winner's
wallet), so the escrow balance for that match is <code>0</code>.
</details>

---

## 🔍 Step 4 — Verify the payout

**What you're doing:** inspecting the on-chain settlement and payout transaction using Stellar Explorer (Stellar Expert and Stellar Laboratory).

**Why it matters:** blockchain transactions are publicly verifiable. Checking your transaction on an explorer confirms that the escrow contract transferred the pot directly to the winner's wallet on the Stellar ledger, providing independent verification beyond local CLI output.

### Option 1: Verify on Stellar Expert

Stellar Expert is a block explorer for inspecting accounts, transactions, and Soroban contracts on Stellar.

- **Inspect the winner's account transactions:**
  ```text
  https://stellar.expert/explorer/testnet/account/<WINNER_ADDRESS>
  ```
  Replace `<WINNER_ADDRESS>` with `$P1_ADDR` (or the winner's address). Under the **Payments** / **History** section, you will see the incoming transfer of 20 XLM credited to the winner from the escrow contract.

- **Inspect the escrow contract:**
  ```text
  https://stellar.expert/explorer/testnet/contract/<ESCROW_ID>
  ```
  Replace `<ESCROW_ID>` with `$ESCROW_ID` to inspect recent contract calls, event emissions, and token transfer history.

- **Inspect the payout transaction directly:**
  ```text
  https://stellar.expert/explorer/testnet/tx/<TRANSACTION_HASH>
  ```
  Replace `<TRANSACTION_HASH>` with the transaction hash output from the Step 3b `submit_result` invocation.

### Option 2: Verify on Stellar Laboratory

Alternatively, you can inspect transaction envelopes, operations, and ledger effects directly using Stellar Laboratory:

```text
https://laboratory.stellar.org/#explorer?resource=transactions&endpoint=single&values=<TRANSACTION_HASH>&network=testnet
```

> 📸 _Screenshot:_ `assets/tutorial/04-verify-payout.png` — Stellar Expert view showing the payout transfer transaction to the winner's address.

✅ **Checkpoint:** you can view the on-chain payment operation crediting the winner's account with the pot on the testnet explorer. 🎉 **You've completed and verified a full match lifecycle!**

🧩 **Quick check:** *How does Stellar Expert verify the escrow payout occurred?*
<details><summary>Answer</summary>
The ledger records a contract call to <code>submit_result</code> which invoked the token contract transfer, debiting the escrow contract balance and crediting the winner's address with the payout amount.
</details>

---

## 📊 Step 5 — Read player statistics

**What you're doing:** querying the escrow contract for cumulative match statistics for each player after the game is complete.

**Why it matters:** `get_player_stats` is the analytics endpoint of the protocol. It aggregates every match a player has participated in — wins, losses, draws, and total volume — directly from on-chain state. Reading it after a match confirms the result was recorded correctly and shows the running lifetime totals for each player.

```bash
# Query stats for the winner (Player1)
stellar contract invoke --id $ESCROW_ID --source admin --network testnet \
  -- get_player_stats --player $P1_ADDR

# Query stats for the loser (Player2)
stellar contract invoke --id $ESCROW_ID --source admin --network testnet \
  -- get_player_stats --player $P2_ADDR
```

**Expected output for a finished match (Player1 won):**

```
# Player1 (winner)
{
  "matches_played": 1,
  "wins": 1,
  "losses": 0,
  "draws": 0,
  "total_wagered": 100000000,
  "total_won": 200000000
}

# Player2 (loser)
{
  "matches_played": 1,
  "wins": 0,
  "losses": 1,
  "draws": 0,
  "total_wagered": 100000000,
  "total_won": 0
}
```

**Field reference:**

| Field | Type | Description |
|-------|------|-------------|
| `matches_played` | integer | Total matches completed by this player (wins + losses + draws) |
| `wins` | integer | Matches where this player was declared the winner |
| `losses` | integer | Matches where the opposing player won |
| `draws` | integer | Matches that ended in a draw (both players refunded) |
| `total_wagered` | integer (stroops) | Sum of all stake amounts this player deposited across all matches |
| `total_won` | integer (stroops) | Total payout received (stake × 2 on a win, stake × 1 on a draw, 0 on a loss) |

> 🔢 **Stroops reminder.** `total_wagered: 100000000` means the player wagered 10 XLM. `total_won: 200000000` means they won the full 20 XLM pot (both players' stakes combined).

> 📸 _Screenshot:_ `assets/tutorial/05-player-stats.png` — terminal showing `get_player_stats` output for both players after the match is completed.

✅ **Checkpoint:** `get_player_stats` for Player1 shows `wins: 1` and `total_won: 200000000`. Player2 shows `losses: 1` and `total_won: 0`. These totals accumulate across all matches, so running this tutorial again with the same keys will increment the counters.

🧩 **Quick check:** *What would `total_won` be for each player if the match had ended in a draw?*
<details><summary>Answer</summary>
Both players would show <code>total_won: 100000000</code> (their original 10 XLM stake returned) and <code>draws: 1</code>. Neither player receives the other's stake on a draw — the escrow refunds each depositor their own stake in full.
</details>

---

## 🎬 Video Walkthroughs

Short screen recordings of the full tutorial flow and individual steps. Each walkthrough shows setup, match creation, funding, and payout verification.

### Planned Walkthroughs

| Title | Duration | Status |
|-------|----------|--------|
| **Full Tutorial Run** — From setup to payout, end-to-end | ~15 min | [Coming Soon] |
| **Part 1: Setup & Deploy** — Building contracts and initializing on testnet | ~6 min | [Coming Soon] |
| **Part 2: Create & Fund a Match** — Creating a match and depositing stakes | ~5 min | [Coming Soon] |
| **Part 3: Verify Result & Claim Winnings** — Oracle submission and automatic payout | ~4 min | [Coming Soon] |

### Recording Guidelines

Recordings are produced with [OBS Studio](https://obsproject.com/) or [Loom](https://www.loom.com/) and follow these guidelines:

- Consistent window size and zoom level
- Redacted secret keys and private data
- Captions or text overlays for each major step
- Clear audio or screen annotations

### Contributing a Walkthrough

We welcome video contributions! To submit:

1. Record your walkthrough following the guidelines above
2. Save to `docs/assets/tutorial/` with a descriptive filename (e.g., `setup-and-deploy.mp4`)
3. Update the table above with your link
4. Submit a PR with the video and any new documentation

---

## 🧪 Practice mode & local development

Everything above runs on **testnet**, which *is* practice mode — the funds come
from Friendbot and have no real value. Two ways to practice safely:

### Testnet practice (recommended for first-timers)

You already did this. To run again with a clean slate, generate fresh keys
(`stellar keys generate player1-v2 --network testnet`) and repeat from Setup.
Re-fund any new key via Friendbot.

### Local development (offline, fully reset-able)

Prefer an offline sandbox? Run a local Stellar node and point everything at the
`standalone` network defined in [`environments.toml`](../environments.toml):

```bash
# Start a local network (see Stellar CLI docs for the current command)
stellar network start local

# Then replace `--network testnet` with `--network standalone` in every command,
# and fund accounts against your local Friendbot instead of friendbot.stellar.org.
```

Local mode resets whenever you stop the node — ideal for repeatedly rehearsing
the flow without touching any shared network.

> ⚠️ **Never use mainnet to practice.** Mainnet moves real XLM. This tutorial is
> deliberately testnet/standalone-only.

---

## 🆘 Troubleshooting common issues

| Symptom | Likely cause | Fix |
|---------|--------------|-----|
| `account not found` / funding errors | Account not funded on testnet | Re-run the Friendbot loop in [Setup](#️-setup--get-to-a-deployed-contract) |
| `error: contract not found` | Wrong/empty `$ESCROW_ID` or `$ORACLE_ID` | Re-run deploy; confirm the variables are still set (`echo $ESCROW_ID`) |
| Build fails with a `wasm32` error | Wasm target missing | `rustup target add wasm32-unknown-unknown` |
| `deposit` fails with an auth error | Wrong `--source` for the player | The `--source` must match the `--player` whose stake is being deposited |
| Payout (`submit_result` on escrow) rejected | Called by `admin` instead of the oracle | `--source` and `--caller` must both be the oracle address (`$ORACLE_ID`) |
| `ContractPaused` error | Admin paused the contract | `stellar contract invoke --id $ESCROW_ID --source admin --network testnet -- unpause` |
| Variables empty in a new terminal | Shell variables don't persist | Re-export `ESCROW_ID`, `ORACLE_ID`, `XLM_TOKEN`, `P1_ADDR`, `P2_ADDR` |

Still stuck? Open an issue or ask in the project's discussions — and please note
**which step** and **what command** so we can improve this tutorial.

---

## ✅ Next steps

- 🧩 Test your understanding with the [tutorial quiz & checklist](tutorial-quiz.md)
- 📊 Query both players' career statistics with `get_player_stats` to see how totals accumulate over multiple matches
- 📖 Read the full [demo walkthrough](../demo/demo-script.md) for governance,
  timeouts, and cancellation flows
- 🏛️ Understand the design in the [architecture overview](architecture.md)
- 🔮 Learn how results are verified in the [oracle design](oracle.md)
- 🤝 Ready to contribute? See the [contributing guidelines](../CONTRIBUTING.md)

---

> 🔄 **Keep this tutorial alive.** If a step confused you, that's a documentation
> bug. Open an issue or PR noting where you got stuck — we update this guide
> whenever user feedback reveals a rough edge (see the **Feedback loop** section
> of the [tutorial quiz](tutorial-quiz.md)).
