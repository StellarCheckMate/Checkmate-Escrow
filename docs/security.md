# Threat Model & Security

This document outlines the security considerations, trust assumptions, and risk mitigations for the Checkmate-Escrow smart contracts on Stellar Soroban.

## Table of Contents

- [Oracle Trust Assumptions](#oracle-trust-assumptions)
- [Admin Key Risks](#admin-key-risks)
- [Re-initialization Protection](#re-initialization-protection)
- [Pause Mechanism](#pause-mechanism)
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

## Admin Key Risks

Both contracts have admin addresses with elevated privileges. Compromise of admin keys represents a significant risk.

### Escrow Contract Admin Powers

- **Pause/Unpause**: Can halt all contract operations (`create_match`, `deposit`, `submit_result`)
- **Admin Rotation**: Can change the admin address to a new one
- **Emergency Control**: Can intervene in case of detected issues

### Oracle Contract Admin Powers

- **Pause/Unpause**: Can halt result submissions
- **Admin Rotation**: Can change the oracle admin address
- **Result Submission**: Can submit results directly (bypassing automated oracle)

### Risk Mitigations

1. **Multi-sig Recommendation**: Use multi-signature wallets for admin addresses in production
2. **Key Separation**: Use different admin addresses for escrow and oracle contracts
3. **Cold Storage**: Store admin keys offline when not in use
4. **Monitoring**: Monitor admin operations via contract events
5. **Timelock**: Consider implementing timelocks for critical admin operations

### Compromise Scenarios

| Scenario | Impact | Mitigation |
|----------|--------|------------|
| Escrow admin key stolen | Contract can be paused, funds locked | Immediate pause, admin rotation |
| Oracle admin key stolen | Incorrect results can be submitted | Pause oracle, rotate admin, dispute results |
| Both keys compromised | Complete contract takeover | Emergency pause, contract migration |

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

### Emergency Response Protocol

1. **Detection**: Monitor for suspicious activity (unusual transaction patterns, oracle delays)
2. **Immediate Pause**: Admin pauses affected contract(s)
3. **Investigation**: Analyze the incident while contract is paused
4. **Resolution**: Either unpause after false alarm or migrate to new contract
5. **Post-mortem**: Update security measures based on lessons learned

## Known Limitations

### Smart Contract Limitations

1. **No Native Token Support**: Only supports Stellar assets (XLM, USDC), not native tokens
2. **Fixed Timeout**: Match expiration timeout is hardcoded (~24 hours)
3. **No Partial Withdrawals**: Players cannot withdraw partial stakes
4. **Single Oracle**: Only one oracle address per escrow contract

### Oracle Limitations

1. **API Dependency**: Relies on external Lichess/Chess.com APIs remaining available
2. **Rate Limiting**: Subject to platform API rate limits
3. **Game Format**: Only supports standard chess games, not variants
4. **Real-time Delay**: Results submitted after games complete, not in real-time

### Platform Limitations

1. **Stellar Network**: Subject to Stellar network outages or congestion
2. **Soroban Limits**: Bounded by Soroban resource limits (CPU, memory, storage)
3. **Ledger TTL**: Contract state expires if not extended (~30 days)

### Security Limitations

1. **Admin Trust**: Admin keys must be kept secure (no on-chain enforcement)
2. **Oracle Centralization**: Single point of failure for result verification
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

---

## Security Contacts

For security-related issues or concerns:

- **Critical Vulnerabilities**: Report immediately via [security@checkmate-escrow.com](mailto:security@checkmate-escrow.com)
- **General Security Questions**: [security@checkmate-escrow.com](mailto:security@checkmate-escrow.com)
- **Bug Bounty**: See our bug bounty program at [bounty.checkmate-escrow.com](https://bounty.checkmate-escrow.com)

## Version History

- **v1.0.0**: Initial security documentation
- **Last Updated**: April 27, 2026</content>
<parameter name="filePath">/home/farouq/Desktop/Checkmate-Escrow/docs/security.md