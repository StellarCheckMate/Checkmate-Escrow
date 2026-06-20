import type { WalletState, WalletType } from '../wallets/types';

interface Props {
  wallet: WalletState & {
    connect: (type: WalletType) => Promise<void>;
    disconnect: () => void;
  };
}

export function WalletConnector({ wallet }: Props) {
  const { connected, publicKey, error, connect, disconnect } = wallet;

  if (connected && publicKey) {
    return (
      <div role="region" aria-label="Wallet">
        <span title={publicKey}>
          {publicKey.slice(0, 6)}…{publicKey.slice(-4)}
        </span>
        <button type="button" onClick={disconnect}>
          Disconnect
        </button>
      </div>
    );
  }

  return (
    <div role="region" aria-label="Connect wallet">
      <button type="button" onClick={() => connect('freighter')}>
        Connect Freighter
      </button>
      <button type="button" onClick={() => connect('albedo')}>
        Connect Albedo
      </button>
      {error && <p role="alert">{error}</p>}
    </div>
  );
}
