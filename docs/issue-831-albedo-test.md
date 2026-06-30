# Add WalletConnector test: Albedo connection

## Issue

Issue #831: `WalletConnector.test.tsx` covers Freighter but not Albedo.

## Acceptance Criteria

- Add `test_wallet_connector_albedo_connect`
- Mock `albedoGetPublicKey` to resolve with a test address
- Assert public key is displayed after connection

## Changes Made

### Modified Files

- `frontend/src/test/WalletConnector.test.tsx`

### Detailed Change Description

The test file `frontend/src/test/WalletConnector.test.tsx` was updated to include coverage for the Albedo wallet connection flow.

**New Test Added:**
- `test_wallet_connector_albedo_connect`: This test verifies the full integration flow when a user clicks the "Connect Albedo" button.

**Mock Setup:**
- Added a top-level `vi.mock('../wallets/albedo', ...)` to mock the Albedo wallet adapter exports (`albedoIsAvailable` and `albedoGetPublicKey`).
- Imported the mocked module with `import * as albedo from '../wallets/albedo'` so the test can configure mock return values.

**Test Logic:**
1. Mocked `albedoIsAvailable` to return `true`, simulating a browser environment where the Albedo extension is accessible.
2. Mocked `albedoGetPublicKey` to resolve with a valid test Stellar public key: `GALBEDO1234567890ABCDE1234567890ABCDE1234567890ABCDE1234567890`.
3. Rendered the `WalletConnectorWithHook` component, which composes `useWallet` with `WalletConnector` to exercise the real connection flow.
4. Simulated a user click on the "Connect Albedo" button.
5. Used `waitFor` to assert that the truncated public key (`GALBED…7890`) appears in the DOM after the asynchronous connection succeeds.

**Behavior Verified:**
- The component transitions from the "Connect Albedo" button state to the "connected" state.
- The public key is truncated to the format `first 6 characters…last 4 characters` and displayed in a `span` element.
- The "Disconnect" button is rendered alongside the public key.

## Test Results

All 7 tests in `WalletConnector.test.tsx` pass:

- `renders connect buttons when disconnected`
- `calls connect with correct wallet type`
- `shows truncated key and disconnect when connected`
- `calls disconnect`
- `shows error message`
- `test_wallet_connector_freighter_not_available`
- `test_wallet_connector_albedo_connect`

## Verification

```bash
cd frontend && npx vitest run src/test/WalletConnector.test.tsx
```

## Related

- Pull Request: https://github.com/StellarCheckMate/Checkmate-Escrow/pull/922

closes #831
