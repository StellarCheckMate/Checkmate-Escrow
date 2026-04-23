# TODO: Issue #127 - Add Test for get_match stake_amount and token

## Approved Plan Steps (complete iteratively):

✅ 1. User checkout to new branch `blackboxai/issue-127-get-match-stake-token-test`  
   Command: `git checkout -b blackboxai/issue-127-get-match-stake-token-test`

2. **Edit contracts/escrow/src/tests.rs** - Add new test `test_get_match_returns_stake_and_token`  
   - Use setup(), create_match(player1, player2, stake_amount: 200i128, token, &String::from_str(&env, "test_stake_token"), Platform::Lichess)  
   - let m = client.get_match(&id);  
   - assert_eq!(m.stake_amount, 200i128); assert_eq!(m.token, token); assert_eq!(id, 0u64);

3. Run tests: `cd contracts/escrow && cargo test`  
   - Review and approve generated snapshot `test_snapshots/tests/test_get_match_returns_stake_and_token.1.json`

4. Commit changes: `git add . && git commit -m "test(escrow): verify get_match returns correct stake_amount and token (#127)"`

5. Push: `git push origin blackboxai/issue-127-get-match-stake-token-test`

6. Create PR: `gh pr create --title "test(escrow): verify get_match returns correct stake_amount and token (#127)" --body "Closes #127\n\n- Adds test verifying get_match returns exact stake_amount and token from create_match inputs.\n- Uses distinct stake 200i128 to differentiate from existing tests."`

**Next: Awaiting user to checkout branch and confirm `git status` / branch name.**

