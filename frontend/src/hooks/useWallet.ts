import { useState, useCallback } from 'react';
import { freighterIsAvailable, freighterGetPublicKey } from '../wallets/freighter';
import { albedoIsAvailable, albedoGetPublicKey } from '../wallets/albedo';
import type { WalletState, WalletType } from '../wallets/types';

const INITIAL: WalletState = { type: null, publicKey: null, connected: false, error: null };

export function useWallet() {
  const [state, setState] = useState<WalletState>(INITIAL);

  const connect = useCallback(async (type: WalletType) => {
    setState((s) => ({ ...s, error: null }));
    try {
      let publicKey: string;
      if (type === 'freighter') {
        const available = await freighterIsAvailable();
        if (!available) throw new Error('Freighter extension not detected. Please install it.');
        publicKey = await freighterGetPublicKey();
      } else {
        if (!albedoIsAvailable()) throw new Error('Albedo is not available in this environment.');
        publicKey = await albedoGetPublicKey();
      }
      setState({ type, publicKey, connected: true, error: null });
    } catch (err) {
      setState(INITIAL);
      setState((s) => ({ ...s, error: (err as Error).message }));
    }
  }, []);

  const disconnect = useCallback(() => {
    setState(INITIAL);
  }, []);

  return { ...state, connect, disconnect };
}
