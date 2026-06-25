import { useState, useCallback } from 'react';
import { Networks } from '@stellar/stellar-sdk';
import { freighterSign } from '../wallets/freighter';
import { albedoSign } from '../wallets/albedo';
import type { WalletType } from '../wallets/types';

const NETWORK = import.meta.env.VITE_STELLAR_NETWORK === 'mainnet'
  ? Networks.PUBLIC
  : Networks.TESTNET;

export function useTransaction(walletType: WalletType | null) {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const sign = useCallback(async (xdr: string): Promise<string | null> => {
    if (!walletType) {
      setError('No wallet connected.');
      return null;
    }
    setLoading(true);
    setError(null);
    try {
      const { signedXdr } = walletType === 'freighter'
        ? await freighterSign(xdr, NETWORK)
        : await albedoSign(xdr, 'testnet');
      return signedXdr;
    } catch (err) {
      setError((err as Error).message);
      return null;
    } finally {
      setLoading(false);
    }
  }, [walletType]);

  return { sign, signing: loading, loading, error };
}
