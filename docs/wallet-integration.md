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
