# Double-Initialize Fix TODO

**Progress: Starting implementation**

## Steps:
- [ ] 1. Add `AlreadyInitialized = 7,` to `Error` enum in `src/errors.rs`
- [ ] 2. Edit `initialize` in `src/lib.rs` to check `env.storage().instance().has(&DataKey::Oracle)` and panic with `Error::AlreadyInitialized` if exists, else set Oracle and MatchCount
- [ ] 3. Add `test_double_initialize_fails()` in `src/tests.rs` that calls initialize twice and asserts second panics
- [ ] 4. Run `cargo test` in contracts/escrow to verify and update snapshots if needed
- [x] 5. Update TODO.md after each step

Current status: Files analyzed, plan approved.

