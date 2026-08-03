# Security Policy

## Supported Versions

| Version | Status | Support |
|---------|--------|---------|
| 1.x (main) | Active | Full support; security updates released on discovery |
| < 1.0 | Deprecated | No longer supported; upgrade to 1.x immediately |

## Reporting Security Vulnerabilities

If you discover a security vulnerability, **please do not open a public GitHub issue**. Instead:

1. **Contact the maintainers privately** via [GitHub Private Security Advisory](https://github.com/StellarCheckMate/Checkmate-Escrow/security/advisories):
   - Click **Report a vulnerability** on the Security tab
   - Describe the issue in detail, including:
     - Affected component (escrow contract, oracle, frontend, etc.)
     - Steps to reproduce
     - Potential impact (funds at risk, privacy leak, data corruption, etc.)
     - Your proposed fix (if any)

2. **Alternative contact**: If you cannot use GitHub's advisory system, email the core maintainers at:
   **security@stellarcheckmate.dev**
   
   Encrypt sensitive reports using our PGP public key (see [PGP Encryption](#pgp-encryption) below).

## Response Timeline (SLA)

We commit to the following response times for all vulnerability reports:

| Milestone | Target |
|-----------|--------|
| **Acknowledge** receipt of your report | Within **48 hours** |
| **Initial assessment** (severity triage) | Within **5 business days** |
| **Fix development** begins | Within **7 days** of confirmed vulnerability |
| **Patch release** for critical / high issues | Within **14 days** of confirmation |
| **Patch release** for medium / low issues | Within **30 days** of confirmation |
| **Public disclosure** coordination | After patch ships (or 90 days, whichever is sooner) |

Critical vulnerabilities that put mainnet funds at direct risk may be handled on an expedited timeline with direct maintainer coordination.

## Bug Bounty Program

Checkmate-Escrow operates a community bug bounty for responsible disclosures. Rewards are paid in XLM on the Stellar mainnet.

### Reward Tiers

| Severity | Description | Reward |
|----------|-------------|--------|
| **Critical** | Direct fund loss or theft, authentication bypass in smart contracts | Up to **5,000 XLM** |
| **High** | Oracle manipulation enabling incorrect payouts, escrow drain without player consent | Up to **2,000 XLM** |
| **Medium** | DoS of contract or oracle, privacy leak of match data, access-control bypass with limited impact | Up to **500 XLM** |
| **Low** | Non-exploitable bugs, minor logic errors with negligible impact | Recognition + **50 XLM** |

### Bounty Rules

- Only **first reporter** of a given vulnerability is eligible for a reward.
- The vulnerability must be **previously unknown** to the maintainers.
- You must give us **reasonable time** to fix the issue before public disclosure.
- Rewards are discretionary — the maintainers have final say on severity classification and reward amount.
- Participants must comply with all applicable laws. We do not authorise testing against production environments with real funds.
- Bounties are paid after a fix is shipped and the reporter has confirmed the patch.

> **Note**: The bounty program is community-funded and rewards are paid on a best-effort basis. Reward amounts may be adjusted based on program funding. Check the [Security tab](https://github.com/StellarCheckMate/Checkmate-Escrow/security) for the latest programme status.

## PGP Encryption

For reports that include sensitive details (private keys, exploit code, fund-at-risk scenarios), please encrypt your email using our PGP public key.

**Key ID**: `0xABCD1234` *(replace with actual key ID when generated)*  
**Fingerprint**: `ABCD EFGH IJKL MNOP QRST UVWX YZ01 2345 6789 ABCD` *(replace with actual fingerprint)*

```
-----BEGIN PGP PUBLIC KEY BLOCK-----
[Maintainers: publish your actual PGP public key here once generated.
See https://docs.github.com/en/authentication/managing-commit-signature-verification
for how to generate and export a GPG key.]
-----END PGP PUBLIC KEY BLOCK-----
```

To encrypt a report:

```bash
# Import the public key (once)
gpg --import checkmate-escrow-security.asc

# Encrypt your report
gpg --armor --recipient security@stellarcheckmate.dev \
    --encrypt your-vulnerability-report.txt

# The .asc output can safely be emailed or pasted into the advisory form
```

If you are unable to use PGP, use GitHub's private advisory system — communications there are end-to-end encrypted between you and the maintainers.

## Safe Harbour

We recognise that security researchers may need to test systems to discover vulnerabilities. Provided that you:

- Report the issue to us **privately** before any public disclosure
- **Do not** access, modify, or destroy data outside the scope of your testing
- **Do not** disrupt service availability or engage in denial-of-service attacks
- **Do not** test against production environments or real user funds
- Comply with the terms of this policy

…you will not face legal action from Checkmate-Escrow or its maintainers for responsible disclosure.

## Scope

This policy covers:

- ✅ Smart contract code ([`contracts/escrow/src`](contracts/escrow/src), [`contracts/oracle/src`](contracts/oracle/src))
- ✅ Oracle service code ([`oracle-service/`](oracle-service))
- ✅ Frontend code ([`frontend/`](frontend))
- ✅ Event indexer ([`services/event-indexer/`](services/event-indexer))
- ✅ WebSocket server ([`services/websocket-server/`](services/websocket-server))

**Out of scope**:

- ❌ Third-party services (Stellar RPC nodes, Lichess, Chess.com APIs)
- ❌ Infrastructure (GitHub, CI/CD runners, deployment systems)
- ❌ Social engineering or phishing
- ❌ Issues that don't affect confidentiality, integrity, or availability

## Vulnerability Examples

**In scope** (please report):

- Reentrancy attacks in smart contracts
- Oracle manipulation or false result injection
- Escrow fund loss or theft
- Authorization bypass
- Data corruption in contract storage
- Private key or seed phrase exposure in source code

**Out of scope** (don't report):

- Typos in documentation
- Slow performance without security impact
- Third-party service outages
- User mistakes (e.g., losing their own private keys)

## Credits

We are grateful to security researchers who responsibly disclose vulnerabilities. With your permission, we will acknowledge your contribution in the fix release notes and in this document.

## Additional Resources

- [Error Codes Reference](docs/error-codes.md) — understand contract errors
- [Threat Model & Security](docs/security.md) — architecture security assumptions
- [Security Audit Checklist](docs/SECURITY_AUDIT_CHECKLIST.md) — internal audit coverage
- [Stellar Soroban Security](https://developers.stellar.org/soroban/security) — platform-level security best practices
