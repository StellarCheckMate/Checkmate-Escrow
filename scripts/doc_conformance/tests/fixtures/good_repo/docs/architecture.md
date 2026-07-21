# Architecture (fixture)

## Match States

| State | Terminal |
|-------|----------|
| `Pending` | No |
| `Active` | No |
| `Completed` | Yes |
| `Cancelled` | Yes |

## Transition Table

| From | To | Entry Point |
|------|----|--------------|
| N/A | `Pending` | `create_match` |
| `Pending` | `Active` | `deposit` |
| `Active` | `Completed` | `submit_result` |
| `Pending` | `Cancelled` | `cancel_match` |

### `Match` Struct

| Field | Type | Description |
|-------|------|--------------|
| `id` | `u64` | Match id. |
| `player1` | `Address` | Player 1. |
| `player2` | `Address` | Player 2. |
| `stake_amount` | `i128` | Stake. |
| `state` | `MatchState` | State. |

> Internal fields — `player1_deposited` and `player2_deposited` are internal bookkeeping.

### Contract Functions

| Function | Signature | Description |
|----------|-----------|--------------|
| `create_match` | `(...)` | Creates a match. |
| `get_match` | `(...)` | Gets a match. |
| `set_match_timeout` | `(...)` | Sets timeout. |
