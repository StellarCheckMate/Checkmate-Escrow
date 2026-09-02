# Soroban RPC TLS Enforcement

## What changed

`SorobanClient::new` (in `oracle-service/src/soroban_client.rs`) previously
built its `reqwest::Client` with no TLS restrictions. If `STELLAR_RPC_URL`
were ever misconfigured to a plain `http://` endpoint, oracle result
submissions could be intercepted or modified in transit (MITM).

Two changes close this gap:

1. **Transport-level enforcement** — the `reqwest::Client` builder now calls
   `.https_only(true)`, so any request over plain HTTP fails at the
   transport layer regardless of configuration.
2. **Startup-time validation** — when the `ORACLE_ENV` environment variable
   is set to `production` (case-insensitive), `SorobanClient::new` returns
   `OracleServiceError::Config` immediately if `rpc_url` does not start
   with `https://`, instead of allowing the service to start and fail
   later (or silently downgrade) at request time.

## Why not always reject HTTP at construction?

Local/dev/test environments (e.g. `wiremock` mock servers) legitimately use
`http://127.0.0.1:<port>`. Gating the hard failure on `ORACLE_ENV=production`
keeps those workflows working while still failing closed in production.

## Test coverage

`oracle-service/src/soroban_client.rs` (`#[cfg(test)] mod tests`) adds:

- `http_rpc_url_rejected_in_production_mode` — asserts construction fails
  when `ORACLE_ENV=production` and `rpc_url` is `http://...`.
- `https_rpc_url_accepted_in_production_mode` — asserts an `https://` URL is
  not rejected for TLS reasons under the same mode.
