# Add albedo.ts unit tests

## Issue

Issue #839: The Albedo wallet integration has no unit tests.

## Acceptance Criteria

1. Create `frontend/src/test/albedo.test.ts`
2. Test `albedoIsAvailable` in browser-like environment
3. Test `albedoGetPublicKey` returns pubkey or throws

## Changes Made

### Created Files

- `frontend/src/test/albedo.test.ts`

### Detailed Change Description

Created a dedicated unit test file for the Albedo wallet adapter (`frontend/src/wallets/albedo.ts`).

**Mock Setup:**
- Mocked the `@albedo-link/intent` package using `vi.mock()` at the top level.
- Exposed the `publicKey` method as a mock function for configuring return values and rejections.

**Test 1: `test_albedo_is_available_browser`**
- Calls `albedoIsAvailable()` directly in the jsdom test environment.
- Asserts the function returns `true` when `window` is defined, simulating a browser-like environment.

**Test 2: `test_albedo_get_public_key_success`**
- Mocks `albedo.publicKey({})` to resolve with an object containing a fake public key (`GALBEDO...`).
- Calls `albedoGetPublicKey()` and asserts the returned string matches the mocked pubkey.
- Verifies the mock was called with `{}` as the argument.

**Test 3: `test_albedo_get_public_key_throws`**
- Mocks `albedo.publicKey({})` to reject with an error (`User rejected`).
- Calls `albedoGetPublicKey()` and asserts the promise rejects with the expected error message using `rejects.toThrow()`.

**Behavior Verified:**
- `albedoIsAvailable()` correctly detects browser environments.
- `albedoGetPublicKey()` successfully propagates the `pubkey` from the Albedo API response.
- `albedoGetPublicKey()` properly throws when the underlying Albedo call fails.

## Test Results

All 3 tests pass successfully:

- `test_albedo_is_available_browser`
- `test_albedo_get_public_key_success`
- `test_albedo_get_public_key_throws`

```json
{
  "Test Files": 1,
  "Tests": 3,
  "Passed": 3,
  "Failed": 0
}
```

## Related

- Pull Request: https://github.com/StellarCheckMate/Checkmate-Escrow/pull/923

closes #839
