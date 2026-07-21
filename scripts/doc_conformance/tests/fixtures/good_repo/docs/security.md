# Security (fixture)

<!-- doc-conformance: verified path=contracts/escrow/src/lib.rs line=9 sha256=bbaa55f15d653c0614ce3d2f144a8394ca8a5b59215f0a4d279ed2e9d99e7a15 -->

Match timeout is configurable in the range 17,280 to 1,555,200 ledgers via `set_match_timeout`.

Multi-token matches are supported via `create_match_with_conversion`.
