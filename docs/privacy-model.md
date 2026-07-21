# Balance-Privacy Model

**Last updated:** 2026-07-21 · **Verified against commit:** `85234ce`

This document describes what the escrow contract's balance-snapshot APIs
hide from non-admin callers, how, and — just as importantly — what they do
**not** hide. It covers:

- [`EscrowContract::get_balance_snapshots`](../contracts/escrow/src/lib.rs) / [`get_latest_snapshot`](../contracts/escrow/src/lib.rs) — per-match balance history
- [`EscrowContract::get_balance_at_timestamp`](../contracts/escrow/src/lib.rs) — per-player aggregate balance history

Background: [issue #1065](https://github.com/StellarCheckMate/Checkmate-Escrow/issues/1065).

---

## Threat model

Three caller classes exist for the per-match snapshot getters
(`authorize_snapshot_query` in `lib.rs`):

- **Admin** — full access, sees exact `stake_amount`/`escrow_balance`.
- **Match participant** (`player1`/`player2`) — partial access: a
  *redacted* `BalanceSnapshot`.
- **Anyone else** — rejected with `Error::Unauthorized`.

"Non-admin" below means a match participant calling one of these two
functions — the only callers who ever see a redacted snapshot, since
everyone else is rejected outright. The goal of redaction is that a
participant's view of a match's balance history, plus whatever else is
independently public (the token contract's own balance/transfer history,
which Soroban does not let this contract hide), should not let them
reconstruct the other side's exact amounts with certainty.

`get_balance_at_timestamp` is unauthenticated and public by design (see its
doc comment) — it only ever exposes a *player's aggregate* balance, never a
per-match stake amount, so it sits outside the redaction threat model above.
Its problem was a different one: the same return value meant two different
things (see "Pruned vs. zero" below).

## What changed

### 1. Commitment instead of bare zero

Previously, non-admins saw `stake_amount: 0` and `escrow_balance: 0` — a
placeholder indistinguishable from a real zero balance, with nothing to
verify it against later. `BalanceSnapshot` now also carries:

```rust
pub nonce: BytesN<32>,      // random salt, admin-only
pub commitment: BytesN<32>, // sha256(stake_amount || escrow_balance || nonce)
```

`commitment` is computed once, at `record_snapshot` time, and stored in both
the full and redacted views. `stake_amount`/`escrow_balance` are still
zeroed for non-admins — this is not a reveal-later scheme where the values
are recoverable by the participant themselves — but the participant now
holds something they can check a *future, intentional* admin disclosure
against (e.g. during a dispute, the admin reveals a specific snapshot's true
amounts and `nonce`; anyone can recompute the hash and confirm it matches
the commitment that was on-chain the whole time). `nonce` itself is zeroed
in the redacted view, since revealing it ahead of that disclosure would let
a non-admin brute-force `commitment` against a guessed amount — see
"Non-guarantees" below on why this only bounds, not eliminates, that risk.

This is a hash-commitment, not a Pedersen commitment. It was chosen over an
elliptic-curve scheme because `soroban_sdk::Crypto` exposes `sha256` (and
`keccak256`) directly, while Pedersen commitments need a curve/generator
pair the SDK doesn't provide out of the box — adding one would mean vendoring
or hand-rolling curve arithmetic in a `no_std` contract, a much larger
surface for a marginal privacy gain over a salted hash here.

### 2. Deposit-timing fields are now also redacted

The original redaction left `player1_deposited`/`player2_deposited`
(exact deposit-timing signal), `token`, `token_symbol`, and `ledger` fully
visible even to non-admins. Combined with the token contract's own public
balance history, those were enough to narrow down — sometimes pin down
exactly — the amounts the zeroed fields were meant to hide: know *which*
token, *when* each side moved funds, and *when* the match transitioned, and
you can often read the amount straight off the token contract's transfer
history.

`redact_snapshot` now also zeroes `player1_deposited`/`player2_deposited`.
`token`/`token_symbol` remain visible — a participant already knows which
token they deposited, and without the timing signal, knowing the token
contract alone isn't enough to isolate a specific transfer in its history.

### 3. The snapshot event no longer leaks the raw balance

`record_snapshot` used to publish `(match_id, index, escrow_balance)` as a
contract event on every snapshot. **Soroban events are public regardless of
which function or caller triggered them** — so the exact escrow balance was
broadcast to every observer on every snapshot, independent of anything
`get_balance_snapshots`/`get_latest_snapshot` redacted. This made the
getter-level redaction close to theater: nobody needed to call the redacted
getter when the real number was already sitting in the event stream.

The event now publishes `commitment` in place of `escrow_balance`.

### 4. Pruned vs. zero is now distinguishable

`get_balance_at_timestamp` returned a bare `i128`, with `0` meaning either
"this player genuinely had no escrow position at that point" or "the
32-entry ring buffer (`MAX_PLAYER_SNAPSHOTS`) has overwritten the snapshot
that would have answered this" — indistinguishable outcomes, which is a real
problem if this data is ever used as dispute or compliance evidence: you
cannot tell "provably zero" from "unknown."

It now returns:

```rust
pub enum BalanceAtTimestamp {
    Known(i128), // a retained snapshot answered the query
    NoHistory,   // no pruning occurred; genuinely no snapshot at/before this ledger
    Pruned,      // the ring buffer has overwritten every snapshot old enough to answer
}
```

The distinction is derived from data the function already computes — no new
storage. `start` (the oldest index still guaranteed live in the ring buffer)
is `0` exactly when nothing has ever been pruned for that player; if the
walk from newest-to-oldest reaches `start` without finding a qualifying
snapshot, the answer is `NoHistory` when `start == 0` and `Pruned` when
`start > 0`.

### 5. `MAX_PLAYER_SNAPSHOTS` — decision: leave it at 32, no archival path added

Raising the cap (or adding an off-chain archival path) was explicitly
"consider and justify either way," not a required change, so no code change
was made here. Reasoning:

- Every additional slot is a permanent-until-TTL persistent storage entry,
  paid for in rent on every write, for every player, forever. Raising the
  cap trades a fixed, bounded cost today for an unbounded one as player
  count and match volume grow — the ring buffer exists specifically to cap
  that growth.
- `Pruned` (this change) already turns "silently wrong" into "loudly
  unknown," which is the actual correctness bug the issue is about. It does
  not, by itself, need a bigger buffer or an archive to be correct.
- An off-chain archival path (an indexer subscribing to the `player`/
  `snapshot` and `match`/`snapshot` events and persisting them) is the right
  place for long-tail audit history if a real need for it shows up — it's
  strictly cheaper than growing on-chain storage and doesn't require a
  contract migration to adjust retention later. This repo already has
  [`docs/EVENT_INDEXER_API.md`](./EVENT_INDEXER_API.md) describing exactly
  this kind of event-driven indexer for other data; balance-snapshot
  archival would follow the same shape.
- No user-visible need for deeper on-chain history has been identified yet.
  If one is (e.g. a compliance requirement for N-year retention), that's a
  product decision that should drive the specific retention window rather
  than a speculative increase now.

## Non-guarantees

This section exists so nobody mistakes the above for stronger guarantees
than it actually provides.

- **Match participants already know their own stake.** Redaction protects
  against reconstructing *the other side's* position or a third party's
  correlation attempt — not against a player inferring the amount they
  themselves deposited, which they trivially already know.
- **The commitment is only as hiding as its nonce stays secret.** `nonce` is
  32 bytes from `env.prng()` (Soroban's on-chain PRNG — sufficient to make
  brute-forcing `commitment` against every plausible `(stake_amount,
  escrow_balance)` pair impractical, but this is a deterministic-host PRNG,
  not a hardware TRNG; treat it as computationally, not
  information-theoretically, hiding). Once an admin reveals `nonce` and the
  true amounts for one snapshot — e.g. during dispute resolution — that
  disclosure is final for that snapshot. Nothing about this scheme supports
  revoking or re-hiding a disclosed value.
- **The base token contract's own transparency is unaffected.** This fix
  changes what the escrow contract's getters and events expose. It does
  nothing to the token contract's own `balance()` reads or transfer events,
  which are public by the nature of the underlying asset contract. An
  observer willing to do deep forensic analysis across the token contract's
  full history may still be able to narrow down amounts for a specific
  match, especially for low-traffic tokens with few concurrent matches —
  redaction here raises the cost of that analysis, it does not make it
  impossible.
- **`get_balance_at_timestamp` still exposes aggregate balance.** It was
  already public/unauthenticated by design (no per-match stake exposed) and
  that hasn't changed — only the pruned-vs-zero ambiguity was fixed.
- **This does not add access control anywhere.** `get_balance_snapshots`/
  `get_latest_snapshot` still reject non-participants outright with
  `Error::Unauthorized`; nothing here changes who is allowed to call what,
  only what a caller who is already allowed to call it can see.
