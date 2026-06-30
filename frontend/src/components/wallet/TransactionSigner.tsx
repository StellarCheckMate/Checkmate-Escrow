import { useState } from 'react';
import { useTransaction } from '../../hooks/useTransaction';
import type { WalletType } from '../../wallets/types';

interface Props {
  walletType: WalletType | null;
  xdr: string;
  onSigned: (signedXdr: string) => void;
  label?: string;
}

export function TransactionSigner({ walletType, xdr, onSigned, label = 'Sign Transaction' }: Props) {
  const { sign, signing, error } = useTransaction(walletType);
  const [done, setDone] = useState(false);

  async function handleSign() {
    const result = await sign(xdr);
    if (result) {
      setDone(true);
      onSigned(result);
    }
  }

  return (
    <div>
      <button type="button" onClick={handleSign} disabled={signing || done || !walletType}>
        {signing ? 'Signing…' : done ? 'Signed ✓' : label}
      </button>
      {error && <p role="alert">{error}</p>}
    </div>
  );
}
