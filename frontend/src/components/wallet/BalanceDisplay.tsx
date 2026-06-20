import { useBalance } from '../../hooks/useBalance';

interface Props {
  publicKey: string | null;
}

export function BalanceDisplay({ publicKey }: Props) {
  const { balance, loading, error } = useBalance(publicKey);

  if (!publicKey) return null;

  return (
    <div role="status" aria-live="polite" aria-label="XLM balance">
      {loading && !balance && <span>Loading balance…</span>}
      {error && <span role="alert">{error}</span>}
      {balance != null && <span>{balance} XLM</span>}
    </div>
  );
}
