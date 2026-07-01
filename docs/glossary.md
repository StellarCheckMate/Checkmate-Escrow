# Glossary

A single reference for the terms used across Checkmate-Escrow's contracts, services, and documentation. New contributors can start here, then follow the links into the deeper guides.

Terms are listed alphabetically. Where a term maps directly to something in the code, the relevant contract function, struct field, or error is named so you can find it quickly.

## Admin

The privileged contract role set at deployment (emitted in the `escrow` / `init` event alongside the oracle address). The admin manages operational controls rather than match outcomes: pausing and unpausing the contract, rotating the oracle, transferring the admin role, configuring the match timeout, and managing the token [allowlist](#allowlist) via `add_allowed_token`. The admin cannot decide who wins a match — only the [oracle](#oracle) can submit results. See [runbook-rotation.md](runbook-rotation.md) and [runbook-pause.md](runbook-pause.md).

## Allowlist

The admin-managed set of tokens accepted for new matches. By default, before any token has been added, **any** Stellar token address is accepted. Once the admin adds at least one token with `add_allowed_token`, only allowlisted tokens may be used to create matches; attempting to use any other token fails with `TokenNotAllowed`. The allowlist is enforced in `create_match`.

## Cancelled

A terminal [match](#match) state. A match reaches `Cancelled` when either player calls `cancel_match` while it is still [Pending](#pending), or when anyone calls `expire_match` after the timeout has elapsed. Any stake already deposited is refunded, and `completed_ledger` is recorded. See the [match lifecycle](match-lifecycle.md).

## Completed

A terminal [match](#match) state reached when the [oracle](#oracle) submits a result for an [Active](#active) match via `submit_result` (or `submit_result_with_oracle_record`). The [payout](#payout) executes inline as part of the same transaction, escrow holds zero funds afterward, and `completed_ledger` is recorded.

## Active

A [match](#match) state. A match becomes `Active` once both players have completed their [deposits](#deposit) while it was [Pending](#pending). The escrow now holds `2 × stake` (the [pot](#pot)), the game is in progress, and only the [oracle](#oracle) can advance the match to [Completed](#completed).

## Deposit

The act of a player transferring their `stake_amount` into the escrow contract (`deposit`). A match starts [Pending](#pending) with no deposits; each player deposits once. When both deposits are present, the match transitions to [Active](#active). Deposits are refunded if the match is later [Cancelled](#cancelled).

## Draw

A match outcome (the `Draw` variant of the `Winner` enum) where neither player wins. Instead of paying the whole [pot](#pot) to one player, each player's [stake](#stake) is returned to them, and the match moves to [Completed](#completed).

## Epoch

A general blockchain term for a defined period or checkpoint used to group activity or coordinate state changes. Stellar itself does not expose a Checkmate-specific "epoch"; the network measures the passage of time in [ledgers](#ledger), and Checkmate-Escrow's time-based rules (such as match expiry and storage TTL) are expressed in ledger sequence numbers rather than epochs.

## Escrow

The Soroban smart contract (and the funds it custodies) that holds both players' [stakes](#stake) from [deposit](#deposit) until the match reaches a terminal state. The escrow enforces the rules for creating matches, depositing, cancelling, expiring, and paying out, so that no party can withhold or redirect funds. This is the core contract of the project (`contracts/escrow`).

## Freighter

A browser-extension wallet for Stellar. Users connect Freighter to the Checkmate-Escrow frontend to sign transactions such as creating a match or depositing a stake. See [wallet-integration.md](wallet-integration.md); the integration lives in `frontend/src/wallets/freighter`.

## game_id

The identifier that links an on-chain [match](#match) to a specific chess game on Lichess or Chess.com. It is supplied to `create_match` and must be non-empty and within the maximum allowed length (`InvalidGameId` otherwise) and must not already be registered for another match (`DuplicateGameId`). The [oracle](#oracle) uses the `game_id` to fetch and verify the real-world result.

## Ledger

Stellar's unit of finalized state — a batch of transactions closed by the network, roughly every few seconds, identified by an increasing sequence number. Checkmate-Escrow records ledger sequence numbers to mark when matches are created, completed, or cancelled (`completed_ledger`), and to enforce time-based rules such as expiry and storage TTL (time-to-live, also measured in ledgers).

## Match

A single wagered chess game between two players, represented by the `Match` struct. It carries the participants, `stake_amount`, token address, [game_id](#game_id), lifecycle [state](#match-lifecycle-states), deposit flags, and outcome. Matches move through the [lifecycle states](#match-lifecycle-states) below.

## Match lifecycle states

The states a [match](#match) moves through, modeled by the `MatchState` enum: [Pending](#pending) → [Active](#active) → [Completed](#completed), with [Cancelled](#cancelled) as the alternative terminal state reached from Pending. Completed and Cancelled are terminal. The full state machine, including every guard and error path, is documented in [match-lifecycle.md](match-lifecycle.md).

## Oracle

The authorized off-chain service together with its on-chain account that bridges external chess-platform data to the contract. The oracle reads the result of the game identified by [game_id](#game_id) from Lichess/Chess.com and submits the verified outcome with `submit_result`. Only the oracle's authorization is accepted for result submission, which is what makes payouts automatic without a human middleman. The oracle address is set at initialization and can be rotated by the [admin](#admin). See [oracle.md](oracle.md) and `contracts/oracle`.

## Payout

The settlement transfer that happens when a match completes: the full [pot](#pot) (`2 × stake`) is sent to the winning player, or — in a [draw](#draw) — each player's stake is returned. Payout executes inline within the result-submission transaction, so settlement is immediate.

## Pending

The initial [match](#match) state after `create_match`, while the contract awaits [deposits](#deposit). Zero, one, or both deposits may be present; the match stays Pending until both arrive (then [Active](#active)) or it is [Cancelled](#cancelled) via `cancel_match` or `expire_match`.

## Pot

The total amount held in escrow for an [Active](#active) match: `2 × stake`, i.e. both players' [stakes](#stake) combined. On a decisive result the entire pot is paid to the winner; on a [draw](#draw) it is split back to each player as their original stake.

## Soroban

Stellar's smart contract platform. Contracts are written in Rust, compiled to WebAssembly (WASM), and deployed to the Stellar network. Checkmate-Escrow's escrow and oracle contracts are Soroban contracts (see `contracts/`).

## Stake

The token amount each player commits to a [match](#match), recorded as `stake_amount` and required to be greater than zero (`InvalidAmount` otherwise). Both players stake the same amount; together they form the [pot](#pot) that is paid out on completion.

## Wave-ready

A label applied to GitHub issues that have been vetted and prepared for contribution under the project's Drips Wave program. Wave-ready issues carry a point value by complexity and can be claimed by commenting `/wave claim`. See the [Drips Wave Contributor Guide](wave-guide.md).

## XLM

The native asset of the Stellar network, also called the lumen. XLM pays Stellar transaction and Soroban resource fees. Match [stakes](#stake) are not restricted to XLM — any allowlisted Stellar token can be used (subject to the [allowlist](#allowlist)) — but XLM is always needed to cover network fees.

---

## Related documentation

- [Architecture Overview](architecture.md) — components, public API, and data model
- [Match Lifecycle](match-lifecycle.md) — the complete state machine
- [Oracle Design](oracle.md) — how results are verified and submitted
- [Drips Wave Contributor Guide](wave-guide.md) — claiming wave-ready issues
- [Error Codes Reference](error-codes.md) — every contract error and how to recover
- [FAQ](faq.md) — common questions
