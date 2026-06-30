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

2. **Alternative contact**: If you cannot use GitHub's advisory system, email the maintainers directly (contact info in [CONTRIBUTING.md](CONTRIBUTING.md)).

## Response Timeline

We aim to:
- **Acknowledge** your report within **48 hours**
- **Assess** the vulnerability and develop a fix within **7 days**
- **Release** a patched version within **14 days** of confirmation (or sooner for critical issues)
- **Coordinate** public disclosure with you before announcing the fix

Critical vulnerabilities affecting mainnet funds may be handled with expedited timelines and direct coordination.

## Safe Harbour

We recognize that security researchers may need to test systems to discover vulnerabilities. Provided that you:
- Report the issue to us privately before any public disclosure
- Avoid accessing, modifying, or destroying data outside the scope of your testing
- Don't disrupt service availability or engage in denial-of-service attacks
- Comply with the terms of this policy

...you will not face legal action from Checkmate-Escrow or its maintainers for responsible disclosure.

## Scope

This policy covers:
- ✅ Smart contract code ([`contracts/escrow/src`](contracts/escrow/src), [`contracts/oracle/src`](contracts/oracle/src))
- ✅ Oracle service code ([`oracle-service/`](oracle-service))
- ✅ Frontend code ([`frontend/`](frontend))

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

We're grateful to security researchers who responsibly disclose vulnerabilities. With your permission, we will acknowledge your contribution in the fix release and this document.

## Additional Resources

- [Error Codes Reference](docs/error-codes.md) — understand contract errors
- [Threat Model & Security](docs/security.md) — architecture security assumptions
- [Stellar Soroban Security](https://developers.stellar.org/soroban/security) — platform-level security best practices
