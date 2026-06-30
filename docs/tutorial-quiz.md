# 🧩 Tutorial Quiz & Checklist

Interactive companion to the
[step-by-step tutorial](tutorial-step-by-step.md). Use it two ways:

1. **Progress checklist** — tick each box as you complete the hands-on steps.
2. **Knowledge quiz** — answer the questions to confirm you understood *why*
   each step works, not just *how*.

Everything here is **testnet practice mode** — no real funds, nothing to lose.

---

## ✅ Progress checklist

Copy this into a GitHub issue/PR (or just tick along in your editor) as you go.

### Setup

- [ ] Installed Rust, the `wasm32-unknown-unknown` target, Stellar CLI, and `curl`
- [ ] Built the contracts with `./scripts/build.sh`
- [ ] Generated `admin`, `player1`, and `player2` testnet keys
- [ ] Funded all three accounts via Friendbot
- [ ] Deployed and initialized the escrow and oracle contracts
- [ ] Captured `$ESCROW_ID`, `$ORACLE_ID`, `$XLM_TOKEN`, `$P1_ADDR`, `$P2_ADDR`

### Step 1 — Create a match

- [ ] Created a match as `player1`
- [ ] Verified the match state is `Pending`
- [ ] Understood that stake amounts are in **stroops** (`1 XLM = 10,000,000`)

### Step 2 — Deposit funds

- [ ] Deposited `player1`'s stake
- [ ] Deposited `player2`'s stake
- [ ] Confirmed the match is funded (state is now `Active`)
- [ ] Can explain the difference between funded status and escrow balance

### Step 3 — Check the result

- [ ] Recorded the verified result on the oracle contract
- [ ] Executed the payout via the escrow contract (called as the oracle address)
- [ ] Confirmed the match state is `Completed`
- [ ] Confirmed the escrow balance returned to `0`

### Wrap-up

- [ ] Whole flow completed in **under 15 minutes**
- [ ] Read at least one "why it matters" callout per step
- [ ] Noted any confusing moment to report (see [Feedback loop](#-feedback-loop))

---

## 🧠 Knowledge quiz

Answer each, then expand to check. No peeking first — these mirror the real
points new users trip on.

**Q1. After Player1 deposits but Player2 has not, what is the match state?**
<details><summary>Show answer</summary>
Still <code>Pending</code>. A match becomes <code>Active</code> only when
<em>both</em> players have deposited.
</details>

**Q2. You stake "10 XLM". What number do you pass to the stake amount?**
<details><summary>Show answer</summary>
<code>100000000</code> — amounts are in stroops, and
<code>1 XLM = 10,000,000 stroops</code>.
</details>

**Q3. A match is `Active` with both deposits in. What does the funded check
return, and what does the escrow balance return?**
<details><summary>Show answer</summary>
Funded → <code>true</code>; escrow balance → <code>2 × stake</code> (e.g.
<code>200000000</code> for a 10 XLM stake). One reports deposit <em>flags</em>,
the other reports the <em>amount held</em>.
</details>

**Q4. Who is allowed to trigger the payout on the escrow contract?**
<details><summary>Show answer</summary>
Only the configured <strong>oracle address</strong> (the oracle contract id in
the tutorial). The escrow admin <em>cannot</em> submit payouts directly — the
call's <code>--source</code> and <code>--caller</code> must both be the oracle
address.
</details>

**Q5. The match is `Completed`. Why does the escrow balance read `0` when the
winner just received 20 XLM?**
<details><summary>Show answer</summary>
The escrow balance reports funds <em>held in escrow for that match</em>. After
payout the pot has left escrow (it is in the winner's wallet), so the match's
escrow balance is <code>0</code>.
</details>

**Q6. It's a draw. What happens to the stakes?**
<details><summary>Show answer</summary>
Each player gets their own stake back (10 XLM each), rather than one player
taking the full 20 XLM pot.
</details>

**Q7. Which network should you use to practice, and why never mainnet?**
<details><summary>Show answer</summary>
Use <strong>testnet</strong> (or local <code>standalone</code>). Testnet XLM is
free via Friendbot and worthless, so mistakes cost nothing. Mainnet moves real
funds.
</details>

### Score yourself

| Correct | Where to go next |
|---------|------------------|
| 6–7 | You're ready to contribute — see the [contributing guidelines](../CONTRIBUTING.md) |
| 4–5 | Re-skim the "why it matters" callouts in the [tutorial](tutorial-step-by-step.md) |
| 0–3 | Re-run the hands-on steps — repetition on testnet is free and the fastest way to learn |

---

## 🔄 Feedback loop

This tutorial improves only when new users tell us where they got stuck.

If any step confused you:

1. Note **which step** and **the exact command** that didn't behave as described.
2. Check the **Troubleshooting common issues** table in the
   [step-by-step tutorial](tutorial-step-by-step.md) — your issue may already be covered.
3. If not, **open an issue or PR** describing the confusion. Documentation gaps
   are treated as bugs and are great first contributions.

> 🙌 New contributors who run this tutorial and report friction points help us
> keep it accurate and under 15 minutes. Thank you!
