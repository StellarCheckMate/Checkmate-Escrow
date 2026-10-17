import { useState, useEffect } from 'react';
import { Horizon } from '@stellar/stellar-sdk';
import { fetchTokenDecimals, formatTokenAmount } from '../utils/tokenFormat';

const HORIZON_URL = import.meta.env.VITE_HORIZON_URL ?? 'https://horizon-testnet.stellar.org';

// Native XLM balances are reported with 7 decimal places.
const NATIVE_DECIMALS = 7;

export function useBalance(publicKey: string | null) {
  const [balance, setBalance] = useState<string | null>(null);
  const [rawBalance, setRawBalance] = useState<string | null>(null);
  const [decimals, setDecimals] = useState<number>(NATIVE_DECIMALS);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Reset balance synchronously during render when publicKey becomes null,
  // instead of from inside an effect (avoids a cascading extra render).
  const [prevPublicKey, setPrevPublicKey] = useState(publicKey);
  if (publicKey !== prevPublicKey) {
    setPrevPublicKey(publicKey);
    if (!publicKey) {
      setBalance(null);
      setRawBalance(null);
      setDecimals(NATIVE_DECIMALS);
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
        const raw = xlm?.balance ?? '0';
        const tokenDecimals = await fetchTokenDecimals('native');
        if (!cancelled) {
          setRawBalance(raw);
          setDecimals(tokenDecimals);
          setBalance(formatTokenAmount(raw, tokenDecimals));
        }
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

  return { balance, rawBalance, decimals, loading, error };
}
