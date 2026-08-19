import { useState, useEffect } from 'react';
import { Horizon } from '@stellar/stellar-sdk';

const HORIZON_URL = import.meta.env.VITE_HORIZON_URL ?? 'https://horizon-testnet.stellar.org';

export function useBalance(publicKey: string | null) {
  const [balance, setBalance] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Reset balance synchronously during render when publicKey becomes null,
  // instead of from inside an effect (avoids a cascading extra render).
  const [prevPublicKey, setPrevPublicKey] = useState(publicKey);
  if (publicKey !== prevPublicKey) {
    setPrevPublicKey(publicKey);
    if (!publicKey) {
      setBalance(null);
    }
  }

  useEffect(() => {
    if (!publicKey) {
      return;
    }

    let cancelled = false;
    async function fetch() {
      setLoading(true);
      setError(null);
      try {
        const server = new Horizon.Server(HORIZON_URL);
        const account = await server.loadAccount(publicKey!);
        const xlm = account.balances.find(
          (b): b is Horizon.HorizonApi.BalanceLine<'native'> => b.asset_type === 'native'
        );
        if (!cancelled) setBalance(xlm?.balance ?? '0');
      } catch (err) {
        if (!cancelled) setError((err as Error).message);
      } finally {
        if (!cancelled) setLoading(false);
      }
    }

    fetch();
    const interval = setInterval(fetch, 10_000);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, [publicKey]);

  return { balance, loading, error };
}
