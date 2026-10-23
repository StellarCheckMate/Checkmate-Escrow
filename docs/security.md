# Threat Model & Security

This document outlines the security considerations, trust assumptions, and risk mitigations for the Checkmate-Escrow smart contracts on Stellar Soroban.

## Table of Contents

- [Oracle Trust Assumptions](#oracle-trust-assumptions)
- [Oracle Compromise](#oracle-compromise)
- [Admin Key Risks](#admin-key-risks)
- [Contract Upgrade Multi-Sig](#contract-upgrade-multi-sig)
- [Re-initialization Protection](#re-initialization-protection)
- [Pause Mechanism](#pause-mechanism)
- [Cross-Contract Reentrancy Protection](#cross-contract-reentrancy-protection)
- [Oracle Service HTTP Hardening](#oracle-service-http-hardening)
- [Frontend Security](#frontend-security)
- [Known Limitations](#known-limitations)
- [Deployment Security](#deployment-security)
- [Operational Security](#operational-security)

## Oracle Trust Assumptions

The oracle system is a critical component that bridges off-chain chess game results to on-chain payouts. The following trust assumptions and mitigations apply:


### Trust Model

**Trusted Oracle**: The oracle address configured during contract initialization is trusted to:
- Submit accurate game results from Lichess/Chess.com APIs
- Only submit results for valid, completed matches
- Not submit fraudulent or manipulated results

### Mitigations

1. **Platform API Verification**: The oracle must verify results against official Lichess/Chess.com APIs before submission
2. **Game ID Validation**: Each `game_id` can only be used once (enforced on-chain via `DuplicateGameId` error)
3. **Match State Validation**: Results can only be submitted for matches that exist and are in the correct state
4. **Admin Oversight**: Oracle admin can pause the oracle contract if suspicious activity is detected
5. **Result Immutability**: Once submitted, results cannot be changed or overwritten

### Attack Vectors & Protections

| Attack Vector | Protection |
|---------------|------------|
| Oracle submits wrong winner | Platform API verification, game ID uniqueness |
| Oracle submits for non-existent match | On-chain validation (`MatchNotFound` error) |
| Oracle submits before deposits complete | State validation (`NotFunded` error) |
| Oracle key compromise | Admin can pause contract, rotate oracle address |
| Front-running result submission | Atomic result submission with proper sequencing |

## Oracle Compromise

### Attack Vector
If an attacker compromises the private key of the authorized oracle, they gain the ability to call the `submit_result` function as the oracle. The attacker can bypass the off-chain platform API verification checks entirely and submit fraudulent or manipulated match outcomes.

### Impact
By submitting malicious results, the attacker can force the escrow contract to release locked funds to an arbitrary address, causing a fraudulent payout. This allows them to drain active escrows for matches that are currently funded but not yet settled.

### Current Mitigations
1. **Contract Pausing**: The escrow admin can immediately call `pause` to halt match resolution and prevent the malicious oracle from submitting further results.
2. **Oracle Rotation**: The escrow admin can rotate the compromised oracle key to a secure new address by calling the `update_oracle` admin function in the escrow contract.
3. **Audit Trail / Event Emission**: The `update_oracle` function emits an `oracle_up` event (with the topic `admin`) containing the `old_oracle` and `new_oracle` addresses as data. This provides a clear, verifiable on-chain history of oracle rotation for indexing and alerting services.

### Recommended Hardening
To prevent and mitigate oracle compromise in production environments, the following hardening measures are recommended:
1. **Hardware Security Modules (HSM)**: Store the oracle private key in an HSM (e.g., AWS KMS or similar secure enclave) to prevent key extraction.
2. **Multi-signature Oracle**: Transition from a single-key oracle to a multi-signature oracle or a consensus of multiple independent oracles, requiring multiple signatures before a result is considered valid on-chain.

## Admin Key Risks

Both contracts have admin addresses with elevated privileges. Compromise of admin keys represents a significant risk.

### Escrow Contract Admin Powers

- **Pause/Unpause**: Can halt all contract operations (`create_match`, `deposit`, `submit_result`)
- **Admin Rotation**: Can change the admin address to a new one
- **Emergency Control**: Can intervene in case of detected issues

### Oracle Contract Admin Powers

- **Pause/Unpause**: Can halt result submissions (and block `delete_result` while paused)
- **Admin Rotation**: Can change the oracle admin address
- **Result Submission**: Can submit results directly (bypassing automated oracle)
- **Result Deletion**: Can remove a stored result via `delete_result`. This is an irreversible on-chain action — see [Result Deletion Policy](oracle.md#result-deletion-policy-delete_result) for risks and expected use.

### Risk Mitigations

1. **Multi-sig Recommendation**: Use multi-signature wallets for admin addresses in production
2. **Key Separation**: Use different admin addresses for escrow and oracle contracts
3. **Cold Storage**: Store admin keys offline when not in use
4. **Monitoring**: Monitor admin operations via contract events
5. **Timelock**: Consider implementing timelocks for critical admin operations
6. **Upgrade Multi-sig**: Contract upgrades require a second authorized address to call `confirm_upgrade` before `execute_upgrade` can proceed. A single compromised admin key cannot unilaterally deploy arbitrary code — see [Contract Upgrade Multi-Sig](#contract-upgrade-multi-sig) below.

### Compromise Scenarios

| Scenario | Impact | Mitigation |
|----------|--------|------------|
| Escrow admin key stolen | Contract can be paused, funds locked | Immediate pause, admin rotation |
| Oracle admin key stolen | Incorrect results can be submitted | Pause oracle, rotate admin, dispute results |
| Both keys compromised | Complete contract takeover | Emergency pause, contract migration |

## Contract Upgrade Multi-Sig

`execute_upgrade` enforces a two-step confirmation requirement. After
`schedule_upgrade` stores a pending WASM hash, a **second authorized address**
must call `confirm_upgrade` before `execute_upgrade` can proceed. If no
confirmation is on record, `execute_upgrade` returns
`Error::UpgradeNotConfirmed` (#58).

### Why this matters

Without the confirmation requirement, a single compromised admin key could:

1. Upload a malicious WASM blob.
2. Schedule an upgrade.
3. Wait for the 7-day review period.
4. Execute the upgrade, replacing the contract with arbitrary code.

With the confirmation requirement, the attacker also needs to compromise the
separate `upgrader` key before execution is possible.

### Threat model

| Scenario | Impact without multi-sig | Impact with multi-sig |
|----------|-------------------------|-----------------------|
| Admin key compromised | Attacker can deploy arbitrary code after review period | Attacker cannot execute — upgrader confirmation missing |
| Upgrader key compromised | N/A | Attacker can confirm but not schedule — both keys needed |
| Both keys compromised | Complete contract takeover | Same — use hardware wallets for both keys |

### Recommendations

- Store the `upgrader` key on a separate hardware wallet or HSM from the admin key.
- The `upgrader` address does not require admin privileges — it only needs to sign `confirm_upgrade`.
- Publicly announce and log the upgrader confirmation so the community can verify the multi-sig step was completed by the expected party.
- See [docs/upgrade-safety.md](upgrade-safety.md) for the full upgrade procedure.

## Re-initialization Protection

Both contracts implement strict re-initialization protection to prevent takeover after deployment.

### Protection Mechanism

```rust
// In initialize() function
if env.storage().instance().has(&DataKey::Oracle) {
    panic!("Contract already initialized");
}
```

- **Single Initialization**: `initialize()` can only be called once per contract
- **Deployer Authorization**: Only the account that deployed the contract can initialize it
- **State Persistence**: Initialization state is stored permanently in contract storage

### Security Benefits

1. **Front-run Prevention**: Eliminates the window where observers could initialize with malicious parameters
2. **Immutable Configuration**: Oracle and admin addresses cannot be changed via re-initialization
3. **Deployment Atomicity**: Deploy + initialize should be done in the same transaction

### Historical Context

Prior to fix [#216], contracts had no deployer guard, allowing front-running attacks where malicious actors could initialize contracts with their own admin/oracle addresses immediately after deployment.

## Panic vs Error Behavior

The current contract surface uses a deliberate mix of explicit panics and typed errors:

- `initialize()` in both `EscrowContract` and `OracleContract` contains an explicit `panic!("Contract already initialized")` path when initialization is attempted a second time.
- All other public contract operations return `Result<_, Error>` and map failures to typed contract errors such as `Unauthorized`, `MatchNotFound`, `ContractPaused`, `InvalidGameId`, and `AlreadySubmitted`.
- Authorization failures from `require_auth()` are surfaced as contract errors through the Soroban runtime, not as Rust panics.
- `get_result()` returns `Error::ResultNotFound` for missing entries rather than panicking.

For client implementations, the preferred strategy is to use generated `try_` wrappers where available. These wrappers convert contract error outcomes into host-language `Result` values and make it possible to handle failure paths explicitly.

### Migration Guidance

Clients and test code should prefer `try_` variants for contract invocation whenever possible:

- Use `try_initialize` instead of `initialize` to safely detect duplicate initialization without triggering a hard contract panic.
- Use `try_submit_result`, `try_create_match`, `try_deposit`, and similar wrappers so that authorization and state validation failures can be inspected programmatically.
- When `try_` wrappers are unavailable, ensure the host-side caller explicitly handles the contract error code produced by failed invocations.

This document should be revisited after issues #1 and #2 land to capture any contract or API surface changes that affect panic/error semantics and migration steps.

## Pause Mechanism

Both contracts implement emergency pause functionality for rapid response to security incidents.

### Escrow Contract Pause

**Blocked Operations When Paused:**
- `create_match()` - New matches cannot be created
- `deposit()` - Players cannot fund existing matches
- `submit_result()` - Oracle cannot submit results

**Allowed Operations When Paused:**
- `get_match()` - Read-only operations continue
- `cancel_match()` - Players can still cancel pending matches
- `expire_match()` - Timeout-based expiration still works

### Oracle Contract Pause

**Blocked Operations When Paused:**
- `submit_result()` - Result submissions are blocked

**Allowed Operations When Paused:**
- `has_result()` - Read-only result checking continues
- `get_result()` - Result retrieval continues

### Security Properties

1. **Admin-Only Control**: Only admin can pause/unpause
2. **Event Emission**: Pause/unpause operations emit events for monitoring
3. **State Persistence**: Pause state survives contract upgrades
4. **Granular Control**: Different pause behaviors for different operations

## Freeze Mechanism

Freezing is the targeted alternative to a contract-wide pause: `admin_freeze_player`
blocks **one** player address from participating without affecting any other user.

**Blocked Operations for a Frozen Player:**
- `create_match()` (and `create_match_with_conversion` / `create_match_with_referrer`) - the frozen player can neither create a match nor be named as a player in a new one
- `deposit()` - the frozen player cannot fund matches

**Allowed Operations for a Frozen Player (fund safety is never blocked):**
- `cancel_match()` / `expire_match()` - the frozen player can still recover their own stake from pending matches
- `claim_vested_payout()` - the frozen player can still claim payouts/refunds from already-funded matches
- `heartbeat_match()` / `dispute_and_rollback_match()` - existing-match recovery paths remain open
- Oracle settlement of already-`Active` matches proceeds normally

### Security Properties

1. **Admin-Only Control**: Only admin can freeze/unfreeze
2. **Targeted Scope**: Other users and existing matches are completely unaffected (unlike `pause`)
3. **Auditability**: The freeze `reason` is stored on-chain; `freeze`/`unfreeze` events are emitted for monitoring
4. **No Fund Seizure**: A freeze never blocks a player from withdrawing or claiming their own funds
5. **Error Code**: Rejections surface as `Error::ContractPaused` (code 9) — the error enum is at its 50-variant XDR cap, so this reuses the closest existing code (mirroring how the token blacklist reuses `TokenNotAllowed`)

### Emergency Response Protocol

1. **Detection**: Monitor for suspicious activity (unusual transaction patterns, oracle delays)
2. **Immediate Pause**: Admin pauses affected contract(s)
3. **Investigation**: Analyze the incident while contract is paused
4. **Resolution**: Either unpause after false alarm or migrate to new contract
5. **Post-mortem**: Update security measures based on lessons learned

## Cross-Contract Reentrancy Protection

### Threat Description

`deposit()` makes a cross-contract call to `token::Client::transfer()` before
all internal state updates are committed.  If the token contract is malicious
or implements transfer hooks (analogous to ERC-777 callbacks on EVM), it could
call back into `deposit()` for the same `match_id` while the escrow contract's
state still reflects the pre-transfer values.  An attacker could exploit this
to:

- Deposit once but have funds counted multiple times.
- Advance a match from `Pending` → `Active` without a legitimate second deposit.
- Drain escrow funds across repeated re-entrant calls before the deposit flag
  is set.

Stellar Soroban's current execution model does not permit true mid-instruction
re-entrancy the way EVM does, but defence-in-depth requires the contract to be
correct regardless of future VM changes or the introduction of hook-capable
token standards.

### Protection Mechanism

A per-`match_id` reentrancy guard is implemented using
`DataKey::DepositInProgress(match_id)` stored in **temporary storage**:

```rust
// Before the cross-contract token transfer:
if env.storage().temporary()
       .get::<DataKey, bool>(&DataKey::DepositInProgress(match_id))
       .unwrap_or(false)
{
    return Err(Error::DepositInProgress);
}
env.storage().temporary()
   .set(&DataKey::DepositInProgress(match_id), &true);

// ... token.transfer() cross-contract call ...
// ... all state updates (deposit flags, match state, snapshots) ...

// After all state updates are complete:
env.storage().temporary()
   .remove(&DataKey::DepositInProgress(match_id));
```

Key properties:

| Property | Detail |
|----------|--------|
| **Scope** | Per `match_id` — guards are independent for different matches |
| **Storage type** | Temporary — automatically cleared at transaction end even if an unexpected execution path skips the explicit `remove` |
| **Error code** | `Error::DepositInProgress` (code 46) |
| **Guard set** | Before `token.transfer()`, after all read-only validation passes |
| **Guard cleared** | After all persistent state writes and snapshot recording complete |
| **Error path cleanup** | Every early-return error path explicitly removes the guard before returning |

### Attack Vectors & Protections

| Attack Vector | Protection |
|---------------|------------|
| Malicious token callback re-enters `deposit()` for same `match_id` | `DepositInProgress` guard rejects with `Error::DepositInProgress` |
| Callback targets a different `match_id` | Normal validation (state, player auth, already-funded checks) rejects it independently |
| Guard not removed on error path | Temporary storage TTL ensures automatic cleanup; explicit removal on all error paths provides defence-in-depth |
| Flash-loan-funded multi-deposit race | Guard is set before the first token transfer, so the second call for the same player is also blocked |

### Test Coverage

`test_deposit_rejects_cross_contract_reentrancy_via_token_callback` in
`contracts/escrow/src/tests/security.rs` verifies:

1. When the `DepositInProgress` flag is set (simulating mid-transfer callback
   state), a concurrent `deposit()` call for the same `match_id` is rejected
   with `Error::DepositInProgress`.
2. After the guard is cleared, `deposit()` proceeds normally.

## Oracle Service HTTP Hardening

The oracle service's exposed HTTP endpoints (`/health`, `/api/docs`,
`/api/openapi.yaml`) are protected by two `axum` middleware layers, attached
in `oracle-service/src/main.rs` and implemented in
`oracle-service/src/middleware/`:

1. **WAF** — blocks request bursts over 20 req/s from one IP (`429`),
   oversized bodies over 1 MiB (`413`), URIs over 2,048 characters (`400`),
   and null bytes in the URI (`400`).
2. **Rate limiter** — token-bucket limiting per IP (100 req/min) and per
   `X-Api-Key` (1,000 req/min), returning `429` with standard
   `X-RateLimit-*`/`Retry-After` headers.

Both layers run on every request before it reaches a handler; see
[docs/oracle.md § HTTP Layer Protections](oracle.md#http-layer-protections-waf--rate-limiting)
for the layering order and full detail.

## Frontend Security

### Transaction Simulation Before Signing

Before users sign transactions, the frontend should simulate them to verify their correctness and prevent unexpected behavior. This protects against malicious dApps or XSS attacks that attempt to inject unauthorized operations into transactions.

#### Simulation Process

1. **Build Transaction**: Construct the transaction XDR locally with all intended operations
2. **Validate Operations**: Ensure operation types, parameters, and amounts match user intent
3. **Simulate on RPC**: Submit the transaction to an RPC node with `skipSigning: true` (or equivalent) to dry-run the transaction without broadcasting
4. **Check Result**: Verify:
   - Simulation succeeds (no `InvokeHostFunction` error)
   - Return value type matches expectations (e.g., `Ok` variant for a contract call)
   - Gas estimate is within reasonable bounds
5. **Show Summary to User**: Display the decoded transaction details (operations, amounts, contract function, arguments) before asking the user to sign
6. **Sign After Confirmation**: Only after user approves the decoded summary should the wallet be asked to sign

#### Implementation Example

```typescript
import { SorobanRpc } from '@stellar/js-sdk';

const rpc = new SorobanRpc.Server(horizonUrl);

// Build transaction
const txBuilder = new TransactionBuilder(...)
  .addOperation(Contract.Escrow.deposit({ match_id: 42, ... }))
  .setNetworkPassphrase(networkPassphrase)
  .setTimeout(300)
  .build();

const txXdr = txBuilder.toXDR();

// Simulate before signing
try {
  const simulation = await rpc.simulateTransaction(txBuilder);
  if (SorobanRpc.Api.isSimulationSuccess(simulation)) {
    // Transaction would succeed; safe to sign
    console.log('Simulation succeeded. Gas estimate:', simulation.latestLedger);
  } else {
    // Simulation failed; do not proceed
    console.error('Simulation failed:', simulation);
    throw new Error('Transaction would fail');
  }
} catch (err) {
  console.error('Simulation error:', err);
  throw err;
}

// Show user the decoded transaction summary before signing
displayTransactionSummary(txBuilder.operations);

// Only after user confirms, ask wallet to sign
const signedXdr = await wallet.sign(txXdr);
```

#### Protection Against Attacks

| Attack | Mitigation |
|--------|-----------|
| Malicious dApp injects extra operations | Simulation fails or shows unexpected operations in summary |
| Wrong contract address | Simulation fails or decoding shows mismatch |
| Wrong function arguments (e.g., wrong recipient, wrong amount) | Simulation fails or user sees unexpected values in summary |
| XSS injects code to modify XDR after build | Simulation catches malformed/unsigned XDR; wallet shows decoded operations before sign |
| Wallet extension replaced with malicious one | No protection; user must ensure extensions come from official sources |

### Cross-Site Scripting (XSS) Prevention

XSS attacks on the Checkmate-Escrow frontend can compromise the wallet connection flow, allowing attackers to redirect transactions, steal signing requests, or intercept signed transactions.

#### Content Security Policy (CSP) Headers

The frontend deployment must enforce strict Content Security Policy headers to prevent inline script execution and restrict external script sources.

**Recommended CSP Headers:**

```
Content-Security-Policy:
  default-src 'self';
  script-src 'self' https://albedo.link;
  connect-src 'self' https://horizon-testnet.stellar.org https://horizon.stellar.org https://albedo.link;
  style-src 'self' 'unsafe-inline';
  img-src 'self' data: https:;
  frame-src https://albedo.link;
  object-src 'none';
  base-uri 'self';
  form-action 'self';
  upgrade-insecure-requests;
```

**Header Breakdown:**

| Directive | Purpose |
|-----------|---------|
| `default-src 'self'` | All resources default to same-origin only |
| `script-src 'self' https://albedo.link` | Only load scripts from your domain or Albedo (for popup) |
| `connect-src 'self' https://horizon-*.stellar.org https://albedo.link` | Allow API calls to Horizon (RPC) and Albedo signing popup |
| `style-src 'self' 'unsafe-inline'` | Load styles from same-origin; inline styles if needed (prefer stylesheets instead) |
| `img-src 'self' data: https:` | Images from same-origin, data URIs, or HTTPS |
| `frame-src https://albedo.link` | Albedo signing popup is allowed as a framed context |
| `object-src 'none'` | Disable plugins (Flash, etc.) |
| `base-uri 'self'` | Prevent `<base>` tag hijacking |
| `form-action 'self'` | Forms can only POST to same-origin |
| `upgrade-insecure-requests` | Force HTTP → HTTPS upgrades |

**Implementation:**

- **Webpack/Vite**: Configure headers in `vite.config.ts` (dev) and deployment middleware (production)
- **Nginx**: Add to `server` block
- **Express.js**: Use `helmet()` middleware with custom CSP config
- **Vercel/Netlify**: Configure in `vercel.json` / `netlify.toml` via `_headers` file

#### Subresource Integrity (SRI)

For any external JavaScript dependencies (e.g., `@stellar/js-sdk`), use Subresource Integrity (SRI) hashes to ensure the file has not been tampered with:

```html
<script src="https://cdn.example.com/stellar.js"
        integrity="sha384-abc123...xyz"
        crossorigin="anonymous"></script>
```

Generate SRI hashes with:
```bash
npm install -g npm-sri
npm-sri https://cdn.example.com/stellar.js
```

#### Input Validation & Output Encoding

1. **Validate Transaction XDR**: Before passing XDR to the wallet, validate it is well-formed (use `TransactionBuilder.fromXDR()` or `FeeBumpTransaction.fromXDR()` to parse)
2. **Encode User Output**: When displaying contract addresses, account IDs, or transaction hashes, use `.substring(0, 4) + '...' + .substring(-4)` (truncate) rather than `innerHTML` to prevent injection
3. **Sanitize Error Messages**: If displaying error messages from RPC nodes or contract failures, sanitize them before rendering (use a library like `xss` or `DOMPurify`)

#### Example with DOMPurify

```typescript
import DOMPurify from 'dompurify';

// Safe rendering of account address
const displayAddress = (address: string) => {
  const truncated = address.substring(0, 6) + '...' + address.substring(-6);
  return DOMPurify.sanitize(truncated); // Still safe for plain text, but defensive
};

// Safe error message display
const showError = (message: string) => {
  const sanitized = DOMPurify.sanitize(message, { ALLOWED_TAGS: [] });
  errorDiv.textContent = sanitized; // textContent is safer than innerHTML
};
```

### Wallet Connection Best Practices

#### Wallet Security Model

Checkmate-Escrow supports **Freighter** and **Albedo** for signing. Both wallets implement the following security principles:

1. **No Private Key Export**: Private keys are never exposed to the frontend or user
2. **Explicit User Approval**: Every transaction requires explicit user approval in the wallet UI
3. **Transaction Inspection**: Both wallets decode and display transaction details before signing
4. **Isolated Signing Context**: Freighter's extension sandbox and Albedo's popup window isolate signing from the host page

#### Connection Best Practices

1. **Use Official Wallets**: Link to official wallet installation pages:
   - Freighter: https://freighter.app
   - Albedo: https://albedo.link (no installation required)

2. **Validate Wallet Availability**: Before attempting connection, check that the wallet extension or popup window is available:

   ```typescript
   import { isFreighterInstalled } from '@stellar/freighter-api';

   if (!isFreighterInstalled()) {
     showMessage('Freighter not installed. Please install it first.');
   }

   // Albedo is always available in a browser (popup-based)
   ```

3. **Handle Disconnection Gracefully**: 
   - Users may revoke wallet access, close the browser extension, or manually disconnect
   - Display a "Reconnect" button rather than showing errors
   - Clear local state (auth tokens, cached balances) on disconnect
   - Do not cache signed transactions across sessions without re-authorization

4. **Reject Unexpected Network Changes**: 
   - Wallets may switch networks (testnet ↔ mainnet)
   - Always verify the connected network matches the expected network before signing
   - Reject transactions if the network mismatches:

   ```typescript
   const expectedNetwork = 'Test SDF Network ; September 2015'; // testnet
   const userNetwork = await wallet.getNetwork();
   
   if (userNetwork !== expectedNetwork) {
     throw new Error(
       `Network mismatch. Expected: ${expectedNetwork}, got: ${userNetwork}`
     );
   }
   ```

5. **Rate-Limit Signing Requests**: Prevent rapid-fire signing requests that could confuse users or enable replay attacks:

   ```typescript
   let lastSignTime = 0;
   const SIGN_COOLDOWN_MS = 2000; // 2 seconds between sign requests

   async function signWithCooldown(xdr: string) {
     const now = Date.now();
     if (now - lastSignTime < SIGN_COOLDOWN_MS) {
       throw new Error('Please wait before signing another transaction');
     }
     lastSignTime = now;
     return await wallet.sign(xdr);
   }
   ```

6. **Display Clear Feedback**: Show users what action is in progress:
   - "Connecting to wallet..."
   - "Signing transaction..." (disable buttons during signing)
   - "Transaction signed. Submitting to network..." 
   - "Transaction submitted. Waiting for confirmation..."

#### Wallet-Specific Considerations

**Freighter (Browser Extension)**
- Available in Chrome, Firefox, Edge
- Keys stored in extension storage (browser sandbox)
- Extension popup overlays the page during signing
- Availability check: `isFreighterInstalled()` from `@stellar/freighter-api`

**Albedo (Web Popup)**
- No installation required — works in any browser
- User logs into `albedo.link` with their chosen key manager (TOTP, password, Ledger, etc.)
- Opens a popup window for signing (popup blocker must allow it)
- Always available: check `typeof window !== 'undefined'` and trigger from user gesture (click event)

#### Session Management

1. **Expiration**: Do not cache wallet sessions indefinitely. Prompt user to re-connect after:
   - 24 hours of inactivity
   - Page reload / new session
   - Manual disconnect

2. **Multi-Tab Safety**: If user connects in one tab and disconnects in another, sync state:
   - Use `localStorage` change events or `BroadcastChannel` to notify other tabs
   - Clear wallet session state across all tabs on disconnect

   ```typescript
   // Broadcast disconnect to other tabs
   window.localStorage.setItem('wallet:disconnect', Date.now().toString());

   // Listen for disconnect from other tabs
   window.addEventListener('storage', (event) => {
     if (event.key === 'wallet:disconnect') {
       disconnect(); // Clear local state
     }
   });
   ```

3. **Secure Storage**: Store only public data (address, balance, transaction history):
   - Never store private keys, seed phrases, or wallet passwords
   - Never store signed XDR longer than needed (clear after broadcast)
   - Use `sessionStorage` for temporary data, `localStorage` for persistent public data

#### Error Handling

| Scenario | Cause | User Action |
|----------|-------|-------------|
| "Wallet not installed" | Freighter not found | Redirect to installation page |
| "Connection denied" | User rejected access in wallet | Offer to retry or choose different wallet |
| "Signing cancelled" | User clicked cancel in wallet popup | Offer to retry transaction |
| "Invalid network" | Wallet switched to different network | Prompt user to switch back |
| "Timeout signing" | Wallet popup closed or locked | Offer to retry transaction |

### Monitoring & Incident Response

#### Frontend Security Monitoring

1. **CSP Violations**: Log CSP violations to identify injection attempts:
   ```typescript
   document.addEventListener('securitypolicyviolation', (e) => {
     console.error('CSP violation:', {
       blockedURI: e.blockedURI,
       violatedDirective: e.violatedDirective,
       originalPolicy: e.originalPolicy,
     });
     // Send to monitoring service (Sentry, Datadog, etc.)
   });
   ```

2. **Wallet Anomalies**:
   - Multiple failed signing attempts from same session
   - Network mismatches (user switches networks mid-session)
   - Unusual transaction patterns (e.g., rapid successive transactions)

3. **XSS Attempts**:
   - Monitor console for unexpected errors
   - Log failed deserialization of XDR or responses
   - Alert on unusual API calls or network traffic

#### Incident Response Checklist

- [ ] Disable wallet connection if XSS vulnerability detected
- [ ] Rotate CSRF tokens if CSRF found
- [ ] Review CSP logs for injection attempts
- [ ] Audit recent transactions for unauthorized activity
- [ ] Notify users if sensitive data may have been compromised
- [ ] Update CSP/SRI hashes after patching vulnerabilities

---

## Known Limitations

<!-- doc-conformance: verified path=contracts/escrow/src/lib.rs line=77 sha256=a53a22fce1721f98852569ab296136700baf0f930d585a36272ab4a1feec5771 -->
<!-- doc-conformance: verified path=contracts/escrow/src/lib.rs line=80 sha256=1572038928a413dc2a9ebef591e136dfd9ed9fc3d75549c41a53451fd4f33c54 -->
<!-- doc-conformance: verified path=contracts/escrow/src/lib.rs line=83 sha256=7f65c4fb2fd2bdbf73be63e7e9d896cd7c3596ccfe3806513966026eea57cce1 -->

### Smart Contract Limitations

1. **Token Support Is Allowlist-Gated, Not "Native"**: The contract accepts any Stellar Asset Contract (SAC) token — including XLM's wrapped SAC and Soroban-native token contracts such as USDC — subject to an optional admin-managed allowlist (`add_allowed_token` / `is_token_allowed`). There is no separate "native XLM" fast-path distinct from the generic token interface; every token, including XLM, is moved via the standard `token::Client` transfer interface. A `create_match_with_conversion` path additionally supports two-token ("multi-token") matches where each player stakes a different token at an oracle-validated conversion rate — see [Roadmap v1.0.1](roadmap.md#v101--multi-token-conversion-rate-hardening-complete) for the settlement-correctness history of that feature.
2. **Configurable Timeout, Not Fixed**: Match expiration is **not** hardcoded. `set_match_timeout` (admin-only) accepts any value in `[MIN_MATCH_TIMEOUT_SECONDS, MAX_MATCH_TIMEOUT_SECONDS]` = `[86,400, 7,776,000]` seconds (1 day to 90 days). If never set, `DEFAULT_MATCH_TIMEOUT_SECONDS` (2,592,000 seconds, 30 days) applies. See `contracts/escrow/src/lib.rs:77-83` and `set_match_timeout` (`contracts/escrow/src/lib.rs:2616`).
3. **No Partial Withdrawals**: Players cannot withdraw partial stakes
4. **Single Oracle (EscrowContract)**: `EscrowContract` still trusts exactly one
   configured oracle address as the authoritative trigger for payouts (see
   `submit_result` / `get_oracle`). This is unchanged by the m-of-n consensus
   feature below, which lives in the separate `OracleContract` and governs its
   own audit-log finalization, not `EscrowContract`'s payout authorization.

### Oracle Limitations

1. **API Dependency**: Relies on external Lichess/Chess.com APIs remaining available
2. **Rate Limiting**: Subject to platform API rate limits
3. **Game Format**: Only supports standard chess games, not variants
4. **Real-time Delay**: Results submitted after games complete, not in real-time
5. **`OracleContract` admin override survives m-of-n**: `OracleContract` now
   supports genuine m-of-n oracle consensus (`submit_oracle_result`,
   `set_consensus_threshold`) with load-bearing staking/slashing — see
   [docs/oracle.md § m-of-n Oracle Consensus](oracle.md#m-of-n-oracle-consensus)
   for the protocol, its Byzantine-fault-tolerance bound, and the migration
   path. However, the legacy admin-gated `submit_result` /
   `submit_batch_results` functions remain callable regardless of the
   configured threshold, so the admin key retains a standing unilateral
   override on `OracleContract`'s own result storage even in an otherwise
   fully-migrated m-of-n deployment.

### Platform Limitations

1. **Stellar Network**: Subject to Stellar network outages or congestion
2. **Soroban Limits**: Bounded by Soroban resource limits (CPU, memory, storage)
3. **Ledger TTL**: Contract state expires if not extended (~30 days)

### Security Limitations

1. **Admin Trust**: Admin keys must be kept secure (no on-chain enforcement)
2. **Oracle Centralization**: `EscrowContract` payouts remain gated by a
   single configured oracle address (see above) regardless of how many
   independent oracles are registered and voting in `OracleContract`.
   Within `OracleContract` itself, centralization is now opt-in rather than
   structural: an admin that configures `set_consensus_threshold(1)` (the
   default) or continues using `submit_result` keeps a single point of
   failure for result verification; raising the threshold and registering
   independent oracles trades that for the Byzantine-fault-tolerance bound
   documented in [docs/oracle.md](oracle.md#byzantine-fault-tolerance-bound),
   at the cost of the residual admin override noted above.
3. **No Upgrade Path**: No built-in contract upgrade mechanism
4. **Event Monitoring**: Security depends on off-chain monitoring of contract events

## Deployment Security

### Pre-Deployment Checklist

- [ ] Verify contract bytecode matches audited source
- [ ] Test deployment on testnet with same parameters
- [ ] Prepare multi-sig admin addresses
- [ ] Set up monitoring for contract events
- [ ] Plan emergency response procedures

### Deployment Sequence Security

1. **Atomic Deploy+Init**: Deploy and initialize in same transaction to prevent front-running
2. **Address Validation**: Verify all addresses (oracle, admin) before deployment
3. **Network Verification**: Double-check target network (mainnet vs testnet)
4. **Backup Keys**: Ensure admin keys are backed up securely

### Post-Deployment Verification

```bash
# Verify initialization
stellar contract invoke --id $ESCROW_ID -- get_admin
stellar contract invoke --id $ORACLE_ID -- get_admin

# Test basic functionality
stellar contract invoke --id $ESCROW_ID -- is_paused
```

## Operational Security

### Key Management

1. **Hardware Security Modules (HSM)**: Use HSMs for admin key storage
2. **Key Rotation**: Regularly rotate admin and oracle keys
3. **Access Controls**: Limit who has access to operational keys
4. **Backup Procedures**: Secure, tested key recovery procedures

### Monitoring & Alerting

**Critical Events to Monitor:**
- Contract pause/unpause events
- Admin address changes
- Active match cap alerts (`match/limit_reached` when `TooManyActiveMatches` limit is hit)
- Unusual transaction patterns
- Oracle submission delays
- API failures

**Recommended Alerts:**
- Multiple failed result submissions
- Unexpected contract state changes
- Large stake amounts
- Rapid match creation

### Incident Response

1. **Detection**: Automated monitoring detects anomalies
2. **Assessment**: Security team evaluates threat level
3. **Containment**: Pause contracts if necessary
4. **Recovery**: Restore normal operations or migrate
5. **Lessons Learned**: Update security measures

### Regular Security Audits

- [ ] Quarterly security reviews
- [ ] Annual third-party audits
- [ ] Penetration testing of oracle infrastructure
- [ ] Code review of contract upgrades

## `admin_recover_match` Time-Lock (Issue #1812)

`admin_recover_match` is an admin escape hatch for `Pending` matches whose
token was blacklisted after creation, making `expire_match` permanently fail
with `TokenNotAllowed`.  Without a guard, a single compromised admin key could
immediately drain any active escrow — no game needs to occur, no player
consent is required.

### The Risk

| Scenario | Without time-lock | With time-lock |
|----------|-------------------|----------------|
| Admin key compromised | Attacker drains any funded escrow instantly | Attacker must wait until the match has already timed out (≥ 1 day at minimum timeout) |
| Admin calls function on fresh match | Succeeds immediately | Rejected with `Error::RecoveryTimelockNotElapsed` (code 59) |
| Admin calls after match timeout elapses | N/A | Succeeds — match is genuinely stuck |

### The Fix

`admin_recover_match` now enforces a **time-lock equal to the configured match
timeout** before it will act.  Concretely:

```rust
let elapsed = env.ledger().sequence().saturating_sub(m.created_ledger);
let timeout = Self::current_match_timeout(&env);
if elapsed < timeout {
    return Err(Error::RecoveryTimelockNotElapsed);
}
```

This reuses the **same ledger-elapsed threshold** that `expire_match` checks
(`elapsed >= current_match_timeout()`), so admin recovery is only possible
once the match would already be eligible for normal expiry.  The time-lock
cannot be shortened below `MIN_MATCH_TIMEOUT_SECONDS` (86,400 seconds / 1 day)
because `set_match_timeout` enforces that bound.

### Security Properties

1. **No new trust assumptions**: the time-lock is enforced entirely in the
   smart contract without any off-chain component.
2. **No new admin privilege escalation**: the fix only *removes* the ability to
   act early; it does not add any new capability.
3. **Fund safety preserved**: a frozen or blacklisted-token match can still be
   recovered by the admin once the timeout window has closed, so funds are
   never permanently locked.
4. **Error surfaced as code 59** (`RecoveryTimelockNotElapsed`) — distinct from
   `MatchNotExpired` (code 14, used by `expire_match`) so callers and monitoring
   can distinguish the two rejection reasons.

### Threat Model Update

| Attack | Before fix | After fix |
|--------|-----------|-----------|
| Compromised admin key drains fresh escrow | ✅ possible (instant) | ❌ blocked until match timeout elapses |
| Legitimate admin recovers blacklisted-token match | ✅ | ✅ (after match timeout) |
| Admin bypasses oracle for Active match | Not this function's scope | Not this function's scope (see `admin_resolve_stalled_match`) |

---

## Security Contacts

For security-related issues or concerns:

- **Critical Vulnerabilities**: Report immediately via [security@checkmate-escrow.com](mailto:security@checkmate-escrow.com)
- **General Security Questions**: [security@checkmate-escrow.com](mailto:security@checkmate-escrow.com)
- **Bug Bounty**: See our bug bounty program at [bounty.checkmate-escrow.com](https://bounty.checkmate-escrow.com)

## Version History

- **v1.0.0**: Initial security documentation
- **Last Updated**: May 28, 2026</content>
<parameter name="filePath">/home/farouq/Desktop/Checkmate-Escrow/docs/security.md