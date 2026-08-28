# Wallet Integration Guide

Checkmate-Escrow supports **Freighter** and **Albedo** for transaction signing. No private keys are ever stored or transmitted — all signing happens inside the wallet extension or popup.

## Supported Wallets

| Wallet    | Type              | Install |
|-----------|-------------------|---------|
| Freighter | Browser extension | [freighter.app](https://freighter.app) |
| Albedo    | Web popup         | No install needed |

## Hooks

### `useWallet()`

Manages connection state for either wallet.

```tsx
import { useWallet } from './hooks/useWallet';

const { connected, publicKey, type, error, connect, disconnect } = useWallet();

// Connect
await connect('freighter');  // or 'albedo'

// Disconnect
disconnect();
```

State resets cleanly on disconnect; switching wallets does not require a page reload — just call `connect` with the new wallet type.

### `useBalance(publicKey)`

Fetches the account's native XLM balance from Horizon and refreshes every 10 seconds.

```tsx
import { useBalance } from './hooks/useBalance';

const { balance, loading, error } = useBalance(publicKey);
```

Configure the Horizon endpoint via `VITE_HORIZON_URL` (defaults to testnet).

### `useTransaction(walletType)`

Signs a transaction XDR using the active wallet.

```tsx
import { useTransaction } from './hooks/useTransaction';

const { sign, signing, error } = useTransaction(walletType);
const signedXdr = await sign(xdr);
```

Returns `null` on cancellation or error; `error` is set with the message.

## Components

### `<WalletConnector wallet={...} />`

Renders connect buttons when disconnected; shows a truncated public key and a disconnect button when connected.

```tsx
import { WalletConnector } from './components/wallet';

const wallet = useWallet();
<WalletConnector wallet={wallet} />
```

### `<BalanceDisplay publicKey={...} />`

Shows the live XLM balance for the connected account. Renders nothing when `publicKey` is null.

```tsx
<BalanceDisplay publicKey={wallet.publicKey} />
```

### `<TransactionSigner walletType xdr onSigned label? />`

Button that triggers signing and calls `onSigned(signedXdr)` on success. Disabled until a wallet is connected.

```tsx
<TransactionSigner
  walletType={wallet.type}
  xdr={unsignedXdr}
  onSigned={(signed) => submitToStellar(signed)}
/>
```

### `<WalletErrorBoundary>`

Wraps wallet UI to catch unexpected render errors.

```tsx
import { WalletErrorBoundary } from './components/wallet/WalletErrorBoundary';

<WalletErrorBoundary>
  <WalletConnector wallet={wallet} />
</WalletErrorBoundary>
```

## Environment Variables

| Variable               | Default                                    | Description              |
|------------------------|--------------------------------------------|--------------------------|
| `VITE_STELLAR_NETWORK` | `testnet`                                  | `testnet` or `mainnet`   |
| `VITE_HORIZON_URL`     | `https://horizon-testnet.stellar.org`      | Horizon server URL       |

## Security Notes

- Private keys are never accessed, stored, or transmitted by the frontend.
- All signing is delegated to the wallet (Freighter extension or Albedo popup).
- XDR is passed to the wallet as-is; the wallet shows the user what they are signing.

---

## Albedo Wallet Integration

Albedo is a web-based key management service that signs Stellar transactions through a secure popup. Unlike Freighter, Albedo requires no browser extension — it opens `albedo.link` in a popup window and the user signs there.

### How Albedo differs from Freighter

| Feature | Freighter | Albedo |
|---------|-----------|--------|
| Distribution | Browser extension (Chrome/Firefox) | Web popup (no install) |
| Key storage | Local extension storage | Albedo servers (user-controlled) |
| Sign flow | Extension popup | `albedo.link` popup window |
| Availability check | `isConnected()` API call | Always available in a browser (`typeof window !== 'undefined'`) |
| Network passphrase | Passed as `networkPassphrase` | Passed as `network` |

### Connecting and getting a public key

Albedo uses an *intent* model: each action (requesting a public key, signing a transaction) opens the Albedo popup and resolves a promise when the user approves or cancels.

```ts
import { albedoGetPublicKey, albedoIsAvailable } from './wallets/albedo';

// Albedo is always available in a browser — no install check needed
if (albedoIsAvailable()) {
  const publicKey = await albedoGetPublicKey();
  // e.g. "GABC123..."
}
```

Internally this calls `albedo.publicKey({})`, which opens the Albedo popup asking the user to share their public key. The promise resolves with the selected account's G-address on approval, or rejects if the user cancels.

### Signing a transaction

```ts
import { albedoSign } from './wallets/albedo';

const network = 'Test SDF Network ; September 2015'; // testnet passphrase
const signedResult = await albedoSign(unsignedXdr, network);
const signedXdr = signedResult.signedXdr;
```

`albedoSign` calls `albedo.tx({ xdr, network, submit: false })`. The `submit: false` flag tells Albedo to return the signed XDR without broadcasting — the frontend submits the transaction through its own Stellar RPC client. On user approval the popup closes and the promise resolves; on cancellation or error it rejects.

### The Albedo sign dialog

When `albedoSign` is called, the user sees the Albedo transaction-review popup:

```
┌─────────────────────────────────────────────┐
│  albedo.link                            [✕]  │
│─────────────────────────────────────────────│
│  Sign transaction                           │
│                                             │
│  Network: Test SDF Network                  │
│  From:    GABC...XYZ                        │
│                                             │
│  Operations:                                │
│    [1] InvokeHostFunction                   │
│        Contract: CESC...ROW                 │
│        Function: deposit                    │
│        Args: match_id=42                    │
│                                             │
│  [ Reject ]              [ Sign & submit ]  │
└─────────────────────────────────────────────┘
```

The dialog shows the decoded operations so the user can verify what they are signing before approving.

### Using Albedo via `useWallet()`

The `useWallet` hook handles Albedo the same way as Freighter — pass `'albedo'` to `connect`:

```tsx
import { useWallet } from './hooks/useWallet';

const { connected, publicKey, connect, disconnect } = useWallet();

// Connect with Albedo
await connect('albedo');

// publicKey is now set to the Albedo account's G-address
console.log(publicKey); // "GABC..."

// Disconnect clears local state; no Albedo logout is required
disconnect();
```

Switching from Freighter to Albedo mid-session does not require a page reload — call `connect('albedo')` and the hook resets cleanly.

### Error handling

Both `albedoGetPublicKey` and `albedoSign` reject their promises on user cancellation or Albedo errors. Wrap calls in try/catch and surface the error via the `error` field returned by `useWallet` or `useTransaction`:

```ts
try {
  await connect('albedo');
} catch (err) {
  // User closed the popup, or Albedo is blocked by a popup blocker
  console.error('Albedo connection failed:', err);
}
```

**Popup blockers:** Albedo opens a new window. Some browsers block popups unless the action originates from a direct user gesture (e.g. a button click). Always trigger Albedo calls inside a click handler — never on mount or in a `useEffect`.

### Network passphrase reference

| Stellar network | Passphrase |
|-----------------|-----------|
| Testnet | `Test SDF Network ; September 2015` |
| Mainnet | `Public Global Stellar Network ; September 2015` |
| Futurenet | `Test SDF Future Network ; October 2022` |

Pass the passphrase for the network your contract is deployed to. A mismatch will cause the signed transaction to be rejected by the RPC node.
