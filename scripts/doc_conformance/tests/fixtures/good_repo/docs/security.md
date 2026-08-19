# Security (fixture)

<!-- doc-conformance: verified path=contracts/escrow/src/lib.rs line=9 sha256=1572038928a413dc2a9ebef591e136dfd9ed9fc3d75549c41a53451fd4f33c54 -->

Match timeout is configurable in the range 86,400 to 7,776,000 seconds via `set_match_timeout`.

Multi-token matches are supported via `create_match_with_conversion`.
