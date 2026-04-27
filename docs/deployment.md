# Deployment Guide

## Testnet Deployment

### 1. Generate a deployer identity

```bash
stellar keys generate deployer --network testnet
```

### 2. Fund the account via Friendbot

```bash
stellar keys fund deployer --network testnet
```

### 3. Build the contracts

```bash
./scripts/build.sh
```

### 4. Deploy

```bash
./scripts/deploy_testnet.sh
```

### 5. Configure `.env`

```env
STELLAR_NETWORK=testnet
STELLAR_RPC_URL=https://soroban-testnet.stellar.org
CONTRACT_ESCROW=<deployed-contract-id>
CONTRACT_ORACLE=<deployed-contract-id>
```

---

## Mainnet Deployment

> ⚠️ Mainnet transactions are irreversible and involve real funds. Complete the pre-deployment checklist before proceeding.

### Pre-Deployment Checklist

- [ ] All contract tests pass (`cargo test`)
- [ ] Contracts audited or peer-reviewed for logic errors and reentrancy
- [ ] Oracle authorization logic verified — only the trusted oracle key can call `submit_result`
- [ ] Payout and draw-refund paths manually tested on testnet with real match flows
- [ ] Deployer key is a hardware wallet or stored in a secrets manager (not a plaintext file)
- [ ] Oracle hot key is separate from the deployer key and has minimum required permissions
- [ ] `.env` and key files are excluded from version control (confirm `.gitignore`)
- [ ] Contract WASM hash recorded for post-deployment verification

### Key Management

**Deployer key** — used once to deploy contracts, then should be kept offline.

- Use a hardware wallet (Ledger) or an HSM where possible.
- If using a software key, generate it on an air-gapped machine and import only the public key to CI.
- Never store the secret key in `.env`, CI environment variables, or any file tracked by git.

**Oracle hot key** — used by the oracle service to submit match results.

- Generate a dedicated keypair with no other permissions:
  ```bash
  stellar keys generate oracle-hot --network mainnet
  ```
- Store the secret in a secrets manager (AWS Secrets Manager, HashiCorp Vault, etc.).
- Rotate the key if it is ever exposed.

**Backup**

- Store mnemonic / secret key backups encrypted (e.g., GPG) in at least two offline locations.
- Document the recovery procedure and test it before going live.

### Deploy Steps

1. **Configure mainnet identity**

   ```bash
   stellar keys generate deployer --network mainnet
   # Fund the account with enough XLM to cover deployment fees (~10 XLM)
   ```

2. **Build optimized WASM**

   ```bash
   ./scripts/build.sh
   ```

3. **Deploy escrow contract**

   ```bash
   stellar contract deploy \
     --wasm target/wasm32-unknown-unknown/release/escrow.wasm \
     --source deployer \
     --network mainnet
   ```

4. **Deploy oracle contract**

   ```bash
   stellar contract deploy \
     --wasm target/wasm32-unknown-unknown/release/oracle.wasm \
     --source deployer \
     --network mainnet
   ```

5. **Initialize contracts** — invoke any `init` / `set_oracle` entry points with the oracle hot key's public address.

6. **Verify WASM hashes**

   ```bash
   stellar contract info --id <CONTRACT_ESCROW> --network mainnet
   stellar contract info --id <CONTRACT_ORACLE> --network mainnet
   ```

   Confirm the reported WASM hash matches `sha256sum` of the local WASM files.

7. **Configure `.env`**

   ```env
   STELLAR_NETWORK=mainnet
   STELLAR_RPC_URL=https://soroban-mainnet.stellar.org
   CONTRACT_ESCROW=<deployed-contract-id>
   CONTRACT_ORACLE=<deployed-contract-id>
   ```

8. **Move deployer key offline** — once deployment is confirmed, the deployer secret key should not remain on any internet-connected machine.

### Security Considerations

- **Upgrade path**: Soroban contracts are immutable once deployed. Plan for a proxy/migration pattern before deploying if upgrades may be needed.
- **Oracle trust**: The oracle key is the single point of trust for result submission. Compromise of this key allows fraudulent payouts. Rotate immediately if exposed.
- **Stake limits**: Consider enforcing a maximum stake per match in the contract to limit blast radius from bugs or oracle compromise.
- **Monitoring**: Set up alerts on the escrow contract address for unexpected large outflows.
- **RPC endpoint**: Use a reliable, authenticated RPC provider for the oracle service in production rather than the public endpoint.
