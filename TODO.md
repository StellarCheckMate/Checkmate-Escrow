- [x] Create new off-chain crate directory `oracle-service/`
- [x] Add `oracle-service` to workspace `Cargo.toml`
- [x] Implement `oracle-service/src/oracle/chess_com_client.rs` (HTTP wrapper + 30 req/min policy + 30s timeouts)
- [x] Implement Chess.com game id validation (numeric string)
- [x] Implement result fetching + parsing (map Chess.com game outcomes to on-chain `Winner`)
- [x] Add error types: `oracle-service/src/oracle/errors.rs`
- [x] Implement unit tests with mock HTTP responses
- [x] Add integration tests (ignored by default) against a real known sandbox/test id via env var
- [x] Update `docs/oracle.md` with Chess.com rate limits + offline fallback guidance
- [x] Run `cargo test -p oracle-service`


