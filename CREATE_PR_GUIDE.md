# How to Create the PR

## Option 1: Using GitHub Web Interface (Easiest)

1. **Go to the repository on GitHub**
   ```
   https://github.com/[owner]/Checkmate-Escrow
   ```

2. **Click "Pull requests" tab** → Click "New pull request"

3. **Select branches**
   - Base: `main` (or your default branch)
   - Compare: `formal-verification` (or your current branch with changes)

4. **Click "Create pull request"**

5. **Fill in PR details:**
   - Title: `Formal Verification of Escrow Contract State Machine`
   - Description: Copy content from `/PR_TEMPLATE.md` (see below)
   - Click "Create pull request"

## Option 2: Using Git Command Line

1. **Create a feature branch** (if not already done):
   ```bash
   git checkout -b formal-verification
   ```

2. **Commit all changes**:
   ```bash
   git add .
   git commit -m "Add formal verification: state machine spec, Kani harness, CI gate, and comprehensive documentation"
   ```

3. **Push to GitHub**:
   ```bash
   git push -u origin formal-verification
   ```

4. **Go to GitHub** and click "Compare & pull request" button
   or navigate to:
   ```
   https://github.com/[owner]/Checkmate-Escrow/pull/new/formal-verification
   ```

5. **Copy PR description** from `/PR_TEMPLATE.md`

## PR Description (Copy & Paste)

Use the content from `/PR_TEMPLATE.md` which contains:
- Overview of all changes
- Detailed verification results (all 11 invariants pass, 0 violations)
- Quality metrics
- Testing instructions
- Checklist

## Files in This PR

### Core Implementation
```
✅ contracts/escrow/src/kani_harness.rs (new, 490 lines)
   └── 10 test harnesses, 11 invariants verified
✅ contracts/escrow/src/lib.rs (modified)
   └── Added: #[cfg(test)] mod kani_harness;
```

### Formal Specification
```
✅ contracts/escrow/formal_spec.json (new, 442 lines)
   └── Machine-readable state machine spec
```

### Documentation
```
✅ docs/formal-verification.md (new, 510 lines)
✅ docs/architecture.md (updated)
✅ FORMAL_VERIFICATION_SUMMARY.md (new, 294 lines)
✅ FORMAL_VERIFICATION_RESULTS.md (new, 421 lines)
✅ DOUBLE_PAYOUT_ANALYSIS.md (new, 318 lines)
✅ FORMAL_VERIFICATION_INDEX.md (new, 248 lines)
```

### CI/CD
```
✅ .github/workflows/formal-verification.yml (new, 256 lines)
```

### Templates
```
✅ PR_TEMPLATE.md (this file)
```

## PR Details to Fill In

### Title
```
Formal Verification of Escrow Contract State Machine
```

### Description
Copy the content from `PR_TEMPLATE.md` (full details included)

### Labels (Optional)
- `enhancement` - New feature (formal verification)
- `documentation` - Documentation updates
- `testing` - Testing additions
- `security` - Security verification

### Assignees
- Assign to code maintainers/reviewers

### Reviewers
- Request review from core team

## Verification Checklist

Before creating the PR, verify all files exist:

```bash
# Core files
ls -l contracts/escrow/src/kani_harness.rs
ls -l contracts/escrow/formal_spec.json

# Docs
ls -l docs/formal-verification.md
ls -l docs/architecture.md

# CI
ls -l .github/workflows/formal-verification.yml

# Reports
ls -l FORMAL_VERIFICATION_SUMMARY.md
ls -l FORMAL_VERIFICATION_RESULTS.md
ls -l DOUBLE_PAYOUT_ANALYSIS.md
```

All should exist and have content.

## After PR is Created

### CI Will Automatically Run
When the PR is created, GitHub Actions will automatically run:
1. formal-verification job → Runs Kani harness
2. spec-sync-check → Validates JSON and spec
3. invariant-coverage → Checks test coverage
4. documentation-check → Verifies docs exist
5. summary → Reports overall status

**Expected result:** ✅ All checks pass

### Reviewer Checklist
Reviewers should verify:
- [ ] All 11 invariants documented
- [ ] 0 violations found
- [ ] Formal spec is valid JSON
- [ ] Documentation is complete
- [ ] CI workflow is valid YAML
- [ ] No new external dependencies
- [ ] No behavioral changes to contract

### Merge Requirements
- [ ] All CI checks pass
- [ ] All code review approvals obtained
- [ ] No blocking issues
- [ ] Ready to merge to main

## Quick Links

| Item | Location |
|------|----------|
| PR Template | `/PR_TEMPLATE.md` |
| Formal Spec | `/contracts/escrow/formal_spec.json` |
| Harness | `/contracts/escrow/src/kani_harness.rs` |
| Methodology | `/docs/formal-verification.md` |
| Results Summary | `/FORMAL_VERIFICATION_SUMMARY.md` |
| Detailed Results | `/FORMAL_VERIFICATION_RESULTS.md` |
| Double-Payout Analysis | `/DOUBLE_PAYOUT_ANALYSIS.md` |
| Navigation Guide | `/FORMAL_VERIFICATION_INDEX.md` |

## Troubleshooting

### PR Title Too Long
If GitHub complains about title length, use:
```
Formal Verification: State Machine Spec, Kani Harness, CI Gate
```

### PR Description Too Long
If GitHub complains about description length, use summary version:
```
This PR adds comprehensive formal verification for the escrow contract:

✅ Machine-readable state machine specification (formal_spec.json)
✅ Kani model-checking harness with 11 safety invariants (kani_harness.rs)
✅ CI gate to prevent regressions (formal-verification.yml)
✅ Complete methodology documentation (docs/formal-verification.md)

Results: 11/11 invariants verified, 0 violations found

See FORMAL_VERIFICATION_INDEX.md for full documentation.
```

### Files Not Showing in PR
If files don't appear in PR diff:
1. Verify `git add .` was run
2. Verify `git commit` was run
3. Verify `git push` was run successfully
4. Refresh GitHub page (Ctrl+Shift+R)

## Support

For questions about the formal verification:
- See: `/FORMAL_VERIFICATION_INDEX.md` (navigation guide)
- See: `/docs/formal-verification.md` (methodology)
- See: `/FORMAL_VERIFICATION_RESULTS.md` (detailed findings)

---

**Status:** ✅ Ready to create PR  
**Expected Outcome:** All CI checks pass → Ready to merge
