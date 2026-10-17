# Escrow Contract — Admin Event Reference

This document tracks the on-chain events emitted by admin-only functions on
the escrow contract, for audit-trail purposes.

## Implementation summary

Admin functions (`add_allowed_token`, `remove_allowed_token`,
`set_protocol_config`, `set_fee_tiers`, `add_token_to_blacklist`) emitted
events describing *what* changed but not *who* changed it. Full audit
trails require knowing which admin address called each function. Every
listed event's data payload now includes the caller's `Address` alongside
the existing fields (see table below). A regression test,
`test_admin_events_include_caller_address` in
`contracts/escrow/src/tests/events.rs`, asserts the caller address is
present in the `token_add` event payload.

## Admin event table

All admin functions now include the **caller's address** in their event
payload so that the full audit trail (who called what, and when) can be
reconstructed from on-chain events alone, without needing to correlate
against the enclosing transaction's source account.

| Function | Topics | Data payload | Notes |
|----------|--------|---------------|-------|
| `add_allowed_token` | `("admin", "token_add")` | `(token: Address, caller: Address)` | `caller` is the contract admin. |
| `remove_allowed_token` | `("admin", "tok_rm")` | `(token: Address, caller: Address)` | `caller` is the contract admin. |
| `set_protocol_config` | `("admin", "protocol_config_set")` | `caller: Address` | Emitted on every call. A second event, `("escrow", "stablecoin_mode")` with payload `(new_mode: bool, caller: Address)`, is also emitted when `stablecoin_only_mode` changes. |
| `set_fee_tiers` | `("admin", "fee_tiers_set")` | `(tier_count: u32, caller: Address)` | `caller` is the contract admin. |
| `add_token_to_blacklist` | `("admin", "tok_blacklist")` | `(token: Address, reason: String, caller: Address)` | `caller` is the contract admin. |

For all of the above, `caller` is always the contract's registered `Admin`
address, since every admin function calls `admin.require_auth()` before
emitting its event.

## Protocol configuration layout

`ProtocolConfig` is split into three sub-structs so that each concern can be
reasoned about and extended independently:

| Sub-struct | Fields |
|------------|--------|
| `FeeConfig` | `platform_fee_bps`, `fee_tiers` |
| `TimeoutConfig` | `min_timeout_ledgers`, `max_timeout_ledgers`, `default_timeout_ledgers` |
| `StakeConfig` | `min_stake`, `max_stake`, `stablecoin_only_mode` |

`ProtocolConfig` embeds these as `fee: FeeConfig`, `timeout: TimeoutConfig`
and `stake: StakeConfig`. For backward ABI compatibility,
`get_protocol_config` still returns the combined `ProtocolConfig` struct, so
existing clients reading the flat configuration are unaffected.

## Slashing grace period

See [`docs/oracle.md`](../../docs/oracle.md) and the oracle contract's
`slashing_grace_period_ledgers` config field for details on staged/pending
oracle slashes and governance cancellation via `admin_cancel_slash`.
