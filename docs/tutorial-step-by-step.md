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
| **Total** | | **< 15 min** |

> 💡 **First time only:** the contract build (`./scripts/build.sh`) can take a
> few minutes as Rust compiles dependencies. Subsequent runs are fast.

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
> two contract IDs printed after deployment. See the
> [assets guide](assets/tutorial/README.md) for how to add yours.

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

## 🎬 Video walkthroughs

Short screen recordings of each step. Recordings are produced with
[OBS Studio](https://obsproject.com/) or [Loom](https://www.loom.com/) and
checked into `docs/assets/tutorial/` (or linked below once published).

| Walkthrough | Length | Link |
|-------------|--------|------|
| Full run (setup → payout) | ~12 min | _Add link — see [assets guide](assets/tutorial/README.md)_ |
| Step 1 — Create a match | ~2 min | _Add link_ |
| Step 2 — Deposit funds | ~2 min | _Add link_ |
| Step 3 — Check result & payout | ~3 min | _Add link_ |

> 📹 **Contributing a recording?** Follow the capture checklist in the
> [assets guide](assets/tutorial/README.md) so recordings stay consistent
> (same window size, redacted secret keys, captions on each step).

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
